// SPDX-License-Identifier: MIT
/*
 * homa endpoint: one Homa endpoint of the crate's engine, any number of peers,
 * both roles.
 *
 * The surface is the endpoint of the Homa design note, second version,
 * homa/design.md section 7.4, lines 2438 to 2539, over the rules of the engine
 * of its first version, homa/design-v1.md section 2.4, lines 504 to 565, with
 * the method reply_reserve and the local port of an RPC that the track plan
 * adds.
 */

//! The endpoint of the crate's engine.
//!
//! An [`Endpoint`] is a state machine with no input or output of its own: the
//! caller hands it packets with [`Endpoint::handle`], takes the packets it
//! wants sent with [`Endpoint::poll_transmit`], takes what it reports with
//! [`Endpoint::poll_event`], and runs its timers at [`Endpoint::next_timeout`]
//! with [`Endpoint::handle_timeout`], every call with the time of the
//! caller's clock. [`Endpoint::pump`] does all four over one [`Carrier`].
//!
//! The engine holds Homa's rules with the values of a [`Profile`]: messages
//! whose length leads their first packet, unscheduled bytes and grants from
//! the receiver, SRPT at the sender and at the receiver with a FIFO share,
//! RESEND, BUSY and `RPC_UNKNOWN`, at most once through ACK and `NEED_ACK`,
//! patient RPCs, and peer death after unanswered RESENDs. On a form with
//! LIMITS each end states what it accepts and asks for the peer's with
//! `NEED_LIMITS`.
//!
//! **The mechanisms of Network.** Each builds with its feature, and a profile
//! that asks for one the build left out fails [`Endpoint::new`] with
//! [`ConfigError::Missing`]:
//!
//! - All or nothing scheduling, [`Scheduling::AllOrNothing`], feature
//!   `start-msg`: a message longer than its unscheduled bytes opens with
//!   `START_MSG` and sends no DATA before a GRANT, as `HomaModule` does since
//!   its commit `acf40a4`, and a receiver whose first packet of a message is
//!   DATA takes the whole message as unscheduled.
//! - Priorities, feature `prio`: unscheduled bytes at the level the
//!   receiver's cutoffs give their message, scheduled bytes at the level its
//!   GRANT names, control packets at the highest level, and CUTOFFS in answer
//!   to a DATA packet of another cutoff version, at most once a millisecond
//!   a peer (the module `prio`).
//! - Overcommitment, feature `overcommit`: a receiver grants to up to
//!   `max_overcommit` messages of a downlink at once, one a sender, in SRPT
//!   order (the module `grant`).
//! - Incast control, feature `incast`: a client with more than the threshold
//!   of RPCs outstanding marks a new request, a server answers a marked
//!   request unscheduled up to the incast bytes alone, and the client grants
//!   the rest of that response.
//! - The FIFO share by interval of Network, the full form: every interval in
//!   which `fifo_grant_increment` bytes take `fifo_permille` of the link rate
//!   the oldest message of a shared downlink is granted that many bytes more,
//!   as `HomaModule` times it. A point to point profile takes its share by
//!   count, `fifo_every`.
//! - The rate pacer, feature `pacer-rate`: DATA of a message of at least
//!   `defer_min_bytes` waits while the link's backlog passes `max_backlog`
//!   (the module `pacer`).
//!
//! **Identifiers.** A client names its RPCs from `Config::first_id` on, even
//! and inside the identifier bits of the wire form, and a server names an RPC
//! by the client's identifier with the low bit set. One endpoint names each
//! of its RPCs by one identifier, so a request whose identifier a live RPC of
//! another peer holds is refused as [`Refused::NoSlot`] until that RPC ends,
//! which a client with a random first identifier meets as seldom as two of
//! its ranges overlap.

mod recv;
mod send;
mod timer;

use core::{
    cell::Cell,
    hash::Hash,
    marker::PhantomData, //
};

use crate::{
    grant,
    heap::{
        Heap,
        Item, //
    },
    store::{
        to_usize,
        Pages,
        Reserve,
        NONE,
        NO_PAGE, //
    },
    table,
    wire::{
        Form,
        Packet,
        Ports, //
    },
    Admission,
    CommitError,
    Config,
    ConfigError,
    Event,
    Failure,
    Gone,
    Limits,
    LinkError,
    Nanos,
    Parts,
    PeerPort,
    Profile,
    RpcId,
    RpcInfo,
    RpcState,
    Scheduling,
    Stats,
    Store,
    View,
    MAX_PAGES,
    MAX_POOL, //
};

/// The bytes of the header buffer, the largest header of either form, 90 for
/// the full form.
pub(crate) const HEADER_LEN: usize = 90;

/// The bit of a queue reference that names a peer rather than a slot.
pub(crate) const PEER_TAG: u32 = 1 << 31;

/// The smallest `max_packet` a link runs with, which lets a LIMITS travel in
/// the 20 bytes of an ATT value at the default ATT MTU of 23.
const MIN_PACKET: u16 = 20;

/// A vacant slot.
pub(crate) const VACANT: u8 = 0;
/// A client RPC whose request leaves or waits, with no byte of its answer.
pub(crate) const C_OUT: u8 = 1;
/// A client RPC whose answer arrives.
pub(crate) const C_IN: u8 = 2;
/// A client RPC whose answer is whole and not yet released.
pub(crate) const C_DONE: u8 = 3;
/// A client RPC the application released, whose ACK waits to leave.
#[cfg(feature = "client")]
pub(crate) const C_ACK: u8 = 4;
/// A server RPC whose request arrives.
pub(crate) const S_IN: u8 = 5;
/// A server RPC that holds its whole request and has no reply.
pub(crate) const S_EXEC: u8 = 6;
/// A server RPC whose reply has pages reserved and is not committed.
pub(crate) const S_RES: u8 = 7;
/// A server RPC whose reply leaves or waits for its ACK.
pub(crate) const S_OUT: u8 = 8;
/// An RPC that ended and waits to report its event.
pub(crate) const ENDED: u8 = 9;

/// The request is patient.
pub(crate) const PATIENT: u8 = 1 << 0;
/// The request reports `RPC_UNKNOWN` in place of a restart.
#[cfg(feature = "client")]
pub(crate) const REPORT_UNKNOWN: u8 = 1 << 1;
/// The timer asked the peer for the RPC since the RPC last heard.
pub(crate) const RESENT: u8 = 1 << 2;
/// The slot is in the control queue.
pub(crate) const CTL_QUEUED: u8 = 1 << 3;
/// The slot is in the event queue.
pub(crate) const EV_QUEUED: u8 = 1 << 4;
/// The position of the slot names the transmit heap.
pub(crate) const IN_TX: u8 = 1 << 5;
/// The position of the slot names the grant heap.
pub(crate) const IN_GRANT: u8 = 1 << 6;
/// The outgoing message went out whole at least once.
pub(crate) const SENT: u8 = 1 << 7;

/// A GRANT is owed for the incoming message.
pub(crate) const GRANT: u8 = 1 << 0;
/// A RESEND is owed for the incoming message.
pub(crate) const RESEND: u8 = 1 << 1;
/// A BUSY is owed for the RPC.
pub(crate) const BUSY: u8 = 1 << 2;
/// A `NEED_ACK` is owed for the reply.
pub(crate) const NEED_ACK: u8 = 1 << 3;
/// A `START_MSG` is owed for the outgoing message.
pub(crate) const START: u8 = 1 << 4;

/// The request is marked for incast control: on a client it leaves marked,
/// and on a server it arrived marked, so its response is unscheduled up to
/// the incast bytes alone.
pub(crate) const INCAST: u8 = 1 << 0;
/// The outgoing message opened with `START_MSG`, so every byte of it waits
/// for a grant.
pub(crate) const SCHEDULED: u8 = 1 << 1;

/// No event waits.
pub(crate) const EV_NONE: u8 = 0;
/// A whole request waits to be reported.
pub(crate) const EV_REQUEST: u8 = 1;
/// A whole response waits to be reported.
pub(crate) const EV_RESPONSE: u8 = 2;
/// A server RPC freed without a reply waits to be reported.
pub(crate) const EV_DROPPED: u8 = 3;
/// A failed client RPC waits to be reported, the failure in the low bits.
pub(crate) const EV_FAILED: u8 = 16;

/// The peer is linked or admitted.
pub(crate) const UP: u8 = 1 << 0;
/// The peer's LIMITS arrived.
pub(crate) const KNOWN: u8 = 1 << 1;
/// The peer is in the control queue.
pub(crate) const P_CTL_QUEUED: u8 = 1 << 2;
/// The peer is in the event queue.
pub(crate) const P_EV_QUEUED: u8 = 1 << 3;
/// The peer's limits wait to be reported.
pub(crate) const EV_LIMITS: u8 = 1 << 4;
/// The peer's end waits to be reported.
pub(crate) const EV_GONE: u8 = 1 << 5;
/// This end sent the peer a LIMITS.
pub(crate) const LIMITS_SENT: u8 = 1 << 6;
/// This end sent the peer a RESEND or a `NEED_ACK`.
pub(crate) const ASKED: u8 = 1 << 7;

/// A LIMITS is owed to the peer.
pub(crate) const P_LIMITS: u8 = 1 << 0;
/// Acknowledgements wait for the peer.
#[cfg(feature = "client")]
pub(crate) const P_ACKS: u8 = 1 << 1;
/// An `RPC_UNKNOWN` is owed to the peer.
pub(crate) const P_UNKNOWN: u8 = 1 << 2;
/// An ACK of an RPC this end no longer holds is owed to the peer.
#[cfg(feature = "client")]
pub(crate) const P_UNACKED: u8 = 1 << 3;
/// This end's cutoffs are owed to the peer.
#[cfg(feature = "prio")]
pub(crate) const P_CUTOFFS: u8 = 1 << 4;

/// The DATA packet `poll_transmit` returned last, which the simulators
/// trace.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Choice {
    /// Whether a DATA packet was chosen since the endpoint was built.
    pub(crate) valid: bool,
    /// The RPC of the packet, as this end names it.
    pub(crate) id: u64,
    /// The bytes its message had left before the packet.
    pub(crate) bytes_left: u32,
    /// Whether the FIFO share chose it.
    pub(crate) fifo: bool,
}

/// The earliest deadline of the timers of an endpoint as last computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Due {
    /// A change since the last computation may have moved it.
    Stale,
    /// The deadline, `None` where no timer runs.
    Known(Option<Nanos>),
}

/// The counters of the downlink and the uplink all peers share under
/// [`Scope::Shared`](crate::Scope::Shared).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Shared {
    /// Bytes unscheduled or granted on the downlink and not yet received.
    pub(crate) expected: u32,
    /// DATA packets sent on the uplink, whose count chooses the FIFO turn.
    pub(crate) sent: u8,
    /// GRANTs given on the downlink, whose count chooses the FIFO turn.
    pub(crate) grants: u8,
    /// When the next FIFO grant of the share by interval is due.
    pub(crate) fifo_at: Nanos,
}

