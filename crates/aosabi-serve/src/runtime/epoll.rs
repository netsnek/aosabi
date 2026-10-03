// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the epoll reactor of the feature linux, and the reactor of a
 * build.
 *
 * No analogue on the reference side.
 */

//! `EpollReactor` and `platform_reactor`.
//!
//! The epoll reactor adds a source once, edge triggered, for reading and for
//! writing, with its [󰀀](crate::runtime::reactor::SourceId) as the event data,
//! keeps the waker per source and interest, and waits in `epoll_wait` up to the
//! least deadline, at most `c_int::MAX` milliseconds a wait, so no processor time
//! is spent while nothing is ready, whatever the deadline. A deadline that is due,
//! the turn of an executor that holds a woken task, gives `epoll_wait` a timeout
//! of 0, which reads the sources ready now with no wait. A socket tries its read
//! or write before it registers, and one thread runs the loop, so an edge that
//! comes after a `WouldBlock` is read by the next wait and never lost. The calls
//! are the safe `rustix::event::epoll`.

use std::io;

#[cfg(all(feature = "linux", target_os = "linux"))]
pub use self::linux::EpollReactor;
use super::reactor::{
    PollReactor,
    Reactor, //
};

/// The reactor a build runs: `EpollReactor` under the feature `linux` on a
/// Linux target, and `PollReactor` on every other build.
///
/// # Errors
///
/// Returns the error of the kernel when it refuses an epoll instance.
pub fn platform_reactor() -> io::Result<Box<dyn Reactor>> {
    #[cfg(all(feature = "linux", target_os = "linux"))]
    {
        Ok(Box::new(EpollReactor::new()?))
    }
    #[cfg(not(all(feature = "linux", target_os = "linux")))]
    {
        Ok(Box::new(PollReactor::new()))
    }
}

/// Returns the poll reactor of design B', which every build can choose.
#[must_use]
pub fn poll_reactor() -> Box<dyn Reactor> {
    Box::new(PollReactor::new())
}

#[cfg(all(feature = "linux", target_os = "linux"))]
mod linux {
    use core::{
        cell::RefCell,
        task::Waker, //
    };
    use std::{
        io,
        os::fd::{
            BorrowedFd,
            OwnedFd, //
        },
    };

    use rustix::event::{
        Timespec,
        epoll::{
            self,
            CreateFlags,
            Event,
            EventData,
            EventFlags, //
        },
    };

    use crate::runtime::reactor::{
        Interest,
        Reactor,
        SourceId, //
    };

    /// The events one wait reads at most.
    const EVENTS: usize = 256;

    /// The longest wait in milliseconds, `c_int::MAX`, the most `epoll_wait`
    /// and `epoll_pwait` take.
    ///
    /// The crate names rustix without its feature `linux_5_11`, so rustix converts
    /// a longer timeout to a `c_int` of milliseconds and fails with `EINVAL` on
    /// its libc backend, which a mips target takes, and calls `epoll_pwait2` on
    /// its `linux_raw` backend, which a kernel before 5.11 lacks.
    const MAX_WAIT_MS: u64 = 2_147_483_647;

    /// The state of the epoll reactor.
    struct EpollState {
        /// The wakers of each source by number, `None` for a free slot.
        wakers: Vec<Option<[Option<Waker>; 2]>>,
        /// The free slots.
        free: Vec<u32>,
    }

    /// The reactor of design C, which waits in `epoll_wait`.
    pub struct EpollReactor {
        /// The epoll instance.
        epoll: OwnedFd,
        /// The wakers.
        state: RefCell<EpollState>,
    }

    impl EpollReactor {
        /// Returns an epoll reactor with no source.
        ///
        /// # Errors
        ///
        /// Returns the error of `epoll_create1`.
        pub fn new() -> io::Result<Self> {
            let epoll = epoll::create(CreateFlags::CLOEXEC)?;
            Ok(Self {
                epoll,
                state: RefCell::new(EpollState {
                    wakers: Vec::new(),
                    free: Vec::new(),
                }),
            })
        }

