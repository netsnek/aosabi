// SPDX-License-Identifier: MIT
/*
 * homa endpoint recv: reassembly, the answers of a receiver and the checks of
 * every packet taken.
 *
 * The rules are those of receiving, of LIMITS and of retransmission in the
 * Homa design note, first version, homa/design-v1.md section 2.4, lines 504 to
 * 565, with the additions of its section 1.5, lines 282 to 320, and the check
 * of a destination port of the second version, homa/design.md section 7.3.
 */

//! The receiving half of the endpoint.
//!
//! [`Endpoint::handle`] decodes a packet with the wire form and checks it
//! against this end's limits before it stores a byte: a DATA packet whose
//! `msg_length` exceeds the `max_message` this end announced, the first
//! packet of an RPC beyond the peer's `max_rpcs`, and a packet whose offset
//! lies beyond the unscheduled bytes and every grant are refused and answered
//! with LIMITS. The first packet of a message takes the pages of the whole
//! message, so no length from the wire asks for more than the pool and the
//! limits allow.
//!
//! **Reassembly.** A message keeps its contiguous prefix and up to
//! `Config::max_gaps` missing ranges behind it, and a packet that would open
//! one more is refused as [`Refused::ThirdGap`] and comes back through
//! RESEND. The gap words of a slot hold the end of the furthest byte received
//! and each missing range as its start and its end, a range whose start
//! equals its end being unused.
//!
//! **Answers.** A server answers a RESEND or a GRANT for an RPC it does not
//! know with `RPC_UNKNOWN`, and a client answers DATA or a RESEND for an RPC
//! it does not know the same way. A sender that holds the RPC and has nothing
//! to send answers BUSY, as does a server whose RPC still executes. A client
//! answers `NEED_ACK` of an RPC it no longer holds, or holds whole, with ACK,
//! and a server frees an RPC whose ACK arrives. A build without the role
//! `client` holds no client RPC, so it answers DATA of one with
//! `RPC_UNKNOWN`, as it answers any RPC it does not know, and takes no
//! `NEED_ACK`, and the paths of a client stay out of a server's image. A
//! received LIMITS replaces the assumed values, raises the granted bytes of
//! every message sent to the peer to its unscheduled bytes, and fails a
//! request longer than its `max_message` with
//! [`Failure::TooLong`](crate::Failure::TooLong).
//!
//! **The full form.** A `START_MSG` opens a message whose every byte waits for
//! a GRANT, a DATA packet of another cutoff version than this end's answers
//! with CUTOFFS at most once a millisecond a peer, and a received CUTOFFS
//! replaces the levels of the peer. The ACK of a server's RPC frees it from
//! any port of the client's host, as `HomaModule` finds such an RPC by the
//! address and the identifier alone, and a client that marked a request for
//! incast control grants the response of it beyond the incast bytes.

use core::hash::Hash;

use crate::{
    endpoint::{
        failure_code,
        Cx,
        Endpoint,
        BUSY,
        C_DONE,
        C_OUT,
        EV_DROPPED,
        EV_FAILED,
        EV_LIMITS,
        EV_REQUEST,
        EV_RESPONSE,
        INCAST,
        KNOWN,
        RESENT,
        S_EXEC,
        S_IN,
        S_OUT,
        S_RES,
        UP, //
    },
    grant,
    store::{
        to_usize,
        NONE, //
    },
    wire::{
        AckEntry,
        Data,
        DecodeError,
        Form,
        Packet,
        Ports, //
    },
    Failure,
    Limits,
    Nanos,
    PeerPort,
    Refused,
    Store,
    MAX_PAGES, //
};

#[cfg(feature = "client")]
use crate::endpoint::{
    C_ACK,
    C_IN,
    P_ACKS, //
};

