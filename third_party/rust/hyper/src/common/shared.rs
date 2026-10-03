// SPDX-License-Identifier: MIT
/*
 * hyper: the state the parts of a connection share, on one thread without
 * the feature sync and thread safe with it.
 */
//! The state the parts of a connection share.
//!
//! Without `sync` a connection runs on one thread, so its timer and the watch
//! of its body share their state through `Rc`, `Cell` and `RefCell`, and the
//! types of the connection are not `Send`. A target without atomics serves
//! HTTP/1 so, in safe code and with no crate, and so does a server with `std`
//! that runs each connection on one thread. With `sync` the state is an
//! `Arc`, an atomic and a waker under a `Mutex`, as hyper shares it across
//! threads.

#[cfg(not(feature = "sync"))]
pub(crate) use alloc::rc::Rc as Share;
#[cfg(feature = "sync")]
pub(crate) use alloc::sync::Arc as Share;

#[cfg(all(
    any(feature = "client", feature = "server"),
    feature = "http1",
    not(feature = "sync")
))]
use core::cell::{Cell, RefCell};
#[cfg(all(
    any(feature = "client", feature = "server"),
    feature = "http1",
    feature = "sync"
))]
use core::sync::atomic::{AtomicUsize, Ordering};
#[cfg(all(any(feature = "client", feature = "server"), feature = "http1"))]
use core::task::Waker;

#[cfg(all(
    any(feature = "client", feature = "server"),
    feature = "http1",
    feature = "sync"
))]
use std::sync::{Mutex, PoisonError};

/// The timer of a connection, which every part of it may read.
#[cfg(all(
    feature = "sync",
    any(
        all(feature = "server", feature = "http1"),
        all(any(feature = "client", feature = "server"), feature = "http2"),
    )
))]
pub(crate) type SharedTimer = Share<dyn crate::rt::bare::Timer + Send + Sync>;
/// The timer of a connection, which every part of it may read.
#[cfg(all(
    not(feature = "sync"),
    any(
        all(feature = "server", feature = "http1"),
        all(any(feature = "client", feature = "server"), feature = "http2"),
    )
))]
pub(crate) type SharedTimer = Share<dyn crate::rt::bare::Timer>;

/// A number one part of a connection writes and another reads.
#[cfg(all(any(feature = "client", feature = "server"), feature = "http1"))]
pub(crate) struct Number {
    #[cfg(feature = "sync")]
    value: AtomicUsize,
    #[cfg(not(feature = "sync"))]
    value: Cell<usize>,
}

#[cfg(all(any(feature = "client", feature = "server"), feature = "http1"))]
impl Number {
    pub(crate) fn new(value: usize) -> Number {
        Number {
            #[cfg(feature = "sync")]
            value: AtomicUsize::new(value),
            #[cfg(not(feature = "sync"))]
            value: Cell::new(value),
        }
    }

    /// Stores `value` and returns the number it replaces.
    pub(crate) fn swap(&self, value: usize) -> usize {
        #[cfg(feature = "sync")]
        return self.value.swap(value, Ordering::SeqCst);
        #[cfg(not(feature = "sync"))]
        return self.value.replace(value);
    }

    /// Returns the number, ordered with every `swap`.
    pub(crate) fn load(&self) -> usize {
        #[cfg(feature = "sync")]
        return self.value.load(Ordering::SeqCst);
        #[cfg(not(feature = "sync"))]
        return self.value.get();
    }

    /// Returns the number with no ordering, for a look that decides nothing.
    pub(crate) fn peek(&self) -> usize {
        #[cfg(feature = "sync")]
        return self.value.load(Ordering::Relaxed);
        #[cfg(not(feature = "sync"))]
        return self.value.get();
    }
}

/// The waker of the one task that waits on a state a connection shares.
#[cfg(all(any(feature = "client", feature = "server"), feature = "http1"))]
pub(crate) struct WakerSlot {
    #[cfg(feature = "sync")]
    waker: Mutex<Option<Waker>>,
    #[cfg(not(feature = "sync"))]
    waker: RefCell<Option<Waker>>,
}

#[cfg(all(any(feature = "client", feature = "server"), feature = "http1"))]
impl WakerSlot {
    pub(crate) fn new() -> WakerSlot {
        WakerSlot {
            #[cfg(feature = "sync")]
            waker: Mutex::new(None),
            #[cfg(not(feature = "sync"))]
            waker: RefCell::new(None),
        }
    }

    /// Keeps `waker` as the one to wake, in place of any before it.
    pub(crate) fn register(&self, waker: &Waker) {
        // A change of the slot is complete before its lock is released, so a
        // panic elsewhere leaves a valid slot and the poison is ignored.
        #[cfg(feature = "sync")]
        let mut slot = self.waker.lock().unwrap_or_else(PoisonError::into_inner);
        #[cfg(not(feature = "sync"))]
        let mut slot = self.waker.borrow_mut();
        match &mut *slot {
            Some(kept) if kept.will_wake(waker) => {}
            _ => *slot = Some(waker.clone()),
        }
    }

    /// Wakes the waker kept, if any, and forgets it.
    pub(crate) fn wake(&self) {
        #[cfg(feature = "sync")]
        let waker = self
            .waker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        #[cfg(not(feature = "sync"))]
        let waker = self.waker.borrow_mut().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}
