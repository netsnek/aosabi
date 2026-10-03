// SPDX-License-Identifier: MIT
use alloc::boxed::Box;
use core::time::Duration;
use core::{fmt, pin::Pin};
#[cfg(feature = "std")]
use std::time::Instant;

use super::shared::{Share, SharedTimer};
use crate::rt::bare;
use crate::rt::Sleep;
#[cfg(feature = "std")]
use crate::rt::Timer;

/// A user-provided timer to time background tasks.
///
/// It measures time from an epoch of its own, as a [`bare::Timer`] does.
#[derive(Clone)]
pub(crate) enum Time {
    Timer(SharedTimer),
    Empty(Clock),
}

/// The clock of a connection without a timer.
///
/// With `std` it is the `Instant` the builder or connection was made at, which
/// the pings of HTTP/2 measure their round trips from, and without `std` there is none.
#[cfg(feature = "std")]
pub(crate) type Clock = Instant;
#[cfg(not(feature = "std"))]
pub(crate) type Clock = ();

/// A `hyper::rt::Timer` measured from the `Instant` it was given at.
#[cfg(feature = "std")]
struct RtTimer<M> {
    timer: M,
    epoch: Instant,
}

#[cfg(feature = "std")]
impl<M: Timer> bare::Timer for RtTimer<M> {
    fn now(&self) -> Duration {
        self.timer.now().saturating_duration_since(self.epoch)
    }

    fn sleep_until(&self, deadline: Duration) -> Pin<Box<dyn Sleep>> {
        self.timer.sleep_until(self.epoch + deadline)
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Sleep>> {
        self.timer.sleep(duration)
    }

    fn reset(&self, sleep: &mut Pin<Box<dyn Sleep>>, new_deadline: Duration) {
        self.timer.reset(sleep, self.epoch + new_deadline);
    }
}

#[cfg(all(feature = "server", feature = "http1"))]
#[derive(Clone, Copy, Debug)]
pub(crate) enum Dur {
    Default(Option<Duration>),
    Configured(Option<Duration>),
}

impl fmt::Debug for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Time").finish()
    }
}

impl Time {
    /// Returns the time of a builder or a connection without a timer.
    pub(crate) fn empty() -> Time {
        #[cfg(feature = "std")]
        return Time::Empty(Instant::now());
        #[cfg(not(feature = "std"))]
        return Time::Empty(());
    }

    /// Wraps a `hyper::rt::Timer`, whose epoch is the `Instant` of the wrap.
    #[cfg(feature = "std")]
    pub(crate) fn rt<M>(timer: M) -> Time
    where
        M: Timer + Send + Sync + 'static,
    {
        let epoch = timer.now();
        Time::Timer(Share::new(RtTimer { timer, epoch }))
    }

    #[cfg(all(feature = "server", feature = "http1"))]
    pub(crate) fn bare<M>(timer: M) -> Time
    where
        M: bare::Timer + Send + Sync + 'static,
    {
        Time::Timer(Share::new(timer))
    }

    #[cfg(all(any(feature = "client", feature = "server"), feature = "http2"))]
    pub(crate) fn sleep(&self, duration: Duration) -> Pin<Box<dyn Sleep>> {
        match &self {
            Time::Empty(_) => {
                panic!("You must supply a timer.")
            }
            Time::Timer(t) => t.sleep(duration),
        }
    }

    #[cfg(all(feature = "server", feature = "http1"))]
    pub(crate) fn sleep_until(&self, deadline: Duration) -> Pin<Box<dyn Sleep>> {
        match &self {
            Time::Empty(_) => {
                panic!("You must supply a timer.")
            }
            Time::Timer(t) => t.sleep_until(deadline),
        }
    }

    pub(crate) fn now(&self) -> Duration {
        match &self {
            #[cfg(feature = "std")]
            Time::Empty(epoch) => Instant::now().saturating_duration_since(*epoch),
            #[cfg(not(feature = "std"))]
            Time::Empty(()) => Duration::ZERO,
            Time::Timer(t) => t.now(),
        }
    }

    pub(crate) fn reset(&self, sleep: &mut Pin<Box<dyn Sleep>>, new_deadline: Duration) {
        match &self {
            Time::Empty(_) => {
                panic!("You must supply a timer.")
            }
            Time::Timer(t) => t.reset(sleep, new_deadline),
        }
    }

    /// Returns the seconds since the Unix epoch of the wall clock of the timer.
    #[cfg(all(feature = "server", feature = "http1", not(feature = "std")))]
    pub(crate) fn unix_seconds(&self) -> Option<u64> {
        match &self {
            Time::Empty(_) => None,
            Time::Timer(t) => t.unix_seconds(),
        }
    }

    #[cfg(all(feature = "server", feature = "http1"))]
    pub(crate) fn check(&self, dur: Dur, name: &'static str) -> Option<Duration> {
        match dur {
            Dur::Default(Some(dur)) => match self {
                Time::Empty(_) => {
                    warn!("timeout `{}` has default, but no timer set", name,);
                    None
                }
                Time::Timer(..) => Some(dur),
            },
            Dur::Configured(Some(dur)) => match self {
                Time::Empty(_) => panic!("timeout `{name}` set, but no timer set",),
                Time::Timer(..) => Some(dur),
            },
            Dur::Default(None) | Dur::Configured(None) => None,
        }
    }
}
