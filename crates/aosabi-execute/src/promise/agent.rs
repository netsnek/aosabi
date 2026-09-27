// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise agent: the agent of one request and its job queue.
 *
 * Translation of the job rules of ECMA-262 section 9.5,
 * https://tc39.es/ecma262/#sec-jobs, and of HostEnqueuePromiseJob,
 * https://tc39.es/ecma262/#sec-hostenqueuepromisejob.
 */

//! The agent and the job queue.
//!
//! An agent of ECMA-262 is one thread of execution with its job queues.
//! [`Agent`] is that for one request: the queue of promise jobs, the timer
//! wheel, the spawned futures, the ambient hooks and the trace. A clone is the
//! same agent.
//!
//! Every other build of the crate holds none of its fields and none of its
//! paths.

use alloc::{
    boxed::Box,
    collections::VecDeque,
    rc::Rc,
    sync::Arc,
    vec::Vec, //
};
use core::{
    cell::{
        Cell,
        RefCell, //
    },
    task::Waker, //
};

use crate::promise::{
    ambient::AmbientHooks,
    future::{
        Tasks,
        WakeSet, //
    },
    promise::{
        ReactionJob,
        ThenableJob, //
    },
    timers::Wheel,
    trace::TraceEvent, //
};

/// The agent of one request: its job queue, its wheel, its futures and its trace.
///
/// A clone shares the state. An agent belongs to one thread and is neither
/// `Send` nor `Sync`.
#[derive(Clone)]
pub struct Agent(pub(super) Rc<AgentInner>);

/// The state every clone of an [`Agent`] shares.
pub(super) struct AgentInner {
    /// The options the agent was made with.
    pub(super) options: AgentOptions,
    /// The queue of promise jobs, first in first out.
    // An `unsafe` cell would skip the flag.
    jobs: RefCell<VecDeque<Job>>,
    /// The count of jobs started so far.
    jobs_run: Cell<u64>,
    /// The timer wheel.
    // An `unsafe` cell would skip the flag.
    pub(super) wheel: RefCell<Wheel>,
    /// The spawned futures.
    pub(super) tasks: RefCell<Tasks>,
    /// The futures a foreign waker flagged, which another thread may write.
    pub(super) wake_set: Arc<WakeSet>,
    /// The hooks that carry ambient state across an await.
    pub(super) ambient_hooks: RefCell<Option<AmbientHooks>>,
    /// The recorded events.
    pub(super) trace: RefCell<Vec<TraceEvent>>,
    /// The clock of the host, which the timers read under [`TimerMode::Host`].
    pub(super) host_clock: RefCell<Option<HostTime>>,
    /// What a host that owns the microtask queue holds of the agent.
    #[cfg(feature = "host-queue")]
    host: HostQueue,
}

/// What a host that owns the microtask queue holds of an agent, behind the feature `host-queue`.
// An `unsafe` cell would skip each borrow flag.
#[cfg(feature = "host-queue")]
#[derive(Default)]
struct HostQueue {
    /// The callback of the host, told of every job, which [`Agent::set_enqueue_notifier`] sets.
    notifier: RefCell<Option<EnqueueNotifier>>,
    /// Whether the host owns the queue, so that a drain runs the jobs it granted alone.
    owned: Cell<bool>,
    /// The jobs the host granted the next drains, [`Agent::grant_jobs`].
    granted: Cell<usize>,
    /// Whether a settlement of the host runs, [`Agent::run_settlement`].
    settling: Cell<bool>,
    /// Whether the settlement that runs holds two levels, [`Agent::run_adopted_settlement`].
    adopted: Cell<bool>,
    /// The reservation of the host the settlement that runs keeps jobs for.
    reservation: Cell<Option<u32>>,
    /// The jobs a settlement of the host enqueued, each with the reservation its own jobs go to
    /// and whether they are held as well, which the next drain runs first.
    held: RefCell<VecDeque<(Job, Option<u32>, bool)>>,
    /// The reservation the held job that runs keeps the jobs it enqueues for.
    capturing: Cell<Option<u32>>,
    /// The jobs each reservation of the host keeps, [`Agent::release_reserved`].
    reserved: RefCell<Vec<(u32, VecDeque<Job>)>>,
    /// The promises of the host the agent waits on, [`Agent::hold_host_work`].
    work: Cell<usize>,
}

