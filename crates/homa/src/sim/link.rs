// SPDX-License-Identifier: MIT
/*
 * homa sim link: the loopback, the lossy link and the baseline of AoFrame
 * alone.
 *
 * The three links are those of the Homa design note, first version,
 * homa/design-v1.md section 2.5, lines 566 to 1088, with the baseline its
 * section 6.6, lines 1983 to 2004, sets AoEnvelope(Homa) against, which the
 * track plan reads as GraphTP(Homa).
 */

//! The simulated links between two ends.
//!
//! [`Loopback`] and [`Lossy`] move whole packets between two ends, each end a
//! [`Carrier`] through [`LinkEnd`] with [`Side`] as its peer, at the times
//! their [`LinkModel`] gives. A packet handed to an end waits for the wire
//! and leaves when its turn comes, and it arrives at the other end `latency`
//! later, where the carrier's `recv` takes it once [`Loopback::step`] or
//! [`Lossy::step`] has moved it there. An end is ready while it holds fewer
//! packets not yet on their way than the model allows, which is the
//! readiness the engine's pacer asks for. A link built with
//! [`Loopback::framed`] or [`Lossy::framed`] times each packet on its wire
//! by the bytes of its `AoFrame`, as a byte stream carries it, and delivers
//! the packet itself.
//!
//! [`Lossy`] draws its faults from the stream "faults" of its generator, and
//! corrupts a packet by flipping one bit of its delimited `AoFrame` and
//! reading the frame back, so a damaged packet is lost as COBS or CRC-32C
//! finds it, or arrives damaged where both miss the flip.
//!
//! [`Baseline`] moves whole messages, each in one `AoFrame` on a byte
//! stream at the model's rate, first in first out, with no Homa header, no
//! correlation, no retransmission and no flow control, and records each
//! message delivered or lost in a [`Trace`].

use alloc::{
    collections::VecDeque,
    vec,
    vec::Vec, //
};

use crate::{
    sim::{
        picked,
        Faults,
        LinkModel,
        Rng,
        Side,
        Trace,
        TraceEvent, //
    },
    stream::{
        self,
        Decoder,
        FrameError,
        Mode, //
    },
    wire::{
        Compact,
        Form,
        Packet, //
    },
    Carrier,
    Nanos,
    Priority, //
};

/// Nanoseconds in a second.
const SECOND: u128 = 1_000_000_000;

/// One packet on its way.
#[derive(Clone, Debug)]
struct Flight {
    /// When it leaves the sender.
    depart: Nanos,
    /// When it reaches the receiver.
    arrive: Nanos,
    /// The packet, header and payload.
    bytes: Vec<u8>,
    /// Why it is lost on its way, or `None`.
    lost: Option<&'static str>,
}

/// One end of a link: the packets it sends and the packets it received.
#[derive(Clone, Debug, Default)]
struct Wire {
    /// The packets on their way from this end, in the order they leave.
    flights: VecDeque<Flight>,
    /// When the wire has sent everything handed to it.
    busy_until: Nanos,
    /// The event the last packet leaves at.
    event_at: Nanos,
    /// The packets that event carries.
    event_used: u8,
    /// The packets that arrived at this end and wait for its receiver.
    inbox: VecDeque<Vec<u8>>,
}

/// What a lossy link injected so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Injected {
    /// Packets lost by `drop_per_million`.
    pub dropped: u64,
    /// GRANTs lost because their message was picked.
    pub grants_dropped: u64,
    /// LIMITS lost by `drop_limits_per_million`.
    pub limits_dropped: u64,
    /// Packets delivered twice.
    pub duplicated: u64,
    /// Packets whose frame had a bit flipped.
    pub corrupted: u64,
    /// Corrupted packets COBS and CRC-32C both missed, which arrived damaged.
    pub corrupted_delivered: u64,
    /// Packets lost because the link was down.
    pub down: u64,
    /// Packets that arrived longer than the receiver's buffer and were lost.
    pub truncated: u64,
}

/// The state both packet links share.
#[derive(Clone, Debug)]
struct Link {
    /// How the link moves packets.
    model: LinkModel,
    /// The time of the last step.
    now: Nanos,
    /// The two ends, A and B.
    wires: [Wire; 2],
    /// The faults and their generator, `None` on a perfect link.
    faults: Option<(Faults, Rng)>,
    /// What the faults injected.
    injected: Injected,
    /// The form of `AoFrame` whose bytes time a packet on the wire, `None`
    /// for the bytes of the packet alone.
    frame: Option<Mode>,
}

