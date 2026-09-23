// SPDX-License-Identifier: MIT
/*
 * homa sim fabric: hosts on one top of rack switch with priority queues.
 *
 * The model is the Fabric of the Homa design note, second version,
 * homa/design.md section 7.8, lines 2784 to 2815: hosts with links of one
 * rate to one top of rack switch with strict priority queues on each port and
 * one buffer all ports share, enough to reproduce the experiments of SIGCOMM
 * 2018 section 5 at small scale. The bytes a packet takes on the wire beside
 * its Homa bytes are those of HomaModule's own accounting at commit 50d65305:
 * 18 bytes of Ethernet header and CRC and 24 of preamble, start delimiter and
 * gap, homa_wire.h lines 59 to 70, at
 * https://github.com/PlatformLab/HomaModule/tree/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3,
 * read, and the 20 bytes of an IPv4 header.
 */

//! The simulated fabric of a data centre rack.
//!
//! A [`Fabric`] is `hosts` hosts, each with a link of `link_bits_per_s` in
//! each direction to one switch. A host's network card sends one packet at a
//! time, and its port as a [`Carrier`] is ready while the card is free, so
//! the engine chooses every packet at the moment the link can take it and
//! SRPT preempts at the granularity of one packet. [`FabricNet::touched`]
//! names the hosts whose card became free or that received a packet, which
//! the driver of a run pumps. A packet reaches the switch `propagation`
//! after its last bit left the host, and with `spray` on, a further delay up
//! to `propagation` drawn for each packet, as a core that sprays the packets
//! of one message over its paths reorders them. The switch stores a packet
//! whole and queues it at the port of its receiver in one of
//! `priority_queues` strict priority queues, the level of the packet mapped
//! onto them, and a port sends the head of its highest non empty queue
//! whenever its link is free. Every queue of every port draws on one buffer
//! of `buffer_bytes`, and a packet that finds no room is dropped. A packet
//! reaches its receiver `propagation` after its last bit left the switch.
//!
//! A packet takes on the wire its Homa bytes, the 20 bytes of an IPv4 header
//! and the 42 bytes of Ethernet framing `HomaModule` counts,
//! [`WIRE_OVERHEAD`] in all, so a DATA packet of 1,424 bytes of payload
//! takes 1,542 bytes at an MTU of 1,500.
//!
//! [`FabricNet::queues`] reports the most bytes a port's queues held at once,
//! the packets dropped, and for each host the share of time per mille its
//! downlink was busy since the counters were last cleared with
//! [`FabricNet::clear_counters`].

use alloc::{
    collections::{
        BinaryHeap,
        VecDeque, //
    },
    vec::Vec, //
};
use core::cmp::Reverse;

use crate::{
    sim::{
        Rng,
        Trace,
        TraceEvent, //
    },
    Carrier,
    Nanos,
    Priority, //
};

/// The bytes a packet takes on the wire beside its Homa bytes: an IPv4 header
/// of 20 bytes, and 18 bytes of Ethernet header and CRC with 24 of preamble,
/// start delimiter and inter packet gap.
pub const WIRE_OVERHEAD: u32 = 20 + 18 + 24;

/// The most hosts the counters of [`FabricStats`] cover.
pub const STAT_HOSTS: usize = 64;

/// One top of rack switch with priority queues per port, hosts, a link rate
/// and a shared buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fabric {
    /// The hosts on the switch.
    pub hosts: u16,
    /// The rate of every link in each direction, bits a second.
    pub link_bits_per_s: u64,
    /// The time from the last bit leaving a link to its arrival.
    pub propagation: Nanos,
    /// The bytes every queue of every port of the switch holds together.
    pub buffer_bytes: u32,
    /// The strict priority queues of each port, 1 to 8.
    pub priority_queues: u8,
    /// Whether each packet takes a further delay up to `propagation` on its
    /// way to the switch, as a core that sprays packets does.
    pub spray: bool,
}

impl Fabric {
    /// Builds the network, whose host `h` is a [`Carrier`] with `Peer =
    /// u16`, drawing the delays of spraying from the stream "spray" of
    /// `rng`.
    #[must_use]
    pub fn build(self, rng: Rng) -> FabricNet {
        let hosts = usize::from(self.hosts);
        FabricNet {
            fabric: self,
            rng: rng.stream("spray"),
            now: 0,
            hosts: (0..hosts).map(|_| Host::default()).collect(),
            ports: (0..hosts).map(|_| Port::default()).collect(),
            events: BinaryHeap::new(),
            seq: 0,
            buffered: 0,
            max_queue_bytes: 0,
            drops: 0,
            counted_from: 0,
            queues: usize::from(self.priority_queues.clamp(1, 8)),
            flights: Flights::default(),
            touched: Vec::new(),
            marked: (0..hosts).map(|_| false).collect(),
        }
    }

