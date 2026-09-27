// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise timers: the timer wheel of an agent, on its virtual
 * clock or on the clock of its host.
 *
 * No clause of ECMA-262. The rules are the ones of the timers of node,
 * https://nodejs.org/api/timers.html#settimeoutcallback-delay-args and
 * https://nodejs.org/api/timers.html#setintervalcallback-delay-args, over a
 * clock the agent controls or the clock of its host.
 */

//! The timer wheel.
//!
//! A timer of a behaviour table and the `sleep` of a twin stand for a
//! `setTimeout` of the reference side. In process, real time would make the
//! order of a response depend on the load of the machine, so the wheel of
//! [`TimerMode::Virtual`] keeps a clock of its own. That clock starts at 0 for
//! every agent, it moves only when a timer fires, and it jumps to the deadline
//! of the timer that fires.
//!
//! A served request runs under [`TimerMode::Host`], where a timer fires once
//! the clock of the host has reached its deadline, so a twin waits as long as
//! its fixture does, and the ping of a stream is an interval of that clock.
//! The order stays the one of node in both modes: the least deadline first,
//! and two equal deadlines in creation order. The module reads no clock of
//! the operating system, only the [`HostClock`] the host hands over, and it
//! names `core` and `alloc` and no item of `std`.
//!
//! # The rules
//!
//! The plan of the timers states seven rules. This module implements rules 1,
//! 3, 4 and 5, and [󰀀](crate::promise::host) the other three.
//!
//! 1. In host mode the time of the agent is `now_ms()` of the clock minus
//!    the reading taken when [`Agent::set_host_clock`] installed it, a
//!    deadline is that time plus `max(delay, 1)`, and a timer fires only when
//!    its deadline is at or before the time, least `(deadline, sequence)`
//!    first, one timer per macrotask with a drain of the job queue between
//!    two, as in the virtual mode.
//! 3. An interval reschedules itself at the time its callback runs plus its
//!    period, before the callback runs, and [`Agent::clear_timer`] inside the
//!    callback stops it.
//! 4. A cleared timer stays in the heap and is skipped when it is popped.
//!
//! [`TimerMode::Virtual`]: crate::promise::TimerMode::Virtual
//! [`TimerMode::Host`]: crate::promise::TimerMode::Host
//! [`HostClock`]: crate::promise::agent::HostClock

use alloc::{
    boxed::Box,
    collections::BinaryHeap,
    rc::Rc,
    vec::Vec, //
};
use core::cmp::Ordering;

use crate::{
    promise::{
        Agent,
        Promise,
        agent::HostTime, //
    },
    value::Value, //
};

/// The id of a timer, which [`Agent::clear_timer`] takes.
///
/// It is the sequence of the first record of the timer, so the ids of an
/// agent grow in the order its timers were made and are never used twice.
/// An interval keeps its id across every reschedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerId(u64);

/// What a timer runs.
enum TimerCallback {
    /// The callback of a timeout, which runs once.
    Once(Box<dyn FnOnce(&Agent)>),
    /// The callback of an interval and its period, which is at least 1.
    Every(Rc<dyn Fn(&Agent)>, u64),
}

/// The callback of a timer that is due, taken out of the wheel.
enum Due {
    /// A timeout, which is gone from the wheel.
    Once(Box<dyn FnOnce(&Agent)>),
    /// An interval, whose next record is in the wheel already.
    Every(Rc<dyn Fn(&Agent)>),
}

/// The timers of one agent and its clock.
pub(super) struct Wheel {
    /// The virtual time in timer units.
    now: u64,
    /// The count of records made so far, which orders two equal deadlines.
    sequence: u64,
    /// The records that have not been popped.
    records: BinaryHeap<TimerRecord>,
    /// The ids of the timers that are neither fired nor cleared, ascending.
    ///
    /// A timer is live while its id is here. Every live timer has exactly
    /// one record in the heap, and a record whose id is not here is a
    /// cleared one that waits to be popped (rule 4).
    live: Vec<u64>,
}

/// One record of the heap.
struct TimerRecord {
    /// The time the record fires at.
    deadline: u64,
    /// The creation order of the record.
    sequence: u64,
    /// The timer the record belongs to.
    id: u64,
    /// What the timer runs.
    callback: TimerCallback,
}

