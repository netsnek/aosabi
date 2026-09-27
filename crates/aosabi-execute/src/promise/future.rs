// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise future: the adapter between a Rust future and a promise.
 *
 * Translation of AsyncFunctionStart of ECMA-262 section 27.7.5.1,
 * https://tc39.es/ecma262/#sec-async-functions-abstract-operations-async-function-start,
 * with a Rust future as the body, and of Await of section 27.7.5.3,
 * https://tc39.es/ecma262/#await.
 */

//! Futures on the job queue.
//!
//! A resolver of an Aosabi service is a Rust `async fn`, and the twin of it
//! on the reference side is a JavaScript async function. [`Agent::spawn`] is
//! the call of that function: it runs the future to its first await at once
//! and answers a promise. [`PromiseFuture`] is the await: it registers one
//! reaction, and the job of that reaction polls the awaiting future, so the
//! continuation runs in the tick JavaScript runs it in.
//!
//! A future that waits on something other than a promise of its agent gets
//! woken by a foreign waker. The waker flags the future in the wake set
//! of the agent, which is the one part of an agent another thread may touch,
//! and the host loop polls a flagged future after it drained the job queue.

use core::{
    any::Any,
    cell::RefCell,
    future::{
        Future,
        IntoFuture, //
    },
    pin::Pin,
    task::{
        Context,
        Poll,
        Waker, //
    },
};
use std::{
    collections::VecDeque,
    rc::Rc,
    sync::{
        Arc,
        Mutex,
        MutexGuard,
        PoisonError,
        atomic::{
            AtomicUsize,
            Ordering, //
        }, //
    },
    task::Wake,
};

use crate::{
    promise::{
        Agent,
        Promise,
        ResolvingFunctions,
        promise::{
            Handlers,
            perform_promise_then, //
        }, //
    },
    value::{
        Completion,
        ErrorObject,
        Value, //
    }, //
};

/// The body of a spawned async function.
pub type LocalFuture = Pin<Box<dyn Future<Output = Completion> + 'static>>;

/// The spawned futures of one agent, by id.
///
/// An id is the index of a slot and is never used twice, so a late wake of a
/// future that is done finds an empty slot and nothing else. The waker of a
/// future that is done and that no code holds a clone of waits for the next
/// future the agent spawns, which takes it with its own id, so a spawn
/// allocates no waker once a future of the agent is done.
pub(super) struct Tasks {
    /// One slot per spawned future, `None` once the future is done.
    slots: Vec<Option<Task>>,
    /// The wakers of the futures that are done and that no code holds a clone of.
    spare: Vec<SharedWaker>,
}

/// One spawned future that is not done.
struct Task {
    /// The future, which is taken out of the slot for the time of a poll.
    future: Option<LocalFuture>,
    /// The resolving functions of the promise [`Agent::spawn`] returned.
    capability: ResolvingFunctions,
    /// The ambient state captured when the future was spawned.
    snapshot: Option<Rc<dyn Any>>,
    /// The waker every poll of the future hands out.
    waker: Arc<TaskWaker>,
    /// Whether a poll was asked for while the future was being polled.
    repoll: bool,
}

/// What a poll needs of a task while the slot stays in the agent.
struct CheckedOut {
    /// The future to poll.
    future: LocalFuture,
    /// The ambient state to put in place.
    snapshot: Option<Rc<dyn Any>>,
    /// The waker to hand out.
    waker: Arc<TaskWaker>,
}

impl Tasks {
    /// Creates an empty set.
    pub(super) fn new() -> Self {
        Self {
            slots: Vec::new(),
            spare: Vec::new(),
        }
    }

    /// Adds `task`, made with its id and its waker, and returns its id.
    ///
    /// The waker is a spare one that takes the id, or a new one of the wake
    /// set `set` of the agent.
    fn insert(&mut self, set: &SharedWakeSet, task: impl FnOnce(SharedWaker) -> Task) -> usize {
        let id = self.slots.len();
        let waker = match self.spare.pop() {
            Some(waker) => {
                waker.id.store(id, Ordering::Relaxed);
                waker
            }
            None => Arc::new(TaskWaker {
                id: AtomicUsize::new(id),
                set: Arc::clone(set),
            }),
        };
        self.slots.push(Some(task(waker)));
        id
    }

