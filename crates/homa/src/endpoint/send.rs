// SPDX-License-Identifier: MIT
/*
 * homa endpoint send: packetization, the transmit order and the restarts of a
 * sender.
 *
 * The rules are those of sending and of RPC_UNKNOWN in the Homa design note,
 * first version, homa/design-v1.md section 2.4, lines 504 to 565, with the
 * FIFO share of its section 1.4 and the limits of a restart of its section
 * 1.5, and the report of RPC_UNKNOWN in place of a restart that the track plan
 * adds after docs/graphtp.md section 2.7.
 */

//! The sending half of the endpoint.
//!
//! A message is cut into packets of the peer's `max_packet` bytes, header
//! included, which never cross a page, so every payload is borrowed whole
//! from the pages. The first `unsched_bytes` of the receiver leave at once and
//! the rest as GRANTs arrive, a packet whose first byte is granted going out
//! whole, which rounds the grants up to whole packets.
//!
//! Whenever [`Endpoint::poll_transmit`] is called it returns, for a peer the
//! carrier is ready for, a control packet first and then one DATA packet of
//! the message with the fewest bytes left to send among those with bytes to
//! send now, ties to the older, except that every `fifo_every`th DATA packet
//! on a link comes from the oldest such message, which is SRPT with the FIFO
//! share by count. An acknowledgement a client owes rides the next DATA packet
//! to the same server, and leaves in an ACK packet of up to five when no DATA
//! packet to that server is ready or five are owed. A build without the role
//! `client` owes no acknowledgement and restarts no request, so neither path
//! is in its image.
//!
//! A client whose request meets `RPC_UNKNOWN` before a byte of its answer
//! arrived sends the request again from offset 0, as `HomaModule` does, unless
//! the request asked with [󰀃](crate::RequestOptions::report_unknown) for
//! [`Failure::RpcUnknown`] in place of the restart, or the request no longer
//! fits the peer's `max_message`, which fails it with [`Failure::TooLong`]. An
//! RPC that received a byte of its answer never restarts.

use core::hash::Hash;

use crate::{
    endpoint::{
        Choice,
        Cx,
        Endpoint,
        BUSY,
        C_IN,
        GRANT,
        INCAST,
        KNOWN,
        LIMITS_SENT,
        NEED_ACK,
        PEER_TAG,
        P_LIMITS,
        P_UNKNOWN,
        RESEND,
        RESENT,
        SCHEDULED,
        SENT,
        START,
        S_OUT,
        UP, //
    },
    pacer,
    store::{
        to_usize,
        NONE, //
    },
    wire::{
        AckEntry,
        Data,
        Form,
        Packet,
        Ports, //
    },
    Nanos,
    PeerPort,
    Priority,
    Store,
    Transmit,
    MAX_PAGES, //
};

#[cfg(feature = "client")]
use crate::{
    endpoint::{
        failure_code,
        C_OUT,
        EV_FAILED,
        PATIENT,
        P_ACKS,
        P_UNACKED,
        REPORT_UNKNOWN, //
    },
    wire::Acks,
    Failure,
    RequestOptions, //
};
#[cfg(feature = "server")]
use crate::{
    endpoint::{
        S_EXEC,
        S_RES, //
    },
    ReplyWriter,
    RpcId, //
};
#[cfg(any(feature = "client", feature = "server"))]
use crate::{
    SendError,
    Writer, //
};

/// The most acknowledgements one ACK packet carries, Homa's own bound.
#[cfg(feature = "client")]
const MAX_ACKS: usize = 5;

/// A packet chosen for the carrier: its peer, its level, the length of its
/// header in the header buffer, and where its payload stands in the pool.
struct Out<P> {
    /// The peer the packet goes to.
    peer: P,
    /// The level the packet leaves at.
    priority: Priority,
    /// Bytes of the header in the header buffer.
    header: usize,
    /// The start and the end of the payload in the page pool.
    payload: Option<(usize, usize)>,
}

/// The next DATA packet of a message, its header written into the header
/// buffer.
#[derive(Clone, Copy)]
struct Plan {
    /// The offset of its payload.
    offset: u32,
    /// Bytes of its payload.
    pay: u32,
    /// The start of its payload in the page pool.
    start: usize,
    /// Bytes of its header.
    header: usize,
    /// The client slot whose acknowledgement it carries, or [`NONE`].
    #[cfg(feature = "client")]
    ack: u32,
}

/// Returns a failure to queue a message with another error of the writer.
#[cfg(any(feature = "client", feature = "server"))]
const fn lift<E>(error: SendError) -> SendError<E> {
    match error {
        SendError::NoSlot => SendError::NoSlot,
        SendError::TooLong => SendError::TooLong,
        SendError::Full => SendError::Full,
        SendError::NoPeer => SendError::NoPeer,
        SendError::NoRequest => SendError::NoRequest,
        SendError::Port => SendError::Port,
        SendError::Write(never) => match never {},
    }
}

