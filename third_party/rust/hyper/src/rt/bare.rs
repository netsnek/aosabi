// SPDX-License-Identifier: MIT
/*
 * hyper: the runtime traits of the HTTP/1 server that need no standard
 * library, with hyper's own IO error.
 */
//! Runtime traits that need no standard library.
//!
//! The HTTP/1 server reads and writes its IO through [`Read`] and [`Write`] of
//! this module, which answer [`IoError`] in place of `std::io::Error`, so the
//! server builds `no_std` with `alloc`. With the feature `std`, every type
//! that implements `hyper::rt::Read` or `hyper::rt::Write` implements the
//! trait of this module of the same name through a blanket implementation,
//! and the `std::io::Error` it answers becomes the cause of the [`IoError`],
//! which `Error::source` returns.

use alloc::boxed::Box;
use core::fmt;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::time::Duration;

use super::{ReadBufCursor, Sleep};
use crate::error::StdError;

/// Reads bytes from a source without the standard library.
///
/// It is `hyper::rt::Read` with [`IoError`] in place of `std::io::Error`.
pub trait Read {
    /// Attempts to read bytes into the `buf`.
    ///
    /// On success, returns `Poll::Ready(Ok(()))` and places data in the
    /// unfilled portion of `buf`. If no data was read (`buf.remaining()` is
    /// unchanged), it implies that EOF has been reached.
    ///
    /// If no data is available for reading, the method returns `Poll::Pending`
    /// and arranges for the current task (via `cx.waker()`) to receive a
    /// notification when the object becomes readable or is closed.
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: ReadBufCursor<'_>,
    ) -> Poll<Result<(), IoError>>;
}

/// Writes bytes asynchronously without the standard library.
///
/// It is `hyper::rt::Write` with [`IoError`] in place of `std::io::Error`. The
/// vectored write needs `std::io::IoSlice`, so it exists with the feature
/// `std` alone.
pub trait Write {
    /// Attempts to write bytes from `buf` into the destination.
    ///
    /// On success, returns `Poll::Ready(Ok(num_bytes_written))`. If
    /// successful, it must be guaranteed that `n <= buf.len()`. A return value
    /// of `0` means that the underlying object is no longer able to accept
    /// bytes, or that the provided buffer is empty.
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>>;

    /// Attempts to flush the object.
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>>;

    /// Attempts to shut down this writer.
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>>;

    /// Returns whether this writer has an efficient `poll_write_vectored`
    /// implementation.
    ///
    /// The default implementation returns `false`.
    #[cfg(feature = "std")]
    fn is_write_vectored(&self) -> bool {
        false
    }

    /// Like `poll_write`, except that it writes from a slice of buffers.
    #[cfg(feature = "std")]
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<Result<usize, IoError>> {
        let buf = bufs
            .iter()
            .find(|b| !b.is_empty())
            .map_or(&[][..], |b| &**b);
        self.poll_write(cx, buf)
    }
}

#[cfg(feature = "std")]
impl<T: super::Read + ?Sized> Read for T {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: ReadBufCursor<'_>,
    ) -> Poll<Result<(), IoError>> {
        super::Read::poll_read(self, cx, buf).map_err(IoError::from)
    }
}

#[cfg(feature = "std")]
impl<T: super::Write + ?Sized> Write for T {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        super::Write::poll_write(self, cx, buf).map_err(IoError::from)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        super::Write::poll_flush(self, cx).map_err(IoError::from)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        super::Write::poll_shutdown(self, cx).map_err(IoError::from)
    }

    fn is_write_vectored(&self) -> bool {
        super::Write::is_write_vectored(self)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<Result<usize, IoError>> {
        super::Write::poll_write_vectored(self, cx, bufs).map_err(IoError::from)
    }
}

/// A timer that measures time from an epoch it chooses, without the standard
/// library.
///
/// It is the bare form of `hyper::rt::Timer`, whose `Instant` needs `std`. A
/// time is a [`Duration`] since the epoch of the timer, and the epoch is the
/// timer's to choose, the moment it started for instance.
pub trait Timer {
    /// Returns the time since the epoch of the timer.
    fn now(&self) -> Duration;

    /// Returns a future that resolves at `deadline`, a time since the epoch.
    fn sleep_until(&self, deadline: Duration) -> Pin<Box<dyn Sleep>>;

    /// Returns a future that resolves in `duration` time.
    ///
    /// The default implementation sleeps until the time [`Timer::now`] answers
    /// and `duration`.
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Sleep>> {
        self.sleep_until(self.now() + duration)
    }

    /// Resets a future to resolve at `new_deadline` instead.
    fn reset(&self, sleep: &mut Pin<Box<dyn Sleep>>, new_deadline: Duration) {
        *sleep = self.sleep_until(new_deadline);
    }

    /// Returns the seconds since the Unix epoch, the wall clock of the `Date`
    /// field of an answer.
    ///
    /// The default implementation returns `None`, a timer without a wall
    /// clock. Without the feature `std`, a server whose timer has none writes
    /// no `Date`, and with `std` the date comes from the system clock.
    fn unix_seconds(&self) -> Option<u64> {
        None
    }
}