impl<P, S, W> Endpoint<P, S, W>
where
    P: Copy + Eq + Hash + PeerPort,
    S: Store<P>,
    W: Form,
{
    /// Takes one packet from `peer`.
    ///
    /// A packet whose destination port the endpoint does not own is dropped
    /// before its peer is looked up or admitted, and counted in
    /// [`Stats::wrong_port`](crate::Stats::wrong_port) alone.
    ///
    /// # Errors
    ///
    /// Returns why the packet was not taken. It is counted in
    /// [`Stats::refused`](crate::Stats::refused) and changes nothing else,
    /// but the LIMITS it may owe the peer.
    pub fn handle(&mut self, now: Nanos, peer: P, packet: &[u8]) -> Result<(), Refused> {
        let result = self.take(now, peer, packet);
        let stats = &mut self.core.stats;
        match result {
            Ok(Taken::Acted) => stats.received = stats.received.saturating_add(1),
            Ok(Taken::WrongPort) => stats.wrong_port = stats.wrong_port.saturating_add(1),
            Err(Refused::Freeze) => {
                stats.freezes_dropped = stats.freezes_dropped.saturating_add(1);
                stats.refused = stats.refused.saturating_add(1);
            }
            Err(_) => stats.refused = stats.refused.saturating_add(1),
        }
        result.map(|_| ())
    }

    /// Takes one packet, as [`Endpoint::handle`] does before it counts.
    fn take(&mut self, now: Nanos, peer: P, packet: &[u8]) -> Result<Taken, Refused> {
        if now < self.core.now {
            return Err(Refused::ClockBackwards);
        }
        self.core.now = now;
        let decoded = W::decode(packet).map_err(Refused::Malformed)?;
        if decoded.packet == Packet::Freeze {
            return Err(Refused::Freeze);
        }
        let mut cx = self.cx();
        if decoded.ports.dst != 0 && !cx.owns(decoded.ports.dst) {
            return Ok(Taken::WrongPort);
        }
        let index = match cx.find_peer(peer) {
            Some(index) if cx.peer_flags(index) & UP != 0 => index,
            Some(_) => return Err(Refused::NoPeer),
            None => cx.admit_open(peer).ok_or(Refused::NoPeer)?,
        };
        if let Some(entry) = cx.parts.peers.get_mut(to_usize(index)) {
            entry.stamp = now;
            entry.tries = 0;
        }
        if decoded.need_limits {
            cx.owe_limits(index);
        }
        cx.dispatch(index, decoded.ports, &decoded.packet)
            .map(|()| Taken::Acted)
    }
}

/// What became of a packet the endpoint did not refuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Taken {
    /// The engine acted on the packet.
    Acted,
    /// The packet names a destination port the endpoint does not own and was
    /// dropped.
    WrongPort,
}

impl<P: Copy + Eq + Hash + PeerPort> Cx<'_, '_, P> {
    /// Acts on one decoded packet from peer `peer`.
    fn dispatch(&mut self, peer: u32, ports: Ports, packet: &Packet<'_>) -> Result<(), Refused> {
        match *packet {
            Packet::Data(data) => self.on_data(peer, ports, &data),
            Packet::Grant {
                id,
                offset,
                priority,
            } => {
                self.on_grant(peer, id, offset, priority);
                Ok(())
            }
            Packet::Resend {
                id, offset, length, ..
            } => {
                self.on_resend(peer, id, offset, length);
                Ok(())
            }
            Packet::RpcUnknown { id } => {
                self.on_unknown(peer, id);
                Ok(())
            }
            Packet::Busy { id } => {
                if let Some(index) = self.slot_of(peer, id ^ 1) {
                    self.heard(index);
                }
                Ok(())
            }
            #[cfg(feature = "client")]
            Packet::NeedAck { id } => {
                self.on_need_ack(peer, id ^ 1);
                Ok(())
            }
            #[cfg(not(feature = "client"))]
            Packet::NeedAck { .. } => Ok(()),
            Packet::Ack { id, acks } => {
                // The RPC an ACK names is acknowledged as well, as HomaModule
                // ends the RPC of an ACK before its entries.
                if id != 0 {
                    self.on_ack(
                        peer,
                        AckEntry {
                            client_id: id,
                            server_port: ports.dst,
                        },
                    );
                }
                for entry in &acks {
                    self.on_ack(peer, entry);
                }
                Ok(())
            }
            Packet::Limits(limits) => {
                self.on_limits(peer, limits);
                Ok(())
            }
            Packet::Cutoffs { cutoffs, version } => {
                self.on_cutoffs(peer, cutoffs, version);
                Ok(())
            }
            Packet::StartMsg { id, msg_length } => self.on_start_msg(peer, ports, id, msg_length),
            Packet::Freeze => Err(Refused::Freeze),
        }
    }