    /// Takes the future of the task `id` out for a poll.
    ///
    /// A task whose future is out already is being polled further up the
    /// stack. It is marked instead, and that poll repeats itself.
    fn check_out(&mut self, id: usize) -> Option<CheckedOut> {
        let task = self.slots.get_mut(id)?.as_mut()?;
        let Some(future) = task.future.take() else {
            task.repoll = true;
            return None;
        };
        Some(CheckedOut {
            future,
            snapshot: task.snapshot.clone(),
            waker: Arc::clone(&task.waker),
        })
    }

    /// Puts a pending future back, or hands it back when a poll was asked for meanwhile.
    fn check_in(&mut self, id: usize, future: LocalFuture) -> Option<LocalFuture> {
        let task = self.slots.get_mut(id)?.as_mut()?;
        if core::mem::take(&mut task.repoll) {
            return Some(future);
        }
        task.future = Some(future);
        None
    }

    /// Empties the slot of a future that is done and returns its resolving functions.
    ///
    /// The waker of the future becomes a spare one where the slot holds the
    /// only reference to it, so no code can wake the future it named.
    fn finish(&mut self, id: usize) -> Option<ResolvingFunctions> {
        let task = self.slots.get_mut(id)?.take()?;
        if Arc::strong_count(&task.waker) == 1 {
            self.spare.push(task.waker);
        }
        Some(task.capability)
    }

    /// Returns whether something outside of the agent holds the waker of a pending future.
    ///
    /// The agent holds one reference to the waker of a task, and a poll
    /// holds a second one only while it runs. Any further reference is a
    /// clone a foreign future kept in order to wake the task later.
    // A waker that counts clones ends the walk.
    pub(super) fn has_outstanding_waker(&self) -> bool {
        self.slots
            .iter()
            .flatten()
            .any(|task| Arc::strong_count(&task.waker) > 1)
    }
}

/// The futures a foreign waker flagged and the waker of the outer runtime.
///
/// This is the one part of an agent that is `Send` and `Sync`, because a
/// `Waker` has to be, and a foreign future may wake from another thread.
// An `unsafe` `Sync` cell would skip the locks.
pub(super) struct WakeSet {
    /// The ids of the flagged futures, in the order they were flagged.
    woken: Mutex<VecDeque<usize>>,
    /// The waker [`Drive`](crate::promise::Drive) was last polled with.
    outer: Mutex<Option<Waker>>,
}

/// Locks `mutex` and reads through a poison mark.
///
/// A holder of these locks only pushes, pops or swaps, so a panic of a
/// holder leaves the data whole.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl WakeSet {
    /// Creates an empty set.
    pub(super) fn new() -> Self {
        Self {
            woken: Mutex::new(VecDeque::new()),
            outer: Mutex::new(None),
        }
    }

    /// Flags the future `id` and wakes the outer runtime.
    fn flag(&self, id: usize) {
        {
            let mut woken = lock(&self.woken);
            if !woken.contains(&id) {
                woken.push_back(id);
            }
        }
        let outer = lock(&self.outer).take();
        if let Some(outer) = outer {
            outer.wake();
        }
    }

    /// Takes the id of the first flagged future.
    pub(super) fn take_first(&self) -> Option<usize> {
        lock(&self.woken).pop_front()
    }

    /// Returns whether a future is flagged.
    pub(super) fn has_woken(&self) -> bool {
        !lock(&self.woken).is_empty()
    }

    /// Returns the count of the flagged futures.
    fn flagged(&self) -> usize {
        lock(&self.woken).len()
    }

    /// Wakes the outer runtime when it parked, and flags no future.
    ///
    /// A host that owns the queue of the agent runs a job or a settlement
    /// outside a poll, so the runtime that parked the loop polls it again.
    #[cfg(feature = "host-queue")]
    pub(super) fn wake_outer(&self) {
        let outer = lock(&self.outer).take();
        if let Some(outer) = outer {
            outer.wake();
        }
    }

    /// Keeps `outer` to be woken by the next flag and returns whether it was kept.
    ///
    /// Nothing is kept when a future is flagged already. The check and the
    /// store happen under the lock of the list, so a flag lands either
    /// before the check or after the store and is never lost.
    pub(super) fn park(&self, outer: &Waker) -> bool {
        let woken = lock(&self.woken);
        if !woken.is_empty() {
            return false;
        }
        *lock(&self.outer) = Some(outer.clone());
        true
    }
}

