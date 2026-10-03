// SPDX-License-Identifier: MIT
/*
 * hyper::proto::h1::node::answers: the answers Node writes before any
 * application code runs.
 *
 * Translation of socketOnError of Node 24.13.0, lib/_http_server.js:899-951.
 * It holds quirk Q36 of docs/byte-equality.md section 13.1.
 */
//! The four fixed answers of Node and the rule that writes them or ends the connection, quirk Q36.

use super::parse::Refusal;

/// The answer to a request llhttp refuses.
const BAD_REQUEST: &[u8] = b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n";

/// The answer to a head or a trailer section past its count.
const HEADER_FIELDS_TOO_LARGE: &[u8] =
    b"HTTP/1.1 431 Request Header Fields Too Large\r\nConnection: close\r\n\r\n";

/// The answer to chunk extensions past their bound and to a bound of the card.
const PAYLOAD_TOO_LARGE: &[u8] = b"HTTP/1.1 413 Payload Too Large\r\nConnection: close\r\n\r\n";

/// The answer to a head or a request past its timer.
#[cfg(feature = "node-timers")]
pub(crate) const REQUEST_TIMEOUT: &[u8] =
    b"HTTP/1.1 408 Request Timeout\r\nConnection: close\r\n\r\n";

/// Returns the fixed answer to `refusal`, which carries no `Date` (quirk Q36).
pub(crate) fn fixed(refusal: Refusal) -> &'static [u8] {
    match refusal {
        Refusal::Invalid(_) => BAD_REQUEST,
        Refusal::HeaderOverflow => HEADER_FIELDS_TOO_LARGE,
        Refusal::TooLarge => PAYLOAD_TOO_LARGE,
    }
}

/// Returns whether Node writes a fixed answer before it ends the connection.
///
/// Node writes one only while no answer is in flight or the oldest answer in
/// flight has not written its head, the condition
/// `!this._httpMessage || !this._httpMessage._headerSent` of
/// `lib/_http_server.js:929-930`, and `oldest_started` tells whether the
/// oldest answer in flight has written its head (quirk Q36).
pub(crate) fn written(oldest_started: Option<bool>) -> bool {
    oldest_started != Some(true)
}