    /// Takes the cutoffs of peer `peer`, which this end sends by from now
    /// on, and reports a change.
    #[cfg(feature = "prio")]
    fn on_cutoffs(&mut self, peer: u32, cutoffs: [u32; 8], version: u16) {
        let taken = crate::prio::Cutoffs {
            bytes: cutoffs,
            version,
        };
        let Some(entry) = self.parts.peer_prio.get_mut(to_usize(peer)) else {
            return;
        };
        if entry.cutoffs == taken {
            return;
        }
        entry.cutoffs = taken;
        entry.event = true;
        self.ev_push(peer | crate::endpoint::PEER_TAG);
    }

    /// Ignores the cutoffs of a peer in a build without the feature `prio`,
    /// which sends every packet at one level.
    #[cfg(not(feature = "prio"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature prio"
    )]
    fn on_cutoffs(&mut self, _peer: u32, _cutoffs: [u32; 8], _version: u16) {}

    /// Owes peer `peer` this end's cutoffs where a DATA packet of it named
    /// another version, at most once a millisecond a peer, as `HomaModule`
    /// answers at most once a jiffy.
    #[cfg(feature = "prio")]
    fn check_cutoffs(&mut self, peer: u32, version: u16) {
        const GAP: Nanos = 1_000_000;
        if !self.core.full_form || version == self.core.own.cutoffs.version {
            return;
        }
        let now = self.core.now;
        let due = self
            .parts
            .peer_prio
            .get(to_usize(peer))
            .is_some_and(|entry| !entry.sent || now >= entry.sent_at.saturating_add(GAP));
        if due {
            self.owe_peer(peer, crate::endpoint::P_CUTOFFS);
        }
    }

    /// Owes no cutoffs in a build without the feature `prio`.
    #[cfg(not(feature = "prio"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature prio"
    )]
    fn check_cutoffs(&mut self, _peer: u32, _version: u16) {}

    /// Takes a `START_MSG` for the RPC the peer names `id`: the first packet
    /// of a scheduled message, whose every byte this end grants.
    ///
    /// A request opens its server RPC with no byte granted, and a response
    /// turns its client RPC to its response, as `homa_start_msg_pkt` does. A
    /// `START_MSG` of a message that has begun changes nothing but the timer
    /// of its RPC.
    fn on_start_msg(
        &mut self,
        peer: u32,
        ports: Ports,
        id: u64,
        msg_length: u32,
    ) -> Result<(), Refused> {
        let local = id ^ 1;
        let announce = Data {
            id,
            msg_length,
            offset: 0,
            retransmit: false,
            ack: None,
            cutoff_version: 0,
            incast: false,
            payload: &[],
        };
        let index = if local & 1 == 0 {
            match self.client_start(peer, local, &announce)? {
                Some(index) => index,
                None => return Ok(()),
            }
        } else {
            match self.find_slot(local) {
                Some(index) => {
                    if self.parts.slots.get(to_usize(index)).map(|s| s.peer) != Some(peer) {
                        return Err(Refused::NoSlot);
                    }
                    self.heard(index);
                    return Ok(());
                }
                None => self.begin_request(peer, ports, local, &announce, true)?,
            }
        };
        self.heard(index);
        if msg_length == 0 {
            self.complete(index);
        } else {
            self.grant_key(index, msg_length);
        }
        grant::schedule(self, peer);
        Ok(())
    }

    /// Returns the slot of the client RPC `local` whose response a
    /// `START_MSG` announces, turning the RPC to its response, or `None`
    /// where the packet needs nothing more.
    #[cfg(feature = "client")]
    fn client_start(
        &mut self,
        peer: u32,
        local: u64,
        announce: &Data<'_>,
    ) -> Result<Option<u32>, Refused> {
        let Some(index) = self.slot_of(peer, local) else {
            self.owe_unknown(peer, local);
            return Ok(None);
        };
        if self.slot_state(index) == C_OUT {
            self.begin_response(index, announce, true)?;
            Ok(Some(index))
        } else {
            self.heard(index);
            Ok(None)
        }
    }

    /// Answers a `START_MSG` of the client RPC `local` with `RPC_UNKNOWN`,
    /// since a build without the role `client` holds no client RPC.
    #[cfg(not(feature = "client"))]
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the signature of the function of the role client"
    )]
    fn client_start(
        &mut self,
        peer: u32,
        local: u64,
        _announce: &Data<'_>,
    ) -> Result<Option<u32>, Refused> {
        self.owe_unknown(peer, local);
        Ok(None)
    }

    /// Returns the slot of the RPC `id` as this end names it, where it
    /// belongs to peer `peer`.
    fn slot_of(&self, peer: u32, id: u64) -> Option<u32> {
        let index = self.find_slot(id)?;
        let slot = self.parts.slots.get(to_usize(index))?;
        (slot.peer == peer).then_some(index)
    }

    /// Notes that the RPC of slot `index` heard from its peer, which restarts
    /// its timer.
    fn heard(&mut self, index: u32) {
        let now = self.core.now;
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            slot.stamp = now;
            slot.flags &= !RESENT;
        }
    }

    /// Takes a DATA packet from peer `peer`.
    fn on_data(&mut self, peer: u32, ports: Ports, data: &Data<'_>) -> Result<(), Refused> {
        if let Some(entry) = data.ack {
            self.on_ack(peer, entry);
        }
        self.check_cutoffs(peer, data.cutoff_version);
        let local = data.id ^ 1;
        let index = if local & 1 == 0 {
            self.client_data(peer, local, data)?
        } else {
            match self.find_slot(local) {
                Some(index) => {
                    if self.parts.slots.get(to_usize(index)).map(|s| s.peer) != Some(peer) {
                        return Err(Refused::NoSlot);
                    }
                    if self.slot_state(index) != S_IN {
                        return Ok(());
                    }
                    Some(index)
                }
                None => Some(self.begin_request(peer, ports, local, data, false)?),
            }
        };
        match index {
            Some(index) => self.absorb(index, data),
            None => Ok(()),
        }
    }

    /// Returns the slot of the client RPC `local` whose response a DATA
    /// packet carries, turning the RPC to its response on its first packet,
    /// or `None` where the packet needs nothing more.
    #[cfg(feature = "client")]
    fn client_data(
        &mut self,
        peer: u32,
        local: u64,
        data: &Data<'_>,
    ) -> Result<Option<u32>, Refused> {
        let Some(index) = self.slot_of(peer, local) else {
            self.owe_unknown(peer, local);
            return Ok(None);
        };
        match self.slot_state(index) {
            C_OUT => self.begin_response(index, data, false)?,
            C_IN => {}
            _ => return Ok(None),
        }
        Ok(Some(index))
    }

    /// Answers DATA of the client RPC `local` with `RPC_UNKNOWN`, since a
    /// build without the role `client` holds no client RPC.
    #[cfg(not(feature = "client"))]
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the signature of the function of the role client"
    )]
    fn client_data(
        &mut self,
        peer: u32,
        local: u64,
        _data: &Data<'_>,
    ) -> Result<Option<u32>, Refused> {
        self.owe_unknown(peer, local);
        Ok(None)
    }

    /// Checks the first packet of a message of `len` bytes against the
    /// limits this end gave peer `peer`, and owes the peer LIMITS where it
    /// breaks them.
    fn admit_message(&mut self, peer: u32, data: &Data<'_>) -> Result<Limits, Refused> {
        let local = self
            .parts
            .peers
            .get(to_usize(peer))
            .map(|entry| entry.local)
            .ok_or(Refused::NoPeer)?;
        let len = data.msg_length;
        let unsched = self.unscheduled_of(len, &local);
        let fits = len <= local.max_message && self.core.pages.needed(len) <= MAX_PAGES;
        if !fits || (data.offset > 0 && data.offset >= unsched) {
            self.owe_limits(peer);
            return Err(Refused::OverLimits);
        }
        Ok(local)
    }

    /// Returns the unscheduled bytes of a message of `len` bytes whose first
    /// packet is DATA: under all or nothing scheduling the whole message,
    /// which its sender sends without a grant, and otherwise its first
    /// `unsched_bytes` of `local`.
    fn unscheduled_of(&self, len: u32, local: &Limits) -> u32 {
        let all_or_nothing = self.core.profile.scheduling == crate::Scheduling::AllOrNothing;
        if all_or_nothing {
            len
        } else {
            len.min(local.unsched_bytes)
        }
    }

    /// Opens a server RPC for the first packet of a request, and returns its
    /// slot.
    fn begin_request(
        &mut self,
        peer: u32,
        ports: Ports,
        id: u64,
        data: &Data<'_>,
        scheduled: bool,
    ) -> Result<u32, Refused> {
        let local = self.admit_message(peer, data)?;
        let open = self
            .parts
            .peers
            .get(to_usize(peer))
            .map_or(0, |entry| entry.server_rpcs);
        if open >= u16::from(local.max_rpcs) {
            self.owe_limits(peer);
            return Err(Refused::OverLimits);
        }
        let Some(index) = self.take_slot(false) else {
            self.nudge(peer);
            return Err(Refused::NoSlot);
        };
        let port = if ports.dst == 0 {
            self.core.config.port
        } else {
            ports.dst
        };
        let now = self.core.now;
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            slot.id = id;
            slot.peer = peer;
            slot.port = port;
            slot.state = S_IN;
            slot.stamp = now;
        }
        self.enter_slot(index, id);
        if let Some(entry) = self.parts.peers.get_mut(to_usize(peer)) {
            entry.server_rpcs = entry.server_rpcs.saturating_add(1);
        }
        if !self.begin_incoming(index, data.msg_length, &local, scheduled) {
            self.free_slot(index);
            return Err(Refused::NoSlot);
        }
        Ok(index)
    }

    /// Turns the client RPC of slot `index` to its response on the first
    /// packet of it, freeing the request, which no restart needs any more.
    #[cfg(feature = "client")]
    fn begin_response(
        &mut self,
        index: u32,
        data: &Data<'_>,
        scheduled: bool,
    ) -> Result<(), Refused> {
        let peer = self
            .parts
            .slots
            .get(to_usize(index))
            .map_or(NONE, |slot| slot.peer);
        let local = self.admit_message(peer, data)?;
        let (out_page, out_len) = self
            .parts
            .slots
            .get(to_usize(index))
            .map_or((crate::store::NO_PAGE, 0), |slot| {
                (slot.out_page, slot.out_len)
            });
        let held = if out_page == crate::store::NO_PAGE {
            0
        } else {
            self.core.pages.needed(out_len)
        };
        if self.core.pages.needed(data.msg_length) > self.core.free_pages.saturating_add(held) {
            return Err(Refused::NoSlot);
        }
        self.heap_out(index);
        self.free_out(index);
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            slot.state = C_IN;
        }
        if self.begin_incoming(index, data.msg_length, &local, scheduled) {
            Ok(())
        } else {
            Err(Refused::NoSlot)
        }
    }

    /// Takes the pages of an incoming message of `len` bytes for slot
    /// `index`, grants it its unscheduled bytes, none where it is
    /// `scheduled` and opened with `START_MSG`, and clears its gaps.
    fn begin_incoming(&mut self, index: u32, len: u32, local: &Limits, scheduled: bool) -> bool {
        let Some(first) = self.take_pages(len) else {
            return false;
        };
        if let Some(own) = self.gaps_mut(index) {
            own.fill(0);
        }
        let unscheduled = if scheduled {
            0
        } else {
            self.incast_capped(index, self.unscheduled_of(len, local))
        };
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            self.give_pages(first);
            return false;
        };
        slot.in_page = first;
        slot.in_len = len;
        slot.in_recv = 0;
        slot.in_granted = unscheduled;
        self.settle(index, 0);
        self.count_size(len, unscheduled);
        true
    }

    /// Returns the `unscheduled` bytes of the response of the client RPC of
    /// slot `index`, no more than the incast bytes where its request left
    /// marked, since the server sends no more of it without a grant.
    fn incast_capped(&self, index: u32, unscheduled: u32) -> u32 {
        let marked = self
            .parts
            .slots
            .get(to_usize(index))
            .is_some_and(|slot| slot.marks & INCAST != 0 && slot.id & 1 == 0);
        match self.core.profile.incast {
            Some(incast) if cfg!(feature = "incast") && marked => {
                unscheduled.min(incast.unsched_bytes)
            }
            _ => unscheduled,
        }
    }

    /// Counts the `unscheduled` bytes of an incoming message of `len` bytes
    /// in the histogram of sizes, where the storage holds one.
    #[cfg(feature = "prio-compute")]
    fn count_size(&mut self, len: u32, unscheduled: u32) {
        let limit = self.core.profile.unsched_bytes;
        let Some(bucket) = crate::prio::Sizes::bucket(len, limit) else {
            return;
        };
        if unscheduled == 0 || self.parts.sizes.len() < crate::prio::Sizes::BUCKETS {
            return;
        }
        if let Some(count) = self.parts.sizes.get_mut(bucket) {
            *count = count.saturating_add(u64::from(unscheduled));
        }
    }

    /// Counts nothing in a build without the feature `prio-compute`.
    #[cfg(not(feature = "prio-compute"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature prio-compute"
    )]
    fn count_size(&mut self, _len: u32, _unscheduled: u32) {}

    /// Stores the payload of a DATA packet in the incoming message of slot
    /// `index` and moves the message on.
    fn absorb(&mut self, index: u32, data: &Data<'_>) -> Result<(), Refused> {
        let slot = *self
            .parts
            .slots
            .get(to_usize(index))
            .ok_or(Refused::NoSlot)?;
        if data.msg_length != slot.in_len {
            return Err(Refused::Malformed(DecodeError::Range));
        }
        let count = u32::try_from(data.payload.len()).map_err(|_| Refused::OverLimits)?;
        let end = data.offset.checked_add(count).ok_or(Refused::OverLimits)?;
        if end > slot.in_len {
            return Err(Refused::Malformed(DecodeError::Range));
        }
        if data.offset > 0 && data.offset >= slot.in_granted {
            self.owe_limits(slot.peer);
            return Err(Refused::OverLimits);
        }
        let before = self.owed(index);
        let mut recv = slot.in_recv;
        let own = self.gaps_mut(index).ok_or(Refused::NoSlot)?;
        if !fill(own, &mut recv, data.offset, end) {
            return Err(Refused::ThirdGap);
        }
        let high = own.first().copied().unwrap_or(0);
        self.heard(index);
        let pages = self.core.pages;
        pages.write(self.parts.pages, slot.in_page, data.offset, data.payload);
        if let Some(entry) = self.parts.slots.get_mut(to_usize(index)) {
            entry.in_recv = recv;
            if high > entry.in_granted {
                entry.in_granted = high.min(entry.in_len);
            }
            if data.incast && entry.id & 1 == 1 {
                entry.marks |= INCAST;
            }
        }
        self.settle(index, before);
        if recv >= slot.in_len {
            self.complete(index);
        } else {
            let left = slot.in_len.saturating_sub(self.received(index));
            let granted_all = self
                .parts
                .slots
                .get(to_usize(index))
                .is_some_and(|entry| entry.in_granted >= entry.in_len);
            if granted_all {
                self.heap_out(index);
            } else {
                self.grant_key(index, left);
            }
        }
        grant::schedule(self, slot.peer);
        Ok(())
    }

    /// Ends the incoming message of slot `index`, which arrived whole, and
    /// reports it.
    fn complete(&mut self, index: u32) {
        let before = self.owed(index);
        self.heap_out(index);
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        let (state, event) = if slot.state == S_IN {
            (S_EXEC, EV_REQUEST)
        } else {
            (C_DONE, EV_RESPONSE)
        };
        let peer = slot.peer;
        slot.state = state;
        slot.in_granted = slot.in_len;
        if let Some(expected) = self.expected_mut(peer) {
            *expected = expected.saturating_sub(before);
        }
        self.report(index, event);
    }

    /// Takes a GRANT of `offset` at `priority` for the RPC the peer names
    /// `id`.
    fn on_grant(&mut self, peer: u32, id: u64, offset: u32, priority: crate::Priority) {
        let local = id ^ 1;
        let Some(index) = self.slot_of(peer, local) else {
            if local & 1 == 1 {
                self.owe_unknown(peer, local);
            }
            return;
        };
        if !matches!(self.slot_state(index), C_OUT | S_OUT) {
            return;
        }
        self.heard(index);
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            let offset = offset.min(slot.out_len);
            if offset > slot.out_granted {
                slot.out_granted = offset;
            }
            slot.levels = (slot.levels & 0b0000_0111) | (priority.level() << 3);
        }
        self.refresh_tx(index);
    }

    /// Takes a RESEND of a range of the message this end sends for the RPC
    /// the peer names `id`.
    fn on_resend(&mut self, peer: u32, id: u64, offset: u32, length: u32) {
        let local = id ^ 1;
        let Some(index) = self.slot_of(peer, local) else {
            self.owe_unknown(peer, local);
            return;
        };
        match self.slot_state(index) {
            S_IN | S_EXEC | S_RES => {
                self.heard(index);
                self.owe_slot(index, BUSY);
            }
            C_OUT | S_OUT => self.resend_from(index, offset, length),
            _ => {}
        }
    }

    /// Takes an `RPC_UNKNOWN` for the RPC the peer names `id`.
    fn on_unknown(&mut self, peer: u32, id: u64) {
        let local = id ^ 1;
        let Some(index) = self.slot_of(peer, local) else {
            return;
        };
        match self.slot_state(index) {
            #[cfg(feature = "client")]
            C_OUT => self.restart(index),
            S_IN | S_EXEC | S_RES => self.end_slot(index, EV_DROPPED),
            S_OUT => self.free_slot(index),
            _ => {}
        }
    }

    /// Takes a `NEED_ACK` for the client RPC `id`.
    #[cfg(feature = "client")]
    fn on_need_ack(&mut self, peer: u32, id: u64) {
        match self.slot_of(peer, id) {
            None => self.owe_ack(peer, id),
            Some(index) => match self.slot_state(index) {
                C_DONE => self.owe_ack(peer, id),
                C_ACK => self.owe_peer(peer, P_ACKS),
                _ => {}
            },
        }
    }

    /// Takes one acknowledgement of a response this end sent, and frees its
    /// RPC.
    fn on_ack(&mut self, peer: u32, entry: AckEntry) {
        let id = entry.client_id | 1;
        let found = self
            .slot_of(peer, id)
            .or_else(|| self.slot_of_host(peer, id));
        if let Some(index) = found {
            if self.slot_state(index) == S_OUT {
                self.free_slot(index);
            }
        }
    }

    /// Returns the slot of the RPC `id` where it belongs to another port of
    /// the host of peer `peer`.
    fn slot_of_host(&self, peer: u32, id: u64) -> Option<u32> {
        let index = self.find_slot(id)?;
        let owner = self.parts.slots.get(to_usize(index))?.peer;
        let theirs = self.parts.peers.get(to_usize(owner))?.handle?;
        let sender = self.parts.peers.get(to_usize(peer))?.handle?;
        sender.same_host(&theirs).then_some(index)
    }

    /// Takes the LIMITS of peer `peer`.
    fn on_limits(&mut self, peer: u32, limits: Limits) {
        let Some(entry) = self.parts.peers.get_mut(to_usize(peer)) else {
            return;
        };
        entry.remote = limits;
        entry.flags |= KNOWN | EV_LIMITS;
        self.ev_push(peer | crate::endpoint::PEER_TAG);
        let count = u32::try_from(self.parts.slots.len()).unwrap_or(NONE);
        for index in 0..count {
            let Some(slot) = self.parts.slots.get(to_usize(index)) else {
                break;
            };
            if slot.peer != peer || !matches!(slot.state, C_OUT | S_OUT) {
                continue;
            }
            if slot.out_len > limits.max_message {
                let event = if slot.state == C_OUT {
                    EV_FAILED + failure_code(Failure::TooLong)
                } else {
                    EV_DROPPED
                };
                self.end_slot(index, event);
                continue;
            }
            let unsched = slot.out_len.min(limits.unsched_bytes);
            if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
                if unsched > slot.out_granted {
                    slot.out_granted = unsched;
                }
            }
            self.refresh_tx(index);
        }
    }
}

