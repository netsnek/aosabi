// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the request of the HTTP executor as node:http writes it,
 * and the redirects the fetch of the executor follows.
 *
 * Derived from whatwg-node 0.8.5, cjs/fetchNodeHttp.js, MIT.
 */

//! The request the executor hands the network capability, and the
//! redirects its fetch follows.
//!
//! The fetch of the reference is `@whatwg-node/node-fetch` 0.8.5 under Node,
//! which hands node:http the plain object of the executor's headers. node:http
//! sets each key of the object in its order through `setHeader`, which refuses
//! a name that is no token and a value with a character outside tab, `0x20` to
//! `0x7e` and `0x80` to `0xff`, and keeps one line per name read in any case,
//! at the place of the first spelling with the name and the value of the last,
//! the value written as it stands. [`header_lines`] answers those lines or the
//! `TypeError` node:http throws, which the executor answers as the reference
//! answers a throw of its fetch. The lines travel to the native driver as the
//! field lines of the request, and its `Headers` hold the same fields in the
//! form the Fetch standard normalizes, each name in lower case and each value
//! with its outer whitespace trimmed, for every other reader, the Worker's
//! platform fetch among them.
//!
//! The fetch follows a redirect as `fetchNodeHttp.js:84-100` follows it: an
//! answer whose status is 301, 302, 303, 307 or 308 and whose `location` is
//! not empty sends the method, the headers and the body again to the
//! `location` resolved against the URL of the request, a 303 among them, and
//! every header travels to every hop, an origin the request changes to as
//! well. A `location` that resolves to no URL answers the failed fetch, and
//! one whose scheme is neither `http` nor `https` the plain `Error`
//! `Unsupported protocol: <scheme>` that `getRequestFnForProtocol` throws. A
//! chain of redirects ends with the bound of the call it began as,
//! [`CHAIN_MS`] from the first request on the clock of the runtime, so a loop,
//! which the reference follows with no end, ends with the timeout of the
//! capability.

use core::{
    future::{
        Future,
        poll_fn, //
    },
    pin::Pin,
    task::Poll, //
};
use std::rc::Rc;

use aosabi_core::{
    Thrown,
    clock,
    net::{
        self,
        FetchError, //
    }, //
};
use aosabi_execute::value::{
    ErrorObject,
    Value, //
};
use aosabi_http::{
    Body,
    Request,
    Response,
    parse_url, //
};

use crate::{
    executor_http::{
        errors::fetch_rejection,
        form_data::{
            FormData,
            RequestBody,
            form_data_boundary, //
        }, //
    },
    headers::Headers,
};

/// The statuses whose `location` the fetch follows, `shouldRedirect`.
pub const REDIRECT_STATUSES: [u16; 5] = [301, 302, 303, 307, 308];

/// The bound of a chain of redirects in milliseconds, the bound of the call
/// it began as, `TIMEOUT_MS` of the native driver.
pub const CHAIN_MS: u64 = 30_000;

/// The request the executor sends to every hop: its method, its lines and its body.
pub struct Outbound {
    /// The method.
    method: &'static str,
    /// The field lines as node:http writes them.
    lines: Rc<[(String, String)]>,
    /// The body.
    body: Body,
}

impl Outbound {
    /// Returns the request of `method` with the headers of the executor and the body.
    ///
    /// A form takes the `content-type` of its boundary at the end of the lines
    /// when no line names `content-type` in any case, as the body of the
    /// reference adds it to a request with no such header.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` node:http throws for a header it refuses, and
    /// the plain `Error` of a boundary with no source of randomness.
    pub fn new(method: &'static str, body: &RequestBody, headers: &Headers) -> Result<Self, Value> {
        let mut lines = header_lines(headers)?;
        let body = match body {
            RequestBody::Json(text) => Body::Text(Rc::from(text.as_str())),
            RequestBody::FormData(form) => {
                let boundary = form_data_boundary()
                    .map_err(|error| Value::from(ErrorObject::error(error.to_string())))?;
                if !lines
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                {
                    lines.push((
                        String::from("content-type"),
                        FormData::content_type(&boundary),
                    ));
                }
                Body::Full(Rc::from(form.to_body(&boundary)))
            }
        };
        Ok(Self {
            method,
            lines: Rc::from(lines),
            body,
        })
    }

    /// Returns the Fetch request of this request to `url`.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` the Fetch `Headers` throws for a line, which
    /// no line that passed [`header_lines`] meets.
    fn to(&self, url: &str) -> Result<Request, Value> {
        let mut headers = aosabi_http::Headers::new();
        for (name, value) in self.lines.iter() {
            headers
                .append(name, value)
                .map_err(|error| Value::from(ErrorObject::type_error(error.to_string())))?;
        }
        Ok(Request::new(self.method, url, headers, self.body.clone())
            .with_wire_fields(Rc::clone(&self.lines)))
    }

