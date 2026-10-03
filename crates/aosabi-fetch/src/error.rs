// SPDX-License-Identifier: MIT
/*
 * aosabi-fetch: the failures of an outbound call.
 *
 * No analogue on the reference side, where a failure below HTTP rejects with
 * a TypeError fetch failed whose cause names it.
 */

//! The mapping of every failure onto `FetchError`.
//!
//! A call fails with one `FetchError` of `aosabi-core`, which the context
//! throws as the `TypeError` `fetch failed` with the failure in its host
//! slot. The text of a variant names the address, the file or the rule the
//! failure is about, so a log line that reads the slot tells the failures
//! apart. A request the client cannot write as HTTP/1.1, a method or a field
//! the client owns, is refused as `Protocol` beside the answers the client
//! cannot read, since the capability names no variant of its own for it.

use std::{
    io::{
        self,
        ErrorKind, //
    },
    net::SocketAddr, //
};

use aosabi_core::net::FetchError;

/// Returns the failure of a connect to `addr`.
#[must_use]
pub fn connect(addr: SocketAddr, error: &io::Error) -> FetchError {
    if error.kind() == ErrorKind::TimedOut {
        return FetchError::Timeout;
    }
    FetchError::Connect(format!("{addr}: {error}"))
}

/// Returns the failure of the name resolution of `host`.
#[must_use]
pub fn resolve(host: &str, error: &io::Error) -> FetchError {
    FetchError::Resolve(format!("{host}: {error}"))
}

/// Returns the failure of a TLS handshake.
#[must_use]
pub fn handshake(error: &io::Error) -> FetchError {
    match error.kind() {
        ErrorKind::UnexpectedEof => FetchError::Tls(String::from(
            "the server closed the connection during the handshake",
        )),
        _ => FetchError::Tls(error.to_string()),
    }
}

/// Returns the failure of a read or a write of an open connection.
///
/// A record rustls refuses is a TLS failure, and an end of the stream in the
/// middle of an answer is a protocol failure, a TLS stream that ended with no
/// close notification among them.
#[must_use]
pub fn io(error: &io::Error) -> FetchError {
    match error.kind() {
        ErrorKind::TimedOut => FetchError::Timeout,
        ErrorKind::InvalidData => FetchError::Tls(error.to_string()),
        ErrorKind::UnexpectedEof => protocol("the connection closed before the answer ended"),
        _ => FetchError::Connect(error.to_string()),
    }
}

/// Returns the failure `reason` of the HTTP of a request or an answer.
#[must_use]
pub fn protocol(reason: &str) -> FetchError {
    FetchError::Protocol(String::from(reason))
}