/// The callback a host that owns the microtask queue sets with [`Agent::set_enqueue_notifier`].
///
/// The agent calls it once for every job it enqueues, after the job is in
/// the queue. A host answers each call with one microtask of its own, in
/// which it runs that job, through a drain it grants the job with
/// [`Agent::grant_jobs`] or through [`Agent::run_one_job`], so each job of the
/// agent is one microtask in the queue of the host, in the order the agent
/// enqueued them.
#[cfg(feature = "host-queue")]
pub type EnqueueNotifier = Rc<dyn Fn(&Agent)>;

/// The options of [`Agent::new`].
#[derive(Clone, Copy, Debug, Default)]
pub struct AgentOptions {
    /// Where the time of a timer comes from.
    pub timers: TimerMode,
    /// Whether [`Agent::record`] keeps the events it is given.
    pub trace: bool,
}

/// Where the time of a timer comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TimerMode {
    /// A clock that starts at 0 and moves only when a timer fires.
    #[default]
    Virtual,
    /// The monotonic clock of the host, which [`Agent::set_host_clock`] hands
    /// the agent.
    ///
    /// A served request runs in this mode, so `sleep(5)` of a twin waits five
    /// milliseconds of the host as `setTimeout(resolve, 5)` of its fixture
    /// does. An agent in this mode without a clock reads the time as the
    /// virtual wheel does. Such an agent is driven through
    /// [`Agent::drive`] or [`Agent::drain_until_idle`] on a runtime, which
    /// parks while a timer waits on the clock.
    Host,
}

/// The monotonic clock of a host, which an agent in [`TimerMode::Host`] reads.
///
/// The names hold for the serve layer of the Linux cards, whose runtime
/// implements the trait over its heap of deadlines, and for the hosts of later
/// milestones. The time is in milliseconds, so a timer of the agent waits for
/// the milliseconds of its delay.
pub trait HostClock {
    /// Returns the milliseconds of a monotonic clock of the host.
    fn now_ms(&self) -> u64;

    /// Wakes `waker` when the clock reaches `deadline_ms`.
    ///
    /// The host wakes the waker once at or after the deadline. A deadline
    /// that has passed already wakes the waker at the next turn of the host.
    fn wake_at(&self, deadline_ms: u64, waker: Waker);
}

/// The clock of the host and its reading when the agent received it.
///
/// The time of a host mode agent is the milliseconds since that reading, so
/// it starts at 0 as the virtual clock does, and a deadline goes back to the
/// clock of the host as that reading plus the deadline (rule 1 of
/// [`timers`](crate::promise::timers)).
#[derive(Clone)]
pub(super) struct HostTime {
    /// The clock.
    clock: Rc<dyn HostClock>,
    /// The reading of the clock when [`Agent::set_host_clock`] installed it.
    base: u64,
}

impl HostTime {
    /// Returns the milliseconds since the clock was installed.
    pub(super) fn now(&self) -> u64 {
        self.clock.now_ms().saturating_sub(self.base)
    }

    /// Wakes `waker` when the time of the agent reaches `deadline`.
    pub(super) fn wake_at(&self, deadline: u64, waker: Waker) {
        self.clock
            .wake_at(self.base.saturating_add(deadline), waker);
    }
}