    /// Returns the nanoseconds a packet of `len` Homa bytes takes on a link,
    /// its framing included, rounded up.
    #[must_use]
    pub fn wire_time(&self, len: usize) -> Nanos {
        let bytes = u128::try_from(len)
            .unwrap_or(u128::MAX)
            .saturating_add(u128::from(WIRE_OVERHEAD));
        let rate = u128::from(self.link_bits_per_s.max(1));
        let nanos = bytes.saturating_mul(8_000_000_000).div_ceil(rate);
        Nanos::try_from(nanos).unwrap_or(Nanos::MAX)
    }
}

/// One packet in the fabric.
#[derive(Clone, Debug)]
struct Packet {
    /// The host that sent it.
    from: u16,
    /// The host it goes to.
    to: u16,
    /// Its level.
    level: u8,
    /// Its Homa bytes.
    bytes: Vec<u8>,
}

/// One host: its network card and the packets that arrived.
#[derive(Clone, Debug, Default)]
struct Host {
    /// The packets waiting for the card, the first of them on the wire where
    /// `sending` is set.
    uplink: VecDeque<Packet>,
    /// Whether the first packet of `uplink` is on the wire.
    sending: bool,
    /// The packets that arrived and wait for the host's receiver.
    inbox: VecDeque<(u16, Vec<u8>)>,
}

/// One port of the switch toward a host.
#[derive(Clone, Debug, Default)]
struct Port {
    /// The queues of the port, the lowest level first.
    queues: [VecDeque<Packet>; 8],
    /// The bytes its queues hold.
    queued: u32,
    /// The packet on its link, `None` while the link is free.
    sending: Option<Packet>,
    /// The time the link was busy since the counters were cleared.
    busy: Nanos,
    /// The most bytes its queues held at once.
    most: u32,
}

/// What happens at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Event {
    /// Host `h`'s card sent the last bit of its packet.
    HostSent(u16),
    /// The packet in flight `slot` reaches the switch.
    AtSwitch(usize),
    /// The port toward host `h` sent the last bit of its packet.
    PortSent(u16),
    /// The packet in flight `slot` reaches its host.
    AtHost(usize),
}

/// Hosts on one switch, the network a [`Fabric`] builds.
#[derive(Debug)]
pub struct FabricNet {
    /// The shape of the network.
    fabric: Fabric,
    /// The generator of the delays of spraying.
    rng: Rng,
    /// The time of the last step.
    now: Nanos,
    /// The hosts.
    hosts: Vec<Host>,
    /// The ports of the switch, one a host.
    ports: Vec<Port>,
    /// The events to come, the earliest first, each with its sequence
    /// number and the packet it carries where it carries one.
    events: BinaryHeap<Reverse<(Nanos, u64, Event)>>,
    /// The sequence number of the next event.
    seq: u64,
    /// The bytes all queues of the switch hold.
    buffered: u32,
    /// The most bytes one port's queues held at once.
    max_queue_bytes: u32,
    /// The packets the switch dropped.
    drops: u64,
    /// The time the counters were cleared at.
    counted_from: Nanos,
    /// The queues of each port in use.
    queues: usize,
    /// The packets on their way.
    flights: Flights,
    /// The hosts whose card became free or that received a packet since
    /// [`FabricNet::touched`] was last called.
    touched: Vec<u16>,
    /// Whether each host is in `touched`.
    marked: Vec<bool>,
}

/// The packets on their way between the card of a host and the switch, or
/// the switch and a host, parked in a table while an event names them.
#[derive(Debug, Default)]
struct Flights {
    /// The packets, `None` in a free place.
    slots: Vec<Option<Packet>>,
    /// The free places.
    free: Vec<usize>,
}

