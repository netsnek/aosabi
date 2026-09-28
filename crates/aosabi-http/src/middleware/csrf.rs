// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the csrf middleware of hono, which refuses an unsafe request of
 * a form that another site sends, and the default the base pipeline runs over
 * the same check.
 *
 * Derived from hono 4.11.9, dist/middleware/csrf/index.js, MIT.
 */

//! `csrf`, the check of the site a form request comes from.
//!
//! The middleware refuses a request whose method is neither `GET` nor `HEAD`,
//! whose `Content-Type` names a type a form element sends, whose
//! `Sec-Fetch-Site` the option `sec_fetch_site` does not allow and whose
//! `Origin` the option `origin` does not allow. It throws an `HTTPException`
//! of the status 403 over the response `Forbidden`, which the error handler
//! answers, and it checks the origin only where the `Sec-Fetch-Site` allows
//! nothing.
//!
//! The type is the field `Content-Type`, or `text/plain` where it is missing
//! or empty, and a form type is `application/x-www-form-urlencoded`,
//! `multipart/form-data` or `text/plain` at its start in any case of ASCII,
//! followed by no letter, digit or `_`. A missing `Sec-Fetch-Site`, or one
//! that is none of `same-origin`, `same-site`, `none` and `cross-site`, allows
//! nothing, and the default allows `same-origin` alone. A missing `Origin`
//! allows nothing, and the default allows the origin of the request's own URL.
//! An option that is an empty string reads as the default, since the reference
//! reads it as false.
//!
//! The base pipeline of a Rust service runs the check with the defaults on a
//! request that carries a `Cookie` field, where a cookie can carry a session,
//! through [`behind_default_csrf`] in the entry of the plugin middleware and
//! ahead of it, divergence D70 of `docs/byte-equality.md`. A `csrf` the app
//! registers takes the place of the default on every request it matches,
//! wherever it stands in the chain: the dispatch of the app marks such a
//! request, since [`MiddlewareFn::is_csrf`] tells the middleware apart, and
//! the default lets a marked request through to the app's own check. The
//! TypeScript option keeps the pipeline of a TypeScript app, which runs no csrf, so under the
//! feature `host-platform` there is no default.

use core::future::Future;
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
            Response,
            ResponseInit, //
        },
        url::origin_of,
    },
    http_exception::HttpException,
    web_app::{
        MiddlewareFn,
        RouteError, //
    },
};

/// The values of `Sec-Fetch-Site` the middleware knows.
const SEC_FETCH_SITE_VALUES: [&str; 4] = ["same-origin", "same-site", "none", "cross-site"];

/// The types a form element sends.
const FORM_TYPES: [&str; 3] = [
    "application/x-www-form-urlencoded",
    "multipart/form-data",
    "text/plain",
];

/// A function of the service over a value of a request and its context, which
/// answers whether it allows the value.
pub type AllowFn<E> = Rc<dyn Fn(String, Context) -> LocalBoxFuture<'static, Result<bool, E>>>;

/// The option `origin`, the origins the middleware allows.
pub enum CsrfOrigin<E> {
    /// A string, which allows the one origin equal to it.
    Exact(String),
    /// An array of origins, which allows an origin that stands in it.
    List(Vec<String>),
    /// A function of the service, which answers whether it allows the origin.
    Allow(AllowFn<E>),
}

/// The option `secFetchSite`, the values of `Sec-Fetch-Site` the middleware allows.
pub enum CsrfSecFetchSite<E> {
    /// A string, which allows the one value equal to it.
    Exact(String),
    /// An array of values, which allows a value that stands in it.
    List(Vec<String>),
    /// A function of the service, which answers whether it allows the value.
    Allow(AllowFn<E>),
}

/// The options of `csrf(options)`, each as the middleware of hono reads it.
pub struct CsrfOptions<E> {
    /// The origins the middleware allows, `None` for the origin of the URL of
    /// the request.
    pub origin: Option<CsrfOrigin<E>>,
    /// The values of `Sec-Fetch-Site` the middleware allows, `None` for
    /// `same-origin` alone.
    pub sec_fetch_site: Option<CsrfSecFetchSite<E>>,
}

impl<E> Default for CsrfOptions<E> {
    fn default() -> Self {
        Self {
            origin: None,
            sec_fetch_site: None,
        }
    }
}

/// The mark of a request that a `csrf` of the app matches, which the dispatch
/// of the app sets and on which the default steps aside.
pub(crate) struct AppCsrf;

/// The middleware `csrf(options)`, which [`MiddlewareFn::is_csrf`] tells apart.
struct Csrf<E> {
    /// The options.
    options: Rc<CsrfOptions<E>>,
}

impl<E: RouteError + From<HttpException>> MiddlewareFn<E> for Csrf<E> {
    fn call(&self, c: Context, next: Next<E>) -> LocalBoxFuture<'static, Result<Response, E>> {
        let options = Rc::clone(&self.options);
        Box::pin(async move {
            refuse_forged(&options, &c).await?;
            next.run(c).await
        })
    }

    fn is_csrf(&self) -> bool {
        true
    }
}

