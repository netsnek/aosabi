// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the cors middleware of hono, which answers a preflight and
 * marks every other answer of its path with the fields of cross origin
 * access.
 *
 * Derived from hono 4.11.9, dist/middleware/cors/index.js, MIT.
 */

//! `cors`, the fields of cross origin access.
//!
//! The middleware reads the field `Origin` of the request, `""` where it is
//! missing, and finds the origin it allows: `*` for the origin `*`, the origin
//! of the request where it equals the one origin or stands in the list of
//! origins, and otherwise what the function of the app answers for it. Before
//! the rest of the chain runs it sets `Access-Control-Allow-Origin` to that
//! origin where it is no empty string, `Access-Control-Allow-Credentials` to
//! `true` under the option `credentials` and `Access-Control-Expose-Headers`
//! to the exposed fields where there are any, on the response `c.res` holds. A
//! request of the method `OPTIONS` is a preflight, which the middleware
//! answers itself with the status 204 and the fields of that response,
//! `Vary: Origin` set where the option `origin` is no `*`,
//! `Access-Control-Max-Age`, the allowed methods and the allowed fields, the
//! fields the request asks for in `Access-Control-Request-Headers` where the
//! option names none, and then `Vary: Access-Control-Request-Headers`
//! appended, with no `Content-Length` and no `Content-Type`. Every other
//! request runs the rest of the chain, and where the option `origin` is no `*`
//! the middleware appends `Vary: Origin` to the answer.
//!
//! The function an origin or the allowed methods take is a function of the
//! service, which a host may answer through a call of its own, so it takes the
//! origin of the request and the context and answers a future. The defaults
//! are those of hono, the origin `*` and the methods `GET`, `HEAD`, `PUT`,
//! `POST`, `DELETE` and `PATCH`. The middleware of hono 4.12.25 differs from
//! that of 4.11.9 in the spread of its defaults alone, which changes no
//! answer.

use std::rc::Rc;

use crate::{
    compose::Next,
    context::Context,
    fetch::{
        body::{
            Body,
            LocalBoxFuture, //
        },
        response::{
            FetchError,
            HeadersInit,
            Response,
            ResponseInit, //
        },
    },
    web_app::{
        MiddlewareFn,
        RouteError, //
    },
};

/// The methods a preflight allows by default.
pub const DEFAULT_METHODS: [&str; 6] = ["GET", "HEAD", "PUT", "POST", "DELETE", "PATCH"];

/// A function of the service over the origin of a request and its context,
/// which answers a future of `T`.
pub type FindFn<T, E> = Rc<dyn Fn(String, Context) -> LocalBoxFuture<'static, Result<T, E>>>;

/// The option `origin`, the origins the middleware allows.
pub enum CorsOrigin<E> {
    /// The string `*`, which allows every origin and answers `*`.
    Any,
    /// Any other string, which allows the one origin equal to it.
    Exact(String),
    /// An array of origins, which allows an origin that stands in it.
    List(Vec<String>),
    /// A function of the service, which answers the allowed origin of the
    /// origin of a request, or `None` for none.
    Find(FindFn<Option<String>, E>),
}

/// The option `allowMethods`, the methods a preflight allows.
pub enum AllowMethods<E> {
    /// An array of methods, each as `join` writes it, or an empty array for an
    /// option that is neither an array nor a function.
    Fixed(Vec<String>),
    /// A function of the service, which answers the methods for the origin of
    /// a request.
    Find(FindFn<Vec<String>, E>),
}

/// The options of `cors(options)`, each as the middleware of hono reads it.
pub struct CorsOptions<E> {
    /// The origins the middleware allows.
    pub origin: CorsOrigin<E>,
    /// The methods a preflight allows.
    pub allow_methods: AllowMethods<E>,
    /// The fields a preflight allows, joined with `,`, or `None` for an empty
    /// array, which takes the fields the preflight asks for.
    pub allow_headers: Option<String>,
    /// The fields an answer exposes, joined with `,`, or `None` for an empty
    /// array.
    pub expose_headers: Option<String>,
    /// The string `maxAge.toString()` answers, or `None` for a `maxAge` that
    /// is `null` or `undefined`.
    pub max_age: Option<String>,
    /// Whether the option `credentials` is true.
    pub credentials: bool,
}

impl<E> Default for CorsOptions<E> {
    fn default() -> Self {
        Self {
            origin: CorsOrigin::Any,
            allow_methods: AllowMethods::Fixed(
                DEFAULT_METHODS
                    .iter()
                    .map(|method| (*method).to_owned())
                    .collect(),
            ),
            allow_headers: None,
            expose_headers: None,
            max_age: None,
            credentials: false,
        }
    }
}

