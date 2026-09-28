// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the body-limit middleware of hono, which bounds the size of the
 * body of a request, and the default bound the base pipeline holds every read
 * of a body to.
 *
 * Derived from hono 4.11.9, dist/middleware/body-limit/index.js, MIT.
 */

//! `body_limit`, the bound of the size of the body of a request.
//!
//! The middleware lets a request with no body through. Where the request has a
//! `Content-Length` and no `Transfer-Encoding`, a length over the option
//! `max_size` answers the option `on_error` before the body is read, and any
//! other length goes on. Every other request goes on with its body counted as
//! it is read: the read fails with `Payload Too Large` once the bytes pass the
//! bound, and the answer of the chain gives way to the answer of `on_error`.
//! The default `on_error` throws an `HTTPException` of the status 413 over the
//! text `Payload Too Large`, which the error handler answers.
//!
//! The base pipeline of a Rust service holds every read of a body to
//! [`DEFAULT_MAX_SIZE`], 1 MiB, through [`behind_default_body_limit`] in the
//! entry of the plugin middleware and around it, divergence D76 of
//! `docs/byte-equality.md`. The first read checks the declared length before
//! a byte is read, and every read counts the bytes, so a route that never
//! reads its body answers as it would without the bound. A `body_limit` of the
//! app takes the place of the default on every request it matches, wherever it
//! stands in the chain, since the body limits of one request share one bound,
//! and where two of the app's own match, the smaller bound holds, as each of
//! hono's counts what the one before it lets through. Where a read passed the
//! bound, the answer gives way to the answer of the bound even where the route
//! caught the failed read, as the GraphQL handler does, where hono replaces an
//! answer only when the error of the read reached the error handler. The answer
//! of the bound takes every header of the answer it replaces but
//! `content-type`, as the setter of `c.res` carries them, and
//! `content-length`, which measures the body it replaces. The TypeScript
//! option keeps the pipeline of a TypeScript app, which bounds no body, so under the feature
//! `host-platform` there is no default, and a body its host holds reads as no
//! body to the middleware.

use core::{
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{
        Context as Task,
        Poll, //
    },
};
use std::rc::Rc;

use crate::{
    compose::Next,
    context::Context,
    fetch::{
        body::{
            Body,
            BodyError,
            BodyStream,
            Chunk,
            LocalBoxFuture,
            Stream, //
        },
        response::{
            HeadersInit,
            Response,
            ResponseInit, //
        },
    },
    http_exception::HttpException,
    web_app::{
        MiddlewareFn,
        RouteError, //
    },
};

/// The bound of the default of the base pipeline, 1 MiB.
pub const DEFAULT_MAX_SIZE: usize = 1024 * 1024;

/// The message of a body over the bound.
const ERROR_MESSAGE: &str = "Payload Too Large";

/// A function of the service over the context of a request whose body passes
/// the bound, which answers the request.
pub type OnError<E> = Rc<dyn Fn(Context) -> LocalBoxFuture<'static, Result<Response, E>>>;

/// The options of `body_limit(options)`, each as the middleware of hono reads it.
///
/// The default is the bound of the default of the base pipeline, 1 MiB, with
/// hono's answer.
pub struct BodyLimitOptions<E> {
    /// The largest body in bytes the middleware lets through, `maxSize`.
    pub max_size: usize,
    /// The answer to a body over the bound, `onError`, `None` for hono's 413.
    pub on_error: Option<OnError<E>>,
}

impl<E> Default for BodyLimitOptions<E> {
    fn default() -> Self {
        Self {
            max_size: DEFAULT_MAX_SIZE,
            on_error: None,
        }
    }
}

/// The bound of the body of one request, which the context holds and every
/// body limit on the chain of the request shares.
struct BodyBound {
    /// The largest body in bytes a read lets through.
    max_size: Cell<usize>,
    /// Whether a read passed the bound.
    crossed: Cell<bool>,
    /// Whether a body limit of the app set the bound, which takes it from the
    /// default.
    by_app: Cell<bool>,
    /// Whether a body limit threw hono's 413, which the error handler took in
    /// place of the error of the read, so a body limit further out finds no
    /// `BodyLimitError` in `c.error`.
    thrown: Cell<bool>,
}