impl FabricNet {
    /// Returns host `h` as a carrier.
    pub fn host(&mut self, h: u16) -> FabricPort<'_> {
        FabricPort { net: self, host: h }
    }

    /// Returns the shape of the network.
    #[must_use]
    pub fn fabric(&self) -> Fabric {
        self.fabric
    }

    /// Moves every packet due by `now` and returns the next time a packet
    /// moves.
    ///
    /// A packet the switch drops is recorded in `trace` as
    /// [`TraceEvent::Overflow`].
    pub fn step(&mut self, now: Nanos, trace: &mut Trace) -> Option<Nanos> {
        if now > self.now {
            self.now = now;
        }
        while let Some(&Reverse((at, _, event))) = self.events.peek() {
            if at > self.now {
                break;
            }
            self.events.pop();
            self.happen(at, event, trace);
        }
        self.events.peek().map(|&Reverse((at, _, _))| at)
    }

    /// Returns the hosts whose card became free or that received a packet
    /// since the last call, each once, in the order they did.
    pub fn touched(&mut self) -> Vec<u16> {
        for &h in &self.touched {
            if let Some(mark) = self.marked.get_mut(usize::from(h)) {
                *mark = false;
            }
        }
        core::mem::take(&mut self.touched)
    }

    /// Notes that host `h` changed.
    fn touch(&mut self, h: u16) {
        if let Some(mark) = self.marked.get_mut(usize::from(h)) {
            if !*mark {
                *mark = true;
                self.touched.push(h);
            }
        }
    }

    /// Returns the bytes each switch queue held at most, the drops and the
    /// downlink use since the counters were cleared.
    #[must_use]
    pub fn queues(&self) -> FabricStats {
        let mut stats = FabricStats {
            max_queue_bytes: self.max_queue_bytes,
            drops: self.drops,
            downlink_busy_permille: [0; STAT_HOSTS],
        };
        let span = self.now.saturating_sub(self.counted_from);
        for (h, port) in self.ports.iter().enumerate().take(STAT_HOSTS) {
            let busy = port.busy.min(span);
            let permille = if span == 0 {
                0
            } else {
                u128::from(busy) * 1_000 / u128::from(span)
            };
            stats.downlink_busy_permille[h] = u16::try_from(permille).unwrap_or(1_000);
        }
        stats
    }

    /// Returns the bytes the queues of the port toward host `h` held at
    /// most since the counters were cleared.
    #[must_use]
    pub fn port_most(&self, h: u16) -> u32 {
        self.ports.get(usize::from(h)).map_or(0, |port| port.most)
    }

    /// Clears the counters of queues, drops and downlink use at `now`, so
    /// [`FabricNet::queues`] reports from then on.
    pub fn clear_counters(&mut self, now: Nanos) {
        self.counted_from = now.max(self.now);
        self.max_queue_bytes = self.buffered;
        self.drops = 0;
        for port in &mut self.ports {
            port.busy = 0;
            port.most = port.queued;
        }
    }

    /// Schedules `event` at `at`.
    fn at(&mut self, at: Nanos, event: Event) {
        self.seq = self.seq.wrapping_add(1);
        self.events.push(Reverse((at, self.seq, event)));
    }

    /// Acts on one event due at `at`.
    fn happen(&mut self, at: Nanos, event: Event, trace: &mut Trace) {
        match event {
            Event::HostSent(h) => {
                let Some(host) = self.hosts.get_mut(usize::from(h)) else {
                    return;
                };
                host.sending = false;
                let Some(packet) = host.uplink.pop_front() else {
                    return;
                };
                let mut delay = self.fabric.propagation;
                if self.fabric.spray {
                    delay = delay.saturating_add(self.rng.below(self.fabric.propagation.max(1)));
                }
                let slot = self.flights.park(packet);
                self.at(at.saturating_add(delay), Event::AtSwitch(slot));
                self.start_host(h, at);
                self.touch(h);
            }
            Event::AtSwitch(slot) => {
                if let Some(packet) = self.flights.take(slot) {
                    self.enqueue(at, packet, trace);
                }
            }
            Event::PortSent(h) => {
                let Some(port) = self.ports.get_mut(usize::from(h)) else {
                    return;
                };
                let Some(packet) = port.sending.take() else {
                    return;
                };
                let slot = self.flights.park(packet);
                self.at(
                    at.saturating_add(self.fabric.propagation),
                    Event::AtHost(slot),
                );
                self.start_port(h, at);
            }
            Event::AtHost(slot) => {
                if let Some(packet) = self.flights.take(slot) {
                    let to = packet.to;
                    if let Some(host) = self.hosts.get_mut(usize::from(to)) {
                        host.inbox.push_back((packet.from, packet.bytes));
                    }
                    self.touch(to);
                }
            }
        }
    }

    /// Queues `packet` at the port toward its host, or drops it where the
    /// buffer has no room for it.
    fn enqueue(&mut self, at: Nanos, packet: Packet, trace: &mut Trace) {
        let len = u32::try_from(packet.bytes.len()).unwrap_or(u32::MAX);
        let to = packet.to;
        if self.buffered.saturating_add(len) > self.fabric.buffer_bytes {
            self.drops = self.drops.saturating_add(1);
            trace.events.push(TraceEvent::Overflow {
                at,
                to,
                level: packet.level,
            });
            return;
        }
        let queues = self.queues;
        let Some(port) = self.ports.get_mut(usize::from(to)) else {
            return;
        };
        let queue = usize::from(packet.level) * queues / 8;
        if let Some(queue) = port.queues.get_mut(queue) {
            queue.push_back(packet);
        }
        port.queued = port.queued.saturating_add(len);
        port.most = port.most.max(port.queued);
        self.buffered = self.buffered.saturating_add(len);
        self.max_queue_bytes = self.max_queue_bytes.max(port.queued);
        if port.sending.is_none() {
            self.start_port(to, at);
        }
    }

    /// Starts the next packet of the port toward host `h` at `at` where its
    /// link is free.
    fn start_port(&mut self, h: u16, at: Nanos) {
        let Some(port) = self.ports.get_mut(usize::from(h)) else {
            return;
        };
        if port.sending.is_some() {
            return;
        }
        let Some(packet) = port.queues.iter_mut().rev().find_map(VecDeque::pop_front) else {
            return;
        };
        let len = u32::try_from(packet.bytes.len()).unwrap_or(u32::MAX);
        port.queued = port.queued.saturating_sub(len);
        self.buffered = self.buffered.saturating_sub(len);
        let time = self.fabric.wire_time(packet.bytes.len());
        let from = at.max(self.counted_from);
        let end = at.saturating_add(time);
        port.busy = port.busy.saturating_add(end.saturating_sub(from));
        port.sending = Some(packet);
        self.at(end, Event::PortSent(h));
    }

    /// Starts the next packet of the card of host `h` at `at` where its link
    /// is free.
    fn start_host(&mut self, h: u16, at: Nanos) {
        let Some(host) = self.hosts.get_mut(usize::from(h)) else {
            return;
        };
        if host.sending {
            return;
        }
        let Some(packet) = host.uplink.front() else {
            return;
        };
        let time = self.fabric.wire_time(packet.bytes.len());
        host.sending = true;
        self.at(at.saturating_add(time), Event::HostSent(h));
    }
}