impl Link {
    /// Returns a link of `model` with `faults` drawn from `rng`.
    fn new(model: LinkModel, faults: Option<(Faults, Rng)>) -> Self {
        Self {
            model,
            now: 0,
            wires: [Wire::default(), Wire::default()],
            faults,
            injected: Injected::default(),
            frame: None,
        }
    }

    /// Returns the bytes a packet of `bytes` takes on the wire: its frame in
    /// the form of the link, or the packet alone.
    fn wire_len(&self, bytes: &[u8]) -> usize {
        let Some(mode) = self.frame else {
            return bytes.len();
        };
        let mut out = vec![0u8; stream::frame_len(mode, bytes.len())];
        stream::encode(mode, bytes, &[], &mut out).unwrap_or(out.len())
    }

    /// Returns the packets an end holds before the wire.
    fn depth(&self) -> usize {
        usize::from(self.model.packets_per_event.max(1))
    }

    /// Returns whether end `side` holds fewer packets not yet on their way
    /// than the model allows.
    fn ready(&self, side: Side) -> bool {
        let now = self.now;
        let waiting = self.wires[side.index()]
            .flights
            .iter()
            .filter(|flight| flight.depart > now)
            .count();
        waiting < self.depth()
    }

    /// Returns when a packet of `len` bytes handed to end `side` now leaves.
    fn schedule(&mut self, side: Side, len: usize) -> Nanos {
        let model = self.model;
        let now = self.now;
        let wire = &mut self.wires[side.index()];
        if model.event > 0 {
            let per = model.packets_per_event.max(1);
            let mut at = now
                .max(wire.event_at)
                .div_ceil(model.event)
                .saturating_mul(model.event);
            if at == wire.event_at && wire.event_used >= per {
                at = at.saturating_add(model.event);
            }
            if at != wire.event_at {
                wire.event_at = at;
                wire.event_used = 0;
            }
            wire.event_used = wire.event_used.saturating_add(1);
            return at;
        }
        let start = now.max(wire.busy_until);
        let end = start.saturating_add(wire_time(len, model.bytes_per_s));
        wire.busy_until = end;
        end
    }

    /// Hands the packet of `header` and `payload` from end `side` to the
    /// wire, with the faults of the link.
    fn send(&mut self, side: Side, header: &[u8], payload: &[u8]) {
        let mut bytes = Vec::with_capacity(header.len() + payload.len());
        bytes.extend_from_slice(header);
        bytes.extend_from_slice(payload);
        let on_wire = self.wire_len(&bytes);
        let depart = self.schedule(side, on_wire);
        let arrive = depart.saturating_add(self.model.latency);
        let mut lost = None;
        let mut twice = false;
        if let Some((faults, rng)) = self.faults.as_mut() {
            let fate = fate(faults, rng, side, &mut bytes, depart, &mut self.injected);
            lost = fate.0;
            twice = fate.1;
        }
        let wire = &mut self.wires[side.index()];
        if twice {
            wire.flights.push_back(Flight {
                depart,
                arrive,
                bytes: bytes.clone(),
                lost,
            });
        }
        wire.flights.push_back(Flight {
            depart,
            arrive,
            bytes,
            lost,
        });
    }

    /// Moves every packet due by `now` to its receiver and returns the next
    /// time a packet leaves or arrives.
    fn step(&mut self, now: Nanos) -> Option<Nanos> {
        if now > self.now {
            self.now = now;
        }
        let now = self.now;
        let [from_a, from_b] = &mut self.wires;
        arrive(from_a, &mut from_b.inbox, now);
        arrive(from_b, &mut from_a.inbox, now);
        let mut next: Option<Nanos> = None;
        for wire in &self.wires {
            for flight in &wire.flights {
                for at in [flight.depart, flight.arrive] {
                    if at > now {
                        next = Some(next.map_or(at, |n| n.min(at)));
                    }
                }
            }
        }
        next
    }

    /// Takes the next packet that arrived at end `side` into `buf`.
    fn recv(&mut self, side: Side, buf: &mut [u8]) -> Option<(Side, usize)> {
        loop {
            let bytes = self.wires[side.index()].inbox.pop_front()?;
            if let Some(target) = buf.get_mut(..bytes.len()) {
                target.copy_from_slice(&bytes);
                return Some((side.other(), bytes.len()));
            }
            self.injected.truncated = self.injected.truncated.saturating_add(1);
        }
    }
}