/// A promise job, an abstract closure with no parameters that runs once.
///
/// Translation of the Job of ECMA-262 section 9.5. A job is a value of the
/// queue of its kind: the two jobs of the promise model, whose records the job
/// holds in place, and the closure of user code or of a hook, which
/// [`Job::new`] makes and which alone sits behind a box. So a job the promise
/// model enqueues allocates nothing, and the queue grows by doubling. The kind
/// receives the agent that runs it, so that it captures none.
pub struct Job(JobKind);

/// The kinds of a [`Job`].
// A closure held in place would skip this box.
enum JobKind {
    /// `NewPromiseReactionJob`, one reaction with the value its promise settled with.
    Reaction(ReactionJob),
    /// `NewPromiseResolveThenableJob`, the adoption of a promise a resolve function received.
    Thenable(ThenableJob),
    /// A job of user code or of a hook, a closure.
    Closure(Box<dyn FnOnce(&Agent)>),
}

impl Job {
    /// Creates the job that runs `body`, a job of user code or of a hook.
    pub fn new(body: impl FnOnce(&Agent) + 'static) -> Self {
        Self(JobKind::Closure(Box::new(body)))
    }

    /// Returns the job of the reaction `job`.
    pub(super) fn reaction(job: ReactionJob) -> Self {
        Self(JobKind::Reaction(job))
    }

    /// Returns the job of the adoption `job`.
    pub(super) fn thenable(job: ThenableJob) -> Self {
        Self(JobKind::Thenable(job))
    }

    /// Runs the job on `agent`, its one dispatch on the kind.
    fn run(self, agent: &Agent) {
        match self.0 {
            JobKind::Reaction(job) => job.run(agent),
            JobKind::Thenable(job) => job.run(agent),
            JobKind::Closure(body) => body(agent),
        }
    }
}

impl Agent {
    /// Creates an agent with an empty queue, an empty wheel and its clock at 0.
    #[must_use]
    pub fn new(options: AgentOptions) -> Self {
        Self(Rc::new(AgentInner {
            options,
            jobs: RefCell::new(VecDeque::new()),
            jobs_run: Cell::new(0),
            wheel: RefCell::new(Wheel::new()),
            tasks: RefCell::new(Tasks::new()),
            wake_set: Arc::new(WakeSet::new()),
            ambient_hooks: RefCell::new(None),
            trace: RefCell::new(Vec::new()),
            host_clock: RefCell::new(None),
            #[cfg(feature = "host-queue")]
            host: HostQueue::default(),
        }))
    }

    /// Hands the agent the clock of its host, which its timers read under [`TimerMode::Host`].
    ///
    /// The clock replaces the one set before, and its reading now becomes
    /// the time 0 of the agent (rule 1 of [`timers`](crate::promise::timers)),
    /// so a host installs the clock before it sets a timer. Under
    /// [`TimerMode::Virtual`] the agent keeps the clock and never reads it.
    pub fn set_host_clock(&self, clock: Rc<dyn HostClock>) {
        let base = clock.now_ms();
        *self.0.host_clock.borrow_mut() = Some(HostTime { clock, base });
    }

    /// Returns the clock of the host when the agent reads one.
    ///
    /// That is the clock [`Agent::set_host_clock`] handed over, when the
    /// options of the agent name [`TimerMode::Host`], and `None` otherwise.
    pub(super) fn host_clock(&self) -> Option<HostTime> {
        if self.0.options.timers != TimerMode::Host {
            return None;
        }
        self.0.host_clock.borrow().clone()
    }

    /// Adds `job` at the end of the queue.
    ///
    /// Translation of `HostEnqueuePromiseJob` of ECMA-262 section 9.5.5. The
    /// clause requires that jobs run in the order they were enqueued in, which
    /// one queue per agent gives.
    ///
    /// With the feature `host-queue`, a job that a settlement of the host
    /// enqueues waits for the next drain beside the queue, and a host that
    /// owns the queue is told of every other job once it stands in the queue.
    pub fn enqueue_job(&self, job: Job) {
        #[cfg(feature = "host-queue")]
        let Some(job) = self.hold_settled_job(job) else {
            return;
        };
        self.0.jobs.borrow_mut().push_back(job);
        #[cfg(feature = "host-queue")]
        self.tell_host();
    }