/// Returns the middleware `csrf(options)`.
#[must_use]
pub fn csrf<E: RouteError + From<HttpException>>(options: CsrfOptions<E>) -> impl MiddlewareFn<E> {
    Csrf {
        options: Rc::new(options),
    }
}

/// Returns the future of `rest` behind the default csrf of the base pipeline,
/// the check of `csrf()` on a request of `c` that carries a `Cookie` field and
/// that no `csrf` of the app matches, divergence D70.
///
/// The check runs when the future is first polled, and `rest` is called with
/// the context once the check lets the request through, so nothing of `rest`
/// runs for a request the check refuses.
#[cfg(not(feature = "host-platform"))]
#[must_use]
pub fn behind_default_csrf<T, E, R, F>(
    c: &Context,
    rest: R,
) -> LocalBoxFuture<'static, Result<T, E>>
where
    T: 'static,
    E: RouteError + From<HttpException>,
    R: FnOnce(Context) -> F + 'static,
    F: Future<Output = Result<T, E>> + 'static,
{
    let c = c.clone();
    Box::pin(async move {
        if c.req().header("cookie").is_some() && c.extension::<AppCsrf>().is_none() {
            refuse_forged(&CsrfOptions::<E>::default(), &c).await?;
        }
        rest(c).await
    })
}

/// Returns the future of `rest` over `c` with no check, since under the
/// feature `host-platform` the TypeScript option runs no
/// csrf.
#[cfg(feature = "host-platform")]
pub fn behind_default_csrf<T, E, R, F>(c: &Context, rest: R) -> F
where
    R: FnOnce(Context) -> F,
    F: Future<Output = Result<T, E>>,
{
    rest(c.clone())
}

/// Throws the refusal of `c` where it is an unsafe request of a form whose
/// `Sec-Fetch-Site` and `Origin` `options` both do not allow.
///
/// # Errors
///
/// Returns the `HTTPException` of the status 403, the error the function of
/// an option threw, and the `TypeError` of a URL the parser refuses.
async fn refuse_forged<E: RouteError + From<HttpException>>(
    options: &CsrfOptions<E>,
    c: &Context,
) -> Result<(), E> {
    let req = c.req();
    let method = req.method();
    if method == "GET" || method == "HEAD" {
        return Ok(());
    }
    let content_type = req.header("content-type").filter(|value| !value.is_empty());
    if !is_form_type(content_type.as_deref().unwrap_or("text/plain")) {
        return Ok(());
    }
    let site = req.header("sec-fetch-site");
    if allows_sec_fetch_site(options.sec_fetch_site.as_ref(), site, c).await? {
        return Ok(());
    }
    if allows_origin(options.origin.as_ref(), req.header("origin"), c).await? {
        return Ok(());
    }
    let init = ResponseInit {
        status: Some(403),
        status_text: None,
        headers: None,
    };
    let forbidden = Response::new(Body::Text(Rc::from("Forbidden")), Some(init))
        .map_err(|error| E::refusal(&error))?;
    Err(E::from(HttpException::with_response(403, forbidden)))
}

/// Returns whether `content_type` starts with a type a form element sends.
///
/// The expression has the flag `i` and no flag `u`, so it folds the case of
/// ASCII alone, and its `\b` after the type asks for a character outside `\w`,
/// the letters and digits of ASCII and `_`, or for the end of the value.
fn is_form_type(content_type: &str) -> bool {
    FORM_TYPES.iter().any(|form| {
        let starts = content_type
            .get(..form.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(form));
        let joined = content_type
            .as_bytes()
            .get(form.len())
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        starts && !joined
    })
}

/// Returns whether `option` allows the `Sec-Fetch-Site` `value` of `c`.
async fn allows_sec_fetch_site<E: RouteError>(
    option: Option<&CsrfSecFetchSite<E>>,
    value: Option<String>,
    c: &Context,
) -> Result<bool, E> {
    let Some(value) = value else {
        return Ok(false);
    };
    if !SEC_FETCH_SITE_VALUES.contains(&value.as_str()) {
        return Ok(false);
    }
    Ok(match option {
        Some(CsrfSecFetchSite::Exact(exact)) if !exact.is_empty() => value == *exact,
        Some(CsrfSecFetchSite::List(list)) => list.contains(&value),
        Some(CsrfSecFetchSite::Allow(allow)) => allow(value, c.clone()).await?,
        _ => value == "same-origin",
    })
}

/// Returns whether `option` allows the `Origin` `value` of `c`.
async fn allows_origin<E: RouteError>(
    option: Option<&CsrfOrigin<E>>,
    value: Option<String>,
    c: &Context,
) -> Result<bool, E> {
    let Some(value) = value else {
        return Ok(false);
    };
    Ok(match option {
        Some(CsrfOrigin::Exact(exact)) if !exact.is_empty() => value == *exact,
        Some(CsrfOrigin::List(list)) => list.contains(&value),
        Some(CsrfOrigin::Allow(allow)) => allow(value, c.clone()).await?,
        _ => value == origin_of(&c.req().url()).map_err(|_| E::type_error("Invalid URL"))?,
    })
}
