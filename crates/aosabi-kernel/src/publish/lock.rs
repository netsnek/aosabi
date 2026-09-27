// SPDX-License-Identifier: MIT
/*
 * aosabi-kernel publish lock: the lock the environment supplies around the
 * hub.
 *
 * No analogue in the reference. The kernel has no thread and no critical
 * section of its own, so a server supplies std::sync::Mutex and a
 * microcontroller a critical section, and the lock uses core alone.
 */

//! The lock around the hub.
//!
//! The hub is a plain value that every operation borrows mutably, so the
//! environment decides how two callers take turns: a `std::sync::Mutex` on a
//! card with the standard library, where a publish may come from any thread,
//! a critical section on a microcontroller, and [`LocalLock`] where one
//! thread owns the hub. The module names `core` alone.

use core::cell::RefCell;

/// The lock the environment supplies, because the kernel has no thread and no critical section of its own.
///
/// A server supplies `std::sync::Mutex` and a microcontroller a critical
/// section, and the kernel's free `publish` takes any of them.
pub trait Lock<T> {
    /// Runs `body` with the value locked and returns what `body` returns.
    ///
    /// The lock is held for the whole of `body` and released before this
    /// returns, so a caller that wakes a task after the call wakes it with the
    /// lock free.
    fn with<R>(&self, body: impl FnOnce(&mut T) -> R) -> R;
}

/// A lock of one thread over a `RefCell`.
///
/// The lock is never `Sync`, as a `RefCell` is not, so no second thread
/// reaches the value while the first holds it, and it is `Send` when the
/// value is, so a thread may hand the hub on whole.
// An `unsafe` cell would skip each borrow flag.
pub struct LocalLock<T>(RefCell<T>);

impl<T> LocalLock<T> {
    /// Creates the lock around `value`.
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self(RefCell::new(value))
    }
}

impl<T> Lock<T> for LocalLock<T> {
    /// Runs `body` with the value borrowed mutably.
    ///
    /// # Panics
    ///
    /// Panics when `body` takes the same lock again, which would alias the
    /// value it holds.
    fn with<R>(&self, body: impl FnOnce(&mut T) -> R) -> R {
        body(&mut self.0.borrow_mut())
    }
}