/// Returns the middleware `cors(options)`.
#[must_use]
pub fn cors<E: RouteError>(options: CorsOptions<E>) -> impl MiddlewareFn<E> {
    let options = Rc::new(options);
    move |c: Context, next: Next<E>| -> LocalBoxFuture<'static, Result<Response, E>> {
        let options = Rc::clone(&options);
        Box::pin(async move {
            let requested = c.req().header("origin").unwrap_or_default();
            if let Some(allowed) = allow_origin(&options.origin, &requested, &c).await? {
                set(&c, "Access-Control-Allow-Origin", &allowed)?;
            }
            if options.credentials {
                set(&c, "Access-Control-Allow-Credentials", "true")?;
            }
            if let Some(exposed) = &options.expose_headers {
                set(&c, "Access-Control-Expose-Headers", exposed)?;
            }
            if c.req().method() == "OPTIONS" {
                // The preflight runs as a future of its own behind a box, so
                // a request that is no preflight compiles none of it.
                let preflight: LocalBoxFuture<'_, Result<Response, E>> =
                    Box::pin(preflight(&options, requested, &c));
                return preflight.await;
            }
            next.run(c.clone()).await?;
            if !matches!(options.origin, CorsOrigin::Any) {
                c.header_append("Vary", "Origin");
            }
            Ok(c.res())
        })
    }
}

/// Answers the preflight of `c`, whose origin is `requested`, with the
/// fields that `options` give.
async fn preflight<E: RouteError>(
    options: &CorsOptions<E>,
    requested: String,
    c: &Context,
) -> Result<Response, E> {
    if !matches!(options.origin, CorsOrigin::Any) {
        set(c, "Vary", "Origin")?;
    }
    if let Some(max_age) = &options.max_age {
        set(c, "Access-Control-Max-Age", max_age)?;
    }
    let methods = match &options.allow_methods {
        AllowMethods::Fixed(methods) => methods.clone(),
        AllowMethods::Find(find) => find(requested, c.clone()).await?,
    };
    if !methods.is_empty() {
        set(c, "Access-Control-Allow-Methods", &methods.join(","))?;
    }
    let headers = match &options.allow_headers {
        Some(headers) => Some(headers.clone()),
        None => c
            .req()
            .header("Access-Control-Request-Headers")
            .filter(|asked| !asked.is_empty())
            .map(|asked| split_joined(&asked)),
    };
    if let Some(headers) = headers {
        set(c, "Access-Control-Allow-Headers", &headers)?;
        c.res()
            .append_header("Vary", "Access-Control-Request-Headers")
            .map_err(|error| E::refusal(&error))?;
    }
    let res = c.res();
    res.delete_header("Content-Length");
    res.delete_header("Content-Type");
    Response::new(
        Body::Empty,
        Some(ResponseInit {
            status: Some(204),
            status_text: Some(String::from("No Content")),
            headers: Some(HeadersInit::Fetch(res.headers())),
        }),
    )
    .map_err(|error: FetchError| E::refusal(&error))
}

/// Answers the origin `options` allow for the origin `requested` of `c`, or
/// `None` where it allows none or the origin it answers is an empty string,
/// which the reference reads as false.
async fn allow_origin<E: RouteError>(
    origin: &CorsOrigin<E>,
    requested: &str,
    c: &Context,
) -> Result<Option<String>, E> {
    let found = match origin {
        CorsOrigin::Any => Some(String::from("*")),
        CorsOrigin::Exact(exact) => (exact == requested).then(|| requested.to_owned()),
        CorsOrigin::List(list) => list
            .iter()
            .any(|item| item == requested)
            .then(|| requested.to_owned()),
        CorsOrigin::Find(find) => find(requested.to_owned(), c.clone()).await?,
    };
    Ok(found.filter(|allowed| !allowed.is_empty()))
}

/// Sets the field `name` to `value` on the response `c.res` holds,
/// `c.res.headers.set(name, value)`.
///
/// # Errors
///
/// Returns the `TypeError` of a name or a value the platform refuses.
fn set<E: RouteError>(c: &Context, name: &str, value: &str) -> Result<(), E> {
    c.res()
        .set_header(name, value)
        .map_err(|error| E::refusal(&error))
}

/// Returns the fields of `Access-Control-Request-Headers` as
/// `asked.split(/\s*,\s*/).join(",")` gives them: each comma with the white
/// space right before and after it becomes one comma, and every other
/// character stays.
fn split_joined(asked: &str) -> String {
    let mut out = String::with_capacity(asked.len());
    let mut after_comma = false;
    for ch in asked.chars() {
        if ch == ',' {
            out.truncate(out.trim_end_matches(is_js_space).len());
            out.push(',');
            after_comma = true;
        } else if !(after_comma && is_js_space(ch)) {
            out.push(ch);
            after_comma = false;
        }
    }
    out
}

/// Returns whether `ch` is white space or a line terminator of ECMA-262, the
/// class `\s` of a regular expression.
const fn is_js_space(ch: char) -> bool {
    matches!(ch, '\t'..='\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}')
        || matches!(
            ch,
            '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
}
