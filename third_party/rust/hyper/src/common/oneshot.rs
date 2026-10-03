// SPDX-License-Identifier: MIT
/*
 * hyper: a channel of one value between two tasks, for the upgrade and for
 * the trailers of a body without the standard library.
 */
//! A channel of one value.
//!
//! The upgrade hands its `Upgraded` from the connection to the `OnUpgrade`
//! of the request through this channel. `OnUpgrade` rides in the extensions of
//! a request, which ask `Send + Sync`, so with `std` the state of the channel
//! sits under a `Mutex` in an `Arc` and needs no crate. Without `std` there is
//! no upgrade, the channel carries the trailers of a body, and its state is a
//! `RefCell` in an `Rc` on one thread.

// A build without a protocol keeps the module `upgrade` and makes no pending
// upgrade, so it never opens a channel.
#![cfg_attr(
    not(all(
        any(feature = "client", feature = "server"),
        any(feature = "http1", feature = "http2")
    )),
    allow(dead_code)
)]

#[cfg(not(feature = "std"))]
use alloc::rc::Rc as Share;
#[cfg(feature = "std")]
use alloc::sync::Arc as Share;
#[cfg(not(feature = "std"))]
use core::cell::{RefCell as Lock, RefMut as Guard};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
#[cfg(feature = "std")]
use std::sync::{Mutex as Lock, MutexGuard as Guard, PoisonError};

/// Returns the two ends of a new channel.
pub(crate) fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Share::new(Lock::new(Inner {
        value: None,
        waker: None,
        closed: false,
    }));
    (
        Sender {
            inner: inner.clone(),
        },
        Receiver { inner },
    )
}

/// The end that sends the one value.
pub(crate) struct Sender<T> {
    inner: Share<Lock<Inner<T>>>,
}

/// The end that waits for the one value.
pub(crate) struct Receiver<T> {
    inner: Share<Lock<Inner<T>>>,
}

/// The error of a receiver whose sender was dropped without sending.
#[derive(Debug)]
pub(crate) struct Canceled;

struct Inner<T> {
    value: Option<T>,
    waker: Option<Waker>,
    closed: bool,
}

/// Locks the state of a channel.
///
/// Every change of the state is complete before its lock is released, so a
/// panic elsewhere leaves a valid state and the poison is ignored.
fn lock<T>(inner: &Lock<Inner<T>>) -> Guard<'_, Inner<T>> {
    #[cfg(feature = "std")]
    return inner.lock().unwrap_or_else(PoisonError::into_inner);
    #[cfg(not(feature = "std"))]
    return inner.borrow_mut();
}

impl<T> Sender<T> {
    /// Sends `value`, or gives it back when the receiver is gone.
    pub(crate) fn send(self, value: T) -> Result<(), T> {
        let waker = {
            let mut inner = lock(&self.inner);
            if inner.closed {
                return Err(value);
            }
            inner.value = Some(value);
            inner.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        Ok(())
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let waker = {
            let mut inner = lock(&self.inner);
            inner.closed = true;
            inner.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, Canceled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut inner = lock(&self.inner);
        if let Some(value) = inner.value.take() {
            return Poll::Ready(Ok(value));
        }
        if inner.closed {
            return Poll::Ready(Err(Canceled));
        }
        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        lock(&self.inner).closed = true;
    }
}
