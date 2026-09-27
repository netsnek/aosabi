// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise host: the host loop that runs an agent.
 *
 * Translation of the host side of ECMA-262 section 9.5,
 * https://tc39.es/ecma262/#sec-jobs, in the order node runs it: the promise
 * jobs drain to empty between two macrotasks,
 * https://nodejs.org/en/learn/asynchronous-work/event-loop-timers-and-nexttick.
 */

//! The host loop.
//!
//! ECMA-262 leaves to the host when a job runs. The host here is the loop of
//! [`Drive`], and its order is the one of node. It drains the job queue to
//! empty. It stops when the promise it drives is settled. Otherwise it polls
//! the first future a foreign waker flagged and starts over. Otherwise it
//! fires the timer with the least deadline as one macrotask and starts over,
//! so exactly one timer fires between two drains. Otherwise nothing is left
//! to run, and the loop parks when a foreign waker is outstanding and
//! reports [`Stalled`] when none is.
//!
//! A host that owns the queue of the agent, behind the feature
//! `host-queue`, runs the jobs in microtasks of its own. While it holds a
//! job of the queue the loop fires no timer, as node fires no timer while a
//! microtask waits, and while it owes the agent a job, a held settlement,
//! the jobs a reservation keeps or a promise the loop parks as it parks on a
//! foreign waker and never answers that nothing is left.
//!
//! On the virtual clock every timer is due, so the loop parks only on a
//! foreign waker. Under [`TimerMode::Host`] a timer is due once the clock of
//! the host has reached its deadline. The loop fires every due timer, one per
//! turn with a drain between two, and when the least deadline lies ahead and
//! nothing else can run it hands that deadline and its waker to
//! [`HostClock::wake_at`] and answers `Pending`, so a timer that waits is no
//! stall.
//!
//! [`Drive`] runs the loop until a promise settles. [`Idle`] runs the same
//! loop until nothing is left, which is how a served agent fires the timers
//! a resolver set after its response is written.
//!
//! # The rules
//!
//! The plan of the timers states seven rules. This module implements rules 2, 6
//! and 7, and [󰀀](crate::promise::timers) the other four.
//!
//! 2. When nothing is runnable and a timer is pending, the loop calls
//!    [`HostClock::wake_at`] with the least deadline and returns `Pending`. A
//!    pending timer is outstanding work, so the loop never answers [`Stalled`]
//!    while one exists.
//! 6. [`Agent::run`] stays the host of a virtual agent, and a host mode agent
//!    is driven through [`Agent::drive`] or [`Agent::drain_until_idle`] on a
//!    runtime. An interval on a virtual agent fires forever when nothing else
//!    runs, and the one interval of the stream processor runs on served
//!    agents alone, so the rule costs no check.
//! 7. The free [`set_timeout`] keeps the argument order of
//!    `setTimeout(callback, delay)`, because it is the form an author writes.
//!    Its callback runs on the agent that set it, after the request that set
//!    it may have answered, which is what the event loop of node does with a
//!    timer a resolver sets, and the serve layer keeps that agent until it is
//!    idle. Below HTTP, [`Agent::run`] of a virtual agent fires the timer
//!    when nothing else is runnable, so the request stream sees it too.
//!
//! [`TimerMode::Host`]: crate::promise::TimerMode::Host
//! [`HostClock::wake_at`]: crate::promise::agent::HostClock::wake_at

use alloc::boxed::Box;
use core::{
    fmt,
    future::{
        Future,
        IntoFuture, //
    },
    pin::Pin,
    task::{
        Context,
        Poll,
        Waker, //
    }, //
};

use crate::{
    promise::{
        Agent,
        Promise,
        PromiseFuture,
        PromiseState,
        timers::TimerId, //
    },
    value::Value, //
};

/// The error of a loop that has nothing left to run while its promise is pending.
///
/// The job queue is empty, no timer is set and no foreign waker is
/// outstanding, so nothing can ever settle the promise.
#[derive(Debug)]
pub struct Stalled;

impl fmt::Display for Stalled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the agent has nothing left to run and the promise is pending")
    }
}

impl core::error::Error for Stalled {}

/// The future that runs an agent until a promise settles.
///
/// The output is the settled state, or [`Stalled`]. An outer runtime polls
/// it like any future, and it is woken when a foreign waker flags a future
/// of the agent.
pub struct Drive {
    /// The agent to run.
    agent: Agent,
    /// The promise to run it for.
    until: Promise,
}

impl Future for Drive {
    type Output = Result<PromiseState, Stalled>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        this.agent
            .enter(|| this.agent.turn(&this.until, context.waker()))
    }
}

/// The future that runs an agent until it is idle.
///
/// An agent is idle when no job, no due or pending timer, no woken future
/// and no held waker is left. A live interval keeps it busy until it is
/// cleared, and a timer that waits on the clock of the host parks the future
/// until the clock wakes it (rule 2).
pub struct Idle {
    /// The agent to run.
    agent: Agent,
}

impl Future for Idle {
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        let poll = this
            .agent
            .enter(|| this.agent.run_until(context.waker(), || None::<()>));
        poll.map(|_| ())
    }
}

impl Agent {
    /// Returns the future that runs this agent until `until` settles.
    #[must_use]
    pub fn drive(&self, until: &Promise) -> Drive {
        Drive {
            agent: self.clone(),
            until: until.clone(),
        }
    }