impl TimerRecord {
    /// Returns the key that orders the wheel.
    fn key(&self) -> (u64, u64) {
        (self.deadline, self.sequence)
    }
}

impl PartialEq for TimerRecord {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for TimerRecord {}

impl PartialOrd for TimerRecord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimerRecord {
    /// Orders the record with the least key as the greatest, because the
    /// heap hands out its greatest item.
    fn cmp(&self, other: &Self) -> Ordering {
        other.key().cmp(&self.key())
    }
}

impl Wheel {
    /// Creates an empty wheel with its clock at 0.
    pub(super) fn new() -> Self {
        Self {
            now: 0,
            sequence: 0,
            records: BinaryHeap::new(),
            live: Vec::new(),
        }
    }

    /// Adds a record of the timer `id` at `deadline`.
    fn push(&mut self, id: u64, deadline: u64, callback: TimerCallback) {
        let sequence = self.sequence;
        self.sequence += 1;
        self.records.push(TimerRecord {
            deadline,
            sequence,
            id,
            callback,
        });
    }

    /// Makes a live timer at `deadline` and returns its id.
    ///
    /// The id is the sequence of the first record, which is greater than
    /// every id before it, so the list of live ids stays ascending.
    fn schedule(&mut self, deadline: u64, callback: TimerCallback) -> TimerId {
        let id = self.sequence;
        self.push(id, deadline, callback);
        self.live.push(id);
        TimerId(id)
    }

    /// Returns whether the timer `id` is neither fired nor cleared.
    fn is_live(&self, id: u64) -> bool {
        self.live.binary_search(&id).is_ok()
    }

    /// Ends the timer `id`, which does nothing when it has ended already.
    fn release(&mut self, id: u64) {
        if let Ok(i) = self.live.binary_search(&id) {
            self.live.remove(i);
        }
    }

    /// Pops the cleared records off the top of the heap (rule 4).
    fn skip_cleared(&mut self) {
        while let Some(record) = self.records.peek() {
            if self.is_live(record.id) {
                return;
            }
            self.records.pop();
        }
    }

    /// Returns the least deadline of a live timer.
    fn next_deadline(&mut self) -> Option<u64> {
        self.skip_cleared();
        Some(self.records.peek()?.deadline)
    }

    /// Takes the callback of `record`, which is due at `now`.
    ///
    /// A timeout ends here. An interval gets its next record at `now` plus
    /// its period before its callback runs (rule 3), so a
    /// [`Agent::clear_timer`] inside the callback ends the timer that record
    /// belongs to.
    fn take(&mut self, record: TimerRecord, now: u64) -> Due {
        match record.callback {
            TimerCallback::Once(callback) => {
                self.release(record.id);
                Due::Once(callback)
            }
            TimerCallback::Every(callback, period) => {
                let deadline = now.saturating_add(period);
                let next = TimerCallback::Every(Rc::clone(&callback), period);
                self.push(record.id, deadline, next);
                Due::Every(callback)
            }
        }
    }
}

impl Agent {
    /// Returns the time of the agent in timer units.
    ///
    /// That is the milliseconds since the clock of the host was installed
    /// under [`TimerMode::Host`](crate::promise::TimerMode::Host) with a
    /// clock (rule 1), and the virtual time otherwise.
    #[must_use]
    pub fn now(&self) -> u64 {
        match self.host_clock() {
            Some(host) => host.now(),
            None => self.0.wheel.borrow().now,
        }
    }

    /// Schedules `callback` to run as one macrotask after `delay` timer units.
    ///
    /// The deadline is the current time plus `delay`, and a delay below 1
    /// counts as 1, which is the rule of node. A delay of 0 and a delay of 1
    /// therefore share a deadline and fire in creation order. The current time
    /// is the one of [`Agent::now`], the time since the host clock was
    /// installed under [󰀂](crate::promise::TimerMode::Host) and the virtual
    /// time otherwise. The other half of that rule, a delay above 2147483647,
    /// is not reproduced, because no timer of a behaviour table or of a
    /// fixture comes near it. The returned id clears the timer before it fires
    /// (rule 5).
    // A caller that never clears the timer writes the call as a statement,
    // so the id is no `must_use` value.
    #[expect(clippy::must_use_candidate)]
    pub fn set_timeout(&self, delay: u64, callback: Box<dyn FnOnce(&Agent)>) -> TimerId {
        let deadline = self.now().saturating_add(delay.max(1));
        self.0
            .wheel
            .borrow_mut()
            .schedule(deadline, TimerCallback::Once(callback))
    }

