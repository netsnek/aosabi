// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the Fetch request built from the neutral request of aosabi-http.
 *
 * Derived from hono 1.19.9, dist/request.js, MIT.
 * Derived from hono 1.19.9, dist/server.js, MIT.
 *
 * It carries the quirks Q54_NODE_ADAPTER_URL, Q55_NODE_ADAPTER_HEADERS and
 * Q56_NODE_ADAPTER_BODY_METHODS of docs/byte-equality.md section 13.1.
 */

//! The request of the Node adapter.
//!
//! The adapter builds the Fetch request of a Node request: its URL from the
//! target, the host and the scheme, its `Headers` from the fields in arrival
//! order and its body from the stream of the request, as `newRequest` of
//! `@hono/node-server` 1.19.9 does. A request it cannot build is refused before
//! the application runs, and the listener answers it with a 400 and a null
//! body.
//!
//! The bytes of the head are read as Latin-1, one byte one character, as Node
//! reads them, so a name, a value and the target keep every byte they carry.

use core::{
    fmt,
    pin::Pin,
    task::{
        Context,
        Poll, //
    }, //
};
use std::rc::Rc;

use aosabi_http::{
    fetch::{
        body::{
            Body,
            BodyError,
            BodyStream,
            Chunk,
            Stream, //
        },
        headers::Headers,
        request::Request,
        url::parse_url, //
    },
    message::{
        BodyError as IncomingError,
        IncomingBody,
        NodeIncoming, //
    }, //
};

/// Why the adapter refused to build a request, the `RequestError` of the reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestError {
    /// The message of the error.
    pub message: String,
}

impl RequestError {
    /// Creates the error with `message`.
    fn new(message: &str) -> Self {
        Self {
            message: String::from(message),
        }
    }
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for RequestError {}

/// Returns `bytes` read as Latin-1, one byte one character.
#[must_use]
pub fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| char::from(byte)).collect()
}

/// Returns the URL of `incoming`, the part of `newRequest` that fills `req[urlKey]`.
///
/// A target that starts with `http://` or `https://`, compared byte for byte,
/// is refused on HTTP/2 and otherwise parsed as a whole URL. Any other target
/// is joined to the scheme and the host, which is refused when it is missing,
/// the scheme of HTTP/2 when it is neither `http` nor `https`, and the host
/// when the parsed hostname differs from it in length and from it with a
/// trailing `:` and digits removed.
///
/// # Errors
///
/// Returns [`RequestError`] with the message of the reference, or `Invalid
/// URL` for a URL the parser refuses, which the listener answers with 400.
pub fn request_url(incoming: &NodeIncoming) -> Result<String, RequestError> {
    let target = latin1(&incoming.raw_target);
    if !target.starts_with('/') && (target.starts_with("http://") || target.starts_with("https://"))
    {
        if incoming.http2 {
            return Err(RequestError::new(
                "Absolute URL for :path is not allowed in HTTP/2",
            ));
        }
        let url =
            parse_url(&target, None).map_err(|_| RequestError::new("Invalid absolute URL"))?;
        return Ok(url.href);
    }
    let host = match &incoming.host {
        Some(host) if !host.is_empty() => latin1(host),
        _ => return Err(RequestError::new("Missing host header")),
    };
    let scheme = latin1(&incoming.scheme);
    if incoming.http2 && scheme != "http" && scheme != "https" {
        return Err(RequestError::new("Unsupported scheme"));
    }
    let url = parse_url(&format!("{scheme}://{host}{target}"), None)
        .map_err(|_| RequestError::new("Invalid URL"))?;
    let hostname_len = url.hostname.encode_utf16().count();
    if hostname_len != host.encode_utf16().count() && url.hostname != strip_port(&host) {
        return Err(RequestError::new("Invalid host header"));
    }
    Ok(url.href)
}

/// Returns `host` with a trailing `:` and ASCII digits removed, `host.replace(/:\d+$/, "")`.
fn strip_port(host: &str) -> &str {
    let digits = host.len() - host.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 && host[..host.len() - digits].ends_with(':') {
        &host[..host.len() - digits - 1]
    } else {
        host
    }
}

/// Returns the `Headers` of the fields of `incoming`, in arrival order.
///
/// # Errors
///
/// Returns the refusal of a field the `Headers` refuses.
pub fn request_headers(incoming: &NodeIncoming) -> Result<Headers, RequestError> {
    let mut headers = Headers::new();
    for (name, value) in &incoming.fields {
        if name.first() == Some(&b':') {
            continue;
        }
        headers
            .append(&latin1(name), &latin1(value))
            .map_err(|error| RequestError {
                message: error.message,
            })?;
    }
    Ok(headers)
}

/// The body of a request read from the stream of its connection.
struct IncomingStream(Box<dyn IncomingBody>);

impl Stream for IncomingStream {
    type Item = Chunk;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Chunk>> {
        self.get_mut().0.poll_chunk(cx).map(|item| {
            item.map(|chunk| {
                chunk.map_err(|error| match error {
                    IncomingError::Refused => BodyError::Refused,
                    IncomingError::Closed => BodyError::Closed,
                    IncomingError::TooLarge => BodyError::TooLarge,
                })
            })
        })
    }
}

/// Returns the Fetch request of `incoming`, `newRequest(incoming)` of the adapter.
///
/// A `GET` and a `HEAD` get no body whatever the wire carried, `TRACE` reads as
/// its method with no body, as the adapter makes a `GET` whose method reads
/// `TRACE`, and every other method reads the body as a stream, quirk
/// `Q56_NODE_ADAPTER_BODY_METHODS`.
///
/// # Errors
///
/// Returns [`RequestError`] where the URL or the headers are refused, which
/// the listener answers with 400.
pub fn new_request(incoming: NodeIncoming) -> Result<Rc<Request>, RequestError> {
    let url = request_url(&incoming)?;
    let headers = request_headers(&incoming)?;
    let method = incoming.method;
    let body = if matches!(method, "GET" | "HEAD" | "TRACE") {
        Body::Empty
    } else {
        Body::Stream(BodyStream::new(IncomingStream(incoming.body)))
    };
    Ok(Rc::new(Request::new(method, url, headers, body)))
}
