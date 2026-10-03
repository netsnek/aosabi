// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the task slab, the ready queue and the poll loop.
 *
 * No analogue on the reference side.
 */

//! The executor.
//!
//! A task is a boxed future in a slab. Its waker is `Waker::from` of an
//! `Arc<TaskWaker>`, which pushes the id of the task onto the ready queue
//! once until the task is polled again. The queue sits behind a `Mutex`,
//! because `std::task::Wake` needs `Send` and `Sync`, and one thread never
//! contends it (rule 2 of the runtime). Nothing a task holds is an `Arc` but
//! its waker.
//!
//! Every turn ends in the reactor, as every turn of libuv's loop polls for
//! I/O, with a timeout of 0 while work is pending and up to the closest timer
//! otherwise. A turn that leaves a task woken asks the reactor for the
//! sources ready now without a wait, so a task that is woken on every turn,
//! as a connection whose service wakes it through the channel of its body is,
//! never keeps a socket that became ready from waking its own task. Each task
//! a turn wakes is polled once on the next turn, in the order it was woken,
//! so every kept connection is served in turn under a load that fills the one
//! thread.

use core::{
    cell::RefCell,
    future::Future,
    pin::pin,
    sync::atomic::{
        AtomicBool,
        Ordering, //
    },
    task::{
        Context,
        Poll,
        Waker, //
    },
};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        Mutex,
        PoisonError, //
    },
    task::Wake,
    time::Instant,
};

use aosabi_core::clock::note_turn;

use super::{
    LocalBoxFuture,
    reactor::Reactor,
    timers::RuntimeClock, //
};

/// The id the future of `block_on` takes in the ready queue.
const MAIN: usize = usize::MAX;

/// The ids of the woken tasks, in the order they were woken.
type ReadyQueue = Arc<Mutex<VecDeque<usize>>>;

/// The waker of one task.
struct TaskWaker {
    /// The id of the task in the slab, or [`MAIN`].
    id: usize,
    /// Whether the id waits in the queue already.
    queued: AtomicBool,
    /// The ready queue of the executor.
    ready: ReadyQueue,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if !self.queued.swap(true, Ordering::AcqRel) {
            self.ready
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_back(self.id);
        }
    }
}

/// One task of the slab.
struct Task {
    /// The future, `None` while the loop polls it.
    future: Option<LocalBoxFuture<'static, ()>>,
    /// The waker of the task.
    waker: Arc<TaskWaker>,
}

/// The task slab and the ready queue of one runtime.
pub struct Executor {
    /// The tasks by id, `None` for a free slot.
    tasks: RefCell<Vec<Option<Task>>>,
    /// The free slots of the slab.
    free: RefCell<Vec<usize>>,
    /// The ids of the woken tasks.
    ready: ReadyQueue,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor {
    /// Returns an executor with no task.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tasks: RefCell::new(Vec::new()),
            free: RefCell::new(Vec::new()),
            ready: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Adds `future` as a task and queues it for its first poll.
    pub fn spawn(&self, future: LocalBoxFuture<'static, ()>) {
        let mut tasks = self.tasks.borrow_mut();
        let id = self.free.borrow_mut().pop().unwrap_or(tasks.len());
        let waker = Arc::new(TaskWaker {
            id,
            queued: AtomicBool::new(false),
            ready: Arc::clone(&self.ready),
        });
        waker.wake_by_ref();
        let task = Task {
            future: Some(future),
            waker,
        };
        if id == tasks.len() {
            tasks.push(Some(task));
        } else {
            tasks[id] = Some(task);
        }
    }

    /// Returns the number of live tasks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tasks
            .borrow()
            .iter()
            .filter(|task| task.is_some())
            .count()
    }

    /// Tells whether no task lives.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Runs `future` with every task until `future` is ready.
    ///
    /// A turn polls every woken task once, the future of the caller among
    /// them, notes the turn to the core, fires the due timers of `clock` and
    /// ends in `reactor`, which wakes the tasks of the sources ready now
    /// without a wait when a task is woken already, and waits up to the least
    /// deadline of the heap otherwise. The core reads the turns where the
    /// reference waits for the event loop of Node to turn.
    pub fn run<F: Future>(
        &self,
        future: F,
        clock: &RuntimeClock,
        reactor: &dyn Reactor,
    ) -> F::Output {
        let mut future = pin!(future);
        let main = Arc::new(TaskWaker {
            id: MAIN,
            queued: AtomicBool::new(false),
            ready: Arc::clone(&self.ready),
        });
        main.wake_by_ref();
        let main_waker = Waker::from(Arc::clone(&main));
        loop {
            for id in self.take_ready() {
                if id != MAIN {
                    self.poll_task(id);
                    continue;
                }
                main.queued.store(false, Ordering::Release);
                let mut cx = Context::from_waker(&main_waker);
                if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
                    return output;
                }
            }
            note_turn();
            clock.fire_due(Instant::now());
            wait(clock, reactor, self.has_ready());
        }
    }

    /// Takes the ids of the woken tasks out of the queue.
    fn take_ready(&self) -> VecDeque<usize> {
        core::mem::take(&mut *self.ready.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Tells whether a task waits in the queue.
    fn has_ready(&self) -> bool {
        !self
            .ready
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    }

    /// Polls the task `id` once, and frees its slot when it is done.
    ///
    /// The future leaves the slab while it is polled, so a task that spawns
    /// borrows the slab freely.
    fn poll_task(&self, id: usize) {
        let taken = {
            let mut tasks = self.tasks.borrow_mut();
            let Some(Some(task)) = tasks.get_mut(id) else {
                return;
            };
            task.waker.queued.store(false, Ordering::Release);
            task.future
                .take()
                .map(|future| (future, Arc::clone(&task.waker)))
        };
        let Some((mut future, waker)) = taken else {
            return;
        };
        let waker = Waker::from(waker);
        let mut cx = Context::from_waker(&waker);
        let done = future.as_mut().poll(&mut cx).is_ready();
        let mut tasks = self.tasks.borrow_mut();
        if done {
            tasks[id] = None;
            self.free.borrow_mut().push(id);
        } else if let Some(Some(task)) = tasks.get_mut(id) {
            task.future = Some(future);
        }
    }
}

/// Runs the reactor once, up to the least deadline of the clock, or with no
/// wait when `woken` tells that a task waits in the queue already.
///
/// An error of the reactor is written to standard error and the loop turns
/// again. A wait that fails reads no event and loses none, since an edge the
/// kernel holds stays for the next wait, while a wait that fails on every
/// turn reads no event at all and keeps the loop busy, so a reactor asks the
/// kernel for no wait it refuses, as the epoll reactor bounds its timeout.
fn wait(clock: &RuntimeClock, reactor: &dyn Reactor, woken: bool) {
    let now_ms = clock.now_ms();
    let deadline_ms = if woken {
        Some(now_ms)
    } else {
        clock.next_deadline_ms()
    };
    if let Err(error) = reactor.wait(now_ms, deadline_ms) {
        eprintln!("aosabi-serve: the reactor failed to wait: {error}");
    }
}
