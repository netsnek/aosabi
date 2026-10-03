// SPDX-License-Identifier: MIT
#[cfg(all(any(feature = "client", feature = "server"), feature = "http2"))]
mod compat;
#[cfg(feature = "std")]
mod rewind;

#[cfg(all(any(feature = "client", feature = "server"), feature = "http2"))]
pub(crate) use self::compat::Compat;
#[cfg(feature = "std")]
pub(crate) use self::rewind::Rewind;

// The IO error inside the crate is `std::io::Error` with `std`, which a caller
// of hyper sees today, and hyper's own `IoError` without it.
#[cfg(all(
    not(feature = "std"),
    any(feature = "client", feature = "server"),
    feature = "http1"
))]
pub(crate) use crate::rt::bare::{IoError as Error, IoErrorKind as ErrorKind};
#[cfg(all(
    feature = "std",
    any(feature = "client", feature = "server"),
    not(feature = "http1"),
    feature = "http2"
))]
pub(crate) use std::io::Error;
#[cfg(all(
    feature = "std",
    any(feature = "client", feature = "server"),
    feature = "http1"
))]
pub(crate) use std::io::{Error, ErrorKind, Result};

#[cfg(all(
    not(feature = "std"),
    any(feature = "client", feature = "server"),
    feature = "http1"
))]
pub(crate) type Result<T> = core::result::Result<T, Error>;