/// The levels and the cutoffs this end announces as a receiver.
#[cfg(feature = "prio")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnPrio {
    /// The levels in use and where they split.
    pub(crate) levels: crate::prio::Levels,
    /// The cutoffs this end announces in CUTOFFS.
    pub(crate) cutoffs: crate::prio::Cutoffs,
}

/// No levels and no cutoffs in a build without the feature `prio`.
#[cfg(not(feature = "prio"))]
pub(crate) type OwnPrio = ();

/// The levels and cutoffs of an endpoint nothing has changed:
/// `HomaModule`'s defaults.
#[cfg(feature = "prio")]
const OWN_PRIO: OwnPrio = OwnPrio {
    levels: crate::prio::Levels::module_default(),
    cutoffs: crate::prio::Cutoffs::module_default(),
};

/// No levels and no cutoffs in a build without the feature `prio`.
#[cfg(not(feature = "prio"))]
const OWN_PRIO: OwnPrio = ();

/// The rate pacer of an endpoint and the time it lets the next held DATA
/// packet go.
#[cfg(feature = "pacer-rate")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RatePacer {
    /// The pacer, `None` until the carrier's driver sets one.
    pub(crate) pacer: Option<crate::pacer::Rate>,
    /// When the pacer lets the DATA packet it held go, `None` where it holds
    /// none.
    pub(crate) wake: Option<Nanos>,
}

/// No rate pacer in a build without the feature `pacer-rate`.
#[cfg(not(feature = "pacer-rate"))]
pub(crate) type RatePacer = ();

/// One decision of the grant scheduler, which the simulators trace.
///
/// The simulators read the decisions with [`Endpoint::grant_decisions`], so
/// a trace holds for every GRANT the rank of its message among the messages
/// granted at once and their count, which the level of the grant follows.
#[cfg(feature = "sim")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantDecision {
    /// The RPC of the message, as this end names it.
    pub id: RpcId,
    /// The first byte of the message the grant leaves ungranted.
    pub offset: u32,
    /// The level the grant names.
    pub level: u8,
    /// The messages granted at once ahead of this one.
    pub rank: u8,
    /// The messages granted at once, this one among them.
    pub active: u8,
    /// The window the grant was sized by.
    pub window: u32,
    /// Whether the FIFO share gave the grant.
    pub fifo: bool,
}

/// The decisions of the grant scheduler the simulators have not read yet.
#[cfg(feature = "sim")]
pub(crate) type DecisionLog = alloc::vec::Vec<GrantDecision>;

/// No log of decisions in a build without the feature `sim`.
#[cfg(not(feature = "sim"))]
pub(crate) type DecisionLog = ();

/// The state of an endpoint beside its storage.
#[derive(Debug)]
pub(crate) struct Core {
    /// The sizes the endpoint was built for.
    pub(crate) config: Config,
    /// The values of its links.
    pub(crate) profile: Profile,
    /// The shape of its page pool.
    pub(crate) pages: Pages,
    /// The latest time a call passed.
    pub(crate) now: Nanos,
    /// The next client identifier.
    // A build without the role `client` names no client RPC, so it reads
    // neither field.
    #[cfg_attr(not(feature = "client"), expect(dead_code))]
    pub(crate) next_id: u64,
    /// The identifier bits of the wire form.
    #[cfg_attr(not(feature = "client"), expect(dead_code))]
    pub(crate) id_mask: u64,
    /// The age the next message takes, which breaks ties in the heaps.
    pub(crate) age: u64,
    /// The counters.
    pub(crate) stats: Stats,
    /// Table probes beyond the first, counted by shared lookups too.
    pub(crate) probes: Cell<u64>,
    /// The header of the packet `poll_transmit` returned last.
    pub(crate) header: [u8; HEADER_LEN],
    /// The first free client slot.
    pub(crate) free_client: u32,
    /// The first free server slot.
    pub(crate) free_server: u32,
    /// The first free place of a peer.
    pub(crate) free_peer: u32,
    /// The first free page.
    pub(crate) free_page: u16,
    /// The count of free pages.
    pub(crate) free_pages: u32,
    /// Items in the transmit heap.
    pub(crate) tx_len: u32,
    /// Items in the grant heap.
    pub(crate) grant_len: u32,
    /// The first node of the control queue.
    pub(crate) ctl_head: u32,
    /// The last node of the control queue.
    pub(crate) ctl_tail: u32,
    /// The first node of the event queue.
    pub(crate) ev_head: u32,
    /// The last node of the event queue.
    pub(crate) ev_tail: u32,
    /// Whether the wire form carries LIMITS.
    pub(crate) limits_on_wire: bool,
    /// Whether the wire form is the full form of Network, which carries the
    /// ports, the levels, CUTOFFS and `START_MSG`.
    // A build without the feature `prio` gives every packet one level, so it
    // reads neither this field nor the levels below.
    #[cfg_attr(not(feature = "prio"), expect(dead_code))]
    pub(crate) full_form: bool,
    /// The counters of the shared downlink and uplink.
    pub(crate) shared: Shared,
    /// The DATA packet `poll_transmit` returned last.
    pub(crate) last: Choice,
    /// The earliest deadline of the timers as last computed.
    pub(crate) deadline: Cell<Due>,
    /// The slot the next timer pass starts at, which rotates the RESENDs of a
    /// peer among its RPCs.
    pub(crate) scan_from: u32,
    /// The rate the link sends at in bits a second, which times the FIFO
    /// share by interval.
    pub(crate) link_bits_per_s: u64,
    /// Client slots in use, the outstanding RPCs incast control counts.
    pub(crate) clients_open: u32,
    /// The levels and the cutoffs this end announces as a receiver.
    #[cfg_attr(not(feature = "prio"), expect(dead_code))]
    pub(crate) own: OwnPrio,
    /// The rate pacer.
    // A build without the feature `pacer-rate` holds no packet back.
    #[cfg_attr(not(feature = "pacer-rate"), expect(dead_code))]
    pub(crate) rate: RatePacer,
    /// The decisions of the grant scheduler the simulators have not read.
    // Only the simulators read the decisions.
    #[cfg_attr(not(feature = "sim"), expect(dead_code))]
    pub(crate) decisions: DecisionLog,
}

/// The rate of a link nothing has set, `HomaModule`'s `link_mbps` of 25,000.
pub(crate) const DEFAULT_LINK_BITS_PER_S: u64 = 25_000_000_000;

/// The state and the storage of an endpoint at once, which every engine
/// module works on.
pub(crate) struct Cx<'c, 'p, P> {
    /// The state beside the storage.
    pub(crate) core: &'c mut Core,
    /// The storage.
    pub(crate) parts: Parts<'p, P>,
}

/// One Homa endpoint of the crate's engine, any number of peers, both roles.
///
/// `P` is the carrier's peer, `S` the storage and `W` the wire form. Every
/// method that takes a packet or a message checks each length against this
/// end's limits before it stores a byte, and none panics on any input.
pub struct Endpoint<P, S, W> {
    /// The storage the caller handed over.
    store: S,
    /// The state beside it.
    core: Core,
    /// The peer and the wire form, which live in the types alone.
    marker: PhantomData<(fn() -> P, W)>,
}

/// Returns why `parts` cannot hold what `len` asks for, or `None`.
fn short<P>(parts: &Parts<'_, P>, len: &crate::StorageLen) -> Option<ConfigError> {
    let tables = parts.peer_index.len() >= len.peer_index
        && parts.slot_index.len() >= len.slot_index
        && parts.peer_index.len().is_power_of_two()
        && parts.slot_index.len().is_power_of_two();
    let fits = parts.peers.len() >= len.peers
        && parts.slots.len() >= len.slots
        && parts.tx_heap.len() >= len.tx_heap
        && parts.grant_heap.len() >= len.grant_heap
        && parts.pages.len() >= len.pages
        && parts.gaps.len() >= len.gaps
        && parts.sizes.len() >= len.sizes
        && parts.peer_prio.len() >= len.peer_prio;
    if tables && fits {
        None
    } else {
        Some(ConfigError::TooSmall)
    }
}

/// Returns the feature a profile needs that this build lacks, or `None`.
///
/// All or nothing scheduling needs `START_MSG`, which the feature
/// `start-msg` builds and the full form carries, incast control needs its
/// feature and the full form, whose DATA carries the mark, more than one
/// granted message a downlink needs `overcommit`, and the FIFO share by
/// interval of Network needs the full form, whose feature builds it (the
/// Homa design note, second version, section 7.1: a profile that asks for a
/// mechanism the build left out fails at start and never falls back).
fn missing<W: Form>(profile: &Profile) -> Option<ConfigError> {
    let full = carries_start_msg::<W>();
    if profile.scheduling == Scheduling::AllOrNothing && !(cfg!(feature = "start-msg") && full) {
        return Some(ConfigError::Missing("start-msg"));
    }
    if profile.max_overcommit > 1 && !cfg!(feature = "overcommit") {
        return Some(ConfigError::Missing("overcommit"));
    }
    if profile.incast.is_some() && !(cfg!(feature = "incast") && full) {
        return Some(ConfigError::Missing("incast"));
    }
    if profile.fifo_every == 0 && profile.fifo_permille > 0 && !cfg!(feature = "wire-full") {
        return Some(ConfigError::Missing("wire-full"));
    }
    None
}

/// Returns why `config` cannot build an endpoint, or `None`.
fn refuse(config: &Config) -> Option<ConfigError> {
    let slots = config.slots();
    if config.peers == 0 || slots == 0 || config.page_len == 0 || config.pages == 0 {
        return Some(ConfigError::Empty);
    }
    if config.peers >= PEER_TAG || slots >= PEER_TAG {
        return Some(ConfigError::TooSmall);
    }
    // A pool beyond the page table's reach, or a share of more pages a slot
    // than a message may span, means pages too short for the messages the
    // pool is sized for.
    if config.pages > MAX_POOL || config.pages.div_ceil(slots) > MAX_PAGES {
        return Some(ConfigError::TooSmall);
    }
    if config.first_id == 0 || config.first_id & 1 == 1 {
        return Some(ConfigError::BadFirstId);
    }
    None
}