    /// Runs the jobs of the queue, first in first out, until it is empty.
    ///
    /// A job that enqueues a job extends the same drain, which is how a
    /// microtask checkpoint runs. The jobs run inside [`Agent::enter`].
    ///
    /// With the feature `host-queue`, the drain first runs the jobs a
    /// settlement of the host holds, at once and in order, and while a host
    /// owns the queue it runs as many jobs of the queue as the host granted
    /// and no more, so each runs in the poll the microtask of the host gave
    /// it.
    pub fn drain_jobs(&self) {
        self.enter(|| {
            #[cfg(feature = "host-queue")]
            self.run_settled_jobs();
            while let Some(job) = self.next_drained_job() {
                self.0.jobs_run.set(self.0.jobs_run.get() + 1);
                job.run(self);
            }
        });
    }

    /// Takes the first job out of the queue.
    ///
    /// The borrow of the queue ends here and not in the loop of
    /// [`Agent::drain_jobs`], because the job that runs next enqueues into
    /// the same queue.
    fn next_job(&self) -> Option<Job> {
        self.0.jobs.borrow_mut().pop_front()
    }

    /// Takes the next job a drain runs, the first job of the queue.
    #[cfg(not(feature = "host-queue"))]
    fn next_drained_job(&self) -> Option<Job> {
        self.next_job()
    }

    /// Returns the count of jobs started so far, the tick counter of the trace.
    ///
    /// Code that runs before the first job reads 0, and code inside the
    /// first job reads 1.
    #[must_use]
    pub fn jobs_run(&self) -> u64 {
        self.0.jobs_run.get()
    }
}

/// The queue a host owns, behind the feature `host-queue`.
///
/// A host that runs the microtask queue the code of the app runs in, as the
/// wrapper of the TypeScript option does, takes the queue of the agent with a
/// notifier and runs each job as one microtask of its own, in the order the
/// agent enqueued them. It grants the microtask of a job to the drain of a
/// poll of its request, so the job runs with every ambient state of that poll
/// in place, and runs it through [`Agent::run_one_job`] where no poll reaches
/// that drain. A promise of the host that settles runs the reactions it
/// enqueues inside the microtask of the settlement, where V8 runs the reaction
/// of a `then` in the job its promise settles in, and one that a job settles,
/// through a callback of the app inside a call of the job, runs them at once
/// and keeps the jobs they enqueue for a microtask the host reserved at that
/// call. A settlement the host makes through the resolving functions it handed
/// a thenable of the app runs one level of reactions more at once, since the
/// consumer the app's own code has there reacts on the settlement itself. The
/// agent counts the promises of the host it waits on, so its loop parks and
/// never reports a stall while the host owes it a job, a settlement, a
/// reserved microtask or a promise.
#[cfg(feature = "host-queue")]
impl Agent {
    /// Hands the queue to a host that owns the microtask queue, or takes it back with `None`.
    ///
    /// While a notifier is set, the host owns the queue: a drain runs the
    /// jobs the host granted and no more, every [`Agent::enqueue_job`] calls
    /// the notifier, and the host runs each job in a microtask of its own.
    /// The first notifier is called at once for every job already in the
    /// queue, so the host is told of each job exactly once. A notifier set
    /// while one is set replaces it and is told of no job, since the host was
    /// told of every queued job already. With `None` the agent takes its
    /// queue back: its own drains run every queued job again, the jobs the
    /// host was told of among them, the grants lapse, and
    /// [`Agent::run_one_job`] runs no job, so a microtask the host still
    /// holds for the agent runs nothing.
    pub fn set_enqueue_notifier(&self, notifier: Option<EnqueueNotifier>) {
        let host = &self.0.host;
        let Some(notifier) = notifier else {
            let gone = host.notifier.borrow_mut().take();
            host.owned.set(false);
            host.granted.set(0);
            drop(gone);
            return;
        };
        let replaced = host.notifier.borrow_mut().replace(Rc::clone(&notifier));
        host.owned.set(true);
        if replaced.is_some() {
            return;
        }
        let queued = self.0.jobs.borrow().len();
        for _ in 0..queued {
            notifier(self);
        }
    }

