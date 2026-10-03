// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-serial linux task: the local task of the Linux driver, one
 * endpoint of the crate homa and its GraphTP listener over the clock, the
 * timer and the reactor of the runtime of aosabi-serve.
 *
 * No analogue on the reference side.
 */

//! The task of the Linux driver.
//!
//! A [`Task`] owns one endpoint of the server role over owned storage and the
//! listener that answers its requests. [`Task::turn`] receives what the link
//! holds, runs due timers and transmits while the link is ready through
//! `Endpoint::pump`, hands every whole request to the listener and polls the
//! answers in flight with the context of the task, pumps again so the
//! answers leave, and arms the timer of the runtime at the next deadline of
//! the endpoint, so the task sleeps until a descriptor or a deadline wakes
//! it. [`Registered`] adds a descriptor to the reactor of the runtime and
//! reports every byte it moves, which the poll reactor reads as heat.

use alloc::{
    rc::Rc,
    vec,
    vec::Vec, //
};
use core::{
    future::Future as _,
    hash::Hash,
    pin::Pin,
    task::Context,
    time::Duration, //
};
use std::{
    io,
    os::fd::{
        AsFd,
        BorrowedFd, //
    }, //
};

use aosabi_serve::runtime::{
    Handle,
    Interest,
    RuntimeClock,
    SourceId,
    hyper_rt::{
        RuntimeSleep,
        RuntimeTimer, //
    }, //
};
use graphql_over_graphtp::{
    Listener,
    Progress, //
};
use homa::{
    Carrier,
    Config,
    ConfigError,
    Endpoint,
    Limits,
    LinkError,
    Nanos,
    Owned,
    PeerPort,
    Profile,
    Stats,
    wire::Compact, //
};

/// The passes one poll of a task runs at most before it yields to the other
/// tasks of the runtime.
const PASSES: usize = 16;

/// The endpoint of a driver: the compact form over owned storage.
pub type DriverEndpoint<P> = Endpoint<P, Owned<P>, Compact>;

/// A descriptor added to the reactor of the runtime, removed when it drops.
pub struct Registered<T: AsFd> {
    /// The descriptor.
    inner: T,
    /// The runtime it was added to.
    handle: Handle,
    /// Its number in the reactor.
    id: SourceId,
}

impl<T: AsFd> Registered<T> {
    /// Adds `inner` to the reactor of the runtime of `handle`.
    ///
    /// # Errors
    ///
    /// Returns the error of the reactor, which the epoll reactor answers
    /// for a descriptor the kernel cannot wait on.
    pub fn new(inner: T, handle: &Handle) -> io::Result<Self> {
        let id = handle.reactor().add(inner.as_fd())?;
        Ok(Self {
            inner,
            handle: handle.clone(),
            id,
        })
    }

    /// Returns the descriptor.
    pub fn get(&self) -> &T {
        &self.inner
    }

    /// Returns the descriptor to change it in place.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Wakes the task of `cx` once the descriptor may be ready for
    /// `interest`.
    pub fn register(&self, interest: Interest, cx: &Context<'_>) {
        self.handle
            .reactor()
            .register(self.id, interest, cx.waker());
    }

    /// Reports a byte that moved on the descriptor.
    pub fn progressed(&self) {
        self.handle.reactor().progressed(self.id);
    }
}

impl<T: AsFd> AsFd for Registered<T> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.inner.as_fd()
    }
}

impl<T: AsFd> Drop for Registered<T> {
    fn drop(&mut self) {
        self.handle.reactor().remove(self.inner.as_fd(), self.id);
    }
}

/// Why a task could not be built.
#[derive(Debug)]
pub enum TaskError {
    /// No runtime runs on this thread.
    Runtime(io::Error),
    /// The configuration of the endpoint was refused.
    Config(ConfigError),
    /// The limits of the link were refused.
    Link(LinkError),
}

impl From<TaskError> for io::Error {
    fn from(error: TaskError) -> Self {
        match error {
            TaskError::Runtime(error) => error,
            TaskError::Config(error) => io::Error::other(error),
            TaskError::Link(error) => io::Error::other(error),
        }
    }
}

/// One endpoint of the server role, its listener, and the clock and the
/// timer of the runtime.
pub struct Task<P: Copy + Eq + Hash + PeerPort> {
    /// The endpoint.
    endpoint: DriverEndpoint<P>,
    /// The listener that answers its requests.
    listener: Listener<P>,
    /// The buffer one received packet lands in.
    rx: Vec<u8>,
    /// The runtime the task runs on.
    handle: Handle,
    /// The clock of the runtime, whose start is the endpoint's time 0.
    clock: Rc<RuntimeClock>,
    /// The timer of the runtime.
    timer: RuntimeTimer,
    /// The deadline the timer is armed at and its sleep.
    sleep: Option<(Nanos, RuntimeSleep)>,
    /// The answers the listener handed the endpoint.
    answered: u64,
}