impl<P, S, W> Endpoint<P, S, W>
where
    P: Copy + Eq + Hash + PeerPort,
    S: Store<P>,
    W: Form,
{
    /// Queues a request, copied into a client slot.
    ///
    /// # Errors
    ///
    /// Returns [`SendError::Port`] for a source port the endpoint does not
    /// own, [`SendError::NoPeer`] for a peer whose link is not up,
    /// [`SendError::TooLong`] for a message longer than the peer's
    /// `max_message` or than [`MAX_PAGES`] pages, [`SendError::Full`] when the
    /// peer's `max_rpcs` are open, and [`SendError::NoSlot`] when no client
    /// slot or too few pages are free.
    #[cfg(feature = "client")]
    pub fn request(
        &mut self,
        peer: P,
        message: &[u8],
        options: RequestOptions,
    ) -> Result<crate::RpcId, SendError> {
        self.request_with(peer, message.len(), options, |writer| {
            // The writer holds exactly `message.len()` bytes, so the write
            // cannot run past it.
            let _ = writer.write(message);
            Ok(())
        })
    }

    /// Queues a request the caller writes straight into its pages.
    ///
    /// `write` receives a [`Writer`] for `len` bytes, and bytes it leaves
    /// unwritten are sent as zeros, so no byte of an earlier message leaves.
    ///
    /// # Errors
    ///
    /// Returns what [`Endpoint::request`] returns, and
    /// [`SendError::Write`] when `write` fails, which queues nothing.
    #[cfg(feature = "client")]
    pub fn request_with<E>(
        &mut self,
        peer: P,
        len: usize,
        options: RequestOptions,
        write: impl FnOnce(&mut Writer<'_>) -> Result<(), E>,
    ) -> Result<crate::RpcId, SendError<E>> {
        let mut cx = self.cx();
        let (index, id, len) = cx.open_request(peer, len, options).map_err(lift)?;
        let result = {
            let Some(mut writer) = cx.writer(index, len) else {
                cx.free_slot(index);
                return Err(SendError::NoSlot);
            };
            let result = write(&mut writer);
            writer.finish();
            result
        };
        if let Err(error) = result {
            cx.free_slot(index);
            return Err(SendError::Write(error));
        }
        cx.refresh_tx(index);
        Ok(crate::RpcId::new(id))
    }

    /// Queues the response of a server RPC.
    ///
    /// The request of the RPC is freed once the response is queued.
    ///
    /// # Errors
    ///
    /// Returns [`SendError::NoRequest`] when `id` names no server RPC that
    /// holds its whole request without a reply, [`SendError::TooLong`] for a
    /// message longer than the peer's `max_message` or than [`MAX_PAGES`]
    /// pages, and [`SendError::NoSlot`] when too few pages are free.
    #[cfg(feature = "server")]
    pub fn reply(&mut self, id: RpcId, message: &[u8]) -> Result<(), SendError> {
        self.reply_with(id, message.len(), |writer| {
            // The writer holds exactly `message.len()` bytes.
            let _ = writer.write(message);
            Ok(())
        })
    }

    /// Queues a response the caller writes straight into its pages.
    ///
    /// Bytes `write` leaves unwritten of the `len` it announced are sent as
    /// zeros.
    ///
    /// # Errors
    ///
    /// Returns what [`Endpoint::reply`] returns, and [`SendError::Write`]
    /// when `write` fails, which leaves the RPC as it stood.
    #[cfg(feature = "server")]
    pub fn reply_with<E>(
        &mut self,
        id: RpcId,
        len: usize,
        write: impl FnOnce(&mut Writer<'_>) -> Result<(), E>,
    ) -> Result<(), SendError<E>> {
        let mut cx = self.cx();
        let (index, len) = cx.open_reply(id, len).map_err(lift)?;
        let result = {
            let Some(mut writer) = cx.writer(index, len) else {
                cx.free_out(index);
                return Err(SendError::NoSlot);
            };
            let result = write(&mut writer);
            writer.finish();
            result
        };
        if let Err(error) = result {
            cx.free_out(index);
            return Err(SendError::Write(error));
        }
        cx.queue_reply(index, len);
        Ok(())
    }

    /// Reserves pages for a response of at most `max_len` bytes, written in
    /// place and committed with the length written, so a coded answer has no
    /// second copy.
    ///
    /// No packet of the response leaves before [`ReplyWriter::commit`], and
    /// the commit fixes the length the first DATA packet carries, which keeps
    /// Homa's rule that the length leads the message. A reservation dropped
    /// without a commit returns its pages, and the RPC stays open with its
    /// request.
    ///
    /// # Errors
    ///
    /// Returns [`SendError::NoRequest`] when `id` names no server RPC that
    /// holds its whole request without a reply, [`SendError::TooLong`] for a
    /// `max_len` above the peer's `max_message` or above the [`MAX_PAGES`]
    /// pages of this end's longest answer, and [`SendError::NoSlot`] when too
    /// few pages are free.
    #[cfg(feature = "server")]
    pub fn reply_reserve(&mut self, id: RpcId, max_len: u32) -> Result<ReplyWriter<'_>, SendError> {
        let cx = &mut self.cx();
        let (index, max_len) = cx.open_reply(id, to_usize(max_len))?;
        if let Some(slot) = cx.parts.slots.get_mut(to_usize(index)) {
            slot.state = S_RES;
            slot.out_len = max_len;
        }
        Ok(ReplyWriter::new(self, index, max_len))
    }

    /// Returns the next packet for a peer `ready` accepts, control first,
    /// then SRPT.
    ///
    /// `ready` answers whether the carrier takes one more packet of the given
    /// length for the given peer now. The packet is taken as sent once it is
    /// returned.
    pub fn poll_transmit(
        &mut self,
        now: Nanos,
        ready: impl Fn(P, usize) -> bool,
    ) -> Option<Transmit<'_, P>> {
        if now > self.core.now {
            self.core.now = now;
        }
        let Self { store, core, .. } = self;
        let mut cx = Cx {
            core,
            parts: store.parts(),
        };
        cx.dirty();
        let out = cx.next_packet::<W>(&ready)?;
        let Cx { core, parts } = cx;
        let core: &crate::endpoint::Core = core;
        let pages: &[u8] = parts.pages;
        let header = core.header.get(..out.header)?;
        let payload = match out.payload {
            Some((start, end)) => pages.get(start..end)?,
            None => &[],
        };
        Some(Transmit {
            peer: out.peer,
            priority: out.priority,
            header,
            payload,
        })
    }
}

impl<P: Copy + Eq + Hash + PeerPort> Cx<'_, '_, P> {
    /// Returns whether the endpoint owns the local Homa port `port`.
    pub(crate) fn owns(&self, port: u16) -> bool {
        let first = self.core.config.port;
        port >= first
            && u32::from(port) <= u32::from(first) + u32::from(self.core.config.extra_ports)
    }

    /// Opens a client slot for a request of `len` bytes to `peer`, and
    /// returns the slot, the identifier and the length.
    #[cfg(feature = "client")]
    fn open_request(
        &mut self,
        peer: P,
        len: usize,
        options: RequestOptions,
    ) -> Result<(u32, u64, u32), SendError> {
        let port = match options.source_port {
            0 => self.core.config.port,
            port if self.owns(port) => port,
            _ => return Err(SendError::Port),
        };
        let len = u32::try_from(len).map_err(|_| SendError::TooLong)?;
        let index = match self.find_peer(peer) {
            Some(index) => index,
            None => self.admit_open(peer).ok_or(SendError::NoPeer)?,
        };
        let entry = self
            .parts
            .peers
            .get(to_usize(index))
            .filter(|entry| entry.flags & UP != 0)
            .ok_or(SendError::NoPeer)?;
        let remote = entry.remote;
        if len > remote.max_message || self.core.pages.needed(len) > MAX_PAGES {
            return Err(SendError::TooLong);
        }
        if entry.client_rpcs >= u16::from(remote.max_rpcs) {
            return Err(SendError::Full);
        }
        let slot = self.take_slot(true).ok_or(SendError::NoSlot)?;
        let Some(first) = self.take_pages(len) else {
            self.put_back(slot, true);
            return Err(SendError::NoSlot);
        };
        let id = self.take_id();
        let now = self.core.now;
        let mut flags = 0;
        if options.patient {
            flags |= PATIENT;
        }
        if options.report_unknown {
            flags |= REPORT_UNKNOWN;
        }
        let marks = self.incast_mark();
        if let Some(entry) = self.parts.slots.get_mut(to_usize(slot)) {
            entry.id = id;
            entry.peer = index;
            entry.port = port;
            entry.state = C_OUT;
            entry.stamp = now;
            entry.out_len = len;
            entry.out_page = first;
            entry.flags = flags;
            entry.marks = marks;
        }
        self.start_out(slot, len, remote.unsched_bytes);
        self.enter_slot(slot, id);
        if let Some(entry) = self.parts.peers.get_mut(to_usize(index)) {
            entry.client_rpcs = entry.client_rpcs.saturating_add(1);
        }
        Ok((slot, id, len))
    }