/// Moves every packet of `wire` that arrives by `now` into `inbox`, the
/// packets the other end waits to receive, and drops those lost on their
/// way.
fn arrive(wire: &mut Wire, inbox: &mut VecDeque<Vec<u8>>, now: Nanos) {
    while wire
        .flights
        .front()
        .is_some_and(|flight| flight.arrive <= now)
    {
        let Some(flight) = wire.flights.pop_front() else {
            break;
        };
        if flight.lost.is_none() {
            inbox.push_back(flight.bytes);
        }
    }
}

/// Returns the time `len` bytes take on a wire of `bytes_per_s`, rounded up,
/// and 0 on a wire that takes no time.
fn wire_time(len: usize, bytes_per_s: u32) -> Nanos {
    if bytes_per_s == 0 {
        return 0;
    }
    let bytes = u128::try_from(len).unwrap_or(u128::MAX);
    let nanos = bytes
        .saturating_mul(SECOND)
        .div_ceil(u128::from(bytes_per_s));
    Nanos::try_from(nanos).unwrap_or(Nanos::MAX)
}

/// Draws the faults of one packet from end `side` leaving at `depart`, which
/// may flip a bit of `bytes`, and returns why it is lost and whether it
/// arrives twice.
fn fate(
    faults: &Faults,
    rng: &mut Rng,
    side: Side,
    bytes: &mut Vec<u8>,
    depart: Nanos,
    injected: &mut Injected,
) -> (Option<&'static str>, bool) {
    let dropped = rng.chance(faults.drop_per_million);
    let limits_lost = rng.chance(faults.drop_limits_per_million);
    let corrupt = rng.chance(faults.corrupt_per_million);
    let twice = rng.chance(faults.duplicate_per_million);
    let bit = rng.next_u64();
    if faults.link_down_at.is_some_and(|down| depart >= down) {
        injected.down = injected.down.saturating_add(1);
        return (Some("down"), false);
    }
    if dropped {
        injected.dropped = injected.dropped.saturating_add(1);
        return (Some("drop"), false);
    }
    match Compact::decode(bytes).map(|decoded| decoded.packet) {
        Ok(Packet::Grant { id, .. }) => {
            let key = id ^ (u64::from(side == Side::B) << 63);
            if picked(rng.seed(), key, faults.drop_grants_per_million) {
                injected.grants_dropped = injected.grants_dropped.saturating_add(1);
                return (Some("grant"), false);
            }
        }
        Ok(Packet::Limits(_)) if limits_lost => {
            injected.limits_dropped = injected.limits_dropped.saturating_add(1);
            return (Some("limits"), false);
        }
        _ => {}
    }
    if corrupt {
        injected.corrupted = injected.corrupted.saturating_add(1);
        match damage(bytes, bit) {
            Some(packet) => {
                injected.corrupted_delivered = injected.corrupted_delivered.saturating_add(1);
                *bytes = packet;
            }
            None => return (Some("corrupt"), false),
        }
    }
    if twice {
        injected.duplicated = injected.duplicated.saturating_add(1);
    }
    (None, twice)
}

/// Frames `packet` in delimited `AoFrame`, flips the bit `bit` picks, reads
/// the frame back, and returns the packet the decoder accepts, or `None`
/// where COBS or CRC-32C refuses it.
fn damage(packet: &[u8], bit: u64) -> Option<Vec<u8>> {
    let mut frame = vec![0u8; stream::frame_len(Mode::Delimited, packet.len())];
    let len = stream::encode(Mode::Delimited, packet, &[], &mut frame).ok()?;
    let frame = frame.get_mut(..len)?;
    let bits = u64::try_from(len).ok()?.saturating_mul(8);
    let at = usize::try_from(bit % bits.max(1)).ok()?;
    let byte = frame.get_mut(at / 8)?;
    *byte ^= 1 << (at % 8);
    let mut buf = vec![0u8; Decoder::buffer_len(Mode::Delimited, packet.len())];
    let mut decoder = Decoder::new(Mode::Delimited, &mut buf);
    for &byte in frame.iter() {
        if let Some(Ok(packet)) = decoder.push(byte) {
            return Some(packet.to_vec());
        }
    }
    None
}

/// Two ends of one perfect link, each a [`Carrier`] with `Peer = Side`.
#[derive(Clone, Debug)]
pub struct Loopback {
    /// The link.
    link: Link,
}