impl<P, S, W> Endpoint<P, S, W>
where
    P: Copy + Eq + Hash + PeerPort,
    S: Store<P>,
    W: Form,
{
    /// Builds an endpoint over `store`, which must hold what `config` asks
    /// for.
    ///
    /// Every part of the storage is set to its empty state, so a store that
    /// served another endpoint before is reused as it stands.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Empty`] for no peer, no slot or no page,
    /// [`ConfigError::BadFirstId`] for a first identifier that is odd or 0
    /// inside the identifier bits of the form, [`ConfigError::TooSmall`] for a
    /// part shorter than [`Config::storage`] states, a table whose length is
    /// no power of two, a pool of more than [`MAX_POOL`] pages or a share of
    /// more than [`MAX_PAGES`] pages a slot, and [`ConfigError::Missing`] for
    /// a mechanism of `profile` that the features of this build or the form
    /// `W` leave out, named by the feature it needs.
    pub fn new(config: Config, profile: Profile, mut store: S) -> Result<Self, ConfigError> {
        if let Some(error) = refuse(&config).or_else(|| missing::<W>(&profile)) {
            return Err(error);
        }
        let id_mask = if W::ID_BITS >= 64 {
            u64::MAX
        } else {
            (1u64 << W::ID_BITS) - 1
        };
        let first_id = config.first_id & id_mask;
        if first_id == 0 {
            return Err(ConfigError::BadFirstId);
        }
        let len = config.storage();
        let mut parts = store.parts();
        if let Some(error) = short(&parts, &len) {
            return Err(error);
        }
        let pages = Pages {
            len: config.page_len,
            count: config.pages,
        };
        let mut core = Core {
            config,
            profile,
            pages,
            now: 0,
            next_id: first_id,
            id_mask,
            age: 0,
            stats: Stats::default(),
            probes: Cell::new(0),
            header: [0; HEADER_LEN],
            free_client: NONE,
            free_server: NONE,
            free_peer: NONE,
            free_page: NO_PAGE,
            free_pages: 0,
            tx_len: 0,
            grant_len: 0,
            ctl_head: NONE,
            ctl_tail: NONE,
            ev_head: NONE,
            ev_tail: NONE,
            limits_on_wire: carries_limits::<W>(),
            full_form: carries_start_msg::<W>(),
            shared: Shared::default(),
            last: Choice::default(),
            deadline: Cell::new(Due::Stale),
            scan_from: 0,
            link_bits_per_s: DEFAULT_LINK_BITS_PER_S,
            clients_open: 0,
            own: OWN_PRIO,
            rate: RatePacer::default(),
            decisions: DecisionLog::default(),
        };
        clear(&mut core, &mut parts);
        Ok(Self {
            store,
            core,
            marker: PhantomData,
        })
    }

    /// Returns the state and the storage at once.
    fn cx(&mut self) -> Cx<'_, '_, P> {
        self.core.deadline.set(Due::Stale);
        Cx {
            core: &mut self.core,
            parts: self.store.parts(),
        }
    }

    /// Starts serving a link and queues this end's LIMITS where the form has
    /// them.
    ///
    /// `local` is what this end accepts on the link and `assumed` what it
    /// takes of the peer until the peer's LIMITS arrives.
    ///
    /// # Errors
    ///
    /// Returns [`LinkError::Up`] for a peer whose link is up,
    /// [`LinkError::NoPeerSlot`] when every place of a peer is taken, and
    /// [`LinkError::Params`] for a packet below 20 bytes, a `max_rpcs`,
    /// `max_message` or `unsched_bytes` of 0, or a fixed window shorter than
    /// a packet and than the longest message.
    pub fn link_up(&mut self, peer: P, local: Limits, assumed: Limits) -> Result<(), LinkError> {
        let mut cx = self.cx();
        cx.check_limits(&local)?;
        cx.check_limits(&assumed)?;
        let index = match cx.find_peer(peer) {
            Some(index) => {
                if cx.peer_flags(index) & UP != 0 {
                    return Err(LinkError::Up);
                }
                index
            }
            None => cx.admit(peer).ok_or(LinkError::NoPeerSlot)?,
        };
        cx.start_peer(index, local, assumed);
        Ok(())
    }

    /// Ends a link, failing its client RPCs and dropping its server RPCs.
    ///
    /// A client RPC fails with [`Failure::LinkDown`] and a server RPC without
    /// a reply is reported as [`Event::Dropped`], and [`Event::PeerGone`]
    /// follows with [`Gone::LinkDown`].
    ///
    /// # Errors
    ///
    /// Returns [`LinkError::NoPeer`] for a peer whose link is not up.
    pub fn link_down(&mut self, peer: P) -> Result<(), LinkError> {
        let mut cx = self.cx();
        let index = cx
            .find_peer(peer)
            .filter(|&i| cx.peer_flags(i) & UP != 0)
            .ok_or(LinkError::NoPeer)?;
        cx.end_peer(index, Gone::LinkDown);
        Ok(())
    }

    /// Changes this end's limits on a link and queues a new LIMITS.
    ///
    /// # Errors
    ///
    /// Returns [`LinkError::NoPeer`] for a peer whose link is not up and
    /// [`LinkError::Params`] for limits out of range, as
    /// [`Endpoint::link_up`] does.
    pub fn set_limits(&mut self, peer: P, local: Limits) -> Result<(), LinkError> {
        let mut cx = self.cx();
        cx.check_limits(&local)?;
        let index = cx
            .find_peer(peer)
            .filter(|&i| cx.peer_flags(i) & UP != 0)
            .ok_or(LinkError::NoPeer)?;
        if let Some(entry) = cx.parts.peers.get_mut(to_usize(index)) {
            entry.local = local;
        }
        if cx.core.limits_on_wire {
            cx.owe_peer(index, P_LIMITS);
        }
        Ok(())
    }

    /// Returns the peer's limits once its LIMITS arrived.
    pub fn peer_limits(&self, peer: P) -> Option<Limits> {
        let view = self.store.view();
        let index = find_peer_in(&self.core, &view, peer)?;
        let entry = view.peers.get(to_usize(index))?;
        if entry.flags & (UP | KNOWN) == UP | KNOWN {
            Some(entry.remote)
        } else {
            None
        }
    }

    /// Returns the response of a client RPC once [`Event::Response`] reported
    /// it.
    #[cfg(feature = "client")]
    pub fn response(&self, id: RpcId) -> Option<crate::Reader<'_>> {
        let view = self.store.view();
        let index = find_slot_in(&self.core, &view, id.get())?;
        let slot = view.slots.get(to_usize(index))?;
        if slot.state != C_DONE {
            return None;
        }
        Some(crate::Reader::new(
            view.pages,
            self.core.pages,
            slot.in_page,
            slot.in_len,
        ))
    }

    /// Frees a client slot and queues the acknowledgement.
    ///
    /// A client RPC whose response is whole waits for its ACK to leave, on a
    /// DATA packet to the same server or in an ACK packet, and counts toward
    /// the peer's `max_rpcs` until then. A client RPC released before its
    /// response is whole ends as [`Endpoint::abort`] ends it.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownRpc`](crate::UnknownRpc) when `id` names no client RPC of this end.
    #[cfg(feature = "client")]
    pub fn release(&mut self, id: RpcId) -> Result<(), crate::UnknownRpc> {
        let mut cx = self.cx();
        let index = cx.find_client(id).ok_or(crate::UnknownRpc)?;
        match cx.slot_state(index) {
            C_DONE => {
                cx.acknowledge(index);
                Ok(())
            }
            C_OUT | C_IN | ENDED => {
                cx.free_slot(index);
                Ok(())
            }
            _ => Err(crate::UnknownRpc),
        }
    }

    /// Ends a client RPC locally, without telling the server, as
    /// `HomaModule` does.
    ///
    /// The server learns of it when it next asks, from the `RPC_UNKNOWN`
    /// this end answers.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownRpc`](crate::UnknownRpc) when `id` names no live client RPC of this end.
    #[cfg(feature = "client")]
    pub fn abort(&mut self, id: RpcId) -> Result<(), crate::UnknownRpc> {
        let mut cx = self.cx();
        let index = cx.find_client(id).ok_or(crate::UnknownRpc)?;
        match cx.slot_state(index) {
            C_OUT | C_IN | C_DONE | ENDED => {
                cx.free_slot(index);
                Ok(())
            }
            _ => Err(crate::UnknownRpc),
        }
    }

    /// Returns the request of a server RPC.
    ///
    /// The request stays readable from [`Event::Request`] until the reply is
    /// queued, which frees its pages.
    #[cfg(feature = "server")]
    pub fn request_of(&self, id: RpcId) -> Option<crate::Reader<'_>> {
        let view = self.store.view();
        if !id.is_server() {
            return None;
        }
        let index = find_slot_in(&self.core, &view, id.get())?;
        let slot = view.slots.get(to_usize(index))?;
        if slot.state != S_EXEC {
            return None;
        }
        Some(crate::Reader::new(
            view.pages,
            self.core.pages,
            slot.in_page,
            slot.in_len,
        ))
    }

    /// Returns the local Homa port an RPC belongs to, `port` where the
    /// endpoint owns one port.
    pub fn local_port(&self, id: RpcId) -> Option<u16> {
        let view = self.store.view();
        let index = find_slot_in(&self.core, &view, id.get())?;
        let slot = view.slots.get(to_usize(index))?;
        if slot.state == VACANT {
            None
        } else {
            Some(slot.port)
        }
    }

    /// Returns the next event, in the order the endpoint produced them.
    pub fn poll_event(&mut self) -> Option<Event<P>> {
        self.cx().pop_event()
    }

    /// Receives what is waiting, runs due timers and transmits while the
    /// carrier is ready.
    ///
    /// A packet the endpoint refuses is counted in [`Stats::refused`] and
    /// changes nothing, and a `now` earlier than a time passed before leaves
    /// the timers as they are.
    ///
    /// # Errors
    ///
    /// Returns the carrier's error when the link itself fails.
    pub fn pump<C: crate::Carrier<Peer = P>>(
        &mut self,
        carrier: &mut C,
        now: Nanos,
        rx: &mut [u8],
    ) -> Result<(), C::Error> {
        while let Some((peer, len)) = carrier.recv(rx)? {
            if let Some(packet) = rx.get(..len) {
                // A refusal is counted by `handle` and needs nothing more.
                let _ = self.handle(now, peer, packet);
            }
        }
        // A clock that went backwards runs no timer, which is all the
        // refusal asks.
        let _ = self.handle_timeout(now);
        while let Some(packet) = self.poll_transmit(now, |peer, len| carrier.ready(peer, len)) {
            carrier.send(packet.peer, packet.priority, packet.header, packet.payload)?;
        }
        Ok(())
    }

    /// Sets the rate the link sends at, in bits a second, which times the
    /// FIFO share by interval of a shared downlink.
    ///
    /// An endpoint nothing has set counts with `HomaModule`'s `link_mbps` of
    /// 25,000.
    pub fn set_link_rate(&mut self, bits_per_s: u64) {
        self.core.link_bits_per_s = bits_per_s.max(1);
    }

    /// Sets the rate pacer that holds the DATA packets of long messages
    /// while the link's backlog is long, or none.
    #[cfg(feature = "pacer-rate")]
    pub fn set_pacer(&mut self, pacer: Option<crate::pacer::Rate>) {
        self.core.rate = RatePacer { pacer, wake: None };
        self.core.deadline.set(Due::Stale);
    }

    /// Sets the levels in use and where they split, and returns whether
    /// they were taken, which they are where [`crate::prio::Levels::is_valid`]
    /// holds.
    #[cfg(feature = "prio")]
    pub fn set_levels(&mut self, levels: crate::prio::Levels) -> bool {
        if !levels.is_valid() {
            return false;
        }
        self.core.own.levels = levels;
        true
    }

    /// Returns the levels in use.
    #[cfg(feature = "prio")]
    pub fn levels(&self) -> crate::prio::Levels {
        self.core.own.levels
    }

    /// Returns the cutoffs this end announces as a receiver.
    #[cfg(feature = "prio")]
    pub fn cutoffs(&self) -> crate::prio::Cutoffs {
        self.core.own.cutoffs
    }

    /// Returns the cutoffs peer `peer` announced, as this end sends by
    /// them, `HomaModule`'s defaults under version 0 before its first
    /// CUTOFFS arrives.
    #[cfg(feature = "prio")]
    pub fn peer_cutoffs(&self, peer: P) -> Option<crate::prio::Cutoffs> {
        let view = self.store.view();
        let index = find_peer_in(&self.core, &view, peer)?;
        view.peer_prio
            .get(to_usize(index))
            .map(|entry| entry.cutoffs)
    }

    /// Replaces the cutoffs this receiver announces and bumps their
    /// version, so every sender learns them from the CUTOFFS its next DATA
    /// packet draws.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Empty`] for cutoffs that give no level to a
    /// message of 1 byte.
    #[cfg(feature = "prio")]
    pub fn set_cutoffs(&mut self, cutoffs: crate::prio::Cutoffs) -> Result<(), ConfigError> {
        if cutoffs.bytes.iter().all(|&bytes| bytes == 0) {
            return Err(ConfigError::Empty);
        }
        let version = next_version(self.core.own.cutoffs.version);
        self.core.own.cutoffs = crate::prio::Cutoffs {
            bytes: cutoffs.bytes,
            version,
        };
        Ok(())
    }

    /// Recomputes the cutoffs from the sizes counted since the last call,
    /// takes `levels` as the levels in use, and returns the cutoffs in force
    /// after the call.
    ///
    /// The sizes are counted where the storage holds 32 buckets of them,
    /// [`Config::size_buckets`], and a call that finds none counted keeps the
    /// cutoffs as they are (the rule of [`crate::prio::Sizes::cutoffs`]).
    #[cfg(feature = "prio-compute")]
    pub fn recompute_cutoffs(&mut self, levels: crate::prio::Levels) -> crate::prio::Cutoffs {
        let unsched = self.core.profile.unsched_bytes;
        let version = next_version(self.core.own.cutoffs.version);
        let parts = self.store.parts();
        let mut sizes = crate::prio::Sizes::new();
        if let Some(counted) = parts.sizes.get_mut(..crate::prio::Sizes::BUCKETS) {
            sizes.bytes.copy_from_slice(counted);
            counted.fill(0);
        }
        if levels.is_valid() {
            self.core.own.levels = levels;
        }
        let levels = self.core.own.levels;
        if let Some(cutoffs) = sizes.cutoffs(levels, unsched, version) {
            self.core.own.cutoffs = cutoffs;
        }
        self.core.own.cutoffs
    }

    /// Returns the decisions of the grant scheduler since the last call, in
    /// the order it took them.
    #[cfg(feature = "sim")]
    pub fn grant_decisions(&mut self) -> alloc::vec::Vec<GrantDecision> {
        core::mem::take(&mut self.core.decisions)
    }

    /// Returns the counters.
    pub fn stats(&self) -> Stats {
        let mut stats = self.core.stats;
        stats.probes_beyond_first = self.core.probes.get();
        stats
    }

    /// Writes the state of every RPC into `out` and returns how many it
    /// wrote, as HOMAIOCINFO does.
    ///
    /// An RPC that ended and waits to report its event, and a client RPC
    /// whose ACK waits to leave, is no longer listed.
    pub fn rpc_info(&self, out: &mut [RpcInfo<P>]) -> usize {
        let view = self.store.view();
        let stride = gap_stride(&self.core.config);
        let mut written = 0;
        for (index, slot) in view.slots.iter().enumerate() {
            let Some(entry) = out.get_mut(written) else {
                break;
            };
            let Some(info) = info_of(&view, slot, index, stride) else {
                continue;
            };
            *entry = info;
            written += 1;
        }
        written
    }

    /// Writes the outgoing messages that hold bytes to transmit now, each as
    /// the identifier of its RPC and the bytes it has left, and returns how
    /// many it wrote.
    ///
    /// The simulators list them before [`Endpoint::poll_transmit`], so a
    /// trace names every message that could have gone instead of the one
    /// that went.
    #[cfg(feature = "sim")]
    pub fn transmittable(&self, out: &mut [(u64, u32)]) -> usize {
        let view = self.store.view();
        let heap = view
            .tx_heap
            .get(..to_usize(self.core.tx_len))
            .unwrap_or(&[]);
        let mut written = 0;
        for item in heap.iter().filter(|item| item.key() != u32::MAX) {
            let Some(slot) = view.slots.get(to_usize(item.slot())) else {
                continue;
            };
            let Some(entry) = out.get_mut(written) else {
                break;
            };
            *entry = (slot.id, item.key());
            written += 1;
        }
        written
    }

    /// Returns the DATA packet [`Endpoint::poll_transmit`] returned last: its
    /// RPC, the bytes its message had left before it, and whether the FIFO
    /// share chose it rather than SRPT.
    #[cfg(feature = "sim")]
    pub fn last_data(&self) -> Option<(RpcId, u32, bool)> {
        let last = self.core.last;
        if last.valid {
            Some((RpcId::new(last.id), last.bytes_left, last.fifo))
        } else {
            None
        }
    }
}