impl<P: Copy + Eq + Hash + PeerPort> Task<P> {
    /// Builds the endpoint of `config` and `profile` over owned storage with
    /// `listener` in front of it, on the runtime of this thread.
    ///
    /// # Errors
    ///
    /// Returns [`TaskError::Runtime`] outside a runtime and
    /// [`TaskError::Config`] for a configuration the endpoint refuses.
    pub fn new(config: Config, profile: Profile, listener: Listener<P>) -> Result<Self, TaskError> {
        let handle = Handle::current().map_err(TaskError::Runtime)?;
        let endpoint =
            DriverEndpoint::new(config, profile, Owned::new(&config)).map_err(TaskError::Config)?;
        let clock = handle.clock();
        let timer = RuntimeTimer::of(&handle);
        Ok(Self {
            endpoint,
            listener,
            rx: vec![0; usize::from(profile.max_packet).max(64)],
            handle,
            clock,
            timer,
            sleep: None,
            answered: 0,
        })
    }

    /// Returns the runtime the task runs on.
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Returns the endpoint.
    pub fn endpoint(&self) -> &DriverEndpoint<P> {
        &self.endpoint
    }

    /// Returns the counters of the endpoint.
    pub fn stats(&self) -> Stats {
        self.endpoint.stats()
    }

    /// Returns the answers the listener handed the endpoint.
    pub fn answered(&self) -> u64 {
        self.answered
    }

    /// Returns the time of the endpoint, the nanoseconds since the start of
    /// the clock of the runtime.
    pub fn now(&self) -> Nanos {
        u64::try_from(self.clock.start().elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    /// Starts serving `peer` with the limits `local` of this end, taking
    /// `assumed` of the peer until its LIMITS arrive.
    ///
    /// # Errors
    ///
    /// Returns the [`LinkError`] of the endpoint.
    pub fn link_up(&mut self, peer: P, local: Limits, assumed: Limits) -> Result<(), LinkError> {
        self.endpoint.link_up(peer, local, assumed)
    }

    /// Ends the link of `peer` and lets the listener end the peer's streams
    /// in a turn with `cx`.
    pub fn link_down(&mut self, peer: P, cx: &mut Context<'_>) {
        // A peer whose link is down already answers NoPeer, which needs
        // nothing.
        let _ = self.endpoint.link_down(peer);
        let Progress { answered, .. } = self.listener.turn_endpoint(&mut self.endpoint, cx);
        self.answered += u64::from(answered);
    }

    /// Changes what this end accepts from `peer` and sends its LIMITS again.
    ///
    /// # Errors
    ///
    /// Returns the [`LinkError`] of the endpoint.
    pub fn set_limits(&mut self, peer: P, local: Limits) -> Result<(), LinkError> {
        self.endpoint.set_limits(peer, local)
    }

    /// Runs one turn over `carrier` with the context `cx` and tells whether
    /// the task should run another turn at once.
    ///
    /// A turn pumps the endpoint, hands its requests to the listener, pumps
    /// again so the answers leave, and arms the timer at the next deadline
    /// of the endpoint, and it asks for another turn when that deadline has
    /// passed already.
    ///
    /// # Errors
    ///
    /// Returns the carrier's error when the link itself fails.
    pub fn turn<C: Carrier<Peer = P>>(
        &mut self,
        carrier: &mut C,
        cx: &mut Context<'_>,
    ) -> Result<bool, C::Error> {
        let now = self.now();
        self.endpoint.pump(carrier, now, &mut self.rx)?;
        let Progress { answered, .. } = self.listener.turn_endpoint(&mut self.endpoint, cx);
        self.answered += u64::from(answered);
        self.endpoint.pump(carrier, self.now(), &mut self.rx)?;
        Ok(self.arm(cx))
    }

    /// Arms the timer at the next deadline of the endpoint and tells whether
    /// that deadline has passed already.
    fn arm(&mut self, cx: &mut Context<'_>) -> bool {
        let Some(due) = self.endpoint.next_timeout() else {
            self.sleep = None;
            return false;
        };
        let now = self.now();
        if due <= now {
            return true;
        }
        if self.sleep.as_ref().is_none_or(|(armed, _)| *armed != due) {
            let sleep = self.timer.sleep_for(Duration::from_nanos(due - now));
            self.sleep = Some((due, sleep));
        }
        match self.sleep.as_mut() {
            Some((_, sleep)) => Pin::new(sleep).poll(cx).is_ready(),
            None => false,
        }
    }

    /// Runs turns over `carrier` until one asks for no other, at most
    /// `PASSES` of them, after which it wakes its own task so the others
    /// run first.
    ///
    /// # Errors
    ///
    /// Returns the carrier's error when the link itself fails.
    pub fn run<C: Carrier<Peer = P>>(
        &mut self,
        carrier: &mut C,
        cx: &mut Context<'_>,
    ) -> Result<(), C::Error> {
        for _ in 0..PASSES {
            if !self.turn(carrier, cx)? {
                return Ok(());
            }
        }
        cx.waker().wake_by_ref();
        Ok(())
    }
}