    /// Returns the marks of a new request: [`INCAST`] where incast control
    /// acts and the client RPCs outstanding before it exceed its threshold,
    /// which the request counts in [󰀁](crate::Stats::incast_marked).
    #[cfg(feature = "client")]
    fn incast_mark(&mut self) -> u8 {
        let Some(incast) = self.core.profile.incast else {
            return 0;
        };
        // The new request holds a client slot already, so the RPCs before it
        // are one fewer than the slots in use.
        let before = self.core.clients_open.saturating_sub(1);
        if !cfg!(feature = "incast") || before <= u32::from(incast.threshold) {
            return 0;
        }
        let stats = &mut self.core.stats;
        stats.incast_marked = stats.incast_marked.saturating_add(1);
        INCAST
    }

    /// Admits `peer` with the profile's limits under
    /// [`Admission::Open`](crate::Admission::Open), or answers `None`.
    pub(crate) fn admit_open(&mut self, peer: P) -> Option<u32> {
        if self.core.profile.admission != crate::Admission::Open {
            return None;
        }
        let index = self.admit(peer)?;
        let limits = self.open_limits();
        self.start_peer(index, limits, limits);
        Some(index)
    }

    /// Returns the limits of a peer admitted on its first packet: the
    /// profile's packet and unscheduled bytes, the longest message the pages
    /// hold, and every RPC a byte can count.
    pub(crate) fn open_limits(&self) -> crate::Limits {
        let pages = MAX_PAGES.min(self.core.config.pages);
        crate::Limits {
            max_packet: self.core.profile.max_packet,
            max_message: pages.saturating_mul(self.core.config.page_len),
            unsched_bytes: self.core.profile.unsched_bytes.max(1),
            max_rpcs: u8::MAX,
        }
    }