/// Returns whether the wire form `W` carries `START_MSG`, which the full form
/// does and the compact form does not.
fn carries_start_msg<W: Form>() -> bool {
    let probe = Packet::StartMsg {
        id: 2,
        msg_length: 1,
    };
    let mut out = [0u8; HEADER_LEN];
    W::header(&probe, Ports::default(), false, &mut out).is_ok()
}

/// Returns whether the wire form `W` carries LIMITS.
fn carries_limits<W: Form>() -> bool {
    let probe = Limits {
        max_packet: 20,
        max_message: 1,
        unsched_bytes: 1,
        max_rpcs: 1,
    };
    let mut out = [0u8; HEADER_LEN];
    W::header(&Packet::Limits(probe), Ports::default(), false, &mut out).is_ok()
}

/// Sets every part of the storage to its empty state and links the free
/// lists.
fn clear<P>(core: &mut Core, parts: &mut Parts<'_, P>) {
    let peers = core.config.peers;
    let slots = core.config.slots();
    let client = core.config.client_slots;
    for (i, entry) in parts.peers.iter_mut().enumerate() {
        *entry = crate::Peer::VACANT;
        let i = u32::try_from(i).unwrap_or(NONE);
        entry.ctl_next = if i.saturating_add(1) < peers {
            i + 1
        } else {
            NONE
        };
    }
    for (i, slot) in parts.slots.iter_mut().enumerate() {
        *slot = crate::Slot::VACANT;
        let i = u32::try_from(i).unwrap_or(NONE);
        let next = i.saturating_add(1);
        let last = if i < client { client } else { slots };
        slot.next = if next < last { next } else { NONE };
    }
    parts.peer_index.fill(table::Bucket::VACANT);
    parts.slot_index.fill(table::Bucket::VACANT);
    parts.tx_heap.fill(Item::VACANT);
    parts.grant_heap.fill(Item::VACANT);
    parts.gaps.fill(0);
    parts.sizes.fill(0);
    core.free_peer = if peers > 0 { 0 } else { NONE };
    core.free_client = if client > 0 { 0 } else { NONE };
    core.free_server = if slots > client { client } else { NONE };
    let count = core.config.pages;
    for page in 0..count {
        let this = u16::try_from(page).unwrap_or(NO_PAGE);
        let next = if page + 1 < count {
            u16::try_from(page + 1).unwrap_or(NO_PAGE)
        } else {
            NO_PAGE
        };
        core.pages.set_link(parts.pages, this, next);
    }
    core.free_page = if count > 0 { 0 } else { NO_PAGE };
    core.free_pages = count;
}

/// Returns the place of `peer` in a view, counting the probes of the lookup.
fn find_peer_in<P: Hash + Eq + Copy>(core: &Core, view: &View<'_, P>, peer: P) -> Option<u32> {
    let hash = table::hash(&core.config.hash_key, &peer);
    let mut probes = 0;
    let found = table::find(view.peer_index, hash, &mut probes, |i| {
        view.peers
            .get(to_usize(i))
            .and_then(|entry| entry.handle)
            .is_some_and(|handle| handle == peer)
    });
    core.probes.set(core.probes.get().saturating_add(probes));
    found
}

/// Returns the slot of the RPC `id` in a view, counting the probes of the
/// lookup.
fn find_slot_in<P>(core: &Core, view: &View<'_, P>, id: u64) -> Option<u32> {
    let hash = table::hash(&core.config.hash_key, &id);
    let mut probes = 0;
    let found = table::find(view.slot_index, hash, &mut probes, |i| {
        view.slots
            .get(to_usize(i))
            .is_some_and(|slot| slot.state != VACANT && slot.id == id)
    });
    core.probes.set(core.probes.get().saturating_add(probes));
    found
}

/// Returns what `rpc_info` reports of `slot`, the slot at `index` whose gap
/// words are `stride` long, or `None` for a slot it does not list.
fn info_of<P: Copy>(
    view: &View<'_, P>,
    slot: &crate::Slot,
    index: usize,
    stride: usize,
) -> Option<RpcInfo<P>> {
    let state = match slot.state {
        C_OUT if slot.flags & (PATIENT | SENT) == PATIENT | SENT => RpcState::Parked,
        C_OUT => RpcState::Outgoing,
        C_IN | C_DONE | S_IN => RpcState::Incoming,
        S_EXEC => RpcState::Executing,
        S_RES => RpcState::Reserved,
        S_OUT => RpcState::Answered,
        _ => return None,
    };
    let peer = view.peers.get(to_usize(slot.peer))?.handle?;
    let (length, received) = match slot.state {
        C_IN | S_IN => {
            let start = index.checked_mul(stride)?;
            let own = view.gaps.get(start..start.checked_add(stride)?)?;
            (slot.in_len, received_of(own))
        }
        C_DONE | S_EXEC => (slot.in_len, slot.in_len),
        _ => (slot.out_len, 0),
    };
    Some(RpcInfo {
        id: RpcId::new(slot.id),
        peer,
        state,
        local_port: slot.port,
        length,
        sent: slot.out_max,
        granted: slot.out_granted,
        received,
    })
}

