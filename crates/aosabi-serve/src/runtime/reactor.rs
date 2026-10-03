// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the reactor trait and the poll reactor.
 *
 * No analogue on the reference side.
 */

//! `Reactor` and `PollReactor`.
//!
//! A reactor tells the executor when a socket may have become ready. Without an
//! outside crate and without `unsafe` a socket is learnt ready only by trying
//! it, so the poll reactor has no readiness call: a socket that answers
//! `WouldBlock` registers its waker and reports every byte it moves, and the
//! reactor wakes its sources on a schedule.
//!
//! A source that moved a byte within [`HOT_WINDOW_US`] is hot and is woken on
//! every turn. While a source is hot and a turn moved nothing, the reactor
//! yields the thread for [`YIELD_TURNS`] turns and then sleeps from
//! [`BACKOFF_FLOOR_US`] up to [`BACKOFF_CEIL_US`], doubling, never past the next
//! cold sweep or deadline. Every other source is cold, and the cold sources
//! are woken together at an interval of [`SWEEP_SHARE`] times the cost of the
//! last cold sweep, clamped to [`COLD_FLOOR_US`] and [`COLD_CEIL_US`], which
//! holds the idle sweeps near one percent of a core. With no hot source the
//! reactor sleeps until the next cold sweep or deadline. A turn whose deadline
//! is due, the turn of an executor that holds a woken task, wakes the hot
//! sources and a cold sweep that is due with no yield and no sleep. The
//! constants are the design's, and `35_http_budget` prints the curve for
//! `SWEEP_SHARE` at 99, 49 and 19, so a change of them is a measured change.

use core::{
    cell::RefCell,
    task::Waker,
    time::Duration, //
};
use std::{
    io,
    os::fd::BorrowedFd,
    thread,
    time::Instant, //
};

/// How long a source stays hot after it moved a byte, in microseconds.
pub const HOT_WINDOW_US: u64 = 2_000;

/// How many turns that moved nothing the reactor yields before it sleeps.
pub const YIELD_TURNS: u32 = 16;

/// The first sleep of a hot turn that moved nothing, in microseconds.
pub const BACKOFF_FLOOR_US: u64 = 25;

/// The longest sleep of a hot turn that moved nothing, in microseconds.
pub const BACKOFF_CEIL_US: u64 = 800;

/// The interval of the cold sweeps as a multiple of the cost of the last one.
pub const SWEEP_SHARE: u32 = 99;

/// The shortest interval of the cold sweeps, in microseconds.
pub const COLD_FLOOR_US: u64 = 250;

/// The longest interval of the cold sweeps, in microseconds.
pub const COLD_CEIL_US: u64 = 50_000;

/// A source's number in a reactor, public so that a reactor outside this
/// file, the epoll reactor, makes one in `add`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SourceId(pub u32);

/// What a task waits for on a source.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Interest {
    /// A byte to read, or a connection to accept.
    Read,
    /// Room to write.
    Write,
}

impl Interest {
    /// Returns the index of the interest in a pair of wakers.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Read => 0,
            Self::Write => 1,
        }
    }
}