/// The waker of a spawned future, which a foreign future may clone into another thread.
type SharedWaker = Arc<TaskWaker>;

/// The wake set of an agent, which every waker of its futures holds.
type SharedWakeSet = Arc<WakeSet>;

/// The waker of one spawned future.
struct TaskWaker {
    /// The id of the future, which a spare waker takes anew for the next future.
    ///
    /// The agent changes it only while it holds the one reference to the
    /// waker, and a clone that reaches another thread later carries the
    /// order of its handover, so a relaxed load reads the id of the future
    /// the clone was made for.
    id: AtomicUsize,
    /// The set of the agent that spawned it.
    set: Arc<WakeSet>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.set.flag(self.id.load(Ordering::Relaxed));
    }
}

impl Agent {
    /// Calls an async function whose body is `future` and returns its promise.
    ///
    /// Translation of `AsyncFunctionStart` of ECMA-262 section 27.7.5.1. The
    /// body of an async function runs synchronously to its first await, so
    /// the future is polled once before this function returns. A future that
    /// is ready on that poll gives a promise that is settled already. It is
    /// still a promise, so its reactions run as jobs, after every synchronous
    /// sibling. The output `Completion::Normal` goes through the resolve
    /// function and `Completion::Throw` rejects. The ambient state in place
    /// now is captured and follows the future across every await.
    #[must_use]
    pub fn spawn(&self, future: LocalFuture) -> Promise {
        let (promise, capability) = Promise::with_resolvers(self);
        let snapshot = self.capture_ambient();
        let id = self
            .0
            .tasks
            .borrow_mut()
            .insert(&self.0.wake_set, |waker| Task {
                future: Some(future),
                capability,
                snapshot,
                waker,
                repoll: false,
            });
        self.poll_task(id);
        promise
    }

    /// Polls the spawned future `id` and settles its promise when it is done.
    ///
    /// A poll that was asked for while this one ran repeats it, because the
    /// cause of that request is already consumed.
    pub(super) fn poll_task(&self, id: usize) {
        let Some(checked_out) = self.0.tasks.borrow_mut().check_out(id) else {
            return;
        };
        let CheckedOut {
            mut future,
            snapshot,
            waker,
        } = checked_out;
        let waker = Waker::from(waker);
        loop {
            let poll = self.poll_with_ambient(id, &mut future, &waker, snapshot.clone());
            if let Poll::Ready(completion) = poll {
                // The waker of the poll goes first, so the slot holds the one
                // reference of a waker no future kept, which a later spawn takes.
                drop(waker);
                self.finish_task(id, completion);
                return;
            }
            match self.0.tasks.borrow_mut().check_in(id, future) {
                Some(again) => future = again,
                None => return,
            }
        }
    }

    /// Polls `future` once with the ambient state `snapshot` in place.
    ///
    /// The state that was in place before comes back after the poll, which
    /// is how a store of `AsyncLocalStorage` is visible inside an async
    /// function and gone outside of it.
    fn poll_with_ambient(
        &self,
        id: usize,
        future: &mut LocalFuture,
        waker: &Waker,
        snapshot: Option<Rc<dyn Any>>,
    ) -> Poll<Completion> {
        let previous = self.swap_ambient(snapshot);
        let mut context = Context::from_waker(waker);
        let poll = self.enter_task(id, || future.as_mut().poll(&mut context));
        self.swap_ambient(previous);
        poll
    }

    /// Settles the promise of the spawned future `id` with `completion`.
    fn finish_task(&self, id: usize, completion: Completion) {
        let Some(capability) = self.0.tasks.borrow_mut().finish(id) else {
            return;
        };
        match completion {
            Completion::Normal(value) => capability.resolve(self, value),
            Completion::Throw(reason) => capability.reject(self, reason),
        }
    }

    /// Polls each future flagged before this call once, in the order of the flags.
    ///
    /// A publish of the pub sub on the agent of its subscribers calls it, so
    /// the pump of each subscriber moves the event into its repeater inside
    /// the publish, as the listener of the reference pushes inside
    /// `dispatchEvent`. A future flagged while these polls run waits for the
    /// loop of the host, so the call ends.
    pub fn poll_woken_tasks(&self) {
        for _ in 0..self.0.wake_set.flagged() {
            if !self.poll_woken_task() {
                break;
            }
        }
    }