/// Returns the bytes received of a message whose gap words are `own`: the end
/// of the furthest byte received less every missing range.
pub(crate) fn received_of(own: &[u32]) -> u32 {
    let Some((&high, pairs)) = own.split_first() else {
        return 0;
    };
    let missing: u32 = pairs
        .chunks_exact(2)
        .map(|pair| pair[1].saturating_sub(pair[0]))
        .fold(0, u32::saturating_add);
    high.saturating_sub(missing)
}

/// Returns the words of the gaps of one slot.
pub(crate) const fn gap_stride(config: &Config) -> usize {
    1 + 2 * (config.max_gaps as usize)
}

/// Returns the cutoff version after `version`, which skips 0, the version a
/// sender names before it holds any cutoffs of the receiver.
#[cfg(feature = "prio")]
const fn next_version(version: u16) -> u16 {
    match version.wrapping_add(1) {
        0 => 1,
        next => next,
    }
}

impl<P, S, W> Reserve for Endpoint<P, S, W>
where
    P: Copy + Eq + Hash + PeerPort,
    S: Store<P>,
    W: Form,
{
    fn write_reserved(&mut self, slot: u32, at: u32, bytes: &[u8]) -> bool {
        let cx = self.cx();
        let Some(entry) = cx.parts.slots.get(to_usize(slot)) else {
            return false;
        };
        if entry.state != S_RES {
            return false;
        }
        let first = entry.out_page;
        cx.core.pages.write(cx.parts.pages, first, at, bytes)
    }

    fn commit_reserved(&mut self, slot: u32, len: u32) -> Result<(), CommitError> {
        let mut cx = self.cx();
        if cx.slot_state(slot) != S_RES {
            return Err(CommitError::Gone);
        }
        cx.trim_out(slot, len);
        cx.queue_reply(slot, len);
        Ok(())
    }

    fn cancel_reserved(&mut self, slot: u32) {
        let mut cx = self.cx();
        if cx.slot_state(slot) != S_RES {
            return;
        }
        cx.free_out(slot);
        if let Some(entry) = cx.parts.slots.get_mut(to_usize(slot)) {
            entry.state = S_EXEC;
            entry.out_len = 0;
        }
    }
}

#[cfg(all(feature = "client", feature = "server"))]
impl<P, S, W> crate::Rpcs for Endpoint<P, S, W>
where
    P: Copy + Eq + Hash + PeerPort,
    S: Store<P>,
    W: Form,
{
    type Peer = P;
    /// A request or a reply that cannot be queued, and
    /// [`SendError::NoRequest`](crate::SendError::NoRequest) for an identifier that names no RPC a
    /// release or an abort can end.
    type Error = crate::SendError;
    type Message<'a>
        = crate::Reader<'a>
    where
        Self: 'a;

    fn request(
        &mut self,
        peer: P,
        message: &[u8],
        options: crate::RequestOptions,
    ) -> Result<RpcId, crate::SendError> {
        Endpoint::request(self, peer, message, options)
    }

    fn reply(&mut self, id: RpcId, message: &[u8]) -> Result<(), crate::SendError> {
        Endpoint::reply(self, id, message)
    }

    fn next_event(&mut self) -> Result<Option<Event<P>>, crate::SendError> {
        Ok(self.poll_event())
    }

    fn message(&self, id: RpcId) -> Option<crate::Reader<'_>> {
        if id.is_server() {
            self.request_of(id)
        } else {
            self.response(id)
        }
    }

    /// Frees a message: the response and the client RPC with it, or the
    /// request of a server RPC, whose RPC stays open for its reply.
    fn release(&mut self, id: RpcId) -> Result<(), crate::SendError> {
        if !id.is_server() {
            return Endpoint::release(self, id)
                .map_err(|crate::UnknownRpc| crate::SendError::NoRequest);
        }
        let mut cx = self.cx();
        let index = cx
            .find_slot(id.get())
            .filter(|&i| cx.slot_state(i) == S_EXEC)
            .ok_or(crate::SendError::NoRequest)?;
        cx.free_in(index);
        if let Some(slot) = cx.parts.slots.get_mut(to_usize(index)) {
            slot.in_len = 0;
        }
        Ok(())
    }

    fn abort(&mut self, id: RpcId) -> Result<(), crate::SendError> {
        Endpoint::abort(self, id).map_err(|crate::UnknownRpc| crate::SendError::NoRequest)
    }
}

impl<P: Copy + Eq + Hash + PeerPort> Cx<'_, '_, P> {
    /// Returns the state of slot `index`, [`VACANT`] past the slots.
    pub(crate) fn slot_state(&self, index: u32) -> u8 {
        self.parts
            .slots
            .get(to_usize(index))
            .map_or(VACANT, |slot| slot.state)
    }

    /// Returns the state bits of peer `index`, 0 past the peers.
    pub(crate) fn peer_flags(&self, index: u32) -> u8 {
        self.parts
            .peers
            .get(to_usize(index))
            .map_or(0, |entry| entry.flags)
    }

    /// Returns the handle of peer `index`.
    pub(crate) fn handle(&self, index: u32) -> Option<P> {
        self.parts.peers.get(to_usize(index))?.handle
    }

    /// Returns the downlink or uplink scope of the profile as a question:
    /// whether every peer shares one link.
    pub(crate) fn shared_scope(&self) -> bool {
        self.core.profile.scope == crate::Scope::Shared
    }

    /// Adds `probes` to the count of probes beyond the first.
    fn count_probes(&self, probes: u64) {
        let total = self.core.probes.get().saturating_add(probes);
        self.core.probes.set(total);
    }

    /// Returns the place of `peer`.
    pub(crate) fn find_peer(&self, peer: P) -> Option<u32> {
        let hash = table::hash(&self.core.config.hash_key, &peer);
        let mut probes = 0;
        let peers = &*self.parts.peers;
        let found = table::find(self.parts.peer_index, hash, &mut probes, |i| {
            peers
                .get(to_usize(i))
                .and_then(|entry| entry.handle)
                .is_some_and(|handle| handle == peer)
        });
        self.count_probes(probes);
        found
    }

    /// Returns the slot of the RPC `id`.
    pub(crate) fn find_slot(&self, id: u64) -> Option<u32> {
        let hash = table::hash(&self.core.config.hash_key, &id);
        let mut probes = 0;
        let slots = &*self.parts.slots;
        let found = table::find(self.parts.slot_index, hash, &mut probes, |i| {
            slots
                .get(to_usize(i))
                .is_some_and(|slot| slot.state != VACANT && slot.id == id)
        });
        self.count_probes(probes);
        found
    }

    /// Returns the slot of the client RPC `id`.
    #[cfg(feature = "client")]
    pub(crate) fn find_client(&self, id: RpcId) -> Option<u32> {
        if id.is_server() {
            return None;
        }
        self.find_slot(id.get())
    }

    /// Checks limits a link is to run with.
    pub(crate) fn check_limits(&self, limits: &Limits) -> Result<(), LinkError> {
        if limits.max_packet < MIN_PACKET
            || limits.max_rpcs == 0
            || limits.max_message == 0
            || limits.unsched_bytes == 0
        {
            return Err(LinkError::Params);
        }
        if let crate::Window::Fixed(window) = self.core.profile.window {
            let packet = u32::from(limits.max_packet).min(limits.max_message);
            if window < packet {
                return Err(LinkError::Params);
            }
        }
        Ok(())
    }

    /// Takes a free place for `peer` and enters it in the peer table, or
    /// evicts an idle peer under [`Admission::Open`].
    pub(crate) fn admit(&mut self, peer: P) -> Option<u32> {
        let mut index = self.core.free_peer;
        if index == NONE {
            if self.core.profile.admission != Admission::Open {
                return None;
            }
            index = self.evict()?;
        } else {
            let entry = self.parts.peers.get_mut(to_usize(index))?;
            self.core.free_peer = entry.ctl_next;
        }
        let entry = self.parts.peers.get_mut(to_usize(index))?;
        *entry = crate::Peer::VACANT;
        entry.handle = Some(peer);
        if let Some(prio) = self.parts.peer_prio.get_mut(to_usize(index)) {
            *prio = crate::PeerPrio::VACANT;
        }
        let hash = table::hash(&self.core.config.hash_key, &peer);
        let mut probes = 0;
        let placed = table::insert(self.parts.peer_index, hash, index, &mut probes);
        self.count_probes(probes);
        if placed.is_err() {
            self.forget_peer(index);
            return None;
        }
        Some(index)
    }

    /// Frees the idle peer heard from longest ago and returns its place.
    fn evict(&mut self) -> Option<u32> {
        let busy = P_CTL_QUEUED | P_EV_QUEUED;
        let mut oldest: Option<(u32, Nanos)> = None;
        for (i, entry) in self.parts.peers.iter().enumerate() {
            let idle = entry.handle.is_some()
                && entry.client_rpcs == 0
                && entry.server_rpcs == 0
                && entry.acks == NONE
                && entry.flags & busy == 0;
            if !idle {
                continue;
            }
            let i = u32::try_from(i).ok()?;
            if oldest.is_none_or(|(_, stamp)| entry.stamp < stamp) {
                oldest = Some((i, entry.stamp));
            }
        }
        let (index, _) = oldest?;
        self.forget_peer(index);
        let entry = self.parts.peers.get_mut(to_usize(index))?;
        self.core.free_peer = entry.ctl_next;
        Some(index)
    }

    /// Removes peer `index` from the peer table and returns its place to the
    /// free list.
    pub(crate) fn forget_peer(&mut self, index: u32) {
        let flags = self.peer_flags(index);
        if flags & P_CTL_QUEUED != 0 {
            self.ctl_unlink(index | PEER_TAG);
        }
        if flags & P_EV_QUEUED != 0 {
            self.ev_unlink(index | PEER_TAG);
        }
        let Some(handle) = self.handle(index) else {
            return;
        };
        let key = self.core.config.hash_key;
        let hash = table::hash(&key, &handle);
        let mut probes = 0;
        let peers = &*self.parts.peers;
        // A missing entry leaves nothing to remove, so the error needs no
        // answer.
        let _ = table::remove(self.parts.peer_index, hash, index, &mut probes, |i| {
            peers
                .get(to_usize(i))
                .and_then(|entry| entry.handle)
                .map_or(0, |h| table::hash(&key, &h))
        });
        self.count_probes(probes);
        if let Some(entry) = self.parts.peers.get_mut(to_usize(index)) {
            *entry = crate::Peer::VACANT;
            entry.ctl_next = self.core.free_peer;
        }
        self.core.free_peer = index;
    }

