// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the clock of the runtime and its heap of deadlines.
 *
 * No analogue on the reference side.
 */

//! `RuntimeClock`.
//!
//! A timer is a heap entry of a deadline and the slot of a waker, popped when
//! the clock passes it. A sleep dropped before its deadline empties its slot,
//! and the heap drops every such stale entry once they make up half of it and
//! number at least `STALE_AT_LEAST`, so a connection that moves its deadline
//! with each request holds the heap to at most twice the deadlines that still
//! wait or to those plus 63, whichever is larger. The heap is shared as
//! `Arc<Mutex<..>>`, because hyper bounds a timer and a sleep by
//! `Send + Sync`, and one thread never contends it. The clock is the
//! `HostClock` of the executor's agents, so a served agent runs in
//! `TimerMode::Host` on it and `sleep(5)` of a twin waits five milliseconds.

use core::{
    cmp::Ordering,
    task::Waker, //
};
use std::{
    collections::BinaryHeap,
    sync::{
        Arc,
        Mutex,
        MutexGuard,
        PoisonError, //
    },
    time::{
        Duration,
        Instant, //
    },
};

use aosabi_core::execute::promise::agent::HostClock;

/// The slot a sleep and the heap share, which holds the waker to wake.
pub type WakerSlot = Arc<Mutex<Option<Waker>>>;

/// One deadline of the heap.
struct Entry {
    /// When the entry fires.
    deadline: Instant,
    /// The order of creation, so two entries of one deadline fire in order.
    sequence: u64,
    /// The waker to wake.
    slot: WakerSlot,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    /// The least deadline is the greatest entry, so the max heap of the
    /// standard library pops it first.
    fn cmp(&self, other: &Self) -> Ordering {
        (other.deadline, other.sequence).cmp(&(self.deadline, self.sequence))
    }
}

/// The stale entries the heap holds at least before it drops them.
const STALE_AT_LEAST: usize = 64;

/// The heap of deadlines of one runtime.
#[derive(Default)]
pub struct TimerHeap {
    /// The entries, the least deadline on top.
    entries: BinaryHeap<Entry>,
    /// The next order of creation.
    sequence: u64,
    /// The entries whose sleep was dropped before their deadline, each with
    /// an empty slot.
    stale: usize,
}

impl TimerHeap {
    /// Adds the deadline `deadline` for the waker in `slot`.
    pub fn push(&mut self, deadline: Instant, slot: WakerSlot) {
        let sequence = self.sequence;
        self.sequence += 1;
        self.entries.push(Entry {
            deadline,
            sequence,
            slot,
        });
    }

    /// Counts the entry of a sleep dropped before its deadline, whose slot
    /// the sleep has emptied, and drops every stale entry once they make up
    /// half of the heap and number at least `STALE_AT_LEAST`, which keeps
    /// the work of a drop constant on average.
    pub fn forget(&mut self) {
        self.stale += 1;
        if self.stale >= STALE_AT_LEAST && self.stale * 2 >= self.entries.len() {
            self.entries.retain(|entry| lock(&entry.slot).is_some());
            self.stale = 0;
        }
    }

    /// Returns the least deadline.
    #[must_use]
    pub fn next(&self) -> Option<Instant> {
        self.entries.peek().map(|entry| entry.deadline)
    }

    /// Takes the wakers of every entry whose deadline is at or before `now`.
    pub fn take_due(&mut self, now: Instant) -> Vec<Waker> {
        let mut due = Vec::new();
        while self
            .entries
            .peek()
            .is_some_and(|entry| entry.deadline <= now)
        {
            let Some(entry) = self.entries.pop() else {
                break;
            };
            match lock(&entry.slot).take() {
                Some(waker) => due.push(waker),
                None => self.stale = self.stale.saturating_sub(1),
            }
        }
        due
    }
}

/// The clock of a runtime, its start and its heap of deadlines.
pub struct RuntimeClock {
    /// The instant the clock reads as 0.
    start: Instant,
    /// The deadlines of the runtime.
    heap: Arc<Mutex<TimerHeap>>,
}

impl Default for RuntimeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeClock {
    /// Returns a clock that starts now with no deadline.
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            heap: Arc::new(Mutex::new(TimerHeap::default())),
        }
    }

    /// Returns the instant the clock reads as 0.
    #[must_use]
    pub fn start(&self) -> Instant {
        self.start
    }

    /// Returns the heap of deadlines, which hyper's timer shares.
    #[must_use]
    pub fn heap(&self) -> Arc<Mutex<TimerHeap>> {
        Arc::clone(&self.heap)
    }

    /// Returns the milliseconds since the start.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        millis(self.start.elapsed())
    }

    /// Returns the least deadline in milliseconds since the start, rounded up.
    #[must_use]
    pub fn next_deadline_ms(&self) -> Option<u64> {
        let next = lock(&self.heap).next()?;
        let since = next.saturating_duration_since(self.start);
        let ms = millis(since);
        Some(if since > Duration::from_millis(ms) {
            ms + 1
        } else {
            ms
        })
    }

    /// Wakes the waker of every deadline at or before `now`.
    ///
    /// The wakers are woken after the heap is unlocked, because a woken task
    /// may add a deadline.
    pub fn fire_due(&self, now: Instant) {
        let due = lock(&self.heap).take_due(now);
        due.into_iter().for_each(Waker::wake);
    }
}

impl HostClock for RuntimeClock {
    fn now_ms(&self) -> u64 {
        Self::now_ms(self)
    }

    fn wake_at(&self, deadline_ms: u64, waker: Waker) {
        let deadline = self.start + Duration::from_millis(deadline_ms);
        lock(&self.heap).push(deadline, Arc::new(Mutex::new(Some(waker))));
    }
}

/// Locks `mutex`, which one thread never poisons for another.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Returns the whole milliseconds of `duration`, saturated at `u64::MAX`.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