/// Adds the received range from `start` to `end` to the gap words `own` of a
/// message whose prefix ends at `recv`, and returns false where the range
/// would open one gap more than `own` holds.
///
/// `own` holds the end of the furthest byte received and then each missing
/// range below it as a start and an end. Nothing changes when the range
/// needs a gap `own` has no room for.
fn fill(own: &mut [u32], recv: &mut u32, start: u32, end: u32) -> bool {
    if end <= *recv {
        return true;
    }
    let start = start.max(*recv);
    let Some((high, pairs)) = own.split_first_mut() else {
        return false;
    };
    let free = pairs.chunks_exact(2).position(|pair| pair[0] >= pair[1]);
    if start > *high {
        let Some(slot) = free else {
            return false;
        };
        pairs[slot * 2] = *high;
        pairs[slot * 2 + 1] = start;
        *high = end;
    } else {
        let splits = pairs
            .chunks_exact(2)
            .filter(|pair| pair[0] < start && end < pair[1])
            .count();
        if splits > 0 && free.is_none() {
            return false;
        }
        let mut spare = None;
        for pair in pairs.chunks_exact_mut(2) {
            let (a, b) = (pair[0], pair[1]);
            if a >= b || end <= a || start >= b {
                continue;
            }
            if start <= a && end >= b {
                pair[0] = 0;
                pair[1] = 0;
            } else if start <= a {
                pair[0] = end;
            } else if end >= b {
                pair[1] = start;
            } else {
                pair[1] = start;
                spare = Some((end, b));
            }
        }
        if let Some((a, b)) = spare {
            if let Some(pair) = pairs.chunks_exact_mut(2).find(|pair| pair[0] >= pair[1]) {
                pair[0] = a;
                pair[1] = b;
            }
        }
        if end > *high {
            *high = end;
        }
    }
    *recv = pairs
        .chunks_exact(2)
        .filter(|pair| pair[0] < pair[1])
        .map(|pair| pair[0])
        .min()
        .unwrap_or(*high);
    true
}