/// The body of a request counted against its bound as it is read.
struct Bounded {
    /// The body as the request held it.
    inner: BodyStream,
    /// The bound.
    bound: Rc<BodyBound>,
    /// The length the request declares, [`declared_length`].
    declared: Option<u128>,
    /// The bytes read.
    read: usize,
    /// Whether the declared length was checked.
    checked: bool,
    /// Whether the stream ended at the bound.
    ended: bool,
}

impl Bounded {
    /// Ends the stream at the bound and returns the failure of the read.
    fn cross(&mut self) -> Chunk {
        self.ended = true;
        self.bound.crossed.set(true);
        Err(BodyError::Failed(String::from(ERROR_MESSAGE)))
    }
}

impl Stream for Bounded {
    type Item = Chunk;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Task<'_>) -> Poll<Option<Chunk>> {
        let this = self.get_mut();
        if this.ended {
            return Poll::Ready(None);
        }
        let max_size = this.bound.max_size.get();
        if !this.checked {
            this.checked = true;
            if this.declared.is_some_and(|length| passes(length, max_size)) {
                return Poll::Ready(Some(this.cross()));
            }
        }
        match this.inner.poll_chunk(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                this.read = this.read.saturating_add(chunk.len());
                if this.read > max_size {
                    return Poll::Ready(Some(this.cross()));
                }
                Poll::Ready(Some(Ok(chunk)))
            }
            polled => polled,
        }
    }

    fn take_driver(self: Pin<&mut Self>) -> Option<LocalBoxFuture<'static, ()>> {
        self.get_mut().inner.take_driver()
    }
}

/// Returns the middleware `body_limit(options)`.
#[must_use]
pub fn body_limit<E: RouteError + From<HttpException>>(
    options: BodyLimitOptions<E>,
) -> impl MiddlewareFn<E> {
    let options = Rc::new(options);
    move |c: Context, next: Next<E>| -> LocalBoxFuture<'static, Result<Response, E>> {
        let options = Rc::clone(&options);
        Box::pin(async move {
            let Some(bound) = bind(&c, options.max_size, true) else {
                return next.run(c).await;
            };
            if declared_length(&c).is_some_and(|length| passes(length, options.max_size)) {
                return answer(&options, &bound, &c).await;
            }
            let response = next.run(c.clone()).await?;
            if !bound.crossed.get() || bound.thrown.get() {
                return Ok(response);
            }
            drop_length(&c);
            let answer = answer(&options, &bound, &c).await?;
            c.set_res(answer);
            Ok(c.res())
        })
    }
}

/// Returns the future `rest` behind the default body limit of the base
/// pipeline, which holds every read of the body of the request of `c` to
/// [`DEFAULT_MAX_SIZE`] where no body limit of the app takes its place,
/// divergence D76.
///
/// The bound is made before `rest` is first polled. Where a read passed it,
/// the answer `rest` answers, or the error it answers, gives way to hono's
/// 413, which the error handler answers.
#[cfg(not(feature = "host-platform"))]
#[must_use]
pub fn behind_default_body_limit<T, E, F>(
    c: &Context,
    rest: F,
) -> LocalBoxFuture<'static, Result<T, E>>
where
    T: 'static,
    E: RouteError + From<HttpException>,
    F: Future<Output = Result<T, E>> + 'static,
{
    let c = c.clone();
    Box::pin(async move {
        let Some(bound) = bind(&c, DEFAULT_MAX_SIZE, false) else {
            return rest.await;
        };
        let outcome = rest.await;
        if !bound.crossed.get() || bound.by_app.get() || bound.thrown.get() {
            return outcome;
        }
        drop_length(&c);
        Err(too_large(&bound))
    })
}

/// Returns the future `rest` as it is, since under the feature `host-platform`
/// the TypeScript option bounds no body.
#[cfg(feature = "host-platform")]
pub fn behind_default_body_limit<F: Future>(_c: &Context, rest: F) -> F {
    rest
}