    /// Sends the request to `endpoint` and follows its redirects, the fetch of the reference.
    ///
    /// # Errors
    ///
    /// Returns the value the fetch of the reference rejects with: the error
    /// of the capability through [`fetch_rejection`], the failed fetch for a
    /// `location` that is no URL and for a chain past [`CHAIN_MS`], and the
    /// plain `Error` of a scheme node:http does not speak.
    ///
    /// The first request is bounded by the capability alone, so a request
    /// that meets no redirect reads the clock once and sets no timer of its
    /// own, and each hop after a redirect is bounded by what is left of
    /// [`CHAIN_MS`] since the first request began.
    pub async fn send(&self, endpoint: &str) -> Result<Response, Value> {
        let started = clock::now_ms();
        let mut url = String::from(endpoint);
        let mut fetched = net::fetch(self.to(&url)?)
            .await
            .map_err(|error| fetch_rejection(&error))?;
        while let Some(location) = redirect_location(&fetched) {
            url = resolve_location(&location, &url)?;
            let deadline = started.map(|started| started.saturating_add(CHAIN_MS));
            fetched = bounded(net::fetch(self.to(&url)?), deadline)
                .await
                .map_err(|error| fetch_rejection(&error))?;
        }
        Ok(fetched)
    }
}

/// Returns `fetch` bounded by `deadline` on the clock of the runtime, which
/// ends it with `Timeout`, or `fetch` as it is with no clock.
async fn bounded<F>(fetch: F, deadline: Option<u64>) -> Result<Response, FetchError>
where
    F: Future<Output = Result<Response, FetchError>>,
{
    let Some(deadline) = deadline else {
        return fetch.await;
    };
    let now = clock::now_ms().unwrap_or(deadline);
    if now >= deadline {
        return Err(FetchError::Timeout);
    }
    let mut fetch = core::pin::pin!(fetch);
    let mut timer = clock::delay(deadline - now);
    poll_fn(|cx| {
        if let Poll::Ready(result) = fetch.as_mut().poll(cx) {
            return Poll::Ready(result);
        }
        match Pin::new(&mut timer).poll(cx) {
            Poll::Ready(()) => Poll::Ready(Err(FetchError::Timeout)),
            Poll::Pending => Poll::Pending,
        }
    })
    .await
}

/// Returns the `location` of an answer the fetch follows, or none.
///
/// The answer is followed when its status is one of [`REDIRECT_STATUSES`]
/// and its first `location` line is not empty, `nodeResponse.headers.location
/// && shouldRedirect(statusCode)`, node:http keeping the first of two
/// `location` lines.
fn redirect_location(response: &Response) -> Option<String> {
    if !REDIRECT_STATUSES.contains(&response.status()) {
        return None;
    }
    let headers = response.headers();
    headers
        .list()
        .iter()
        .find(|(name, _)| name == "location")
        .map(|(_, value)| value.clone())
        .filter(|location| !location.is_empty())
}

/// Returns the URL `location` names against `base`, `new URL(location, base)`.
///
/// # Errors
///
/// Returns the failed fetch for a `location` that resolves to no URL, and
/// the plain `Error` `Unsupported protocol: <scheme>` for a URL whose scheme
/// is neither `http` nor `https`.
fn resolve_location(location: &str, base: &str) -> Result<String, Value> {
    let url = parse_url(location, Some(base))
        .map_err(|_| Thrown::from(FetchError::InvalidUrl(String::from(location))).0)?;
    if matches!(url.protocol.as_str(), "http:" | "https:") {
        return Ok(url.href);
    }
    let scheme = url.protocol.strip_suffix(':').unwrap_or(&url.protocol);
    Err(Value::from(ErrorObject::error(format!(
        "Unsupported protocol: {scheme}"
    ))))
}

/// Returns the field lines node:http writes for the object `headers`.
///
/// Each key of the object is set in the order of the keys of an object,
/// which [`Headers::iter`] gives, the way `ClientRequest` hands each key to
/// `setHeader`: the name is checked as a token and the value for a character
/// node:http refuses, and a name that a line holds already in any case takes
/// that line's place with its own spelling and value.
///
/// # Errors
///
/// Returns the `TypeError` `Header name must be a valid HTTP token
/// ["<name>"]` or `Invalid character in header content ["<name>"]` of the
/// first key node:http refuses.
pub fn header_lines(headers: &Headers) -> Result<Vec<(String, String)>, Value> {
    let mut lines: Vec<(String, String)> = Vec::with_capacity(headers.len());
    for (name, value) in headers.iter() {
        if name.is_empty() || !name.bytes().all(is_token_byte) {
            return Err(Value::from(ErrorObject::type_error(format!(
                "Header name must be a valid HTTP token [\"{name}\"]"
            ))));
        }
        if value.chars().any(is_refused_char) {
            return Err(Value::from(ErrorObject::type_error(format!(
                "Invalid character in header content [\"{name}\"]"
            ))));
        }
        let line = (String::from(name), String::from(value));
        match lines
            .iter_mut()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
        {
            Some(known) => *known = line,
            None => lines.push(line),
        }
    }
    Ok(lines)
}

/// Returns whether `byte` is a character of the token node:http checks a
/// header name with in `checkIsHttpToken`: a letter, a digit, one of
/// `!#$%&'*+-.^_|~` or the grave accent.
fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"^_`-!#$%&'*+.|~".contains(&byte)
}

/// Returns whether node:http refuses `c` in a header value,
/// `/[^\t\x20-\x7e\x80-\xff]/`.
fn is_refused_char(c: char) -> bool {
    !matches!(c, '\t' | '\u{20}'..='\u{7e}' | '\u{80}'..='\u{ff}')
}
