// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the runtime, one executor on one thread over a reactor.
 *
 * No analogue on the reference side, where the event loop of Node runs the
 * server. Documentation/architecture.md section 14 asks for one thread, and
 * the runtime starts none.
 */

//! The runtime of a served binary.
//!
//! One executor runs the local tasks of the thread that calls
//! [`Runtime::block_on`], over the reactor of the build: the epoll reactor of
//! [`epoll`] under the feature `linux` on a Linux target, and the poll reactor
//! of [`reactor`] on every other build. The heap of deadlines of [`timers`]
//! is the host clock of the executor's agents and the timer of hyper, the
//! sockets of [`net`] are non blocking wrappers over the standard library's,
//! and [`hyper_rt`] adapts them to the runtime traits of hyper.
//!
//! | Module | Holds |
//! | --- | --- |
//! | [`executor`] | the task slab, the ready queue, the waker and the poll loop |
//! | [`reactor`] | the trait [`Reactor`] and the poll reactor of design B' |
//! | [`epoll`] | the epoll reactor of design C and [`platform_reactor`] |
//! | [`timers`] | [`RuntimeClock`] and the heap of deadlines |
//! | [`net`] | the TCP and Unix listeners and streams |
//! | [`connect`] | the outbound connect of a TCP stream that yields the thread |
//! | [`peer`] | the uid of the peer of a Unix socket |
//! | [`hyper_rt`] | hyper's IO, timer and executor over the runtime |
//!
//! No thread starts. The one thread waits between turns in the reactor, which
//! is `epoll_wait` or the sleeps and yields of the poll reactor, and neither
//! starts a thread.

pub mod connect;
pub mod epoll;
pub mod executor;
pub mod hyper_rt;
pub mod net;
pub mod peer;
pub mod reactor;
pub mod timers;

use core::{
    cell::RefCell,
    future::Future,
    pin::Pin, //
};
use std::{
    io,
    rc::Rc, //
};

pub use self::{
    epoll::platform_reactor,
    executor::Executor,
    reactor::{
        Interest,
        PollReactor,
        Reactor,
        SourceId, //
    },
    timers::RuntimeClock, //
};

/// A future that is not `Send`, boxed and pinned, the form every local task takes.
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// The parts of a runtime its handles share.
struct Shared {
    /// The task slab and the ready queue.
    executor: Executor,
    /// The reactor of the build or the one the caller chose.
    reactor: Box<dyn Reactor>,
    /// The clock and its heap of deadlines.
    clock: Rc<RuntimeClock>,
}

/// The runtime of one thread.
pub struct Runtime {
    /// The executor, the reactor and the clock.
    shared: Rc<Shared>,
}

/// A handle to the runtime whose [`Runtime::block_on`] runs on this thread.
///
/// The sockets of [`net`] take the reactor through it, and [`spawn_local`]
/// the executor.
#[derive(Clone)]
pub struct Handle {
    /// The executor, the reactor and the clock.
    shared: Rc<Shared>,
}

thread_local! {
    /// The handle of the runtime whose `block_on` runs on this thread.
    static CURRENT: RefCell<Option<Handle>> = const { RefCell::new(None) };
}

impl Runtime {
    /// Returns a runtime over the reactor of the build.
    ///
    /// # Errors
    ///
    /// Returns the error of the reactor, which the epoll reactor answers when
    /// the kernel refuses an epoll instance.
    pub fn new() -> io::Result<Self> {
        Ok(Self::with_reactor(platform_reactor()?))
    }

    /// Returns a runtime over `reactor`, which the probes of design B' choose
    /// on Linux.
    #[must_use]
    pub fn with_reactor(reactor: Box<dyn Reactor>) -> Self {
        Self {
            shared: Rc::new(Shared {
                executor: Executor::new(),
                reactor,
                clock: Rc::new(RuntimeClock::new()),
            }),
        }
    }

    /// Runs `future` to its end on this thread, with every local task it
    /// spawns, and returns its output.
    ///
    /// The loop polls every woken task once, fires the timers whose deadline
    /// passed and then waits in the reactor up to the least deadline, or not
    /// at all while a task is woken, until `future` is ready (rule 1 of the
    /// runtime).
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        let _current = Current::enter(self.handle());
        let shared = &self.shared;
        shared
            .executor
            .run(future, &shared.clock, shared.reactor.as_ref())
    }

    /// Returns the clock of the runtime.
    #[must_use]
    pub fn clock(&self) -> Rc<RuntimeClock> {
        Rc::clone(&self.shared.clock)
    }

    /// Returns a handle to the runtime.
    #[must_use]
    pub fn handle(&self) -> Handle {
        Handle {
            shared: Rc::clone(&self.shared),
        }
    }
}

impl Handle {
    /// Returns the handle of the runtime whose `block_on` runs on this thread.
    ///
    /// # Errors
    ///
    /// Returns an error of the kind `Other` outside `block_on`.
    pub fn current() -> io::Result<Self> {
        CURRENT
            .with(|current| current.borrow().clone())
            .ok_or_else(|| io::Error::other("no runtime runs on this thread"))
    }

    /// Returns the reactor of the runtime.
    #[must_use]
    pub fn reactor(&self) -> &dyn Reactor {
        self.shared.reactor.as_ref()
    }

    /// Returns the clock of the runtime.
    #[must_use]
    pub fn clock(&self) -> Rc<RuntimeClock> {
        Rc::clone(&self.shared.clock)
    }

    /// Spawns `future` as a local task of the runtime.
    pub fn spawn(&self, future: LocalBoxFuture<'static, ()>) {
        self.shared.executor.spawn(future);
    }
}

/// Spawns `future` on the runtime whose `block_on` runs on this thread, the
/// twin of `tokio::task::spawn_local`.
///
/// A call outside `block_on` drops `future`, since no loop would ever poll
/// it, and writes a note to standard error.
pub fn spawn_local(future: LocalBoxFuture<'static, ()>) {
    match Handle::current() {
        Ok(handle) => handle.spawn(future),
        Err(error) => eprintln!("spawn_local: {error}"),
    }
}

/// The runtime entered on this thread, restored to the one before on drop.
struct Current {
    /// The handle that was current before, restored on drop.
    previous: Option<Handle>,
}

impl Current {
    /// Makes `handle` the current handle of this thread.
    fn enter(handle: Handle) -> Self {
        let previous = CURRENT.with(|current| current.borrow_mut().replace(handle));
        Self { previous }
    }
}

impl Drop for Current {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|current| *current.borrow_mut() = previous);
    }
}