    /// Returns a slot taken from its free list and never used.
    #[cfg(feature = "client")]
    fn put_back(&mut self, index: u32, client: bool) {
        let head = if client {
            core::mem::replace(&mut self.core.free_client, index)
        } else {
            core::mem::replace(&mut self.core.free_server, index)
        };
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            *slot = crate::Slot::VACANT;
            slot.next = head;
        }
    }

    /// Takes the next client identifier no live RPC holds.
    #[cfg(feature = "client")]
    fn take_id(&mut self) -> u64 {
        let mut id = self.core.next_id;
        for _ in 0..=self.parts.slots.len() {
            id = self.core.next_id;
            let mut next = id.wrapping_add(2) & self.core.id_mask;
            if next == 0 {
                next = 2;
            }
            self.core.next_id = next;
            if self.find_slot(id).is_none() {
                break;
            }
        }
        id
    }

    /// Checks the server RPC `id` for a reply of `len` bytes and takes its
    /// pages, and returns the slot and the length.
    #[cfg(feature = "server")]
    fn open_reply(&mut self, id: RpcId, len: usize) -> Result<(u32, u32), SendError> {
        if !id.is_server() {
            return Err(SendError::NoRequest);
        }
        let index = self
            .find_slot(id.get())
            .filter(|&i| self.slot_state(i) == S_EXEC)
            .ok_or(SendError::NoRequest)?;
        let len = u32::try_from(len).map_err(|_| SendError::TooLong)?;
        let peer = self
            .parts
            .slots
            .get(to_usize(index))
            .map_or(NONE, |slot| slot.peer);
        let max_message = self
            .parts
            .peers
            .get(to_usize(peer))
            .map_or(0, |entry| entry.remote.max_message);
        if len > max_message || self.core.pages.needed(len) > MAX_PAGES {
            return Err(SendError::TooLong);
        }
        let first = self.take_pages(len).ok_or(SendError::NoSlot)?;
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            slot.out_page = first;
        }
        Ok((index, len))
    }

    /// Sends the client request of slot `index` again from offset 0 after
    /// `RPC_UNKNOWN`, reports [`Failure::RpcUnknown`] where the request asked
    /// for it, or fails it with [`Failure::TooLong`] where it no longer fits
    /// the peer.
    #[cfg(feature = "client")]
    pub(crate) fn restart(&mut self, index: u32) {
        let Some(slot) = self.parts.slots.get(to_usize(index)) else {
            return;
        };
        if slot.state != C_OUT {
            return;
        }
        let (flags, len, peer) = (slot.flags, slot.out_len, slot.peer);
        let remote = self
            .parts
            .peers
            .get(to_usize(peer))
            .map(|entry| entry.remote);
        if flags & REPORT_UNKNOWN != 0 {
            self.core.stats.unknown_reported = self.core.stats.unknown_reported.saturating_add(1);
            self.end_slot(index, EV_FAILED + failure_code(Failure::RpcUnknown));
            return;
        }
        let Some(remote) = remote.filter(|remote| len <= remote.max_message) else {
            self.end_slot(index, EV_FAILED + failure_code(Failure::TooLong));
            return;
        };
        let now = self.core.now;
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            slot.out_next = 0;
            slot.out_max = 0;
            slot.stamp = now;
            slot.flags &= !(RESENT | SENT);
        }
        self.start_out(index, len, remote.unsched_bytes);
        self.core.stats.restarts = self.core.stats.restarts.saturating_add(1);
        self.refresh_tx(index);
    }

    /// Moves the outgoing message of slot `index` back to the range a RESEND
    /// names, raising its grant where the profile says so, and answers BUSY
    /// where the range holds bytes this end has not sent yet, or none.
    pub(crate) fn resend_from(&mut self, index: u32, offset: u32, length: u32) {
        let raises = self.core.profile.resend_raises_grant;
        let now = self.core.now;
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        // A scheduled message no GRANT reached yet has sent nothing but its
        // START_MSG, which a RESEND of everything asks for again, as
        // HomaModule answers it.
        if slot.marks & SCHEDULED != 0 && slot.out_granted == 0 && length == u32::MAX {
            slot.stamp = now;
            slot.flags &= !RESENT;
            self.owe_slot(index, START);
            return;
        }
        let end = if length == u32::MAX {
            slot.out_max
        } else {
            offset.saturating_add(length)
        }
        .min(slot.out_len);
        if raises && end > slot.out_granted {
            slot.out_granted = end;
        }
        // Bytes of the range that left already go again, and bytes that
        // have not left yet wait for their turn, which BUSY tells the
        // receiver so its timer does not count the wait as silence.
        let unsent = end > slot.out_next || (slot.out_len == 0 && slot.flags & SENT == 0);
        if offset < slot.out_next {
            slot.out_next = offset;
        }
        if slot.out_len == 0 && slot.flags & SENT != 0 {
            slot.flags &= !SENT;
        }
        slot.stamp = now;
        slot.flags &= !RESENT;
        self.refresh_tx(index);
        if unsent || end <= offset {
            self.owe_slot(index, BUSY);
        }
    }

    /// Chooses the next packet: a control packet first, then DATA.
    fn next_packet<W: Form>(&mut self, ready: &impl Fn(P, usize) -> bool) -> Option<Out<P>> {
        if let Some(out) = self.next_control::<W>(ready) {
            return Some(out);
        }
        self.next_data::<W>(ready)
    }

    /// Walks the control queue and returns the first control packet whose
    /// peer the carrier is ready for.
    fn next_control<W: Form>(&mut self, ready: &impl Fn(P, usize) -> bool) -> Option<Out<P>> {
        let mut prev = NONE;
        let mut node = self.core.ctl_head;
        let bound = self.parts.slots.len() + self.parts.peers.len() + 1;
        for _ in 0..bound {
            if node == NONE {
                return None;
            }
            let next = self.ctl_next(node);
            let (out, empty) = if node & PEER_TAG != 0 {
                self.peer_control::<W>(node & !PEER_TAG, ready)
            } else {
                self.slot_control::<W>(node, ready)
            };
            if empty {
                self.ctl_unlink_after(prev, node);
            } else {
                prev = node;
            }
            if out.is_some() {
                return out;
            }
            node = next;
        }
        None
    }

    /// Returns the next control packet owed to peer `index` where the carrier
    /// is ready, and whether the peer owes nothing more.
    fn peer_control<W: Form>(
        &mut self,
        index: u32,
        ready: &impl Fn(P, usize) -> bool,
    ) -> (Option<Out<P>>, bool) {
        let Some(entry) = self.parts.peers.get(to_usize(index)) else {
            return (None, true);
        };
        let Some(handle) = entry.handle.filter(|_| entry.flags & UP != 0) else {
            return (None, true);
        };
        let (ctl, local, unknown) = (entry.ctl, entry.local, entry.unknown);
        let ports = Ports {
            src: self.core.config.port,
            dst: handle.homa_port(),
        };
        let need = self.need_limits(index);
        if ctl & P_LIMITS != 0 {
            let out = self.control::<W>(handle, &Packet::Limits(local), ports, need, ready);
            if out.is_some() {
                self.limits_sent(index);
                self.clear_peer(index, P_LIMITS);
            }
            return (out, self.peer_idle(index));
        }
        if ctl & P_UNKNOWN != 0 {
            let out = self.control::<W>(
                handle,
                &Packet::RpcUnknown { id: unknown },
                ports,
                need,
                ready,
            );
            if out.is_some() {
                self.clear_peer(index, P_UNKNOWN);
            }
            return (out, self.peer_idle(index));
        }
        #[cfg(feature = "client")]
        if ctl & (P_ACKS | P_UNACKED) != 0 {
            let out = self.ack_packet::<W>(index, handle, ports, need, ready);
            return (out, self.peer_idle(index));
        }
        #[cfg(feature = "prio")]
        if ctl & crate::endpoint::P_CUTOFFS != 0 {
            let out = self.cutoffs_packet::<W>(index, handle, ports, ready);
            return (out, self.peer_idle(index));
        }
        (None, true)
    }

    /// Returns this end's CUTOFFS for peer `index` where the carrier is
    /// ready, and notes when it left.
    #[cfg(feature = "prio")]
    fn cutoffs_packet<W: Form>(
        &mut self,
        index: u32,
        handle: P,
        ports: Ports,
        ready: &impl Fn(P, usize) -> bool,
    ) -> Option<Out<P>> {
        let own = self.core.own.cutoffs;
        let packet = Packet::Cutoffs {
            cutoffs: own.bytes,
            version: own.version,
        };
        let out = self.control::<W>(handle, &packet, ports, false, ready)?;
        self.clear_peer(index, crate::endpoint::P_CUTOFFS);
        let now = self.core.now;
        if let Some(entry) = self.parts.peer_prio.get_mut(to_usize(index)) {
            entry.sent_at = now;
            entry.sent = true;
        }
        let stats = &mut self.core.stats;
        stats.cutoffs_sent = stats.cutoffs_sent.saturating_add(1);
        Some(out)
    }

    /// Returns whether peer `index` owes no control packet.
    fn peer_idle(&self, index: u32) -> bool {
        self.parts
            .peers
            .get(to_usize(index))
            .is_none_or(|entry| entry.ctl == 0)
    }

    /// Clears the control bits `bits` of peer `index`.
    fn clear_peer(&mut self, index: u32, bits: u8) {
        if let Some(entry) = self.parts.peers.get_mut(to_usize(index)) {
            entry.ctl &= !bits;
        }
    }

    /// Notes a LIMITS sent to peer `index`.
    fn limits_sent(&mut self, index: u32) {
        let now = self.core.now;
        if let Some(entry) = self.parts.peers.get_mut(to_usize(index)) {
            entry.limits_at = now;
            entry.flags |= LIMITS_SENT;
        }
        self.core.stats.limits_sent = self.core.stats.limits_sent.saturating_add(1);
    }

    /// Returns whether packets to peer `index` set `NEED_LIMITS`.
    fn need_limits(&self, index: u32) -> bool {
        self.core.limits_on_wire
            && self
                .parts
                .peers
                .get(to_usize(index))
                .is_some_and(|entry| entry.flags & KNOWN == 0)
    }

    /// Returns the bytes of the largest packet to peer `index`: the peer's
    /// `max_packet`, never above what this end's carrier sends.
    fn packet_max(&self, index: u32) -> usize {
        let remote = self
            .parts
            .peers
            .get(to_usize(index))
            .map_or(0, |entry| entry.remote.max_packet);
        usize::from(remote.min(self.core.profile.max_packet))
    }

    /// Writes the header of `packet` into the header buffer and returns the
    /// packet where the carrier is ready for it.
    fn control<W: Form>(
        &mut self,
        peer: P,
        packet: &Packet<'_>,
        ports: Ports,
        need: bool,
        ready: &impl Fn(P, usize) -> bool,
    ) -> Option<Out<P>> {
        let len = W::header(packet, ports, need, &mut self.core.header).ok()?;
        if !pacer::admits(ready, peer, len) {
            return None;
        }
        let priority = self.control_level();
        self.count_sent(priority);
        self.charge(len);
        Some(Out {
            peer,
            priority,
            header: len,
            payload: None,
        })
    }

    /// Counts one packet sent at `priority`.
    fn count_sent(&mut self, priority: Priority) {
        let stats = &mut self.core.stats;
        stats.sent = stats.sent.saturating_add(1);
        if let Some(count) = stats.per_priority.get_mut(usize::from(priority.level())) {
            *count = count.saturating_add(1);
        }
    }

    /// Returns the level of every control packet, the highest level in use.
    #[cfg(feature = "prio")]
    fn control_level(&self) -> Priority {
        if !self.core.full_form {
            return Priority::HIGHEST;
        }
        let levels = self.core.own.levels;
        levels.external(levels.top())
    }

    /// Returns the one level of a build without the feature `prio`.
    #[cfg(not(feature = "prio"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature prio"
    )]
    fn control_level(&self) -> Priority {
        Priority::HIGHEST
    }

    /// Returns the level the DATA packet at `offset` of the outgoing message
    /// of slot `index` leaves at.
    ///
    /// A byte the receiver's unscheduled bytes cover goes at the level the
    /// receiver's cutoffs give the length of the message, as
    /// `homa_unsched_priority` gives it, and a byte a GRANT released at the
    /// level the GRANT named. A byte sent again goes at this end's highest
    /// level. `HomaModule` sends it at the level the RESEND names, which is
    /// the receiver's highest level, so the two differ only where the ends use
    /// different counts of levels.
    #[cfg(feature = "prio")]
    fn data_level(&self, index: u32, offset: u32, retransmit: bool) -> Priority {
        if !self.core.full_form {
            return Priority::HIGHEST;
        }
        let levels = self.core.own.levels;
        let Some(slot) = self.parts.slots.get(to_usize(index)) else {
            return levels.external(levels.top());
        };
        if retransmit {
            return levels.external(levels.top());
        }
        let unsched = if slot.marks & SCHEDULED != 0 {
            0
        } else {
            self.unsched_limit(index)
        };
        if offset < unsched || slot.out_len == 0 {
            let cutoffs = self
                .parts
                .peer_prio
                .get(to_usize(slot.peer))
                .map_or(crate::prio::Cutoffs::module_default(), |entry| {
                    entry.cutoffs
                });
            return levels.external(cutoffs.unscheduled(slot.out_len).level());
        }
        levels.external((slot.levels >> 3) & 7)
    }

    /// Returns the one level of a build without the feature `prio`.
    #[cfg(not(feature = "prio"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature prio"
    )]
    fn data_level(&self, _index: u32, _offset: u32, _retransmit: bool) -> Priority {
        Priority::HIGHEST
    }

    /// Returns the version of peer `peer`'s cutoffs a DATA packet names.
    #[cfg(feature = "prio")]
    fn cutoff_version(&self, peer: u32) -> u16 {
        self.parts
            .peer_prio
            .get(to_usize(peer))
            .map_or(0, |entry| entry.cutoffs.version)
    }

    /// Returns 0, the version a build without the feature `prio` names.
    #[cfg(not(feature = "prio"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature prio"
    )]
    fn cutoff_version(&self, _peer: u32) -> u16 {
        0
    }

    /// Charges a packet of `len` bytes the pacer lets through whatever the
    /// backlog, a control packet, to the backlog of the link.
    #[cfg(feature = "pacer-rate")]
    fn charge(&mut self, len: usize) {
        let now = self.core.now;
        if let Some(pacer) = self.core.rate.pacer.as_mut() {
            pacer.charge(now, len);
        }
    }

    /// Charges nothing in a build without the feature `pacer-rate`.
    #[cfg(not(feature = "pacer-rate"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature pacer-rate"
    )]
    fn charge(&mut self, _len: usize) {}

    /// Returns whether the rate pacer lets a DATA packet of `len` bytes of a
    /// `msg_len` byte message go now, charging it where it does, and notes
    /// when it lets the packet go where it holds it.
    #[cfg(feature = "pacer-rate")]
    fn pace(&mut self, len: usize, msg_len: u32) -> bool {
        let now = self.core.now;
        let Some(pacer) = self.core.rate.pacer.as_mut() else {
            return true;
        };
        if pacer.admit(now, len, msg_len) {
            self.core.rate.wake = None;
            true
        } else {
            self.core.rate.wake = Some(pacer.next_admit(now));
            false
        }
    }

    /// Forgets the packet the rate pacer held, which the next choice of a
    /// DATA packet decides again.
    #[cfg(feature = "pacer-rate")]
    fn unpace(&mut self) {
        self.core.rate.wake = None;
    }

    /// Holds nothing in a build without the feature `pacer-rate`.
    #[cfg(not(feature = "pacer-rate"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature pacer-rate"
    )]
    fn unpace(&mut self) {}

    /// Lets every DATA packet go in a build without the feature
    /// `pacer-rate`.
    #[cfg(not(feature = "pacer-rate"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature pacer-rate"
    )]
    fn pace(&mut self, _len: usize, _msg_len: u32) -> bool {
        true
    }

    /// Returns an ACK packet for peer `index` where one is due and the
    /// carrier is ready: the RPC an ACK is owed for without a slot, and the
    /// acknowledgements of released client RPCs, which leave in their own
    /// packet only where no DATA packet to the peer is ready to carry one or
    /// five wait.
    #[cfg(feature = "client")]
    fn ack_packet<W: Form>(
        &mut self,
        index: u32,
        handle: P,
        ports: Ports,
        need: bool,
        ready: &impl Fn(P, usize) -> bool,
    ) -> Option<Out<P>> {
        let entry = self.parts.peers.get(to_usize(index))?;
        let (ctl, unacked, head) = (entry.ctl, entry.unacked, entry.acks);
        let mut entries = [AckEntry::default(); MAX_ACKS];
        let mut count = 0;
        let mut id = 0;
        if ctl & P_UNACKED != 0 {
            entries[0] = AckEntry {
                client_id: unacked,
                server_port: handle.homa_port(),
            };
            count = 1;
            id = unacked;
        }
        let waiting = self.ack_count(head);
        let explicit = ctl & P_UNACKED != 0 || waiting >= MAX_ACKS || !self.has_data(index);
        if ctl & P_ACKS != 0 && waiting == 0 {
            self.clear_peer(index, P_ACKS);
        }
        if !explicit || (count == 0 && waiting == 0) {
            return None;
        }
        let mut at = head;
        while count < MAX_ACKS && at != NONE {
            let Some(slot) = self.parts.slots.get(to_usize(at)) else {
                break;
            };
            entries[count] = AckEntry {
                client_id: slot.id,
                server_port: handle.homa_port(),
            };
            count += 1;
            at = slot.next;
        }
        let max = self.packet_max(index);
        let mut take = count;
        let len = loop {
            let packet = Packet::Ack {
                id,
                acks: Acks::from_entries(entries.get(..take)?),
            };
            let len = W::header(&packet, ports, need, &mut self.core.header).ok()?;
            if len <= max || take <= 1 {
                break len;
            }
            take -= 1;
        };
        if !pacer::admits(ready, handle, len) {
            return None;
        }
        let priority = self.control_level();
        self.count_sent(priority);
        self.charge(len);
        let from_list = if ctl & P_UNACKED != 0 { take - 1 } else { take };
        if ctl & P_UNACKED != 0 {
            self.clear_peer(index, P_UNACKED);
        }
        for _ in 0..from_list {
            self.pop_ack(index);
        }
        if self.ack_count(self.ack_head(index)) == 0 {
            self.clear_peer(index, P_ACKS);
        }
        Some(Out {
            peer: handle,
            priority,
            header: len,
            payload: None,
        })
    }

    /// Returns the first client slot whose acknowledgement waits for peer
    /// `index`.
    #[cfg(feature = "client")]
    fn ack_head(&self, index: u32) -> u32 {
        self.parts
            .peers
            .get(to_usize(index))
            .map_or(NONE, |entry| entry.acks)
    }

    /// Returns the count of acknowledgements waiting from `head` on, up to
    /// one more than an ACK packet carries.
    #[cfg(feature = "client")]
    fn ack_count(&self, head: u32) -> usize {
        let mut count = 0;
        let mut at = head;
        while at != NONE && count <= MAX_ACKS {
            count += 1;
            at = self
                .parts
                .slots
                .get(to_usize(at))
                .map_or(NONE, |slot| slot.next);
        }
        count
    }

    /// Frees the first client slot whose acknowledgement waits for peer
    /// `index`, now that its ACK left.
    #[cfg(feature = "client")]
    fn pop_ack(&mut self, index: u32) {
        let head = self.ack_head(index);
        if head == NONE {
            return;
        }
        let next = self
            .parts
            .slots
            .get(to_usize(head))
            .map_or(NONE, |slot| slot.next);
        if let Some(entry) = self.parts.peers.get_mut(to_usize(index)) {
            entry.acks = next;
        }
        self.free_slot(head);
    }

    /// Returns whether a DATA packet to peer `index` has bytes to send now.
    #[cfg(feature = "client")]
    fn has_data(&self, index: u32) -> bool {
        let len = to_usize(self.core.tx_len);
        self.parts
            .tx_heap
            .get(..len)
            .unwrap_or(&[])
            .iter()
            .filter(|item| item.key() != u32::MAX)
            .any(|item| {
                self.parts
                    .slots
                    .get(to_usize(item.slot()))
                    .is_some_and(|slot| slot.peer == index)
            })
    }

    /// Returns the next control packet owed for slot `index` where the
    /// carrier is ready, and whether the slot owes nothing more.
    fn slot_control<W: Form>(
        &mut self,
        index: u32,
        ready: &impl Fn(P, usize) -> bool,
    ) -> (Option<Out<P>>, bool) {
        let Some(slot) = self.parts.slots.get(to_usize(index)) else {
            return (None, true);
        };
        let (ctl, id, peer, port, granted, out_len) = (
            slot.ctl,
            slot.id,
            slot.peer,
            slot.port,
            slot.in_granted,
            slot.out_len,
        );
        let grant_level = self.grant_level(index);
        let Some(handle) = self
            .handle(peer)
            .filter(|_| self.peer_flags(peer) & UP != 0)
        else {
            return (None, true);
        };
        let ports = Ports {
            src: port,
            dst: handle.homa_port(),
        };
        let need = self.need_limits(peer);
        let high = self.resend_level();
        let (bit, packet) = if ctl & GRANT != 0 {
            let packet = Packet::Grant {
                id,
                offset: granted,
                priority: grant_level,
            };
            (GRANT, packet)
        } else if ctl & START != 0 {
            let packet = Packet::StartMsg {
                id,
                msg_length: out_len,
            };
            (START, packet)
        } else if ctl & RESEND != 0 {
            let Some((offset, length)) = self.resend_range(index) else {
                self.clear_slot(index, RESEND);
                return (None, self.slot_idle(index));
            };
            let packet = Packet::Resend {
                id,
                offset,
                length,
                priority: high,
            };
            (RESEND, packet)
        } else if ctl & BUSY != 0 {
            (BUSY, Packet::Busy { id })
        } else if ctl & NEED_ACK != 0 {
            (NEED_ACK, Packet::NeedAck { id })
        } else {
            return (None, true);
        };
        let out = self.control::<W>(handle, &packet, ports, need, ready);
        if out.is_some() {
            self.clear_slot(index, bit);
            self.after_control(index, &packet);
        }
        (out, self.slot_idle(index))
    }

    /// Returns whether slot `index` owes no control packet.
    fn slot_idle(&self, index: u32) -> bool {
        self.parts
            .slots
            .get(to_usize(index))
            .is_none_or(|slot| slot.ctl == 0)
    }

    /// Clears the control bits `bits` of slot `index`.
    fn clear_slot(&mut self, index: u32, bits: u8) {
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            slot.ctl &= !bits;
        }
    }

    /// Returns the level of the GRANTs to the incoming message of slot
    /// `index`, which the grant scheduler chose for it.
    #[cfg(feature = "prio")]
    fn grant_level(&self, index: u32) -> Priority {
        let level = self
            .parts
            .slots
            .get(to_usize(index))
            .map_or(0, |slot| slot.levels & 7);
        Priority::new(level).unwrap_or(Priority::HIGHEST)
    }

    /// Returns the one level of a build without the feature `prio`.
    #[cfg(not(feature = "prio"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature prio"
    )]
    fn grant_level(&self, _index: u32) -> Priority {
        Priority::HIGHEST
    }

    /// Returns the level a RESEND names for the bytes it asks for, the highest
    /// level in use, as `homa_request_retrans` names it.
    #[cfg(feature = "prio")]
    fn resend_level(&self) -> Priority {
        if !self.core.full_form {
            return Priority::HIGHEST;
        }
        Priority::new(self.core.own.levels.top()).unwrap_or(Priority::HIGHEST)
    }

    /// Returns the one level of a build without the feature `prio`.
    #[cfg(not(feature = "prio"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature prio"
    )]
    fn resend_level(&self) -> Priority {
        Priority::HIGHEST
    }

    /// Counts what a control packet sent for slot `index` changes: a GRANT
    /// sent, a `START_MSG` sent, a RESEND sent whose range counts as granted.
    fn after_control(&mut self, index: u32, packet: &Packet<'_>) {
        match *packet {
            Packet::Grant { .. } => {
                self.core.stats.grants_sent = self.core.stats.grants_sent.saturating_add(1);
            }
            Packet::StartMsg { .. } => {
                let now = self.core.now;
                if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
                    slot.stamp = now;
                    slot.flags &= !RESENT;
                }
                self.core.stats.start_msg_sent = self.core.stats.start_msg_sent.saturating_add(1);
            }
            Packet::Resend { offset, length, .. } => {
                self.core.stats.resends = self.core.stats.resends.saturating_add(1);
                if self.core.profile.resend_raises_grant && length != u32::MAX {
                    let before = self.owed(index);
                    if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
                        let end = offset.saturating_add(length).min(slot.in_len);
                        if end > slot.in_granted {
                            slot.in_granted = end;
                        }
                    }
                    self.settle(index, before);
                }
            }
            _ => {}
        }
    }

    /// Returns the range a RESEND of slot `index` names: the first missing
    /// range of its incoming message, or everything sent so far of a response
    /// not yet begun.
    fn resend_range(&self, index: u32) -> Option<(u32, u32)> {
        let slot = self.parts.slots.get(to_usize(index))?;
        #[cfg(feature = "client")]
        if slot.state == C_OUT {
            return Some((0, u32::MAX));
        }
        if !matches!(slot.state, C_IN | crate::endpoint::S_IN) {
            return None;
        }
        let stride = crate::endpoint::gap_stride(&self.core.config);
        let start = to_usize(index).checked_mul(stride)?;
        let own = self.parts.gaps.get(start..start.checked_add(stride)?)?;
        let high = own.first().copied().unwrap_or(0);
        if slot.in_recv < high {
            let end = own
                .get(1..)
                .unwrap_or(&[])
                .chunks_exact(2)
                .find(|pair| pair[0] == slot.in_recv && pair[1] > pair[0])
                .map_or(high, |pair| pair[1]);
            return Some((slot.in_recv, end - slot.in_recv));
        }
        let expected = slot.in_granted.min(slot.in_len);
        if slot.in_recv < expected {
            return Some((slot.in_recv, expected - slot.in_recv));
        }
        None
    }

    /// Returns the next DATA packet, SRPT among the messages whose peer the
    /// carrier is ready for, with the FIFO share by count.
    fn next_data<W: Form>(&mut self, ready: &impl Fn(P, usize) -> bool) -> Option<Out<P>> {
        self.unpace();
        let (index, fifo) = self.pick::<W>(ready)?;
        let plan = self.plan::<W>(index)?;
        let slot = self.parts.slots.get(to_usize(index))?;
        let (peer_index, id, left) = (slot.peer, slot.id, slot.out_len.saturating_sub(plan.offset));
        let peer = self.handle(peer_index)?;
        let len = plan.header + to_usize(plan.pay);
        if !pacer::admits(ready, peer, len) {
            return None;
        }
        let (msg_len, max) = self
            .parts
            .slots
            .get(to_usize(index))
            .map_or((0, 0), |slot| (slot.out_len, slot.out_max));
        if !self.pace(len, msg_len) {
            return None;
        }
        let priority = self.data_level(index, plan.offset, plan.offset < max);
        self.core.last = Choice {
            valid: true,
            id,
            bytes_left: left,
            fifo,
        };
        self.sent_data(index, &plan);
        self.count_sent(priority);
        self.turn_uplink(peer_index);
        Some(Out {
            peer,
            priority,
            header: plan.header,
            payload: Some((plan.start, plan.start + to_usize(plan.pay))),
        })
    }

    /// Returns the slot whose DATA packet goes next and whether the FIFO
    /// share chose it.
    fn pick<W: Form>(&mut self, ready: &impl Fn(P, usize) -> bool) -> Option<(u32, bool)> {
        let len = to_usize(self.core.tx_len);
        let top = *self.parts.tx_heap.first().filter(|_| len > 0)?;
        if top.key() == u32::MAX {
            return None;
        }
        let best = if self.ready_for::<W>(top.slot(), ready) {
            top
        } else {
            let mut best: Option<crate::heap::Item> = None;
            for i in 1..len {
                let Some(item) = self.parts.tx_heap.get(i).copied() else {
                    break;
                };
                if item.key() == u32::MAX || best.is_some_and(|b| !item.precedes(b)) {
                    continue;
                }
                if self.ready_for::<W>(item.slot(), ready) {
                    best = Some(item);
                }
            }
            best?
        };
        let link = self.parts.slots.get(to_usize(best.slot()))?.peer;
        if !self.fifo_turn(link) {
            return Some((best.slot(), false));
        }
        let mut oldest = best;
        for i in 0..len {
            let Some(item) = self.parts.tx_heap.get(i).copied() else {
                break;
            };
            if item.key() == u32::MAX
                || item.age() >= oldest.age()
                || !self.same_link(item.slot(), link)
            {
                continue;
            }
            if self.ready_for::<W>(item.slot(), ready) {
                oldest = item;
            }
        }
        Some((oldest.slot(), true))
    }

    /// Returns whether slot `index` sends on the uplink of peer `link`.
    fn same_link(&self, index: u32, link: u32) -> bool {
        self.shared_scope()
            || self
                .parts
                .slots
                .get(to_usize(index))
                .is_some_and(|slot| slot.peer == link)
    }

    /// Returns whether the next DATA packet on the uplink of peer `link` is
    /// the FIFO share's.
    fn fifo_turn(&self, link: u32) -> bool {
        let every = self.core.profile.fifo_every;
        let count = if self.shared_scope() {
            self.core.shared.sent
        } else {
            self.parts
                .peers
                .get(to_usize(link))
                .map_or(0, |entry| entry.sent)
        };
        pacer::fifo_turn(count, every)
    }

    /// Counts one DATA packet on the uplink of peer `link`.
    fn turn_uplink(&mut self, link: u32) {
        let every = self.core.profile.fifo_every;
        if self.shared_scope() {
            self.core.shared.sent = pacer::advance(self.core.shared.sent, every);
        } else if let Some(entry) = self.parts.peers.get_mut(to_usize(link)) {
            entry.sent = pacer::advance(entry.sent, every);
        }
    }

    /// Returns whether the carrier takes the next DATA packet of slot `index`
    /// now.
    fn ready_for<W: Form>(&mut self, index: u32, ready: &impl Fn(P, usize) -> bool) -> bool {
        let Some(plan) = self.plan::<W>(index) else {
            return false;
        };
        let Some(peer) = self
            .parts
            .slots
            .get(to_usize(index))
            .and_then(|slot| self.handle(slot.peer))
        else {
            return false;
        };
        pacer::admits(ready, peer, plan.header + to_usize(plan.pay))
    }

    /// Plans the next DATA packet of slot `index` and writes its header into
    /// the header buffer.
    fn plan<W: Form>(&mut self, index: u32) -> Option<Plan> {
        let slot = *self.parts.slots.get(to_usize(index))?;
        let peer = slot.peer;
        let handle = self.handle(peer)?;
        let max = self.packet_max(peer);
        let offset = slot.out_next.min(slot.out_len);
        let remaining = slot.out_len - offset;
        let (start, room) = if remaining == 0 {
            (0, 0)
        } else {
            let (page, within) = self
                .core
                .pages
                .seek(self.parts.pages, slot.out_page, offset)?;
            let start = usize::from(page)
                .checked_mul(to_usize(self.core.pages.len))?
                .checked_add(within)?;
            (start, to_usize(self.core.pages.len).saturating_sub(within))
        };
        #[cfg(feature = "client")]
        let ack = self.ack_head(peer);
        #[cfg(feature = "client")]
        let ack_entry = self.parts.slots.get(to_usize(ack)).map(|acked| AckEntry {
            client_id: acked.id,
            server_port: handle.homa_port(),
        });
        // A build without the role `client` owes no acknowledgement.
        #[cfg(not(feature = "client"))]
        let ack_entry: Option<AckEntry> = None;
        let ports = Ports {
            src: slot.port,
            dst: handle.homa_port(),
        };
        let need = self.need_limits(peer);
        let cutoff_version = self.cutoff_version(peer);
        let incast = slot.marks & INCAST != 0 && slot.id & 1 == 0;
        let mut pay = to_usize(remaining).min(room);
        // A whole message that does not fit becomes a part, whose header is
        // longer, and the payload shrinks to what that header leaves, so the
        // third pass fits whatever the form.
        for _ in 0..3 {
            let payload = self.parts.pages.get(start..start.checked_add(pay)?)?;
            let data = Data {
                id: slot.id,
                msg_length: slot.out_len,
                offset,
                retransmit: offset < slot.out_max,
                ack: ack_entry,
                cutoff_version,
                incast,
                payload,
            };
            let header = W::header(&Packet::Data(data), ports, need, &mut self.core.header).ok()?;
            if header + pay <= max {
                let pay = u32::try_from(pay).ok()?;
                if pay == 0 && remaining > 0 {
                    return None;
                }
                return Some(Plan {
                    offset,
                    pay,
                    start,
                    header,
                    #[cfg(feature = "client")]
                    ack,
                });
            }
            pay = max.saturating_sub(header).min(pay.saturating_sub(1));
        }
        None
    }

    /// Takes the DATA packet `plan` of slot `index` as sent.
    fn sent_data(&mut self, index: u32, plan: &Plan) {
        let now = self.core.now;
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            let end = plan.offset.saturating_add(plan.pay);
            slot.out_next = end;
            if end > slot.out_max {
                slot.out_max = end;
            }
            if end >= slot.out_len {
                slot.flags |= SENT;
            }
            slot.stamp = now;
            slot.flags &= !RESENT;
        }
        #[cfg(feature = "client")]
        if plan.ack != NONE {
            let peer = self
                .parts
                .slots
                .get(to_usize(index))
                .map_or(NONE, |slot| slot.peer);
            if self.ack_head(peer) == plan.ack {
                self.pop_ack(peer);
            }
        }
        self.refresh_tx(index);
        self.dirty();
    }

    /// Queues `NEED_ACK` for the oldest answered RPC of peer `peer`, whose
    /// ACK frees a slot for a request that found none.
    pub(crate) fn nudge(&mut self, peer: u32) {
        let mut oldest: Option<(u32, Nanos)> = None;
        for (i, slot) in self.parts.slots.iter().enumerate() {
            if slot.state != S_OUT || slot.peer != peer || slot.flags & SENT == 0 {
                continue;
            }
            let Ok(i) = u32::try_from(i) else {
                break;
            };
            if oldest.is_none_or(|(_, stamp)| slot.stamp < stamp) {
                oldest = Some((i, slot.stamp));
            }
        }
        if let Some((index, _)) = oldest {
            self.owe_slot(index, NEED_ACK);
        }
    }

    /// Queues the ACK a `NEED_ACK` of client RPC `id` asks for, which this
    /// end no longer holds or holds whole.
    #[cfg(feature = "client")]
    pub(crate) fn owe_ack(&mut self, peer: u32, id: u64) {
        if let Some(entry) = self.parts.peers.get_mut(to_usize(peer)) {
            entry.unacked = id;
        }
        self.owe_peer(peer, P_UNACKED);
    }

    /// Queues an `RPC_UNKNOWN` of `id` for peer `peer`.
    pub(crate) fn owe_unknown(&mut self, peer: u32, id: u64) {
        if let Some(entry) = self.parts.peers.get_mut(to_usize(peer)) {
            entry.unknown = id;
        }
        self.owe_peer(peer, P_UNKNOWN);
    }

    /// Queues a LIMITS for peer `peer` unless one left within `resend_after`.
    pub(crate) fn owe_limits(&mut self, peer: u32) {
        if !self.core.limits_on_wire {
            return;
        }
        let now = self.core.now;
        let wait = self.core.profile.timers.resend_after;
        let Some(entry) = self.parts.peers.get(to_usize(peer)) else {
            return;
        };
        let recent = entry.flags & LIMITS_SENT != 0 && now < entry.limits_at.saturating_add(wait);
        if !recent && entry.ctl & P_LIMITS == 0 {
            self.owe_peer(peer, P_LIMITS);
        }
    }
}