impl Flights {
    /// Parks `packet` and returns its place.
    fn park(&mut self, packet: Packet) -> usize {
        if let Some(slot) = self.free.pop() {
            if let Some(place) = self.slots.get_mut(slot) {
                *place = Some(packet);
                return slot;
            }
        }
        self.slots.push(Some(packet));
        self.slots.len() - 1
    }

    /// Takes the packet parked at `slot`.
    fn take(&mut self, slot: usize) -> Option<Packet> {
        let packet = self.slots.get_mut(slot)?.take()?;
        self.free.push(slot);
        Some(packet)
    }
}

/// One host of a [`FabricNet`] as a carrier.
#[derive(Debug)]
pub struct FabricPort<'n> {
    /// The network.
    net: &'n mut FabricNet,
    /// The host this is.
    host: u16,
}

impl Carrier for FabricPort<'_> {
    type Peer = u16;
    type Error = core::convert::Infallible;

    /// Answers whether the card of the host is free, whatever the length
    /// and the peer, so the engine hands it one packet at a time.
    fn ready(&self, _peer: u16, _len: usize) -> bool {
        self.net
            .hosts
            .get(usize::from(self.host))
            .is_some_and(|host| host.uplink.is_empty())
    }

    /// Hands the packet to the card of the host toward host `peer` at
    /// `priority`.
    fn send(
        &mut self,
        peer: u16,
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> Result<(), Self::Error> {
        let mut bytes = Vec::with_capacity(header.len() + payload.len());
        bytes.extend_from_slice(header);
        bytes.extend_from_slice(payload);
        let packet = Packet {
            from: self.host,
            to: peer,
            level: priority.level(),
            bytes,
        };
        let now = self.net.now;
        if let Some(host) = self.net.hosts.get_mut(usize::from(self.host)) {
            host.uplink.push_back(packet);
        }
        self.net.start_host(self.host, now);
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8]) -> Result<Option<(u16, usize)>, Self::Error> {
        let Some(host) = self.net.hosts.get_mut(usize::from(self.host)) else {
            return Ok(None);
        };
        while let Some((from, bytes)) = host.inbox.pop_front() {
            if let Some(target) = buf.get_mut(..bytes.len()) {
                target.copy_from_slice(&bytes);
                return Ok(Some((from, bytes.len())));
            }
        }
        Ok(None)
    }
}

/// What the switch of a fabric saw since its counters were cleared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FabricStats {
    /// The most bytes the queues of one port held at once.
    pub max_queue_bytes: u32,
    /// The packets the switch dropped for want of buffer.
    pub drops: u64,
    /// For each of the first 64 hosts, the share of time per mille the link
    /// from the switch to it was busy.
    pub downlink_busy_permille: [u16; STAT_HOSTS],
}

impl Default for FabricStats {
    fn default() -> Self {
        Self {
            max_queue_bytes: 0,
            drops: 0,
            downlink_busy_permille: [0; STAT_HOSTS],
        }
    }
}