    /// Schedules `callback` to run as one macrotask every `period` timer units.
    ///
    /// The first deadline is the current time plus the period, and a period
    /// below 1 counts as 1, as for [`Agent::set_timeout`]. Each run
    /// reschedules the interval before the callback runs (rule 3), and the
    /// interval runs until [`Agent::clear_timer`] ends it. On the virtual
    /// clock nothing else ends it, so an agent with a live interval never runs
    /// out of work there (rule 6 of [󰀂](crate::promise::host)).
    #[must_use = "the id is the one way to end the interval"]
    pub fn set_interval(&self, period: u64, callback: Rc<dyn Fn(&Agent)>) -> TimerId {
        let period = period.max(1);
        let deadline = self.now().saturating_add(period);
        self.0
            .wheel
            .borrow_mut()
            .schedule(deadline, TimerCallback::Every(callback, period))
    }

    /// Ends the timer `id`, a timeout or an interval.
    ///
    /// The record stays in the heap and is skipped when it is popped (rule 4).
    /// A timer that fired or was cleared already is left as it is, which is
    /// what `clearTimeout` of node does with such a timer.
    pub fn clear_timer(&self, id: TimerId) {
        self.0.wheel.borrow_mut().release(id.0);
    }

    /// Returns a promise that fulfils with `undefined` when a timer of `delay` units fires.
    ///
    /// This is `new Promise(resolve => setTimeout(resolve, delay))`. The
    /// timer is made now, and the promise fulfils inside the macrotask of
    /// the timer, so a reaction of it runs in the drain that follows.
    #[must_use]
    pub fn sleep(&self, delay: u64) -> Promise {
        let (promise, resolving_functions) = Promise::with_resolvers(self);
        self.set_timeout(
            delay,
            Box::new(move |agent| resolving_functions.resolve(agent, Value::Undefined)),
        );
        promise
    }

    /// Fires the live timer with the least deadline when it is due and returns whether one fired.
    ///
    /// Two timers of one deadline fire in creation order. On the virtual
    /// clock the least timer is always due and the clock jumps to its
    /// deadline before the callback runs. On the host clock a timer is due
    /// once the time of rule 1 has reached its deadline, and the host clock
    /// moves on its own. The borrow of the wheel ends before the callback
    /// runs, because the callback may set or clear a timer.
    pub(super) fn fire_next_timer(&self) -> bool {
        match self.take_next_timer() {
            Some(Due::Once(callback)) => callback(self),
            Some(Due::Every(callback)) => callback(self),
            None => return false,
        }
        true
    }

    /// Returns the least deadline of a live timer and the clock of the host, when a timer
    /// waits on the clock of the host.
    ///
    /// The host loop hands this deadline to the clock before it answers
    /// `Pending` (rule 2), so the agent is polled again when the timer is
    /// due. A deadline the clock has reached since the loop last looked is
    /// handed over too, and the host wakes the loop at its next turn.
    pub(super) fn next_host_deadline(&self) -> Option<(u64, HostTime)> {
        let host = self.host_clock()?;
        let deadline = self.0.wheel.borrow_mut().next_deadline()?;
        Some((deadline, host))
    }

    /// Takes the next due live timer out of the wheel.
    ///
    /// On the virtual clock the clock moves to the deadline of the timer,
    /// which is also the time an interval reschedules from.
    fn take_next_timer(&self) -> Option<Due> {
        let host_now = self.host_clock().map(|host| host.now());
        let mut wheel = self.0.wheel.borrow_mut();
        let deadline = wheel.next_deadline()?;
        if host_now.is_some_and(|now| deadline > now) {
            return None;
        }
        let record = wheel.records.pop()?;
        let now = host_now.unwrap_or(record.deadline);
        if host_now.is_none() {
            wheel.now = record.deadline;
        }
        Some(wheel.take(record, now))
    }
}
