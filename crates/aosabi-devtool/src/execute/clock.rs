// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool execute clock: the monotonic clock of the build host, which
 * an agent of the execute primitive reads under --timers host.
 *
 * No analogue in the reference. The clock is the HostClock of aosabi-execute
 * over std::time::Instant, so a timer of a behaviour table waits the
 * milliseconds of its units, as the timer of a served agent waits on the
 * clock of the serve layer.
 */

//! The clock of the host mode.
//!
//! An agent in `TimerMode::Host` reads the time of [`WallClock`] in
//! milliseconds and hands it the deadline of its next timer when it has
//! nothing else to run. The seam has no runtime to park on, so it keeps that
//! deadline, sleeps until it in [`WallClock::wait`], and polls the agent once
//! more.

use std::{
    cell::Cell,
    task::Waker,
    thread,
    time::{
        Duration,
        Instant, //
    },
};

use aosabi_execute::promise::agent::HostClock;

/// The monotonic clock of the build host, in milliseconds since it was made.
pub struct WallClock {
    /// The instant the clock reads as 0.
    origin: Instant,
    /// The least deadline an agent handed over since the last wait.
    deadline: Cell<Option<u64>>,
}

impl WallClock {
    /// Returns a clock that reads 0 now.
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
            deadline: Cell::new(None),
        }
    }

    /// Sleeps until the deadline an agent handed over and returns whether there was one.
    pub fn wait(&self) -> bool {
        let Some(deadline) = self.deadline.take() else {
            return false;
        };
        let left = deadline.saturating_sub(self.now_ms());
        if left > 0 {
            thread::sleep(Duration::from_millis(left));
        }
        true
    }
}

impl HostClock for WallClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Keeps the least deadline, because the seam polls the agent itself and needs no waker.
    fn wake_at(&self, deadline_ms: u64, _waker: Waker) {
        let least = self
            .deadline
            .get()
            .map_or(deadline_ms, |held| held.min(deadline_ms));
        self.deadline.set(Some(least));
    }
}