    /// Starts the link of peer `index` with `local` and `assumed`, and owes
    /// the peer this end's LIMITS where the form carries them.
    pub(crate) fn start_peer(&mut self, index: u32, local: Limits, assumed: Limits) {
        let now = self.core.now;
        let Some(entry) = self.parts.peers.get_mut(to_usize(index)) else {
            return;
        };
        entry.local = local;
        entry.remote = assumed;
        entry.stamp = now;
        entry.tries = 0;
        entry.flags = (entry.flags & (P_CTL_QUEUED | P_EV_QUEUED | EV_LIMITS | EV_GONE)) | UP;
        if self.core.limits_on_wire {
            self.owe_peer(index, P_LIMITS);
        }
        self.dirty();
    }

    /// Ends every RPC of peer `index` and reports the peer gone for `why`.
    ///
    /// The incoming messages of the peer leave the grant heap before any RPC
    /// ends, so the grants their ends hand on reach no message of the peer
    /// itself, and under [`Scope::Shared`](crate::Scope::Shared) the downlink
    /// every peer shares grants to its next message once the peer's are
    /// gone.
    pub(crate) fn end_peer(&mut self, index: u32, why: Gone) {
        #[cfg(feature = "client")]
        let failure = match why {
            Gone::LinkDown => Failure::LinkDown,
            Gone::Timeout => Failure::PeerTimeout,
        };
        let count = u32::try_from(self.parts.slots.len()).unwrap_or(NONE);
        for slot in 0..count {
            let incoming = self
                .parts
                .slots
                .get(to_usize(slot))
                .is_some_and(|entry| entry.peer == index && matches!(entry.state, C_IN | S_IN));
            if incoming {
                self.heap_out(slot);
            }
        }
        for slot in 0..count {
            let Some(entry) = self.parts.slots.get(to_usize(slot)) else {
                break;
            };
            if entry.state == VACANT || entry.state == ENDED || entry.peer != index {
                continue;
            }
            match entry.state {
                #[cfg(feature = "client")]
                C_OUT | C_IN => self.end_slot(slot, EV_FAILED + failure_code(failure)),
                S_IN | S_EXEC | S_RES => self.end_slot(slot, EV_DROPPED),
                _ => self.free_slot(slot),
            }
        }
        if self.shared_scope() {
            grant::schedule(self, index);
        }
        let down = why == Gone::LinkDown;
        let Some(entry) = self.parts.peers.get_mut(to_usize(index)) else {
            return;
        };
        entry.gone = u8::from(down);
        entry.tries = 0;
        entry.expected = 0;
        entry.ctl = 0;
        entry.unknown = 0;
        entry.unacked = 0;
        entry.flags |= EV_GONE;
        if down {
            entry.flags &= !(UP | KNOWN);
        }
        self.core.stats.peer_deaths = self.core.stats.peer_deaths.saturating_add(u64::from(!down));
        self.ev_push(index | PEER_TAG);
        self.dirty();
    }

    /// Owes peer `index` the control packets of `bits` and queues it.
    pub(crate) fn owe_peer(&mut self, index: u32, bits: u8) {
        let Some(entry) = self.parts.peers.get_mut(to_usize(index)) else {
            return;
        };
        entry.ctl |= bits;
        if entry.flags & P_CTL_QUEUED == 0 {
            entry.flags |= P_CTL_QUEUED;
            self.ctl_push(index | PEER_TAG);
        }
    }

    /// Owes the control packets of `bits` for slot `index` and queues it.
    pub(crate) fn owe_slot(&mut self, index: u32, bits: u8) {
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        slot.ctl |= bits;
        if slot.flags & CTL_QUEUED == 0 {
            slot.flags |= CTL_QUEUED;
            self.ctl_push(index);
        }
    }

    /// Returns the next node of `node` in the control queue.
    pub(crate) fn ctl_next(&self, node: u32) -> u32 {
        if node & PEER_TAG != 0 {
            self.parts
                .peers
                .get(to_usize(node & !PEER_TAG))
                .map_or(NONE, |entry| entry.ctl_next)
        } else {
            self.parts
                .slots
                .get(to_usize(node))
                .map_or(NONE, |slot| slot.ctl_next)
        }
    }

    /// Sets the next node of `node` in the control queue.
    pub(crate) fn set_ctl_next(&mut self, node: u32, next: u32) {
        if node & PEER_TAG != 0 {
            if let Some(entry) = self.parts.peers.get_mut(to_usize(node & !PEER_TAG)) {
                entry.ctl_next = next;
            }
        } else if let Some(slot) = self.parts.slots.get_mut(to_usize(node)) {
            slot.ctl_next = next;
        }
    }

    /// Appends `node` to the control queue.
    fn ctl_push(&mut self, node: u32) {
        self.set_ctl_next(node, NONE);
        if self.core.ctl_tail == NONE {
            self.core.ctl_head = node;
        } else {
            self.set_ctl_next(self.core.ctl_tail, node);
        }
        self.core.ctl_tail = node;
    }

    /// Takes `node` out of the control queue, where `prev` is the node before
    /// it or [`NONE`] at the head.
    pub(crate) fn ctl_unlink_after(&mut self, prev: u32, node: u32) {
        let next = self.ctl_next(node);
        if prev == NONE {
            self.core.ctl_head = next;
        } else {
            self.set_ctl_next(prev, next);
        }
        if self.core.ctl_tail == node {
            self.core.ctl_tail = prev;
        }
        self.set_ctl_next(node, NONE);
        if node & PEER_TAG != 0 {
            if let Some(entry) = self.parts.peers.get_mut(to_usize(node & !PEER_TAG)) {
                entry.flags &= !P_CTL_QUEUED;
            }
        } else if let Some(slot) = self.parts.slots.get_mut(to_usize(node)) {
            slot.flags &= !CTL_QUEUED;
        }
    }

    /// Takes `node` out of the control queue wherever it stands.
    fn ctl_unlink(&mut self, node: u32) {
        let mut prev = NONE;
        let mut at = self.core.ctl_head;
        let mut steps = 0usize;
        let bound = self.parts.slots.len() + self.parts.peers.len() + 1;
        while at != NONE && steps <= bound {
            if at == node {
                self.ctl_unlink_after(prev, node);
                return;
            }
            prev = at;
            at = self.ctl_next(at);
            steps += 1;
        }
    }

    /// Returns the next node of `node` in the event queue.
    fn ev_next(&self, node: u32) -> u32 {
        if node & PEER_TAG != 0 {
            self.parts
                .peers
                .get(to_usize(node & !PEER_TAG))
                .map_or(NONE, |entry| entry.ev_next)
        } else {
            self.parts
                .slots
                .get(to_usize(node))
                .map_or(NONE, |slot| slot.next)
        }
    }

    /// Sets the next node of `node` in the event queue.
    fn set_ev_next(&mut self, node: u32, next: u32) {
        if node & PEER_TAG != 0 {
            if let Some(entry) = self.parts.peers.get_mut(to_usize(node & !PEER_TAG)) {
                entry.ev_next = next;
            }
        } else if let Some(slot) = self.parts.slots.get_mut(to_usize(node)) {
            slot.next = next;
        }
    }

    /// Appends `node` to the event queue unless it waits there already.
    pub(crate) fn ev_push(&mut self, node: u32) {
        let queued = if node & PEER_TAG != 0 {
            self.parts
                .peers
                .get_mut(to_usize(node & !PEER_TAG))
                .is_none_or(|entry| {
                    let queued = entry.flags & P_EV_QUEUED != 0;
                    entry.flags |= P_EV_QUEUED;
                    queued
                })
        } else {
            self.parts.slots.get_mut(to_usize(node)).is_none_or(|slot| {
                let queued = slot.flags & EV_QUEUED != 0;
                slot.flags |= EV_QUEUED;
                queued
            })
        };
        if queued {
            return;
        }
        self.set_ev_next(node, NONE);
        if self.core.ev_tail == NONE {
            self.core.ev_head = node;
        } else {
            self.set_ev_next(self.core.ev_tail, node);
        }
        self.core.ev_tail = node;
    }

    /// Takes `node` out of the event queue wherever it stands.
    fn ev_unlink(&mut self, node: u32) {
        let mut prev = NONE;
        let mut at = self.core.ev_head;
        let mut steps = 0usize;
        let bound = self.parts.slots.len() + self.parts.peers.len() + 1;
        while at != NONE && steps <= bound {
            if at == node {
                let next = self.ev_next(node);
                if prev == NONE {
                    self.core.ev_head = next;
                } else {
                    self.set_ev_next(prev, next);
                }
                if self.core.ev_tail == node {
                    self.core.ev_tail = prev;
                }
                self.set_ev_next(node, NONE);
                break;
            }
            prev = at;
            at = self.ev_next(at);
            steps += 1;
        }
        if node & PEER_TAG != 0 {
            if let Some(entry) = self.parts.peers.get_mut(to_usize(node & !PEER_TAG)) {
                entry.flags &= !P_EV_QUEUED;
            }
        } else if let Some(slot) = self.parts.slots.get_mut(to_usize(node)) {
            slot.flags &= !EV_QUEUED;
        }
    }

    /// Takes the first event of the queue.
    fn pop_event(&mut self) -> Option<Event<P>> {
        loop {
            let node = self.core.ev_head;
            if node == NONE {
                return None;
            }
            let event = if node & PEER_TAG != 0 {
                self.peer_event(node & !PEER_TAG)
            } else {
                self.slot_event(node)
            };
            if event.is_some() {
                return event;
            }
        }
    }

    /// Takes the next event of peer `index` at the head of the queue.
    fn peer_event(&mut self, index: u32) -> Option<Event<P>> {
        let node = index | PEER_TAG;
        let Some(entry) = self.parts.peers.get_mut(to_usize(index)) else {
            self.ev_unlink(node);
            return None;
        };
        let handle = entry.handle;
        if entry.flags & EV_LIMITS != 0 {
            entry.flags &= !EV_LIMITS;
            let limits = entry.remote;
            let more = entry.flags & EV_GONE != 0;
            if !more && !self.cutoffs_pending(index) {
                self.ev_unlink(node);
            }
            return handle.map(|peer| Event::Limits { peer, limits });
        }
        if self.take_cutoffs_event(index) {
            if self.peer_flags(index) & EV_GONE == 0 {
                self.ev_unlink(node);
            }
            return handle.map(|peer| Event::Cutoffs { peer });
        }
        let Some(entry) = self.parts.peers.get_mut(to_usize(index)) else {
            self.ev_unlink(node);
            return None;
        };
        let gone = entry.flags & EV_GONE != 0;
        let why = if entry.gone == 1 {
            Gone::LinkDown
        } else {
            Gone::Timeout
        };
        entry.flags &= !EV_GONE;
        let up = entry.flags & UP != 0;
        self.ev_unlink(node);
        if !up {
            self.forget_peer(index);
        }
        if gone {
            return handle.map(|peer| Event::PeerGone { peer, why });
        }
        None
    }

    /// Returns whether a change of the cutoffs of peer `index` waits to be
    /// reported.
    #[cfg(feature = "prio")]
    fn cutoffs_pending(&self, index: u32) -> bool {
        self.parts
            .peer_prio
            .get(to_usize(index))
            .is_some_and(|entry| entry.event)
    }