impl Loopback {
    /// Returns a perfect link of `model`.
    #[must_use]
    pub fn new(model: LinkModel) -> Self {
        Self {
            link: Link::new(model, None),
        }
    }

    /// Returns the link timing each packet by the bytes of its `AoFrame` in
    /// the form `frame`, or by the packet alone for `None`.
    #[must_use]
    pub fn framed(mut self, frame: Option<Mode>) -> Self {
        self.link.frame = frame;
        self
    }

    /// Returns end `side` of the link as a carrier.
    pub fn end(&mut self, side: Side) -> LinkEnd<'_> {
        LinkEnd {
            link: &mut self.link,
            side,
        }
    }

    /// Moves every packet due by `now` to its receiver and returns the next
    /// due time.
    ///
    /// The time is the next at which a packet leaves, which makes its end
    /// ready again, or arrives, and `None` where no packet is on its way.
    pub fn step(&mut self, now: Nanos) -> Option<Nanos> {
        self.link.step(now)
    }
}

/// Two ends of one link with the faults of `faults`, drawn from `rng`.
#[derive(Clone, Debug)]
pub struct Lossy {
    /// The link.
    link: Link,
}

impl Lossy {
    /// Returns a link of `model` with `faults` drawn from the stream "faults"
    /// of `rng`.
    #[must_use]
    pub fn new(model: LinkModel, faults: Faults, rng: Rng) -> Self {
        Self {
            link: Link::new(model, Some((faults, rng.stream("faults")))),
        }
    }

    /// Returns the link timing each packet by the bytes of its `AoFrame` in
    /// the form `frame`, as [`Loopback::framed`] does.
    #[must_use]
    pub fn framed(mut self, frame: Option<Mode>) -> Self {
        self.link.frame = frame;
        self
    }

    /// Returns end `side` of the link as a carrier.
    pub fn end(&mut self, side: Side) -> LinkEnd<'_> {
        LinkEnd {
            link: &mut self.link,
            side,
        }
    }

    /// Moves every packet due by `now` to its receiver and returns the next
    /// due time, as [`Loopback::step`] does.
    pub fn step(&mut self, now: Nanos) -> Option<Nanos> {
        self.link.step(now)
    }

    /// Returns what the faults injected so far.
    #[must_use]
    pub fn injected(&self) -> Injected {
        self.link.injected
    }
}

/// One end of a simulated link.
#[derive(Debug)]
pub struct LinkEnd<'l> {
    /// The link.
    link: &'l mut Link,
    /// The end this is.
    side: Side,
}

impl Carrier for LinkEnd<'_> {
    type Peer = Side;
    type Error = core::convert::Infallible;

    /// Answers whether the end holds fewer packets not yet on their way than
    /// the model allows, whatever the length.
    fn ready(&self, _peer: Side, _len: usize) -> bool {
        self.link.ready(self.side)
    }

    /// Hands the packet to the wire toward the other end, whatever `peer`
    /// names, and ignores the level.
    fn send(
        &mut self,
        _peer: Side,
        _priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> Result<(), Self::Error> {
        self.link.send(self.side, header, payload);
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8]) -> Result<Option<(Side, usize)>, Self::Error> {
        Ok(self.link.recv(self.side, buf))
    }
}

/// One message of the baseline on its way.
#[derive(Clone, Debug)]
struct Carried {
    /// The number of the message.
    id: u64,
    /// When its frame arrives whole.
    arrive: Nanos,
    /// The frame, damaged where a fault flipped a bit.
    frame: Vec<u8>,
    /// The bytes of the message.
    len: usize,
    /// Why it is lost on its way, or `None`.
    lost: Option<&'static str>,
}

/// Whole messages in one `AoFrame` each, first in first out, with no Homa,
/// the baseline of ADR-0023.
///
/// Each message is framed whole in the form `mode` and put on a byte stream
/// behind the frames before it, which moves at `bytes_per_s` of the model,
/// so a small message waits for the rest of every large one ahead of it. A
/// link with events delivers at the first event after the last byte. The
/// faults are drawn once a message from the stream "faults": a message is
/// lost whole by `drop_per_million` or from `link_down_at` on, and a bit of
/// its frame flips by `corrupt_per_million`, after which the frame is read
/// back and lost as the decoder refuses it. Nothing is retransmitted.
#[derive(Clone, Debug)]
pub struct Baseline {
    /// How the stream moves bytes.
    model: LinkModel,
    /// The faults.
    faults: Faults,
    /// Their generator.
    rng: Rng,
    /// The form of the frames.
    mode: Mode,
    /// The number of the next message.
    next_id: u64,
    /// When each direction's stream has sent every frame, from A and from B.
    busy_until: [Nanos; 2],
    /// The messages on their way in each direction, from A and from B.
    queues: [VecDeque<Carried>; 2],
}

