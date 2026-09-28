// SPDX-License-Identifier: MIT
/*
 * aosabi-http date: the time value of a Date and the clock of Date.now.
 */

//! `now`, `set_now` and `time_value`.
//!
//! [`now`] is the wall clock of `Date.now()` for the crates that name this one,
//! the whole milliseconds since the epoch, negative before it, and
//! [`time_value`] the time value a `Date` of a time of the system holds. A
//! Worker reads no clock of the system from `std`, whose `SystemTime::now`
//! panics on `wasm32-unknown-unknown`, so a Worker build reads 0 until the glue
//! of the Worker hands the host's time, as the file of `graphql-over-http` does.
//! Under the feature `host-platform`, which the module `core.wasm` of the
//! TypeScript option alone turns on, that build reads the time its host handed
//! last through `set_now`, the `Date.now()` of the host at the call that reads
//! the clock, so a build of the Rust option keeps its bytes.

use std::time::{
    Duration,
    SystemTime,
    UNIX_EPOCH, //
};

/// The largest time value of a `Date`, 8.64e15 milliseconds or 100,000,000
/// days either side of the epoch, written in hours.
const MAX_TIME_VALUE: Duration = Duration::from_hours(2_400_000_000);

/// Returns the time value of `time` in milliseconds, `None` beyond the range of a `Date`.
///
/// `TimeClip` answers an invalid `Date` for a time further than 8.64e15
/// milliseconds from the epoch, the exact distance compared, and otherwise the
/// milliseconds truncated toward zero on either side of the epoch.
#[must_use]
pub fn time_value(time: SystemTime) -> Option<i64> {
    let (distance, before) = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => (after, false),
        Err(before) => (before.duration(), true),
    };
    if distance > MAX_TIME_VALUE {
        return None;
    }
    let ms = i64::try_from(distance.as_millis()).ok()?;
    Some(if before { -ms } else { ms })
}

/// Returns the milliseconds since the epoch, `Date.now()`.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[must_use]
pub fn now() -> i64 {
    time_value(SystemTime::now()).unwrap_or(0)
}

/// Returns 0, the time of a Worker build, see the module documentation.
#[cfg(all(
    target_arch = "wasm32",
    target_os = "unknown",
    not(feature = "host-platform")
))]
#[must_use]
pub fn now() -> i64 {
    0
}

#[cfg(all(
    target_arch = "wasm32",
    target_os = "unknown",
    feature = "host-platform"
))]
std::thread_local! {
    /// The milliseconds since the epoch the host handed last, 0 before.
    static HOST_NOW: core::cell::Cell<i64> = const { core::cell::Cell::new(0) };
}

/// Returns the milliseconds since the epoch the host handed last through
/// [`set_now`], 0 before, the time of a build that reads no clock of the
/// system, see the module documentation.
#[cfg(all(
    target_arch = "wasm32",
    target_os = "unknown",
    feature = "host-platform"
))]
#[must_use]
pub fn now() -> i64 {
    HOST_NOW.get()
}

/// Hands the milliseconds since the epoch `ms` of the host to [`now`], the
/// `Date.now()` of the host at a call that reads the clock.
#[cfg(all(
    target_arch = "wasm32",
    target_os = "unknown",
    feature = "host-platform"
))]
pub fn set_now(ms: i64) {
    HOST_NOW.set(ms);
}

/// Takes the milliseconds `ms` of the host and keeps none, since a build that
/// reads the clock of the system reads the clock the host reads.
#[cfg(all(
    not(all(target_arch = "wasm32", target_os = "unknown")),
    feature = "host-platform"
))]
pub fn set_now(ms: i64) {
    let _ = ms;
}
