// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-network homa_ip: GraphTP(Homa) on Network over the raw socket
 * of homa::ip, the default path of a Linux card, as a local task of the
 * runtime of aosabi-serve.
 *
 * No analogue on the reference side.
 */

//! `GraphTP`(Homa) on Network over raw IP 146.
//!
//! [`serve_ip`] opens the raw socket of the path, builds one endpoint of the
//! Network profile over owned storage with the share of the host's link the
//! card names, adds the socket to the reactor of the runtime, and serves the
//! endpoint's requests through the `GraphTP` listener until the socket fails.
//! The engine names a peer by the source address of the IP header with the
//! source port of the Homa header, so two client processes of one host are two
//! peers to the listener, each with its own streams, its own parked PULL and
//! its own start rule.
//!
//! A path the card marks optional that cannot open prints one line and ends
//! with `Ok(())`, so the service answers HTTP(TCP) alone, and a Homa port
//! another process of the host holds always fails the serve, since a second
//! service on one port would answer the requests of the first.
//!
//! [`serve_ip_with`] serves with any [`Turn`] in place of the listener and
//! keeps what the path counted in [`Counts`], the time the engine spent
//! beside them where the counts ask for it, which is how a notebook prices the
//! engine and the listener on the same path.

mod task;

use alloc::{
    boxed::Box,
    rc::Rc, //
};
use core::{
    cell::Cell,
    future::poll_fn,
    net::SocketAddr,
    task::{
        Context,
        Poll, //
    }, //
};
use std::{
    eprintln,
    io,
    time::Instant, //
};

use aosabi_serve::runtime::{
    Interest,
    LocalBoxFuture, //
};
use graphql_over_graphtp::Listener;
use homa::{
    Carrier,
    Endpoint,
    Owned,
    Priority,
    Stats,
    ip::{
        self,
        OpenError,
        RawSocket, //
    },
    pacer::Rate,
    wire::Full, //
};

pub use self::task::{
    Registered,
    Task, //
};
use crate::network::{
    HomaNetwork,
    NetworkError,
    PACER_BACKLOG_NS,
    PACER_DEFER_MIN,
    Path,
    Requirement,
    link_share,
    profile, //
};

/// The endpoint of the path: the full form over owned storage, whose peer is
/// an address with its Homa port.
pub type NetworkEndpoint = Endpoint<SocketAddr, Owned<SocketAddr>, Full>;

/// The bytes a received packet lands in, the longest IP datagram.
const RX_LEN: usize = 65_536;

/// What answers the requests of the endpoint in each turn of the task.
pub trait Turn {
    /// Takes every event of `endpoint`, hands each whole request to what
    /// answers it, polls what is in flight with `cx`, hands the endpoint
    /// every answer that is ready, and returns how many it handed.
    fn turn(&mut self, endpoint: &mut NetworkEndpoint, cx: &mut Context<'_>) -> u32;
}

impl Turn for Listener<SocketAddr> {
    // The listener's turn stays a function of its own, called once a turn,
    // so the code of the listener is not folded into the loop of the driver.
    #[inline(never)]
    fn turn(&mut self, endpoint: &mut NetworkEndpoint, cx: &mut Context<'_>) -> u32 {
        self.turn_endpoint(endpoint, cx).answered
    }
}

/// What a served path counted, which a harness reads while and after the
/// task runs.
#[derive(Debug, Default)]
pub struct Counts {
    /// Whether the raw socket opened.
    pub opened: Cell<bool>,
    /// The counters of the endpoint after the last turn.
    pub stats: Cell<Stats>,
    /// The counters of the raw socket after the last turn.
    pub driver: Cell<ip::Counts>,
    /// The answers the turn handed the endpoint.
    pub answered: Cell<u64>,
    /// Whether the task times the engine, which costs two readings of the
    /// clock a pump and two a packet.
    pub timed: Cell<bool>,
    /// The nanoseconds the pumps of the endpoint took, the calls of the raw
    /// socket among them, where `timed` holds.
    pub pump_ns: Cell<u64>,
    /// The nanoseconds the calls of the raw socket took inside the pumps,
    /// where `timed` holds.
    pub socket_ns: Cell<u64>,
    /// The pumps the task ran.
    pub pumps: Cell<u64>,
}

impl Counts {
    /// Returns the nanoseconds the engine spent outside the calls of the
    /// raw socket, the system calls not counted.
    #[must_use]
    pub fn engine_ns(&self) -> u64 {
        self.pump_ns.get().saturating_sub(self.socket_ns.get())
    }

    /// Adds `ns` to the counter `cell`.
    fn add(cell: &Cell<u64>, ns: u64) {
        cell.set(cell.get().saturating_add(ns));
    }
}

/// Returns the nanoseconds since `start`.
fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// The raw socket in the reactor of the runtime, which reports each packet
/// it moves and times its calls where the counts ask for it.
struct Link<'c> {
    /// The socket and its place in the reactor.
    raw: Registered<RawSocket>,
    /// The counts of the path.
    counts: &'c Counts,
}

