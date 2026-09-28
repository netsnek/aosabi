// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the Response of the Fetch API that Yoga and the server adapter
 * make.
 *
 * Derived from whatwg-node 0.8.5, cjs/Response.js, MIT.
 * Derived from whatwg-node 0.8.5, cjs/Body.js, MIT.
 */

//! `new_response`, the `new fetchAPI.Response(body, init)` of Yoga.
//!
//! On Node the Response Yoga makes is the ponyfill of `@whatwg-node/node-fetch`,
//! and elsewhere it is the platform's own. The two differ in the headers a string
//! body adds.
//!
//! - The ponyfill gives a string body `content-type: text/plain;charset=UTF-8` when
//!   the init names no type and `content-length` with its byte length when the init
//!   names no length and the length is not 0.
//! - The platform gives the same type and no length header.
//!
//! The function writes the headers of the ponyfill on every platform, the named
//! divergence D57 of `docs/byte-equality.md` section 3. So an answer whose body is
//! a string carries its `content-length` on a Worker and on Bun as it does on Node,
//! a compression applies its threshold of 1,024 bytes by that length, and an answer
//! below the threshold goes out uncoded, where a TypeScript app on a Worker and on Bun codes
//! every answer a client accepts coded, since the threshold of `compress()` reads
//! that header alone.
//!
//! The body of either is a stream, which is what `response.body` answers, so a
//! string body is a stream of one chunk here, and the app hands that stream to hono
//! as the body of its own answer. A string body comes as its UTF-8 bytes, which is
//! what the extraction encodes it to, and those bytes are the one chunk, so the
//! JSON of an answer reaches the connection as the buffer its writer filled, with
//! no check of its text and no copy. The ponyfill's status text defaults to the
//! reason of Node's table, which no reader of Yoga's answer reads, because the app
//! and hono read its status and its headers alone, so only a status text Yoga names
//! is kept.

use std::borrow::Cow;

use aosabi_execute::{
    ErrorObject,
    Value, //
};
use aosabi_http::{
    Body,
    BodyStream,
    Headers,
    HeadersInit,
    Response,
    ResponseInit,
    fetch::body::Chunks, //
};

use crate::utils::header::has;

/// The `content-type` a string body takes when the init names none.
pub const TEXT_PLAIN: &str = "text/plain;charset=UTF-8";

/// The entries of a plain object of headers in the order of its keys, each a static string or one made for the answer.
pub type HeaderEntries = Vec<(Cow<'static, str>, Cow<'static, str>)>;

/// The headers of an init that names none, `{}`.
pub const NO_HEADERS: &[(&str, &str)] = &[];

/// The body of a response Yoga makes.
pub enum ResponseBody {
    /// `null`.
    Null,
    /// A string, as its UTF-8 bytes.
    Text(Vec<u8>),
    /// A readable stream.
    Stream(BodyStream),
}

/// Returns `new fetchAPI.Response(body, { status, statusText, headers })`.
///
/// `headers` are the entries of the plain object of the init in the order of
/// its keys.
///
/// # Errors
///
/// Returns the `TypeError` of a header the headers refuse, and on the
/// platform the error of a status its constructor refuses.
pub fn new_response<N: AsRef<str>, V: AsRef<str>>(
    body: ResponseBody,
    status: u16,
    status_text: Option<&str>,
    headers: &[(N, V)],
) -> Result<Response, Value> {
    let list = Headers::from_entries(headers)
        .map_err(|error| Value::from(ErrorObject::type_error(error.message)))?;
    response_of(body, status, status_text, list)
}

/// Returns the response of `body` over the header list `list`, the part of [`new_response`] past its headers.
///
/// The part takes no type of the headers, so the module holds it once for
/// every type of the entries [`new_response`] is called with.
fn response_of(
    body: ResponseBody,
    status: u16,
    status_text: Option<&str>,
    mut list: Headers,
) -> Result<Response, Value> {
    let body = match body {
        ResponseBody::Null => Body::Empty,
        ResponseBody::Text(text) => {
            if !has(&list, "content-type") {
                list.set("content-type", TEXT_PLAIN)
                    .map_err(|error| Value::from(ErrorObject::type_error(error.message)))?;
            }
            if !text.is_empty() && !has(&list, "content-length") {
                let mut digits = [0; 20];
                list.set("content-length", decimal(text.len(), &mut digits))
                    .map_err(|error| Value::from(ErrorObject::type_error(error.message)))?;
            }
            Body::Stream(BodyStream::new(Chunks::new([Ok(text)])))
        }
        ResponseBody::Stream(stream) => Body::Stream(stream),
    };
    let init = ResponseInit {
        status: Some(status),
        status_text: status_text.map(String::from),
        headers: Some(HeadersInit::Fetch(list)),
    };
    let response = Response::new(body, Some(init)).map_err(|error| {
        let thrown = match error.kind {
            aosabi_http::fetch::response::FetchErrorKind::Type => {
                ErrorObject::type_error(error.message)
            }
            aosabi_http::fetch::response::FetchErrorKind::Range => {
                ErrorObject::range_error(error.message)
            }
        };
        Value::from(thrown)
    })?;
    // The ponyfill and the platform's Response keep no cache of the adapter of
    // @hono/node-server, whose `body` would build the answer again from the init,
    // so a header a plugin sets on the answer, the headers of CORS among them,
    // reaches the answer the app hands hono. Reading the body drops that cache.
    let _ = response.body();
    Ok(response)
}

/// Returns the response of `status` with no body, `new fetchAPI.Response(null, { status, statusText, headers })`.
///
/// # Errors
///
/// Returns what [`new_response`] returns.
pub fn empty_response<N: AsRef<str>, V: AsRef<str>>(
    status: u16,
    status_text: Option<&str>,
    headers: &[(N, V)],
) -> Result<Response, Value> {
    new_response(ResponseBody::Null, status, status_text, headers)
}

/// Returns the entries of a plain object of headers from static string pairs.
#[must_use]
pub fn header_entries(entries: &[(&'static str, &'static str)]) -> HeaderEntries {
    entries
        .iter()
        .map(|&(name, value)| (Cow::Borrowed(name), Cow::Borrowed(value)))
        .collect()
}

/// Returns the decimal digits of `value`, written into the end of `digits`, `String(length)`.
// A `str::from_utf8_unchecked` would skip this check of the digits the loop wrote.
fn decimal(value: usize, digits: &mut [u8; 20]) -> &str {
    let mut rest = value;
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b"0123456789"[rest % 10];
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    // Every byte from `start` on is an ASCII digit, so the default is never
    // taken.
    core::str::from_utf8(&digits[start..]).unwrap_or_default()
}