        /// Wakes the wakers the event `event` names.
        fn dispatch(&self, event: Event) {
            let flags = event.flags;
            let data = event.data;
            let Ok(index) = usize::try_from(data.u64()) else {
                return;
            };
            let failed = flags.intersects(EventFlags::ERR | EventFlags::HUP);
            let readable = failed || flags.intersects(EventFlags::IN | EventFlags::RDHUP);
            let writable = failed || flags.contains(EventFlags::OUT);
            let woken: Vec<Waker> = {
                let mut state = self.state.borrow_mut();
                let Some(Some(wakers)) = state.wakers.get_mut(index) else {
                    return;
                };
                let [read, write] = wakers;
                let mut woken = Vec::new();
                if readable {
                    woken.extend(read.take());
                }
                if writable {
                    woken.extend(write.take());
                }
                woken
            };
            woken.into_iter().for_each(Waker::wake);
        }
    }

    impl Reactor for EpollReactor {
        fn add(&self, fd: BorrowedFd<'_>) -> io::Result<SourceId> {
            let mut state = self.state.borrow_mut();
            let id = match state.free.pop() {
                Some(id) => id,
                None => u32::try_from(state.wakers.len())
                    .map_err(|_| io::Error::other("the epoll reactor holds too many sources"))?,
            };
            let flags = EventFlags::IN | EventFlags::OUT | EventFlags::RDHUP | EventFlags::ET;
            if let Err(error) =
                epoll::add(&self.epoll, fd, EventData::new_u64(u64::from(id)), flags)
            {
                state.free.push(id);
                return Err(error.into());
            }
            let slot = id as usize;
            if slot == state.wakers.len() {
                state.wakers.push(Some([None, None]));
            } else {
                state.wakers[slot] = Some([None, None]);
            }
            Ok(SourceId(id))
        }

        fn remove(&self, fd: BorrowedFd<'_>, id: SourceId) {
            // A descriptor that is closed already left the interest list, so
            // an error here changes nothing.
            let _ = epoll::delete(&self.epoll, fd);
            let mut state = self.state.borrow_mut();
            if let Some(slot) = state.wakers.get_mut(id.0 as usize)
                && slot.take().is_some()
            {
                state.free.push(id.0);
            }
        }

        fn register(&self, id: SourceId, interest: Interest, waker: &Waker) {
            let mut state = self.state.borrow_mut();
            if let Some(Some(wakers)) = state.wakers.get_mut(id.0 as usize) {
                let slot = &mut wakers[interest.index()];
                match slot {
                    Some(held) if held.will_wake(waker) => {}
                    _ => *slot = Some(waker.clone()),
                }
            }
        }

        fn progressed(&self, _id: SourceId) {}

        fn wait(&self, now_ms: u64, deadline_ms: Option<u64>) -> io::Result<()> {
            let timeout = deadline_ms.map(|deadline| {
                // A deadline beyond the longest wait wakes the loop early,
                // which then waits again up to the least deadline it reads
                // on its next turn.
                let ms = deadline.saturating_sub(now_ms).min(MAX_WAIT_MS);
                Timespec {
                    tv_sec: i64::try_from(ms / 1_000).unwrap_or(i64::MAX),
                    // The field is the 32 bit c_long of libc on the player's
                    // mipsel, and the value, at most 999,000,000, fits it.
                    tv_nsec: (ms % 1_000 * 1_000_000).try_into().unwrap_or(0),
                }
            });
            let mut events = [Event {
                flags: EventFlags::empty(),
                data: EventData::new_u64(0),
            }; EVENTS];
            let count = match epoll::wait(&self.epoll, &mut events[..], timeout.as_ref()) {
                Ok(count) => count,
                Err(rustix::io::Errno::INTR) => return Ok(()),
                Err(error) => return Err(error.into()),
            };
            for event in events.iter().take(count) {
                self.dispatch(*event);
            }
            Ok(())
        }
    }
}
