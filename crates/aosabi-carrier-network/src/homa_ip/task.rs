// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-network homa_ip task: the local task of the driver of
 * Network, one endpoint of the crate homa in the full form and what answers
 * its requests, over the clock, the timer and the reactor of the runtime of
 * aosabi-serve.
 *
 * No analogue on the reference side.
 */

//! The task of the driver of Network.
//!
//! A [`Task`] owns one endpoint of the full form and the [`Turn`] that
//! answers its requests. [`Task::run`] receives what the raw socket holds,
//! runs due timers and transmits while the socket takes packets through
//! `Endpoint::pump`, hands every whole request to the turn and polls the
//! answers in flight with the context of the task, pumps again so the
//! answers leave, and arms the timer of the runtime at the next deadline of
//! the endpoint, so the task sleeps until the socket or a deadline wakes it.
//! The endpoint of Network admits a peer on its first packet, so the task
//! takes no link up or down: a peer that falls silent past the dead peer rule
//! ends with `Event::PeerGone`, which the listener reads as the end of the
//! peer's streams. [`Registered`] adds a descriptor to the reactor of the
//! runtime and reports every packet it moves, which the poll reactor reads as
//! heat.

use alloc::{
    rc::Rc,
    vec,
    vec::Vec, //
};
use core::{
    future::Future as _,
    pin::Pin,
    task::Context,
    time::Duration, //
};
use std::{
    io,
    os::fd::{
        AsFd,
        BorrowedFd, //
    },
    time::Instant, //
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
use homa::{
    Carrier,
    Nanos,
    Stats, //
};

use super::{
    NetworkEndpoint,
    Turn, //
};

/// The passes one poll of a task runs at most before it yields to the other
/// tasks of the runtime.
const PASSES: usize = 16;

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

    /// Reports a packet that moved on the descriptor.
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

/// One endpoint of the full form, what answers its requests, and the clock
/// and the timer of the runtime.
pub struct Task<T: Turn> {
    /// The endpoint.
    endpoint: NetworkEndpoint,
    /// What answers its requests.
    turn: T,
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
    /// The answers the turn handed the endpoint.
    answered: u64,
    /// The pumps the task ran.
    pumps: u64,
}

impl<T: Turn> Task<T> {
    /// Takes `endpoint` with `turn` in front of it and a receive buffer of
    /// `rx_len` bytes, on the runtime of this thread.
    ///
    /// # Errors
    ///
    /// Returns the error of a thread that runs no runtime.
    pub fn new(endpoint: NetworkEndpoint, turn: T, rx_len: usize) -> io::Result<Self> {
        let handle = Handle::current()?;
        let clock = handle.clock();
        let timer = RuntimeTimer::of(&handle);
        Ok(Self {
            endpoint,
            turn,
            rx: vec![0; rx_len],
            handle,
            clock,
            timer,
            sleep: None,
            answered: 0,
            pumps: 0,
        })
    }

    /// Returns the runtime the task runs on.
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Returns the endpoint.
    pub fn endpoint(&self) -> &NetworkEndpoint {
        &self.endpoint
    }

    /// Returns the counters of the endpoint.
    pub fn stats(&self) -> Stats {
        self.endpoint.stats()
    }

    /// Returns the answers the turn handed the endpoint.
    pub fn answered(&self) -> u64 {
        self.answered
    }

    /// Returns the pumps the task ran.
    pub fn pumps(&self) -> u64 {
        self.pumps
    }

    /// Returns the time of the endpoint, the nanoseconds since the start of
    /// the clock of the runtime.
    pub fn now(&self) -> Nanos {
        u64::try_from(self.clock.start().elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    /// Pumps the endpoint over `carrier` once and adds the nanoseconds the
    /// pump took to `pump_ns` where it is `Some`.
    fn pump<C: Carrier<Peer = core::net::SocketAddr>>(
        &mut self,
        carrier: &mut C,
        pump_ns: Option<&mut u64>,
    ) -> Result<(), C::Error> {
        let now = self.now();
        self.pumps += 1;
        let Some(pump_ns) = pump_ns else {
            return self.endpoint.pump(carrier, now, &mut self.rx);
        };
        let start = Instant::now();
        let pumped = self.endpoint.pump(carrier, now, &mut self.rx);
        let took = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        *pump_ns = pump_ns.saturating_add(took);
        pumped
    }

    /// Runs one turn over `carrier` with the context `cx` and tells whether
    /// the task should run another turn at once.
    ///
    /// A turn pumps the endpoint, hands its requests to the turn, pumps
    /// again so the answers leave, and arms the timer at the next deadline
    /// of the endpoint, and it asks for another turn when that deadline has
    /// passed already. Where `pump_ns` is `Some` it adds the nanoseconds of
    /// each pump to it.
    ///
    /// # Errors
    ///
    /// Returns the carrier's error when the socket itself fails.
    pub fn turn<C: Carrier<Peer = core::net::SocketAddr>>(
        &mut self,
        carrier: &mut C,
        cx: &mut Context<'_>,
        mut pump_ns: Option<&mut u64>,
    ) -> Result<bool, C::Error> {
        self.pump(carrier, pump_ns.as_deref_mut())?;
        let answered = self.turn.turn(&mut self.endpoint, cx);
        self.answered += u64::from(answered);
        self.pump(carrier, pump_ns)?;
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
    /// `PASSES` of them, after which it wakes its own task so the others run
    /// first, and adds the nanoseconds of each pump to `pump_ns` where it is
    /// `Some`.
    ///
    /// # Errors
    ///
    /// Returns the carrier's error when the socket itself fails.
    pub fn run<C: Carrier<Peer = core::net::SocketAddr>>(
        &mut self,
        carrier: &mut C,
        cx: &mut Context<'_>,
        mut pump_ns: Option<&mut u64>,
    ) -> Result<(), C::Error> {
        for _ in 0..PASSES {
            if !self.turn(carrier, cx, pump_ns.as_deref_mut())? {
                return Ok(());
            }
        }
        cx.waker().wake_by_ref();
        Ok(())
    }
}