/// Returns the bound of the request of `c` with `max_size` in force, made at
/// the first body limit of its chain, which wraps the body in its count, and
/// `None` for a request with no body, which goes on unbounded.
///
/// A body limit of the app takes the bound from the default, and the smaller
/// bound holds where two of the app's own match. The default takes a bound
/// that a body limit of the app made before it as it is.
fn bind(c: &Context, max_size: usize, by_app: bool) -> Option<Rc<BodyBound>> {
    if let Some(bound) = c.extension::<BodyBound>() {
        if by_app {
            let held = bound.max_size.get();
            bound.max_size.set(if bound.by_app.get() {
                held.min(max_size)
            } else {
                max_size
            });
            bound.by_app.set(true);
        }
        return Some(bound);
    }
    let raw = c.req().raw();
    let inner = raw.body().to_stream()?;
    let bound = Rc::new(BodyBound {
        max_size: Cell::new(max_size),
        crossed: Cell::new(false),
        by_app: Cell::new(by_app),
        thrown: Cell::new(false),
    });
    raw.set_body(Body::Stream(BodyStream::new(Bounded {
        inner,
        bound: Rc::clone(&bound),
        declared: declared_length(c),
        read: 0,
        checked: false,
        ended: false,
    })));
    c.set_extension(Rc::clone(&bound));
    Some(bound)
}

/// Returns the answer of `options` to the body of `c` over the bound,
/// `onError(c)`, hono's 413 where the options name none.
///
/// # Errors
///
/// Returns what [`too_large`] returns where the options name no `on_error`,
/// and the error the function of the option threw.
async fn answer<E: RouteError + From<HttpException>>(
    options: &BodyLimitOptions<E>,
    bound: &BodyBound,
    c: &Context,
) -> Result<Response, E> {
    match &options.on_error {
        Some(on_error) => on_error(c.clone()).await,
        None => Err(too_large(bound)),
    }
}

/// Returns what hono throws for a body over the bound, the `HTTPException` of
/// the status 413 over the text `Payload Too Large`, which the error handler
/// answers, or the `TypeError` of a response the platform refuses.
fn too_large<E: RouteError + From<HttpException>>(bound: &BodyBound) -> E {
    bound.thrown.set(true);
    let init = ResponseInit {
        status: Some(413),
        status_text: None,
        headers: None,
    };
    match Response::new(Body::Text(Rc::from(ERROR_MESSAGE)), Some(init)) {
        Ok(res) => E::from(HttpException::with_response(413, res)),
        Err(error) => E::refusal(&error),
    }
}

/// Puts the answer of `c` in place without its `content-length`, which
/// measures the body the answer of the bound replaces, so the setter of
/// `c.res` carries every other header of it but `content-type`.
fn drop_length(c: &Context) {
    let held = c.res();
    if !held.has_header("content-length") {
        return;
    }
    let mut headers = held.headers();
    headers.delete("content-length");
    let init = ResponseInit {
        status: Some(held.status()),
        status_text: None,
        headers: Some(HeadersInit::Fetch(headers)),
    };
    c.put_res(Response::new(Body::Empty, Some(init)).unwrap_or_else(|_| Response::empty(413)));
}

/// Returns the length the request of `c` declares, its `Content-Length` where
/// it has no `Transfer-Encoding`, as `parseInt` reads it, and `None` where it
/// declares none or a length no body passes.
fn declared_length(c: &Context) -> Option<u128> {
    let req = c.req();
    if req.header("transfer-encoding").is_some() {
        return None;
    }
    parse_int(&req.header("content-length")?)
}

/// Returns whether the declared `length` passes `max_size`,
/// `contentLength > maxSize`.
fn passes(length: u128, max_size: usize) -> bool {
    u128::try_from(max_size).is_ok_and(|max_size| length > max_size)
}

/// Returns the integer `parseInt(value, 10)` reads at the start of `value`,
/// and `None` for `NaN` and for a number below 0, which no length passes.
///
/// The white space at the start is skipped, a sign is read, and the longest run
/// of decimal digits after it is the number, which saturates where it passes
/// every length.
fn parse_int(value: &str) -> Option<u128> {
    let value = value.trim_start_matches(is_js_white_space);
    let (negative, digits) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let run = digits.bytes().take_while(u8::is_ascii_digit).count();
    if run == 0 {
        return None;
    }
    let number = digits.bytes().take(run).fold(0_u128, |number, digit| {
        number
            .saturating_mul(10)
            .saturating_add(u128::from(digit - b'0'))
    });
    (!negative || number == 0).then_some(number)
}

/// Returns whether `c` is white space or a line terminator of ECMA-262, which
/// `parseInt` skips at the start of its string.
fn is_js_white_space(c: char) -> bool {
    const ONE: [char; 14] = [
        '\t', '\n', '\u{b}', '\u{c}', '\r', ' ', '\u{a0}', '\u{1680}', '\u{2028}', '\u{2029}',
        '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
    ];
    ONE.contains(&c) || ('\u{2000}'..='\u{200a}').contains(&c)
}