    /// Returns the future that runs this agent until it is idle.
    ///
    /// A host runs it after the answer of a request is written, so a timer a
    /// resolver set fires as the event loop of node fires it (rule 7), and a
    /// stream driver runs it after its stream has ended.
    #[must_use]
    pub fn drain_until_idle(&self) -> Idle {
        Idle {
            agent: self.clone(),
        }
    }

    /// Runs this agent on the calling thread until `until` settles.
    ///
    /// This repeats the poll of [`Agent::drive`] with a waker that does
    /// nothing, which is the whole host of a request that awaits no foreign
    /// future. It is the host of a virtual agent (rule 6). On a host mode
    /// agent whose promise waits on a timer it spins until the clock reaches
    /// the deadline. An agent whose queue a host owns, behind the feature
    /// `host-queue`, is driven by that host through [`Agent::drive`], since
    /// this loop spins for as long as the host owes it a job.
    ///
    /// # Errors
    ///
    /// Returns [`Stalled`] when nothing is left to run and `until` is pending.
    pub fn run(&self, until: &Promise) -> Result<PromiseState, Stalled> {
        let mut drive = self.drive(until);
        let mut context = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(result) = Pin::new(&mut drive).poll(&mut context) {
                return result;
            }
            core::hint::spin_loop();
        }
    }

    /// Runs the host loop until `until` settles, the agent stalls or it has to park.
    fn turn(&self, until: &Promise, outer: &Waker) -> Poll<Result<PromiseState, Stalled>> {
        let settled = || match until.state() {
            PromiseState::Pending => None,
            state => Some(state),
        };
        self.run_until(outer, settled)
            .map(|state| state.ok_or(Stalled))
    }

    /// Runs the host loop until `done` answers, nothing is left to run or it has to park.
    ///
    /// The loop asks `done` after every drain of the job queue. The answer
    /// is `Ready(Some)` with what `done` answered, `Ready(None)` when no job, no
    /// timer, no woken future and no held waker is left, and `Pending` when
    /// the loop parks. The held waker is read before the flagged futures are.
    /// A wake that lands between the two reads releases its waker and flags
    /// its future, and this order sees the flag. A timer that waits on the
    /// clock of the host keeps the loop from ending (rule 2), and its deadline
    /// goes to the clock with the waker of the outer runtime before the loop
    /// parks.
    ///
    /// With the feature `host-queue`, a job of the queue a host owns keeps the
    /// loop from firing a timer, as node fires no timer while a microtask waits.
    /// The jobs, the held settlements, the jobs a reservation keeps and the
    /// promises the host owes count as outstanding work, read before the waker
    /// of any task, so the loop parks and never answers that nothing is left
    /// while the host owes it one of them, and walks no task while it does.
    fn run_until<T>(&self, outer: &Waker, mut done: impl FnMut() -> Option<T>) -> Poll<Option<T>> {
        loop {
            self.drain_jobs();
            if let Some(answer) = done() {
                return Poll::Ready(Some(answer));
            }
            #[cfg(not(feature = "host-queue"))]
            if self.poll_woken_task() || self.fire_next_timer() {
                continue;
            }
            #[cfg(feature = "host-queue")]
            if self.poll_woken_task() || (!self.host_holds_jobs() && self.fire_next_timer()) {
                continue;
            }
            let timer = self.next_host_deadline();
            #[cfg(not(feature = "host-queue"))]
            let held = self.0.tasks.borrow().has_outstanding_waker();
            #[cfg(feature = "host-queue")]
            let held = self.host_owes_work() || self.0.tasks.borrow().has_outstanding_waker();
            if self.0.wake_set.has_woken() {
                continue;
            }
            if !held && timer.is_none() {
                return Poll::Ready(None);
            }
            if !self.0.wake_set.park(outer) {
                continue;
            }
            if let Some((deadline, clock)) = timer {
                clock.wake_at(deadline, outer.clone());
            }
            return Poll::Pending;
        }
    }
}

/// Awaits a timer of `delay` units of the current agent.
///
/// This is `await new Promise(resolve => setTimeout(resolve, delay))`. The
/// timer is made when this function is called, through [`Agent::current`]
/// and [`Agent::sleep`]. Outside of an agent the returned future rejects.
#[must_use]
pub fn sleep(delay: u64) -> PromiseFuture {
    match Agent::current() {
        Some(agent) => agent.sleep(delay).into_future(),
        None => PromiseFuture::detached(),
    }
}

/// Awaits one job of the current agent, which is `await Promise.resolve()`.
///
/// Outside of an agent the returned future rejects.
#[must_use]
pub fn tick() -> PromiseFuture {
    match Agent::current() {
        Some(agent) => Promise::resolve(&agent, Value::Undefined).into_future(),
        None => PromiseFuture::detached(),
    }
}

/// Runs `callback` after `delay` milliseconds on the current agent, `setTimeout(callback, delay)`.
///
/// The timer is made when this function is called, through
/// [`Agent::current`] and [`Agent::set_timeout`], and the callback runs as a
/// macrotask of that agent after `delay` timer units and outside every
/// resolver, with no ambient state of a resolver in place (rule 7). The
/// answer is the id of the timer, which [`Agent::clear_timer`] takes. Outside
/// of an agent the function does nothing and answers `None`.
pub fn set_timeout(callback: impl FnOnce() + 'static, delay: u64) -> Option<TimerId> {
    let agent = Agent::current()?;
    Some(agent.set_timeout(delay, Box::new(move |_| callback())))
}