/// The error of an IO that implements [`Read`] or [`Write`].
///
/// It carries an [`IoErrorKind`] and an optional message, and with the
/// feature `std` the `std::io::Error` it was made from, which
/// `Error::source` returns.
pub struct IoError {
    kind: IoErrorKind,
    message: Option<Box<dyn fmt::Display + Send + Sync>>,
    #[cfg(feature = "std")]
    cause: Option<std::io::Error>,
}

/// The kind of an [`IoError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IoErrorKind {
    /// The connection was reset by the peer.
    ConnectionReset,
    /// The connection was aborted by the peer.
    ConnectionAborted,
    /// The peer is no longer reading what is written.
    BrokenPipe,
    /// The data read is not valid for the operation.
    InvalidData,
    /// A parameter of the operation is not valid.
    InvalidInput,
    /// The operation took longer than it was allowed to.
    TimedOut,
    /// A write accepted zero bytes of a buffer that was not empty.
    WriteZero,
    /// The end of the input came before the operation completed.
    UnexpectedEof,
    /// Any other error.
    Other,
}

impl IoError {
    /// Creates an error of `kind` that displays `error` as its message.
    pub fn new<E>(kind: IoErrorKind, error: E) -> IoError
    where
        E: fmt::Display + Send + Sync + 'static,
    {
        IoError {
            kind,
            message: Some(Box::new(error)),
            #[cfg(feature = "std")]
            cause: None,
        }
    }

    /// Returns the kind of the error.
    pub fn kind(&self) -> IoErrorKind {
        self.kind
    }
}

impl IoErrorKind {
    fn as_str(&self) -> &'static str {
        match *self {
            IoErrorKind::ConnectionReset => "connection reset",
            IoErrorKind::ConnectionAborted => "connection aborted",
            IoErrorKind::BrokenPipe => "broken pipe",
            IoErrorKind::InvalidData => "invalid data",
            IoErrorKind::InvalidInput => "invalid input parameter",
            IoErrorKind::TimedOut => "timed out",
            IoErrorKind::WriteZero => "write zero",
            IoErrorKind::UnexpectedEof => "unexpected end of file",
            IoErrorKind::Other => "other error",
        }
    }
}

impl From<IoErrorKind> for IoError {
    fn from(kind: IoErrorKind) -> IoError {
        IoError {
            kind,
            message: None,
            #[cfg(feature = "std")]
            cause: None,
        }
    }
}

impl fmt::Debug for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = f.debug_struct("IoError");
        builder.field("kind", &self.kind);
        if let Some(message) = &self.message {
            builder.field("message", &format_args!("{message}"));
        }
        #[cfg(feature = "std")]
        if let Some(cause) = &self.cause {
            builder.field("cause", cause);
        }
        builder.finish()
    }
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(feature = "std")]
        if let Some(cause) = &self.cause {
            return fmt::Display::fmt(cause, f);
        }
        match &self.message {
            Some(message) => fmt::Display::fmt(message, f),
            None => f.write_str(self.kind.as_str()),
        }
    }
}

impl StdError for IoError {
    #[cfg(feature = "std")]
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.cause
            .as_ref()
            .map(|cause| cause as &(dyn StdError + 'static))
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for IoError {
    fn from(cause: std::io::Error) -> IoError {
        use std::io::ErrorKind;

        let kind = match cause.kind() {
            ErrorKind::ConnectionReset => IoErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted => IoErrorKind::ConnectionAborted,
            ErrorKind::BrokenPipe => IoErrorKind::BrokenPipe,
            ErrorKind::InvalidData => IoErrorKind::InvalidData,
            ErrorKind::InvalidInput => IoErrorKind::InvalidInput,
            ErrorKind::TimedOut => IoErrorKind::TimedOut,
            ErrorKind::WriteZero => IoErrorKind::WriteZero,
            ErrorKind::UnexpectedEof => IoErrorKind::UnexpectedEof,
            _ => IoErrorKind::Other,
        };
        IoError {
            kind,
            message: None,
            cause: Some(cause),
        }
    }
}

#[cfg(feature = "std")]
impl From<IoError> for std::io::Error {
    fn from(mut error: IoError) -> std::io::Error {
        use std::io::ErrorKind;

        // An error made from a `std::io::Error` gives it back unchanged, so a
        // caller of hyper sees the error its IO answered.
        if let Some(cause) = error.cause.take() {
            return cause;
        }
        let kind = match error.kind {
            IoErrorKind::ConnectionReset => ErrorKind::ConnectionReset,
            IoErrorKind::ConnectionAborted => ErrorKind::ConnectionAborted,
            IoErrorKind::BrokenPipe => ErrorKind::BrokenPipe,
            IoErrorKind::InvalidData => ErrorKind::InvalidData,
            IoErrorKind::InvalidInput => ErrorKind::InvalidInput,
            IoErrorKind::TimedOut => ErrorKind::TimedOut,
            IoErrorKind::WriteZero => ErrorKind::WriteZero,
            IoErrorKind::UnexpectedEof => ErrorKind::UnexpectedEof,
            IoErrorKind::Other => ErrorKind::Other,
        };
        std::io::Error::new(kind, error)
    }
}