    /// Polls the first future a foreign waker flagged and returns whether there was one.
    pub(super) fn poll_woken_task(&self) -> bool {
        let Some(id) = self.0.wake_set.take_first() else {
            return false;
        };
        self.poll_task(id);
        true
    }
}

/// `await promise` inside a Rust future, where `Ok` is the fulfilment and `Err` the rejection.
///
/// Translation of `Await` of ECMA-262 section 27.7.5.3. The first poll
/// registers one reaction with no capability and answers `Pending`, also on a
/// promise that is settled, because an await always costs one job. The job of
/// the reaction stores the settlement and polls the awaiting future inside
/// that job.
pub struct PromiseFuture {
    /// The awaited promise, or `None` when the future was made outside of an agent.
    promise: Option<Promise>,
    /// What the reaction and the future share.
    shared: Rc<Shared>,
    /// Whether the reaction is registered.
    registered: bool,
}

/// The state a [`PromiseFuture`] shares with its reaction.
struct Shared {
    /// The settlement, once the reaction ran.
    settled: RefCell<Option<Result<Value, Value>>>,
    /// The waker of the last poll, for an await outside of a spawned future.
    waker: RefCell<Option<Waker>>,
}

impl PromiseFuture {
    /// Returns the future that awaits `promise`.
    fn new(promise: Option<Promise>) -> Self {
        Self {
            promise,
            shared: Rc::new(Shared {
                settled: RefCell::new(None),
                waker: RefCell::new(None),
            }),
            registered: false,
        }
    }

    /// Returns a future that rejects at once, for an await with no agent to run it.
    pub(super) fn detached() -> Self {
        Self::new(None)
    }

    /// Registers the one reaction of the await on `agent`.
    ///
    /// Inside a spawned future the reaction polls that future. Anywhere else
    /// it wakes the waker of the last poll.
    fn register(&self, agent: &Agent, task: Option<usize>, promise: &Promise) {
        let awaiting = Awaiting {
            shared: Rc::clone(&self.shared),
            task,
        };
        perform_promise_then(agent, promise, Handlers::Await(awaiting), None);
    }
}

/// The handlers of an await, which store the settlement and continue the awaiting code.
///
/// Both handlers of the record do the same with the value or the reason, so
/// one state serves both, in place in the record.
pub(crate) struct Awaiting {
    /// What the reaction and the future share.
    shared: Rc<Shared>,
    /// The spawned future that awaits, or `None` for an await outside of one.
    task: Option<usize>,
}

impl Awaiting {
    /// Stores `settled` for the awaiting future and continues it.
    ///
    /// Inside a spawned future the continuation polls that future, in the
    /// job of the reaction. Anywhere else it wakes the waker of the last
    /// poll.
    pub(crate) fn resume(self, agent: &Agent, settled: Result<Value, Value>) {
        *self.shared.settled.borrow_mut() = Some(settled);
        let waker = self.shared.waker.borrow_mut().take();
        match (self.task, waker) {
            (Some(id), _) => agent.poll_task(id),
            (None, Some(waker)) => waker.wake(),
            (None, None) => {}
        }
    }
}

/// Returns the reason an await rejects with when no agent runs on the thread.
///
/// No analogue in the reference, where a job queue always exists.
fn no_agent_error() -> Value {
    Value::from(ErrorObject::type_error(
        "A promise is awaited outside of an agent.",
    ))
}

impl Future for PromiseFuture {
    type Output = Result<Value, Value>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(settled) = this.shared.settled.borrow_mut().take() {
            return Poll::Ready(settled);
        }
        let frame = Agent::current_frame();
        // The reaction of an await inside a spawned future polls that future
        // and wakes no waker, and a clone of the waker of the task kept here
        // would read as one a foreign future holds, so only an await outside
        // a spawned future keeps its waker.
        if !frame.as_ref().is_some_and(|(_, task)| task.is_some()) {
            *this.shared.waker.borrow_mut() = Some(context.waker().clone());
        }
        if this.registered {
            return Poll::Pending;
        }
        let (Some(promise), Some((agent, task))) = (this.promise.clone(), frame) else {
            return Poll::Ready(Err(no_agent_error()));
        };
        this.register(&agent, task, &promise);
        this.registered = true;
        Poll::Pending
    }
}

impl IntoFuture for Promise {
    type Output = Result<Value, Value>;
    type IntoFuture = PromiseFuture;

    fn into_future(self) -> PromiseFuture {
        PromiseFuture::new(Some(self))
    }
}