/// The reactor of a runtime.
pub trait Reactor {
    /// Adds the socket `fd` and returns its number.
    ///
    /// # Errors
    ///
    /// Returns the error of the kernel, which the poll reactor never answers.
    fn add(&self, fd: BorrowedFd<'_>) -> io::Result<SourceId>;

    /// Removes the source `id` of the socket `fd`.
    fn remove(&self, fd: BorrowedFd<'_>, id: SourceId);

    /// Wakes `waker` once the source `id` may be ready for `interest`.
    fn register(&self, id: SourceId, interest: Interest, waker: &Waker);

    /// A byte moved on the source, which the poll reactor reads as heat.
    fn progressed(&self, id: SourceId);

    /// Blocks until a source may be ready or `deadline_ms` passes, and wakes
    /// the registered tasks.
    ///
    /// The times `now_ms` and `deadline_ms` are milliseconds of the clock of
    /// the runtime, and `None` waits with no deadline. A deadline at or before
    /// `now_ms` does not block and wakes the tasks of the sources that may be
    /// ready now, which the executor asks for on a turn that leaves a task
    /// woken.
    ///
    /// # Errors
    ///
    /// Returns the error of the kernel, which the poll reactor never answers.
    fn wait(&self, now_ms: u64, deadline_ms: Option<u64>) -> io::Result<()>;
}

/// One source of the poll reactor.
#[derive(Default)]
struct PollSource {
    /// When the source last moved a byte.
    moved: Option<Instant>,
    /// The wakers of the two interests.
    wakers: [Option<Waker>; 2],
}

/// The state of the poll reactor.
struct PollState {
    /// The sources by number, `None` for a free slot.
    sources: Vec<Option<PollSource>>,
    /// The free slots.
    free: Vec<u32>,
    /// Whether a byte moved since the last wait.
    moved: bool,
    /// The hot turns in a row that moved nothing.
    idle_turns: u32,
    /// The next sleep of a hot turn that moved nothing.
    backoff: Duration,
    /// When the cold sources are woken next.
    next_sweep: Instant,
    /// When the last cold sweep woke its sources, until the next wait reads
    /// what it cost.
    sweep_started: Option<Instant>,
    /// The interval of the cold sweeps.
    interval: Duration,
}

/// The reactor of design B', which polls with no crate.
pub struct PollReactor {
    /// The sources and the schedule.
    state: RefCell<PollState>,
}

impl Default for PollReactor {
    fn default() -> Self {
        Self::new()
    }
}

impl PollReactor {
    /// Returns a poll reactor with no source.
    #[must_use]
    pub fn new() -> Self {
        let interval = Duration::from_micros(COLD_FLOOR_US);
        Self {
            state: RefCell::new(PollState {
                sources: Vec::new(),
                free: Vec::new(),
                moved: false,
                idle_turns: 0,
                backoff: Duration::from_micros(BACKOFF_FLOOR_US),
                next_sweep: Instant::now() + interval,
                sweep_started: None,
                interval,
            }),
        }
    }
}

impl Reactor for PollReactor {
    fn add(&self, _fd: BorrowedFd<'_>) -> io::Result<SourceId> {
        let mut state = self.state.borrow_mut();
        let source = PollSource::default();
        if let Some(id) = state.free.pop() {
            state.sources[id as usize] = Some(source);
            return Ok(SourceId(id));
        }
        let id = u32::try_from(state.sources.len())
            .map_err(|_| io::Error::other("the poll reactor holds too many sources"))?;
        state.sources.push(Some(source));
        Ok(SourceId(id))
    }

    fn remove(&self, _fd: BorrowedFd<'_>, id: SourceId) {
        let mut state = self.state.borrow_mut();
        if let Some(slot) = state.sources.get_mut(id.0 as usize)
            && slot.take().is_some()
        {
            state.free.push(id.0);
        }
    }

    fn register(&self, id: SourceId, interest: Interest, waker: &Waker) {
        let mut state = self.state.borrow_mut();
        if let Some(Some(source)) = state.sources.get_mut(id.0 as usize) {
            let slot = &mut source.wakers[interest.index()];
            match slot {
                Some(held) if held.will_wake(waker) => {}
                _ => *slot = Some(waker.clone()),
            }
        }
    }

    fn progressed(&self, id: SourceId) {
        let mut state = self.state.borrow_mut();
        state.moved = true;
        if let Some(Some(source)) = state.sources.get_mut(id.0 as usize) {
            source.moved = Some(Instant::now());
        }
    }

    fn wait(&self, now_ms: u64, deadline_ms: Option<u64>) -> io::Result<()> {
        let now = Instant::now();
        let limit = {
            let mut state = self.state.borrow_mut();
            state.measure_sweep(now);
            let deadline = deadline_ms
                .map(|deadline| now + Duration::from_millis(deadline.saturating_sub(now_ms)));
            deadline.map_or(state.next_sweep, |deadline| deadline.min(state.next_sweep))
        };
        let hot = self.state.borrow_mut().take_hot(now);
        let due = deadline_ms.is_some_and(|deadline| deadline <= now_ms);
        if due {
            hot.into_iter().for_each(Waker::wake);
        } else if hot.is_empty() {
            sleep_until(limit);
        } else {
            let pause = self.state.borrow_mut().hot_pause();
            match pause {
                None => thread::yield_now(),
                Some(pause) => sleep_until(limit.min(Instant::now() + pause)),
            }
            hot.into_iter().for_each(Waker::wake);
        }
        let cold = self.state.borrow_mut().take_cold(Instant::now());
        cold.into_iter().for_each(Waker::wake);
        Ok(())
    }
}

impl PollState {
    /// Reads the cost of the last cold sweep, the time from its wakes to this
    /// wait, and sets the interval of the next one.
    fn measure_sweep(&mut self, now: Instant) {
        let Some(started) = self.sweep_started.take() else {
            return;
        };
        let cost = now.saturating_duration_since(started);
        self.interval = (cost * SWEEP_SHARE).clamp(
            Duration::from_micros(COLD_FLOOR_US),
            Duration::from_micros(COLD_CEIL_US),
        );
        self.next_sweep = started + self.interval;
    }

    /// Takes the wakers of the hot sources.
    fn take_hot(&mut self, now: Instant) -> Vec<Waker> {
        let window = Duration::from_micros(HOT_WINDOW_US);
        let mut hot = Vec::new();
        for source in self.sources.iter_mut().flatten() {
            let is_hot = source
                .moved
                .is_some_and(|moved| now.saturating_duration_since(moved) < window);
            if is_hot {
                hot.extend(source.wakers.iter_mut().filter_map(Option::take));
            }
        }
        hot
    }

    /// Returns how long a hot turn waits, `None` for a yield.
    ///
    /// A turn after one that moved a byte starts over with a yield, and the
    /// sleep doubles from the floor up to the ceiling once the yields are
    /// spent.
    fn hot_pause(&mut self) -> Option<Duration> {
        if core::mem::take(&mut self.moved) {
            self.idle_turns = 0;
            self.backoff = Duration::from_micros(BACKOFF_FLOOR_US);
        } else {
            self.idle_turns = self.idle_turns.saturating_add(1);
        }
        if self.idle_turns <= YIELD_TURNS {
            return None;
        }
        let pause = self.backoff;
        self.backoff = (self.backoff * 2).min(Duration::from_micros(BACKOFF_CEIL_US));
        Some(pause)
    }

    /// Takes the wakers of every source when the cold sweep is due.
    fn take_cold(&mut self, now: Instant) -> Vec<Waker> {
        if now < self.next_sweep {
            return Vec::new();
        }
        let cold: Vec<Waker> = self
            .sources
            .iter_mut()
            .flatten()
            .flat_map(|source| source.wakers.iter_mut().filter_map(Option::take))
            .collect();
        self.sweep_started = Some(now);
        self.next_sweep = now + self.interval;
        cold
    }
}

/// Sleeps on this thread until `limit`, and returns at once when it passed.
fn sleep_until(limit: Instant) {
    let now = Instant::now();
    if limit > now {
        thread::sleep(limit - now);
    }
}