    /// Returns whether a host owns the queue through a notifier.
    #[must_use]
    pub fn has_enqueue_notifier(&self) -> bool {
        self.0.host.owned.get()
    }

    /// Runs the first job of the queue and returns whether there was one.
    ///
    /// This is one microtask of a host that owns the queue, run outside any
    /// poll. The job runs inside [`Agent::enter`], as a job of a drain does,
    /// a job it enqueues waits for a microtask of its own, and the runtime
    /// that parked the loop of the agent is woken, so it sees what the job
    /// settled. While no host owns the queue it runs no job and answers
    /// false.
    #[must_use]
    pub fn run_one_job(&self) -> bool {
        if !self.0.host.owned.get() {
            return false;
        }
        let ran = self.enter(|| {
            let Some(job) = self.next_job() else {
                return false;
            };
            self.0.jobs_run.set(self.0.jobs_run.get() + 1);
            job.run(self);
            true
        });
        self.wake_runtime();
        ran
    }

    /// Grants the next drains `count` jobs of the queue the host owns.
    ///
    /// A host answers the microtask of a job with one grant and a poll of the
    /// request of the agent, so the drain of the loop of the agent runs the
    /// job inside that poll, with the ambient state of the poll in place.
    pub fn grant_jobs(&self, count: usize) {
        let granted = &self.0.host.granted;
        granted.set(granted.get().saturating_add(count));
    }

    /// Returns whether a grant waits for a drain.
    #[must_use]
    pub fn has_grants(&self) -> bool {
        self.0.host.granted.get() > 0
    }

    /// Takes back the grants no drain used and returns their count.
    ///
    /// A host whose poll did not reach the drain of the agent runs the job of
    /// such a grant through [`Agent::run_one_job`].
    #[must_use]
    pub fn take_grants(&self) -> usize {
        self.0.host.granted.replace(0)
    }

    /// Runs `body`, a settlement of a promise of the host, and holds the jobs it enqueues.
    ///
    /// The next drain runs the held jobs first, at once and in the order
    /// `body` enqueued them, or [`Agent::run_held_jobs`] and
    /// [`Agent::run_reserved_settlement`] run them at once.
    /// So the reaction of a promise the host settles runs in the microtask of
    /// the settlement, at no microtask of its own. Without a `reservation`
    /// the jobs the held jobs enqueue go to the queue and to the host as any
    /// other. With one they go to that reservation of the host, which keeps
    /// them, untold to the host, until [`Agent::release_reserved`] hands them
    /// to the queue: a host that settles a promise inside a call of a job
    /// reserves the microtask in which V8 runs the reactions of the promise
    /// of the app the settled one stands for, and runs the reactions of the
    /// engine that answer to those there. The runtime that parked the loop is
    /// woken.
    pub fn run_settlement<R>(&self, reservation: Option<u32>, body: impl FnOnce() -> R) -> R {
        let host = &self.0.host;
        let reserving = Restore(&host.reservation, host.reservation.replace(reservation));
        let settling = Restore(&host.settling, host.settling.replace(true));
        let result = body();
        drop(settling);
        drop(reserving);
        self.wake_runtime();
        result
    }

