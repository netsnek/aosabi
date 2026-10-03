// SPDX-License-Identifier: MIT
/*
 * h2: the stream trait of the crate, in place of the one of futures-core.
 */
//! A stream of values, the one trait of `futures-core` the crate called.
//!
//! The codec is a stream of frames that the connection polls. The trait is
//! the crate's own, so no public type implements the `Stream` of
//! `futures-core`, and the inherent `poll_data`, `poll_accept` and
//! `poll_push_promise` are the way to poll the public types.

use std::pin::Pin;
use std::task::{Context, Poll};

/// A source of values that are polled one after another.
pub(crate) trait Stream {
    /// The value the stream yields.
    type Item;

    /// Polls for the next value, and answers `None` once the stream ended.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;
}