impl Carrier for Link<'_> {
    type Peer = SocketAddr;
    type Error = io::Error;

    fn ready(&self, peer: SocketAddr, len: usize) -> bool {
        self.raw.get().ready(peer, len)
    }

    fn send(
        &mut self,
        peer: SocketAddr,
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> io::Result<()> {
        let start = self.counts.timed.get().then(Instant::now);
        let sent = self.raw.get_mut().send(peer, priority, header, payload);
        if let Some(start) = start {
            Counts::add(&self.counts.socket_ns, elapsed_ns(start));
        }
        self.raw.progressed();
        sent
    }

    fn recv(&mut self, buf: &mut [u8]) -> io::Result<Option<(SocketAddr, usize)>> {
        let start = self.counts.timed.get().then(Instant::now);
        let received = self.raw.get_mut().recv(buf);
        if let Some(start) = start {
            Counts::add(&self.counts.socket_ns, elapsed_ns(start));
        }
        if matches!(received, Ok(Some(_))) {
            self.raw.progressed();
        }
        received
    }
}

/// Opens the raw socket of the path of `options`.
///
/// # Errors
///
/// Returns [`NetworkError::NotPermitted`] where the process lacks
/// `CAP_NET_RAW`, [`NetworkError::PortTaken`] where another process of the
/// host holds the Homa port, and [`NetworkError::Io`] for any other failure.
pub fn open(options: &HomaNetwork) -> Result<RawSocket, NetworkError> {
    let Path::Ip(path) = &options.path;
    RawSocket::open(path).map_err(open_error)
}

/// Returns the error of the path for the error of the opening of its raw
/// socket.
fn open_error(error: OpenError) -> NetworkError {
    match error {
        OpenError::NotPermitted => NetworkError::NotPermitted,
        OpenError::PortTaken(port) => NetworkError::PortTaken(port),
        OpenError::Io(error) => NetworkError::Io(error),
    }
}

/// Returns the endpoint of `options`: its configuration, the Network profile
/// with the host's share of `max_incoming`, and the rate pacer at the host's
/// share of the link.
///
/// # Errors
///
/// Returns [`NetworkError::Config`] for a configuration the engine refuses.
pub fn endpoint(options: &HomaNetwork) -> Result<NetworkEndpoint, NetworkError> {
    let config = options.config;
    let mut endpoint = NetworkEndpoint::new(config, profile(options), Owned::new(&config))
        .map_err(NetworkError::Config)?;
    let share = link_share(options);
    endpoint.set_link_rate(share);
    endpoint.set_pacer(Some(Rate::new(share, PACER_BACKLOG_NS, PACER_DEFER_MIN)));
    Ok(endpoint)
}

/// Serves `GraphTP`(Homa) on Network until the path fails, over the raw
/// socket in the reactor of the runtime, with `listener` in front of the
/// endpoint.
///
/// An optional path that cannot open prints one line and returns `Ok(())`,
/// so the service answers HTTP(TCP) alone. `PortTaken` fails the serve on
/// every path, and the driver prints the port and ends the process with 70.
#[must_use]
pub fn serve_ip(
    options: HomaNetwork,
    listener: Listener<SocketAddr>,
) -> LocalBoxFuture<'static, Result<(), NetworkError>> {
    serve_ip_with(options, listener, Rc::new(Counts::default()))
}

/// Serves as [`serve_ip`] does with `turn` in front of the endpoint, and
/// keeps what the path counted in `counts`.
#[must_use]
pub fn serve_ip_with<T: Turn + 'static>(
    options: HomaNetwork,
    turn: T,
    counts: Rc<Counts>,
) -> LocalBoxFuture<'static, Result<(), NetworkError>> {
    Box::pin(async move {
        // A port another process holds fails the serve on every path, and
        // any other error of the opening fails a required path and leaves an
        // optional one to HTTP(TCP), each error mapped once.
        let Path::Ip(path) = &options.path;
        let raw = match RawSocket::open(path) {
            Ok(raw) => raw,
            Err(OpenError::PortTaken(port)) => return Err(NetworkError::PortTaken(port)),
            Err(error) => {
                let error = open_error(error);
                if options.requirement == Requirement::Required {
                    return Err(error);
                }
                eprintln!(
                    "aosabi-carrier-network: GraphTP(Homa) on Network does not start, {error}, so the service answers HTTP(TCP) alone"
                );
                return Ok(());
            }
        };
        counts.opened.set(true);
        let endpoint = endpoint(&options)?;
        let mut task = Task::new(endpoint, turn, RX_LEN).map_err(NetworkError::Io)?;
        let raw = Registered::new(raw, task.handle()).map_err(NetworkError::Io)?;
        let mut link = Link {
            raw,
            counts: &counts,
        };
        poll_fn(|cx| step(&mut task, &mut link, cx)).await
    })
}

/// Runs the turns of one wake of the task and registers its interests.
fn step<T: Turn>(
    task: &mut Task<T>,
    link: &mut Link<'_>,
    cx: &mut Context<'_>,
) -> Poll<Result<(), NetworkError>> {
    let counts = link.counts;
    let mut pump_ns = counts.pump_ns.get();
    let timed = counts.timed.get().then_some(&mut pump_ns);
    let ran = task.run(link, cx, timed);
    counts.pump_ns.set(pump_ns);
    counts.stats.set(task.stats());
    counts.driver.set(link.raw.get().counts());
    counts.answered.set(task.answered());
    counts.pumps.set(task.pumps());
    if let Err(error) = ran {
        return Poll::Ready(Err(NetworkError::Io(error)));
    }
    let drained = link.raw.get_mut().flush();
    link.raw.register(Interest::Read, cx);
    if !drained {
        link.raw.register(Interest::Write, cx);
    }
    Poll::Pending
}

/// Returns the bytes of the buffers a served path holds beside its endpoint,
/// the receive buffer of the task and the datagram buffer of the socket.
#[must_use]
pub const fn path_buffers() -> usize {
    RX_LEN + 65_535
}