impl Baseline {
    /// Returns the baseline of `model` in the form `mode`, with `faults`
    /// drawn from the stream "faults" of `rng`.
    #[must_use]
    pub fn new(model: LinkModel, faults: Faults, rng: Rng, mode: Mode) -> Self {
        Self {
            model,
            faults,
            rng: rng.stream("faults"),
            mode,
            next_id: 0,
            busy_until: [0, 0],
            queues: [VecDeque::new(), VecDeque::new()],
        }
    }

    /// Puts `message` on the stream from end `from` at `now` and returns its
    /// number.
    pub fn send(&mut self, from: Side, now: Nanos, message: &[u8]) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let mut frame = vec![0u8; stream::frame_len(self.mode, message.len())];
        let framed = stream::encode(self.mode, &[], message, &mut frame);
        let dropped = self.rng.chance(self.faults.drop_per_million);
        let corrupt = self.rng.chance(self.faults.corrupt_per_million);
        let bit = self.rng.next_u64();
        let len = framed.unwrap_or(0);
        frame.truncate(len);
        let side = from.index();
        let start = now.max(self.busy_until[side]);
        let end = start.saturating_add(wire_time(len, self.model.bytes_per_s));
        self.busy_until[side] = end;
        let mut arrive = end;
        if self.model.event > 0 {
            arrive = arrive
                .div_ceil(self.model.event)
                .saturating_mul(self.model.event);
        }
        arrive = arrive.saturating_add(self.model.latency);
        let lost = if framed.is_err() {
            Some("too long")
        } else if self.faults.link_down_at.is_some_and(|down| start >= down) {
            Some("down")
        } else if dropped {
            Some("drop")
        } else {
            None
        };
        if lost.is_none() && corrupt && len > 0 {
            let bits = u64::try_from(len).unwrap_or(u64::MAX).saturating_mul(8);
            let at = usize::try_from(bit % bits).unwrap_or(0);
            if let Some(byte) = frame.get_mut(at / 8) {
                *byte ^= 1 << (at % 8);
            }
        }
        self.queues[side].push_back(Carried {
            id,
            arrive,
            frame,
            len: message.len(),
            lost,
        });
        id
    }

    /// Delivers every message due by `now`, records it in `trace` delivered
    /// or lost, and returns the next time a message arrives.
    pub fn step(&mut self, now: Nanos, trace: &mut Trace) -> Option<Nanos> {
        for from in [Side::A, Side::B] {
            let queue = &mut self.queues[from.index()];
            while queue.front().is_some_and(|carried| carried.arrive <= now) {
                let Some(carried) = queue.pop_front() else {
                    break;
                };
                let to = from.other();
                let at = carried.arrive;
                let event = match carried
                    .lost
                    .map_or_else(|| unframe(self.mode, &carried), Err)
                {
                    Ok(length) => TraceEvent::Deliver {
                        at,
                        to,
                        id: carried.id,
                        length,
                    },
                    Err(reason) => TraceEvent::Drop { at, to, reason },
                };
                trace.events.push(event);
            }
        }
        self.queues
            .iter()
            .filter_map(|queue| queue.front().map(|carried| carried.arrive))
            .min()
    }
}

/// Reads the frame of a baseline message back and returns the length of the
/// message the decoder accepts, or why it refuses the frame.
fn unframe(mode: Mode, carried: &Carried) -> Result<u32, &'static str> {
    let mut buf = vec![0u8; Decoder::buffer_len(mode, carried.len)];
    let mut decoder = Decoder::new(mode, &mut buf);
    for &byte in &carried.frame {
        match decoder.push(byte) {
            None => {}
            Some(Ok(packet)) => return u32::try_from(packet.len()).map_err(|_| "too long"),
            Some(Err(FrameError::TooLong)) => return Err("too long"),
            Some(Err(FrameError::Cobs)) => return Err("cobs"),
            Some(Err(FrameError::Crc)) => return Err("crc"),
        }
    }
    Err("cut")
}