    /// Runs `body` as [`Agent::run_settlement`] does, with a level of held jobs more.
    ///
    /// The jobs the held jobs enqueue are held as well, beside them and with
    /// the same reservation, so they run at once in the same drain or in the
    /// same [`Agent::run_reserved_settlement`], and the jobs those enqueue go
    /// to the reservation, or to the queue and the host. A host settles a
    /// promise this way through the resolving functions it handed a thenable
    /// of the app it adopts in place of an async function of the app. That
    /// function hands the thenable its own resolving functions, so its
    /// consumer reacts on the settlement itself, while the code of the agent
    /// reaches its consumer through one reaction more than a settlement
    /// through a callback of the app, the adoption of the promise it derived
    /// from the settled one.
    pub fn run_adopted_settlement<R>(
        &self,
        reservation: Option<u32>,
        body: impl FnOnce() -> R,
    ) -> R {
        let host = &self.0.host;
        let adopting = Restore(&host.adopted, host.adopted.replace(true));
        let result = self.run_settlement(reservation, body);
        drop(adopting);
        result
    }

    /// Hands the jobs the reservation `reservation` keeps to the front of
    /// the queue, in the order they were enqueued, and returns their count.
    ///
    /// The host runs them in the microtask it reserved, through grants to a
    /// drain or [`Agent::run_one_job`], before every job the queue held, as
    /// V8 runs the reactions it queued when the promise of the app settled
    /// before the jobs queued after that.
    #[must_use]
    pub fn release_reserved(&self, reservation: u32) -> usize {
        let released = {
            let mut reserved = self.0.host.reserved.borrow_mut();
            let at = reserved.iter().position(|(held, _)| *held == reservation);
            at.map(|at| reserved.remove(at).1)
        };
        let Some(jobs) = released else {
            return 0;
        };
        let count = jobs.len();
        let mut queue = self.0.jobs.borrow_mut();
        for job in jobs.into_iter().rev() {
            queue.push_front(job);
        }
        count
    }

    /// Returns whether a settlement holds jobs that no drain has run yet.
    #[must_use]
    pub fn has_held_jobs(&self) -> bool {
        !self.0.host.held.borrow().is_empty()
    }

    /// Runs the jobs a settlement holds, at once and in order.
    ///
    /// A host whose poll did not reach the drain of the agent runs them here.
    pub fn run_held_jobs(&self) {
        self.enter(|| self.run_settled_jobs());
    }

    /// Runs at once and in order the jobs the settlement under the
    /// reservation `reservation` holds, and leaves every other held job for
    /// the next drain.
    ///
    /// A host that settles a promise inside a call of a job runs here the
    /// reactions of that settlement as the call returns, while a held job
    /// that made the call may still run.
    pub fn run_reserved_settlement(&self, reservation: u32) {
        self.enter(|| {
            loop {
                let held = {
                    let mut held = self.0.host.held.borrow_mut();
                    let at = held
                        .iter()
                        .position(|(_, kept, _)| *kept == Some(reservation));
                    at.and_then(|at| held.remove(at))
                };
                let Some((job, reservation, adopted)) = held else {
                    return;
                };
                self.run_held_job(job, reservation, adopted);
            }
        });
    }

    /// Notes one more promise of the host that the agent waits on.
    ///
    /// The loop of the agent counts it as outstanding work, as it counts a
    /// waker that a foreign future holds, so it parks and reports no stall
    /// while the host owes the settlement. The host releases it once, with
    /// [`Agent::release_host_work`], when the promise settles or is dropped.
    pub fn hold_host_work(&self) {
        let work = &self.0.host.work;
        work.set(work.get().saturating_add(1));
    }

    /// Notes that the host settled, or dropped, one promise the agent waited on.
    pub fn release_host_work(&self) {
        let work = &self.0.host.work;
        work.set(work.get().saturating_sub(1));
    }