    /// Returns false, since a build without the feature `prio` keeps no
    /// cutoffs of a peer.
    #[cfg(not(feature = "prio"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature prio"
    )]
    fn cutoffs_pending(&self, _index: u32) -> bool {
        false
    }

    /// Takes the report of a change of the cutoffs of peer `index`, and
    /// returns whether one waited.
    #[cfg(feature = "prio")]
    fn take_cutoffs_event(&mut self, index: u32) -> bool {
        self.parts
            .peer_prio
            .get_mut(to_usize(index))
            .is_some_and(|entry| core::mem::replace(&mut entry.event, false))
    }

    /// Returns false, since a build without the feature `prio` keeps no
    /// cutoffs of a peer.
    #[cfg(not(feature = "prio"))]
    #[expect(
        clippy::unused_self,
        reason = "the signature of the function of the feature prio"
    )]
    fn take_cutoffs_event(&mut self, _index: u32) -> bool {
        false
    }

    /// Takes the event of slot `index` at the head of the queue.
    fn slot_event(&mut self, index: u32) -> Option<Event<P>> {
        self.ev_unlink(index);
        let slot = self.parts.slots.get_mut(to_usize(index))?;
        let kind = core::mem::replace(&mut slot.event, EV_NONE);
        let id = RpcId::new(slot.id);
        let peer_index = slot.peer;
        match kind {
            EV_REQUEST => self
                .handle(peer_index)
                .map(|peer| Event::Request { peer, id }),
            #[cfg(feature = "client")]
            EV_RESPONSE => Some(Event::Response { id }),
            EV_DROPPED => {
                self.free_slot(index);
                Some(Event::Dropped { id })
            }
            #[cfg(feature = "client")]
            EV_NONE => None,
            #[cfg(feature = "client")]
            code => {
                self.free_slot(index);
                Some(Event::Failed {
                    id,
                    why: failure_of(code.saturating_sub(EV_FAILED)),
                })
            }
            // A build without the role `client` reports no response and no
            // failed RPC.
            #[cfg(not(feature = "client"))]
            _ => None,
        }
    }

    /// Ends the RPC of slot `index` and leaves it to report `event`.
    pub(crate) fn end_slot(&mut self, index: u32, event: u8) {
        self.stop_slot(index);
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        slot.state = ENDED;
        slot.event = event;
        self.ev_push(index);
    }

    /// Reports the RPC of slot `index` with `event`.
    pub(crate) fn report(&mut self, index: u32, event: u8) {
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            slot.event = event;
        }
        self.ev_push(index);
    }

    /// Takes slot `index` out of its heap, its queues and its peer's counts
    /// and frees its pages, leaving the slot itself in place.
    ///
    /// An incoming message that stood in the grant heap or held bytes its
    /// downlink expects leaves them unfinished here, so the grant scheduler
    /// of its downlink runs again, since no packet of this message will run
    /// it.
    fn stop_slot(&mut self, index: u32) {
        let unfinished = self.parts.slots.get(to_usize(index)).and_then(|slot| {
            let incoming = matches!(slot.state, C_IN | S_IN);
            let held = slot.flags & IN_GRANT != 0 || self.owed(index) > 0;
            (incoming && held).then_some(slot.peer)
        });
        self.heap_out(index);
        let flags = self
            .parts
            .slots
            .get(to_usize(index))
            .map_or(0, |slot| slot.flags);
        if flags & CTL_QUEUED != 0 {
            self.ctl_unlink(index);
        }
        self.forget_expected(index);
        self.free_in(index);
        self.free_out(index);
        self.uncount(index);
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            slot.ctl = 0;
        }
        self.dirty();
        if let Some(peer) = unfinished {
            grant::schedule(self, peer);
        }
    }

    /// Frees slot `index` entirely.
    pub(crate) fn free_slot(&mut self, index: u32) {
        let Some(slot) = self.parts.slots.get(to_usize(index)) else {
            return;
        };
        if slot.state == VACANT {
            return;
        }
        let (flags, id) = (slot.flags, slot.id);
        #[cfg(feature = "client")]
        if slot.state == C_ACK {
            self.unlink_ack(index);
        }
        self.stop_slot(index);
        if flags & EV_QUEUED != 0 {
            self.ev_unlink(index);
        }
        let key = self.core.config.hash_key;
        let hash = table::hash(&key, &id);
        let mut probes = 0;
        let slots = &*self.parts.slots;
        // A slot missing from the table leaves nothing to remove.
        let _ = table::remove(self.parts.slot_index, hash, index, &mut probes, |i| {
            slots
                .get(to_usize(i))
                .map_or(0, |slot| table::hash(&key, &slot.id))
        });
        self.count_probes(probes);
        let client = index < self.core.config.client_slots;
        let head = if client {
            self.core.free_client
        } else {
            self.core.free_server
        };
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            *slot = crate::Slot::VACANT;
            slot.next = head;
        }
        if client {
            self.core.free_client = index;
            self.core.clients_open = self.core.clients_open.saturating_sub(1);
        } else {
            self.core.free_server = index;
        }
    }

    /// Drops the counts of slot `index` from its peer.
    fn uncount(&mut self, index: u32) {
        let Some(slot) = self.parts.slots.get(to_usize(index)) else {
            return;
        };
        let (peer, client) = (slot.peer, index < self.core.config.client_slots);
        if slot.state == ENDED || slot.state == VACANT {
            return;
        }
        if let Some(entry) = self.parts.peers.get_mut(to_usize(peer)) {
            if client {
                entry.client_rpcs = entry.client_rpcs.saturating_sub(1);
            } else {
                entry.server_rpcs = entry.server_rpcs.saturating_sub(1);
            }
        }
    }

    /// Takes a free slot of the client role or the server role.
    pub(crate) fn take_slot(&mut self, client: bool) -> Option<u32> {
        let head = if client {
            self.core.free_client
        } else {
            self.core.free_server
        };
        if head == NONE {
            return None;
        }
        let slot = self.parts.slots.get_mut(to_usize(head))?;
        let next = slot.next;
        *slot = crate::Slot::VACANT;
        if client {
            self.core.free_client = next;
            self.core.clients_open = self.core.clients_open.saturating_add(1);
        } else {
            self.core.free_server = next;
        }
        Some(head)
    }

    /// Enters slot `index` with identifier `id` in the slot table.
    pub(crate) fn enter_slot(&mut self, index: u32, id: u64) -> bool {
        let hash = table::hash(&self.core.config.hash_key, &id);
        let mut probes = 0;
        let placed = table::insert(self.parts.slot_index, hash, index, &mut probes);
        self.count_probes(probes);
        placed.is_ok()
    }

    /// Takes a chain of pages for a message of `len` bytes, [`NO_PAGE`] for an
    /// empty one, or `None` when too few pages are free.
    pub(crate) fn take_pages(&mut self, len: u32) -> Option<u16> {
        let needed = self.core.pages.needed(len);
        if needed == 0 {
            return Some(NO_PAGE);
        }
        if needed > MAX_PAGES || needed > self.core.free_pages {
            return None;
        }
        let first = self.core.free_page;
        let mut last = first;
        for _ in 1..needed {
            last = self.core.pages.link(self.parts.pages, last);
            if last == NO_PAGE {
                return None;
            }
        }
        self.core.free_page = self.core.pages.link(self.parts.pages, last);
        self.core.pages.set_link(self.parts.pages, last, NO_PAGE);
        self.core.free_pages -= needed;
        Some(first)
    }

    /// Returns the chain from `first` to the free pages.
    pub(crate) fn give_pages(&mut self, first: u16) {
        let mut page = first;
        let mut steps = 0;
        while page != NO_PAGE && steps < MAX_PAGES {
            let next = self.core.pages.link(self.parts.pages, page);
            self.core
                .pages
                .set_link(self.parts.pages, page, self.core.free_page);
            self.core.free_page = page;
            self.core.free_pages = self.core.free_pages.saturating_add(1);
            page = next;
            steps += 1;
        }
    }

    /// Frees the pages of the incoming message of slot `index`.
    pub(crate) fn free_in(&mut self, index: u32) {
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        let first = core::mem::replace(&mut slot.in_page, NO_PAGE);
        self.give_pages(first);
    }

    /// Frees the pages of the outgoing message of slot `index`.
    pub(crate) fn free_out(&mut self, index: u32) {
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        let first = core::mem::replace(&mut slot.out_page, NO_PAGE);
        self.give_pages(first);
    }

    /// Cuts the reserved chain of slot `index` to the pages `len` bytes
    /// need.
    fn trim_out(&mut self, index: u32, len: u32) {
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        let keep = self.core.pages.needed(len);
        if keep == 0 {
            let first = core::mem::replace(&mut slot.out_page, NO_PAGE);
            self.give_pages(first);
            return;
        }
        let mut last = slot.out_page;
        for _ in 1..keep {
            last = self.core.pages.link(self.parts.pages, last);
            if last == NO_PAGE {
                return;
            }
        }
        let rest = self.core.pages.link(self.parts.pages, last);
        self.core.pages.set_link(self.parts.pages, last, NO_PAGE);
        self.give_pages(rest);
    }

    /// Returns a writer of the outgoing message of slot `index`.
    #[cfg(any(feature = "client", feature = "server"))]
    pub(crate) fn writer(&mut self, index: u32, len: u32) -> Option<crate::Writer<'_>> {
        let first = self.parts.slots.get(to_usize(index))?.out_page;
        Some(crate::Writer::new(
            self.parts.pages,
            self.core.pages,
            first,
            len,
        ))
    }

    /// Takes slot `index` out of the heap its position names.
    pub(crate) fn heap_out(&mut self, index: u32) {
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        let (pos, flags) = (slot.pos, slot.flags);
        slot.pos = NONE;
        slot.flags &= !(IN_TX | IN_GRANT);
        if pos == NONE {
            return;
        }
        let slots = &mut *self.parts.slots;
        let moved = |slot: u32, at: u32| {
            if let Some(entry) = slots.get_mut(to_usize(slot)) {
                entry.pos = at;
            }
        };
        if flags & IN_TX != 0 {
            let mut heap = Heap::new(self.parts.tx_heap, &mut self.core.tx_len);
            heap.remove(pos, moved);
        } else if flags & IN_GRANT != 0 {
            let mut heap = Heap::new(self.parts.grant_heap, &mut self.core.grant_len);
            heap.remove(pos, moved);
        }
    }

    /// Places slot `index` in the transmit heap with `key`, or moves it
    /// there.
    pub(crate) fn tx_key(&mut self, index: u32, key: u32) {
        let Some(slot) = self.parts.slots.get(to_usize(index)) else {
            return;
        };
        let (pos, flags) = (slot.pos, slot.flags);
        if flags & IN_GRANT != 0 {
            self.heap_out(index);
        }
        let in_tx = flags & IN_TX != 0 && pos != NONE;
        let age = if in_tx {
            0
        } else {
            let age = self.core.age;
            self.core.age = age.saturating_add(1);
            age
        };
        let slots = &mut *self.parts.slots;
        let moved = |slot: u32, at: u32| {
            if let Some(entry) = slots.get_mut(to_usize(slot)) {
                entry.pos = at;
                entry.flags |= IN_TX;
            }
        };
        let mut heap = Heap::new(self.parts.tx_heap, &mut self.core.tx_len);
        // A heap as long as the slots never lacks room, and a stale position
        // leaves the slot where it stood, so neither error needs an answer.
        let _ = if in_tx {
            heap.rekey(pos, key, moved)
        } else {
            heap.push(Item::new(index, key, age), moved)
        };
    }

    /// Places slot `index` in the grant heap with `key`, or moves it there.
    pub(crate) fn grant_key(&mut self, index: u32, key: u32) {
        let Some(slot) = self.parts.slots.get(to_usize(index)) else {
            return;
        };
        let (pos, flags) = (slot.pos, slot.flags);
        if flags & IN_TX != 0 {
            self.heap_out(index);
        }
        let in_grant = flags & IN_GRANT != 0 && pos != NONE;
        let age = if in_grant {
            0
        } else {
            let age = self.core.age;
            self.core.age = age.saturating_add(1);
            age
        };
        let slots = &mut *self.parts.slots;
        let moved = |slot: u32, at: u32| {
            if let Some(entry) = slots.get_mut(to_usize(slot)) {
                entry.pos = at;
                entry.flags |= IN_GRANT;
            }
        };
        let mut heap = Heap::new(self.parts.grant_heap, &mut self.core.grant_len);
        // As in `tx_key`, neither error of the heap can arise here.
        let _ = if in_grant {
            heap.rekey(pos, key, moved)
        } else {
            heap.push(Item::new(index, key, age), moved)
        };
    }

    /// Returns the gap words of slot `index` to change.
    pub(crate) fn gaps_mut(&mut self, index: u32) -> Option<&mut [u32]> {
        let stride = gap_stride(&self.core.config);
        let start = to_usize(index).checked_mul(stride)?;
        self.parts.gaps.get_mut(start..start.checked_add(stride)?)
    }

    /// Returns the bytes of the incoming message of slot `index` received.
    pub(crate) fn received(&self, index: u32) -> u32 {
        let stride = gap_stride(&self.core.config);
        let Some(start) = to_usize(index).checked_mul(stride) else {
            return 0;
        };
        self.parts
            .gaps
            .get(start..start.saturating_add(stride))
            .map_or(0, received_of)
    }

    /// Returns the bytes of slot `index` granted or unscheduled and not
    /// received, which the downlink expects.
    pub(crate) fn owed(&self, index: u32) -> u32 {
        let Some(slot) = self.parts.slots.get(to_usize(index)) else {
            return 0;
        };
        if !matches!(slot.state, C_IN | S_IN) {
            return 0;
        }
        slot.in_granted.saturating_sub(self.received(index))
    }

    /// Returns the counter of expected bytes of the downlink of peer `peer`.
    pub(crate) fn expected_mut(&mut self, peer: u32) -> Option<&mut u32> {
        if self.shared_scope() {
            return Some(&mut self.core.shared.expected);
        }
        self.parts
            .peers
            .get_mut(to_usize(peer))
            .map(|entry| &mut entry.expected)
    }

    /// Moves the expected bytes of the downlink of slot `index` from `before`
    /// to what the slot owes now.
    pub(crate) fn settle(&mut self, index: u32, before: u32) {
        let after = self.owed(index);
        let Some(peer) = self.parts.slots.get(to_usize(index)).map(|slot| slot.peer) else {
            return;
        };
        if let Some(expected) = self.expected_mut(peer) {
            *expected = expected.saturating_sub(before).saturating_add(after);
        }
    }

    /// Drops what slot `index` owes from the expected bytes of its downlink.
    fn forget_expected(&mut self, index: u32) {
        let before = self.owed(index);
        let Some(peer) = self.parts.slots.get(to_usize(index)).map(|slot| slot.peer) else {
            return;
        };
        if let Some(expected) = self.expected_mut(peer) {
            *expected = expected.saturating_sub(before);
        }
    }

    /// Queues the ACK of the client RPC of slot `index`, whose response is
    /// whole, and frees its pages.
    #[cfg(feature = "client")]
    fn acknowledge(&mut self, index: u32) {
        let flags = self
            .parts
            .slots
            .get(to_usize(index))
            .map_or(0, |slot| slot.flags);
        if flags & EV_QUEUED != 0 {
            self.ev_unlink(index);
        }
        self.free_in(index);
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        slot.state = C_ACK;
        let peer = slot.peer;
        let Some(entry) = self.parts.peers.get_mut(to_usize(peer)) else {
            return;
        };
        let head = core::mem::replace(&mut entry.acks, index);
        if let Some(slot) = self.parts.slots.get_mut(to_usize(index)) {
            slot.next = head;
        }
        self.owe_peer(peer, P_ACKS);
    }

    /// Takes client slot `index` out of its peer's list of acknowledgements.
    #[cfg(feature = "client")]
    fn unlink_ack(&mut self, index: u32) {
        let Some(peer) = self.parts.slots.get(to_usize(index)).map(|slot| slot.peer) else {
            return;
        };
        let Some(head) = self.parts.peers.get(to_usize(peer)).map(|entry| entry.acks) else {
            return;
        };
        let next_of =
            |slots: &[crate::Slot], i: u32| slots.get(to_usize(i)).map_or(NONE, |s| s.next);
        if head == index {
            let next = next_of(self.parts.slots, index);
            if let Some(entry) = self.parts.peers.get_mut(to_usize(peer)) {
                entry.acks = next;
            }
            return;
        }
        let mut at = head;
        let mut steps = 0;
        while at != NONE && steps <= self.parts.slots.len() {
            let next = next_of(self.parts.slots, at);
            if next == index {
                let after = next_of(self.parts.slots, index);
                if let Some(slot) = self.parts.slots.get_mut(to_usize(at)) {
                    slot.next = after;
                }
                return;
            }
            at = next;
            steps += 1;
        }
    }

    /// Queues the reply of `len` bytes that the pages of slot `index` hold,
    /// frees its request and starts its transmission.
    pub(crate) fn queue_reply(&mut self, index: u32, len: u32) {
        self.free_in(index);
        let now = self.core.now;
        let unsched = self.unsched_limit(index);
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        slot.state = S_OUT;
        slot.in_len = 0;
        slot.out_len = len;
        slot.out_next = 0;
        slot.out_max = 0;
        slot.stamp = now;
        slot.flags &= !RESENT;
        self.count_honoured(index);
        self.start_out(index, len, unsched);
        self.refresh_tx(index);
        self.dirty();
    }

    /// Returns the bytes the outgoing message of slot `index` sends
    /// unscheduled at most: the peer's unscheduled bytes, and on a server
    /// that honours incast control no more than the incast bytes where the
    /// request arrived marked.
    pub(crate) fn unsched_limit(&self, index: u32) -> u32 {
        let Some(slot) = self.parts.slots.get(to_usize(index)) else {
            return 0;
        };
        let unsched = self
            .parts
            .peers
            .get(to_usize(slot.peer))
            .map_or(0, |entry| entry.remote.unsched_bytes);
        let answers_marked = slot.marks & INCAST != 0 && slot.id & 1 == 1;
        match self.core.profile.incast {
            Some(incast) if cfg!(feature = "incast") && answers_marked => {
                unsched.min(incast.unsched_bytes)
            }
            _ => unsched,
        }
    }

    /// Counts a marked request whose response the server keeps unscheduled
    /// up to the incast bytes alone.
    fn count_honoured(&mut self, index: u32) {
        let marked = self
            .parts
            .slots
            .get(to_usize(index))
            .is_some_and(|slot| slot.marks & INCAST != 0);
        if cfg!(feature = "incast") && marked && self.core.profile.incast.is_some() {
            let stats = &mut self.core.stats;
            stats.incast_honoured = stats.incast_honoured.saturating_add(1);
        }
    }

    /// Sets how the outgoing message of `len` bytes of slot `index` starts:
    /// its first `unsched` bytes granted at once, or under all or nothing
    /// scheduling, where it is longer than them, no byte before a GRANT and a
    /// `START_MSG` owed in place of its first DATA packet.
    pub(crate) fn start_out(&mut self, index: u32, len: u32, unsched: u32) {
        let all_or_nothing =
            cfg!(feature = "start-msg") && self.core.profile.scheduling == Scheduling::AllOrNothing;
        let scheduled = all_or_nothing && len > unsched;
        let Some(slot) = self.parts.slots.get_mut(to_usize(index)) else {
            return;
        };
        if scheduled {
            slot.out_granted = 0;
            slot.marks |= SCHEDULED;
        } else {
            slot.out_granted = len.min(unsched);
            slot.marks &= !SCHEDULED;
        }
        if scheduled {
            self.owe_slot(index, START);
        }
    }

    /// Returns the key of slot `index` in the transmit heap: the bytes its
    /// outgoing message has left where it holds bytes to transmit now, and
    /// `u32::MAX` otherwise.
    pub(crate) fn tx_key_of(&self, index: u32) -> u32 {
        let Some(slot) = self.parts.slots.get(to_usize(index)) else {
            return u32::MAX;
        };
        let live = matches!(slot.state, C_OUT | S_OUT);
        if live && slot.out_next < slot.out_len && slot.out_next < slot.out_granted {
            slot.out_len - slot.out_next
        } else if live && slot.out_len == 0 && slot.flags & SENT == 0 {
            // An empty message is one DATA packet with no payload.
            0
        } else {
            u32::MAX
        }
    }

    /// Places slot `index` in the transmit heap at the key its state gives.
    pub(crate) fn refresh_tx(&mut self, index: u32) {
        let key = self.tx_key_of(index);
        self.tx_key(index, key);
    }

    /// Marks the timers for a new deadline.
    pub(crate) fn dirty(&self) {
        self.core.deadline.set(Due::Stale);
    }
}

/// Returns the code of `failure` in the event of a slot.
pub(crate) const fn failure_code(failure: Failure) -> u8 {
    match failure {
        Failure::LinkDown => 0,
        Failure::PeerTimeout => 1,
        Failure::TooLong => 2,
        Failure::Aborted => 3,
        Failure::RpcUnknown => 4,
    }
}

/// Returns the failure of `code` in the event of a slot.
#[cfg(feature = "client")]
const fn failure_of(code: u8) -> Failure {
    match code {
        0 => Failure::LinkDown,
        1 => Failure::PeerTimeout,
        2 => Failure::TooLong,
        4 => Failure::RpcUnknown,
        _ => Failure::Aborted,
    }
}
