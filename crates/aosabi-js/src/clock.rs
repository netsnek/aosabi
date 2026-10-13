// SPDX-License-Identifier: MIT
/*
 * aosabi-js clock: the clock of the host as the clock of the timers of the
 * agents of the engine.
 *
 * No analogue in the reference, whose timers are those of the event loop of
 * its host.
 */

//! The clock of the host.
//!
//! An agent of the engine reads the time of its timers from the clock its
//! setup hands it and asks that clock to wake it at the least deadline of
//! its timers once nothing else can run. The module reads no clock of its
//! own, so [`HostTimers`] reads the monotonic clock of the host through the
//! import `host_now`, and a deadline becomes one call of the import
//! `host_timer` for the request whose drive met it, after which the host
//! calls the export `wake` of that request once the delay has passed. A
//! drive of a request parks while such a timer waits, as it parks on a
//! promise of the host.
//!
//! The waker the agent hands with a deadline is that of the poll that met
//! it, and the export `wake` drives the request again, which polls the same
//! futures. A deadline asks the host once: a later deadline of a request
//! that holds an earlier one asks nothing, since the drive at the earlier
//! one meets it again, and the export `wake` forgets every deadline of its
//! request, so the drive it starts asks for the next one anew.

#![forbid(unsafe_code)]

use core::{
    cell::RefCell,
    task::Waker, //
};

use aosabi_execute::promise::agent::HostClock;

use crate::{
    abi::import,
    request, //
};

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The deadlines the host was asked to wake, each with its request.
    static ASKED: RefCell<Vec<(u32, u64)>> = const { RefCell::new(Vec::new()) };
}

/// The clock of the host, which the setup of every agent of the engine takes.
pub struct HostTimers;

impl HostClock for HostTimers {
    fn now_ms(&self) -> u64 {
        now()
    }

    fn wake_at(&self, deadline_ms: u64, _waker: Waker) {
        let request = request::current();
        if request == request::BOOT {
            return;
        }
        let asked = ASKED.with_borrow_mut(|asked| {
            if asked
                .iter()
                .any(|&(held, deadline)| held == request && deadline <= deadline_ms)
            {
                return false;
            }
            asked.push((request, deadline_ms));
            true
        });
        if asked {
            let delay = deadline_ms.saturating_sub(now());
            import::host_timer(request, u32::try_from(delay).unwrap_or(u32::MAX));
        }
    }
}

/// Returns the whole milliseconds of the clock of the host, 0 for a reading
/// that is no time.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the reading is a finite time that is not negative, checked first"
)]
fn now() -> u64 {
    let ms = import::host_now();
    if ms.is_finite() && ms >= 0.0 {
        ms as u64
    } else {
        0
    }
}

/// Answers whether the host was asked to wake `request`, so a drive of it
/// parks on the timer of the host.
pub fn waits(request: u32) -> bool {
    ASKED.with_borrow(|asked| asked.iter().any(|&(held, _)| held == request))
}

/// Forgets every deadline of `request`, whose timer of the host fired.
pub fn woke(request: u32) {
    forget(request);
}

/// Forgets every deadline of `request`, a request the module drops.
pub fn forget(request: u32) {
    ASKED.with_borrow_mut(|asked| asked.retain(|&(held, _)| held != request));
}