    /// Returns `job` for the queue, keeps it for the reservation of the held
    /// job that runs, or holds it for the next drain while a settlement of
    /// the host runs.
    fn hold_settled_job(&self, job: Job) -> Option<Job> {
        let host = &self.0.host;
        if let Some(reservation) = host.capturing.get() {
            let mut reserved = host.reserved.borrow_mut();
            match reserved.iter_mut().find(|(held, _)| *held == reservation) {
                Some((_, jobs)) => jobs.push_back(job),
                None => reserved.push((reservation, VecDeque::from([job]))),
            }
            return None;
        }
        if !host.settling.get() {
            return Some(job);
        }
        host.held
            .borrow_mut()
            .push_back((job, host.reservation.get(), host.adopted.get()));
        None
    }

    /// Tells the host that owns the queue of the job just queued.
    fn tell_host(&self) {
        if !self.0.host.owned.get() {
            return;
        }
        let notifier = self.0.host.notifier.borrow().clone();
        if let Some(notifier) = notifier {
            notifier(self);
        }
    }

    /// Runs the jobs a settlement holds, first in first out, each with the
    /// jobs it enqueues kept for its reservation, if it has one, or held
    /// after the others.
    ///
    /// The borrow of the held jobs ends before each job runs, since a job may
    /// settle a promise whose reactions a later settlement holds.
    fn run_settled_jobs(&self) {
        loop {
            let held = self.0.host.held.borrow_mut().pop_front();
            let Some((job, reservation, adopted)) = held else {
                return;
            };
            self.run_held_job(job, reservation, adopted);
        }
    }

    /// Runs the held job `job`, the jobs it enqueues kept for `reservation`,
    /// if any, or held beside the settlement with that reservation when
    /// `adopted` holds.
    fn run_held_job(&self, job: Job, reservation: Option<u32>, adopted: bool) {
        self.0.jobs_run.set(self.0.jobs_run.get() + 1);
        let host = &self.0.host;
        if !adopted {
            let capturing = Restore(&host.capturing, host.capturing.replace(reservation));
            job.run(self);
            drop(capturing);
            return;
        }
        let capturing = Restore(&host.capturing, host.capturing.replace(None));
        let reserving = Restore(&host.reservation, host.reservation.replace(reservation));
        let settling = Restore(&host.settling, host.settling.replace(true));
        let adopting = Restore(&host.adopted, host.adopted.replace(false));
        job.run(self);
        drop(adopting);
        drop(settling);
        drop(reserving);
        drop(capturing);
    }

    /// Takes the next job a drain runs: the first job of the queue, or while
    /// a host owns the queue the first job alone that it granted.
    fn next_drained_job(&self) -> Option<Job> {
        let host = &self.0.host;
        if !host.owned.get() {
            return self.next_job();
        }
        let granted = host.granted.get();
        if granted == 0 {
            return None;
        }
        let Some(job) = self.next_job() else {
            host.granted.set(0);
            return None;
        };
        host.granted.set(granted - 1);
        Some(job)
    }

    /// Returns whether a host owns the queue and holds a job of it that no drain ran yet.
    pub(super) fn host_holds_jobs(&self) -> bool {
        self.0.host.owned.get() && !self.0.jobs.borrow().is_empty()
    }

    /// Returns whether the host owes the agent a job, a held settlement, a
    /// reserved microtask or a promise.
    pub(super) fn host_owes_work(&self) -> bool {
        self.host_holds_jobs()
            || self.0.host.work.get() > 0
            || self.has_held_jobs()
            || !self.0.host.reserved.borrow().is_empty()
    }

    /// Wakes the runtime that parked the loop of the agent, after the host
    /// ran a job or a settlement outside a poll.
    fn wake_runtime(&self) {
        self.0.wake_set.wake_outer();
    }
}

/// Puts the value a cell of a running settlement held back when it goes out of scope.
///
/// The reset sits in a destructor so that a panic that unwinds through a
/// settlement leaves the agent as it found it.
#[cfg(feature = "host-queue")]
struct Restore<'a, T: Copy>(&'a Cell<T>, T);

#[cfg(feature = "host-queue")]
impl<T: Copy> Drop for Restore<'_, T> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}
