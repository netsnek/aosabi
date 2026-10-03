// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the answers of the Node adapter to a request it cannot build,
 * a fetch that rejects and a head Node refuses.
 *
 * Derived from hono 1.19.9, dist/listener.js, MIT.
 *
 * It carries the half of the adapter of the quirk Q59_NODE_RECORD_CHECK_500
 * of docs/byte-equality.md section 13.1.
 */

//! `handleRequestError`, `handleFetchError` and `handleResponseError`.
//!
//! A request the adapter cannot build is answered with 400 and a fetch that
//! rejects with 500, or 504 for a `TimeoutError`, each with a null body, which
//! the listener writes on its object path with the default `content-type`
//! and chunked framing on HTTP/1.1 (A3 and A6 of the plan's version 2 draft).
//! A record that Node's `writeHead` refuses is answered as
//! `handleResponseError` answers: the error on standard error, the status 500
//! with the reason the failed call left, `Content-Type: text/plain`, the body
//! `Error: <message>` and the end of the connection (N14, quirk
//! `Q59_NODE_RECORD_CHECK_500`).

use aosabi_http::{
    fetch::response::Response,
    message::{
        FirstWrite,
        Line,
        NodeBody,
        NodeOutgoing, //
    },
    node::status_text, //
};

/// Returns the answer to a request the adapter cannot build, `handleRequestError()`.
#[must_use]
pub fn handle_request_error() -> Response {
    Response::empty(400)
}

/// Returns the answer to a fetch that rejected, `handleFetchError(e)`.
///
/// 504 when the error is a `TimeoutError` and 500 otherwise, with a null body.
#[must_use]
pub fn handle_fetch_error(error_name: &str) -> Response {
    Response::empty(if error_name == "TimeoutError" {
        504
    } else {
        500
    })
}

/// Returns the answer of `handleResponseError` to a record `writeHead` refused.
///
/// The error goes to standard error as `console.error` writes it,
/// `writeHead(500, { "Content-Type": "text/plain" })` keeps the reason the
/// failed call of the first `status` set, `end` writes `Error: ` and `message`
/// as a string, and the socket is destroyed, quirk `Q59_NODE_RECORD_CHECK_500`.
#[must_use]
pub fn handle_response_error(message: &str, status: u16) -> NodeOutgoing {
    eprintln!("TypeError: {message}");
    NodeOutgoing {
        status: 500,
        reason: Some(status_text(status)),
        record: vec![Line::new("Content-Type", &b"text/plain"[..])],
        body: NodeBody::Full(format!("Error: {message}").into_bytes()),
        first_write: FirstWrite::Text,
        cleanup_incoming: false,
        destroy: true,
    }
}
