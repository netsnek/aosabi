// SPDX-License-Identifier: MIT
/*
 * homa store: the configuration and the caller owned storage of an endpoint.
 *
 * The shapes are those of the Homa design note, second version, homa/design.md
 * section 7.3, lines 2327 to 2437, with the page store its section 7.3 argues
 * for, a slot of at most 72 bytes and a peer of at most 96 from its section
 * 8.3, lines 2941 to 2991, and the rules of storage the first version fixes,
 * homa/design-v1.md section 2.2, lines 364 to 401.
 */

//! The configuration, the storage and the page store of an endpoint.
//!
//! An endpoint runs over storage the caller owns, in typed parts, and
//! allocates nothing. [`Config`] fixes how much of each part it needs, which
//! [`Config::storage`] states, and [`Owned`] allocates that storage once under
//! the feature `alloc`. A caller without `alloc` places the parts in a
//! `static` or in the state of a task that never returns, with
//! [`Peer::VACANT`] and [`Slot::VACANT`] for the typed parts.
//!
//! **The page store.** The pages are one byte slice cut into `page_len` pieces
//! by index arithmetic, as `HomaModule` hands out its receive buffer in pages
//! of 64 KiB. Behind the pages the same slice holds the page table, two bytes
//! a page, which chains the pages of one message in order and the free pages
//! in a list. A slot names the first page of its request and of its response,
//! so a slot stays small whatever its messages hold, and one message spans at
//! most [`MAX_PAGES`] pages. A device sets `page_len` to its longest message
//! and keeps one page a message, and a server sets it to 65,536.
//!
//! A message is read with a [`Reader`] and written with a [`Writer`] or a
//! [`ReplyWriter`], which walk its pages in order, so a message never needs
//! one contiguous buffer.

#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::fmt;

use crate::{
    heap::Item,
    table::{
        self,
        Bucket, //
    },
    Limits,
    Nanos,
    NoRoom,
    Read, //
};

/// The most pages one message spans.
///
/// A message of 1,000,000 bytes, the longest `HomaModule` takes, fits 16 pages
/// of 65,536 bytes.
pub const MAX_PAGES: u32 = 16;

/// The most pages a pool holds, since the page table names a page in two
/// bytes and keeps `u16::MAX` for the end of a chain.
pub const MAX_POOL: u32 = 65_535;

/// The bytes of one entry of the page table.
const LINK_LEN: usize = 2;

/// The page table entry that ends a chain.
pub(crate) const NO_PAGE: u16 = u16::MAX;

/// The index that names no peer, no slot and no position.
pub(crate) const NONE: u32 = u32::MAX;

/// The sizes an endpoint is built for, which fix its storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Peers served at once.
    pub peers: u32,
    /// RPC slots in which this endpoint is the client.
    pub client_slots: u32,
    /// RPC slots in which this endpoint is the server.
    pub server_slots: u32,
    /// Bytes of one page. A device sets it to its longest message, a server
    /// to 65,536.
    pub page_len: u32,
    /// Pages in the pool, at most [`MAX_POOL`].
    pub pages: u32,
    /// Gaps a message may have while it arrives.
    pub max_gaps: u8,
    /// Buckets of the histogram of incoming sizes, 0 without `prio-compute`.
    pub size_buckets: u8,
    /// The first client identifier, even and not 0, from the randomness
    /// capability.
    pub first_id: u64,
    /// The key of the table hash, from the randomness capability.
    pub hash_key: [u8; 16],
    /// The local Homa port, 0 on a carrier without ports.
    pub port: u16,
    /// The count of local Homa ports after `port` the endpoint owns as well,
    /// 0 by default.
    ///
    /// The helper raises it to give each local connection a client port of
    /// its own, and the kernel module layer to own every port of its network
    /// namespace.
    pub extra_ports: u16,
}

impl Config {
    /// Returns the length of every part the configuration needs.
    ///
    /// A table takes the smallest power of two at least twice its entries,
    /// each heap one item a slot, the pages `pages` times `page_len` bytes and
    /// two bytes a page of page table behind them, and the gaps one word a
    /// slot for the end of the furthest byte received and two a gap for its
    /// missing ranges. A length that does not fit a `usize` reads
    /// `usize::MAX`, which no storage meets.
    #[must_use]
    pub const fn storage(&self) -> StorageLen {
        let slots = widen(self.client_slots.saturating_add(self.server_slots));
        let pages = widen(self.pages);
        let per_gap_slot = 1 + 2 * (self.max_gaps as usize);
        StorageLen {
            peers: widen(self.peers),
            slots,
            peer_index: buckets(self.peers),
            slot_index: buckets(self.client_slots.saturating_add(self.server_slots)),
            tx_heap: slots,
            grant_heap: slots,
            pages: pages
                .saturating_mul(widen(self.page_len))
                .saturating_add(pages.saturating_mul(LINK_LEN)),
            gaps: slots.saturating_mul(per_gap_slot),
            sizes: self.size_buckets as usize,
            peer_prio: if cfg!(feature = "prio") {
                widen(self.peers)
            } else {
                0
            },
        }
    }

    /// Returns the count of all slots, client and server.
    pub(crate) const fn slots(&self) -> u32 {
        self.client_slots.saturating_add(self.server_slots)
    }
}

/// Returns a count as a `usize`, `usize::MAX` where it does not fit.
const fn widen(value: u32) -> usize {
    // A u32 fits the usize of every target the crate builds for, the riscv32,
    // wasm32 and 64 bit hosts, whose pointers are 32 bits or more.
    value as usize
}

/// Returns the buckets of a table of `entries`, `usize::MAX` where the
/// capacity does not fit.
const fn buckets(entries: u32) -> usize {
    match table::capacity(entries) {
        Some(capacity) => widen(capacity),
        None => usize::MAX,
    }
}

/// The length of every part of an endpoint's storage.
///
/// Each field counts the elements of its part of [`Parts`], and `pages`
/// counts bytes, since that part is a byte slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageLen {
    /// Places of peers.
    pub peers: usize,
    /// RPC slots, client and server.
    pub slots: usize,
    /// Buckets of the peer table.
    pub peer_index: usize,
    /// Buckets of the slot table.
    pub slot_index: usize,
    /// Items of the transmit heap.
    pub tx_heap: usize,
    /// Items of the grant heap.
    pub grant_heap: usize,
    /// Bytes of the page pool with its page table.
    pub pages: usize,
    /// Words of the gaps of arriving messages.
    pub gaps: usize,
    /// Buckets of the histogram of incoming sizes.
    pub sizes: usize,
    /// The priority state of the peers, one a peer in a build with the
    /// feature `prio` and none without it.
    pub peer_prio: usize,
}

impl StorageLen {
    /// Returns the bytes the storage takes for peers of type `P`.
    #[must_use]
    pub const fn bytes<P>(&self) -> usize {
        let parts = [
            self.peers.saturating_mul(core::mem::size_of::<Peer<P>>()),
            self.slots.saturating_mul(core::mem::size_of::<Slot>()),
            self.peer_index
                .saturating_mul(core::mem::size_of::<Bucket>()),
            self.slot_index
                .saturating_mul(core::mem::size_of::<Bucket>()),
            self.tx_heap.saturating_mul(core::mem::size_of::<Item>()),
            self.grant_heap.saturating_mul(core::mem::size_of::<Item>()),
            self.pages,
            self.gaps.saturating_mul(4),
            self.sizes.saturating_mul(8),
            self.peer_prio
                .saturating_mul(core::mem::size_of::<PeerPrio>()),
        ];
        let mut total = 0usize;
        let mut i = 0;
        while i < parts.len() {
            total = total.saturating_add(parts[i]);
            i += 1;
        }
        total
    }
}

/// The state of one peer, in storage the caller owns.
///
/// It holds the carrier's handle, the limits of both ends, the times of its
/// timers, the replies it owes the peer, the acknowledgements a client owes it
/// and the counters of the link, at most 96 bytes for the handle of a point to
/// point carrier.
#[derive(Clone, Copy, Debug)]
pub struct Peer<P> {
    /// The carrier's handle, `None` in a vacant place.
    pub(crate) handle: Option<P>,
    /// What this end accepts from the peer.
    pub(crate) local: Limits,
    /// What the peer accepts, assumed until its LIMITS arrives.
    pub(crate) remote: Limits,
    /// When the peer was last heard, or when its link came up.
    pub(crate) stamp: Nanos,
    /// When this end last sent the peer its LIMITS.
    pub(crate) limits_at: Nanos,
    /// When this end last sent the peer a RESEND or a `NEED_ACK`.
    pub(crate) resend_at: Nanos,
    /// The RPC an `RPC_UNKNOWN` owed to the peer names, as this end names it.
    pub(crate) unknown: u64,
    /// The RPC an ACK owed to the peer names, which this end holds no more.
    pub(crate) unacked: u64,
    /// Bytes unscheduled or granted on the link and not yet received.
    pub(crate) expected: u32,
    /// The first client slot whose acknowledgement waits to be sent.
    pub(crate) acks: u32,
    /// The next node of the control queue, or of the free list.
    pub(crate) ctl_next: u32,
    /// The next node of the event queue.
    pub(crate) ev_next: u32,
    /// Client RPCs of this end with the peer, counted until their ACK leaves.
    pub(crate) client_rpcs: u16,
    /// Server RPCs of the peer at this end, counted until their ACK arrives.
    pub(crate) server_rpcs: u16,
    /// DATA packets sent to the peer, whose count chooses the FIFO turn.
    pub(crate) sent: u8,
    /// GRANTs given on the link, whose count chooses the FIFO turn.
    pub(crate) grants: u8,
    /// RESEND and `NEED_ACK` packets the peer left unanswered.
    pub(crate) tries: u8,
    /// The state bits of the peer.
    pub(crate) flags: u8,
    /// The control packets owed to the peer.
    pub(crate) ctl: u8,
    /// Why the peer is gone, for the event that reports it.
    pub(crate) gone: u8,
}

/// The state of the priority machinery with one peer, in a part of its own
/// beside the peers, so a peer of a build that sends at one level keeps the
/// bytes of section 8.3 of the Homa design note.
///
/// It holds the peer's cutoffs as this end sends by them, when this end last
/// sent the peer its own, and whether a change of the peer's cutoffs waits to
/// be reported. Before the first CUTOFFS of the peer arrives, a sender takes
/// `HomaModule`'s default cutoffs under version 0, so the receiver answers its
/// first DATA packet with its own.
#[cfg(feature = "prio")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerPrio {
    /// The peer's cutoffs as this end sends by them.
    pub(crate) cutoffs: crate::prio::Cutoffs,
    /// When this end last sent the peer its own cutoffs.
    pub(crate) sent_at: Nanos,
    /// Whether this end sent the peer its own cutoffs at all.
    pub(crate) sent: bool,
    /// Whether a change of the peer's cutoffs waits to be reported.
    pub(crate) event: bool,
}

#[cfg(feature = "prio")]
impl PeerPrio {
    /// The state of a peer that nothing has touched.
    pub const VACANT: Self = Self {
        cutoffs: crate::prio::Cutoffs {
            version: 0,
            ..crate::prio::Cutoffs::module_default()
        },
        sent_at: 0,
        sent: false,
        event: false,
    };
}

/// No state of the priority machinery in a build without the feature
/// `prio`, whose storage holds none of it.
#[cfg(not(feature = "prio"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerPrio;

#[cfg(not(feature = "prio"))]
impl PeerPrio {
    /// The state of a peer that nothing has touched.
    pub const VACANT: Self = Self;
}

/// Limits that name nothing, the values of a vacant place.
const NO_LIMITS: Limits = Limits {
    max_packet: 0,
    max_message: 0,
    unsched_bytes: 0,
    max_rpcs: 0,
};

impl<P> Peer<P> {
    /// An empty place for a peer.
    pub const VACANT: Self = Self {
        handle: None,
        local: NO_LIMITS,
        remote: NO_LIMITS,
        stamp: 0,
        limits_at: 0,
        resend_at: 0,
        unknown: 0,
        unacked: 0,
        expected: 0,
        acks: NONE,
        ctl_next: NONE,
        ev_next: NONE,
        client_rpcs: 0,
        server_rpcs: 0,
        sent: 0,
        grants: 0,
        tries: 0,
        flags: 0,
        ctl: 0,
        gone: 0,
    };
}

/// The state of one RPC without its pages.
///
/// It holds the identifier, the peer, where each message stands, the first
/// page of each, the links of the queues and heaps it is in and its timer, at
/// most 72 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The RPC as this end names it.
    pub(crate) id: u64,
    /// When the RPC last heard from the peer, sent its message whole or
    /// asked for a packet, which its timers count from.
    pub(crate) stamp: Nanos,
    /// The peer of the RPC.
    pub(crate) peer: u32,
    /// Bytes of the outgoing message.
    pub(crate) out_len: u32,
    /// The first byte of the outgoing message to send next.
    pub(crate) out_next: u32,
    /// The end of the furthest byte of the outgoing message sent.
    pub(crate) out_max: u32,
    /// The first byte of the outgoing message the receiver has not granted.
    pub(crate) out_granted: u32,
    /// Bytes of the incoming message, once its first packet arrived.
    pub(crate) in_len: u32,
    /// The end of the contiguous prefix of the incoming message received.
    pub(crate) in_recv: u32,
    /// The first byte of the incoming message this end has not granted.
    pub(crate) in_granted: u32,
    /// The position of the slot in the transmit or the grant heap.
    pub(crate) pos: u32,
    /// The next node of the free list, the list of acknowledgements or the
    /// event queue.
    pub(crate) next: u32,
    /// The next node of the control queue.
    pub(crate) ctl_next: u32,
    /// The first page of the incoming message.
    pub(crate) in_page: u16,
    /// The first page of the outgoing message.
    pub(crate) out_page: u16,
    /// The local Homa port of the RPC.
    pub(crate) port: u16,
    /// Where the RPC stands.
    pub(crate) state: u8,
    /// The option and membership bits of the RPC.
    pub(crate) flags: u8,
    /// The control packets owed for the RPC.
    pub(crate) ctl: u8,
    /// The event the RPC waits to report.
    pub(crate) event: u8,
    /// The levels of the RPC: in bits 0 to 2 the level of the grants to its
    /// incoming message, and in bits 3 to 5 the level the receiver granted
    /// its outgoing message at.
    pub(crate) levels: u8,
    /// The marks of the RPC, [`crate::endpoint`]'s `INCAST` and `SCHEDULED`.
    pub(crate) marks: u8,
}

impl Slot {
    /// An empty RPC slot.
    pub const VACANT: Self = Self {
        id: 0,
        stamp: 0,
        peer: NONE,
        out_len: 0,
        out_next: 0,
        out_max: 0,
        out_granted: 0,
        in_len: 0,
        in_recv: 0,
        in_granted: 0,
        pos: NONE,
        next: NONE,
        ctl_next: NONE,
        in_page: NO_PAGE,
        out_page: NO_PAGE,
        port: 0,
        state: 0,
        flags: 0,
        ctl: 0,
        event: 0,
        levels: 0,
        marks: 0,
    };
}

/// The storage of an endpoint, borrowed from the caller in typed parts.
#[derive(Debug)]
pub struct Parts<'a, P> {
    /// The places of peers.
    pub peers: &'a mut [Peer<P>],
    /// The RPC slots, the client slots first.
    pub slots: &'a mut [Slot],
    /// The buckets of the peer table.
    pub peer_index: &'a mut [Bucket],
    /// The buckets of the slot table.
    pub slot_index: &'a mut [Bucket],
    /// The items of the transmit heap.
    pub tx_heap: &'a mut [Item],
    /// The items of the grant heap.
    pub grant_heap: &'a mut [Item],
    /// The page pool with its page table.
    pub pages: &'a mut [u8],
    /// The gaps of arriving messages.
    pub gaps: &'a mut [u32],
    /// The histogram of incoming sizes.
    pub sizes: &'a mut [u64],
    /// The priority state of the peers, at the index of each peer.
    pub peer_prio: &'a mut [PeerPrio],
}

/// The storage of an endpoint as a reader sees it.
///
/// The methods of an endpoint that take it by shared reference read their
/// storage through a view, since [`Store::parts`] needs it by exclusive
/// reference.
#[derive(Clone, Copy, Debug)]
pub struct View<'a, P> {
    /// The places of peers.
    pub peers: &'a [Peer<P>],
    /// The RPC slots, the client slots first.
    pub slots: &'a [Slot],
    /// The buckets of the peer table.
    pub peer_index: &'a [Bucket],
    /// The buckets of the slot table.
    pub slot_index: &'a [Bucket],
    /// The items of the transmit heap.
    pub tx_heap: &'a [Item],
    /// The items of the grant heap.
    pub grant_heap: &'a [Item],
    /// The page pool with its page table.
    pub pages: &'a [u8],
    /// The gaps of arriving messages.
    pub gaps: &'a [u32],
    /// The histogram of incoming sizes.
    pub sizes: &'a [u64],
    /// The priority state of the peers, at the index of each peer.
    pub peer_prio: &'a [PeerPrio],
}

/// Storage an endpoint runs over.
pub trait Store<P> {
    /// Returns the parts of the storage.
    fn parts(&mut self) -> Parts<'_, P>;

    /// Returns the parts of the storage to read.
    fn view(&self) -> View<'_, P>;
}

impl<P> Store<P> for Parts<'_, P> {
    fn parts(&mut self) -> Parts<'_, P> {
        Parts {
            peers: self.peers,
            slots: self.slots,
            peer_index: self.peer_index,
            slot_index: self.slot_index,
            tx_heap: self.tx_heap,
            grant_heap: self.grant_heap,
            pages: self.pages,
            gaps: self.gaps,
            sizes: self.sizes,
            peer_prio: self.peer_prio,
        }
    }

    fn view(&self) -> View<'_, P> {
        View {
            peers: self.peers,
            slots: self.slots,
            peer_index: self.peer_index,
            slot_index: self.slot_index,
            tx_heap: self.tx_heap,
            grant_heap: self.grant_heap,
            pages: self.pages,
            gaps: self.gaps,
            sizes: self.sizes,
            peer_prio: self.peer_prio,
        }
    }
}

/// Storage allocated once, at construction.
#[cfg(feature = "alloc")]
#[derive(Debug)]
pub struct Owned<P> {
    /// The places of peers.
    peers: Vec<Peer<P>>,
    /// The RPC slots.
    slots: Vec<Slot>,
    /// The buckets of the peer table.
    peer_index: Vec<Bucket>,
    /// The buckets of the slot table.
    slot_index: Vec<Bucket>,
    /// The items of the transmit heap.
    tx_heap: Vec<Item>,
    /// The items of the grant heap.
    grant_heap: Vec<Item>,
    /// The page pool with its page table.
    pages: Vec<u8>,
    /// The gaps of arriving messages.
    gaps: Vec<u32>,
    /// The histogram of incoming sizes.
    sizes: Vec<u64>,
    /// The priority state of the peers.
    peer_prio: Vec<PeerPrio>,
}

#[cfg(feature = "alloc")]
impl<P> Owned<P> {
    /// Allocates the storage `config` needs, once.
    ///
    /// A part whose allocation fails, or whose length does not fit a
    /// `usize`, stays empty, so `Endpoint::new` refuses the storage with
    /// [`ConfigError::TooSmall`](crate::ConfigError::TooSmall) and nothing
    /// panics.
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let len = config.storage();
        Self {
            peers: filled(len.peers, || Peer::VACANT),
            slots: filled(len.slots, || Slot::VACANT),
            peer_index: filled(len.peer_index, || Bucket::VACANT),
            slot_index: filled(len.slot_index, || Bucket::VACANT),
            tx_heap: filled(len.tx_heap, || Item::VACANT),
            grant_heap: filled(len.grant_heap, || Item::VACANT),
            pages: filled(len.pages, || 0),
            gaps: filled(len.gaps, || 0),
            sizes: filled(len.sizes, || 0),
            peer_prio: filled(len.peer_prio, || PeerPrio::VACANT),
        }
    }
}

/// Returns a vector of `len` values of `value`, or an empty one where the
/// allocation fails.
#[cfg(feature = "alloc")]
fn filled<T>(len: usize, value: impl Fn() -> T) -> Vec<T> {
    let mut out = Vec::new();
    if len == usize::MAX || out.try_reserve_exact(len).is_err() {
        return out;
    }
    out.resize_with(len, value);
    out
}

#[cfg(feature = "alloc")]
impl<P> Store<P> for Owned<P> {
    fn parts(&mut self) -> Parts<'_, P> {
        Parts {
            peers: &mut self.peers,
            slots: &mut self.slots,
            peer_index: &mut self.peer_index,
            slot_index: &mut self.slot_index,
            tx_heap: &mut self.tx_heap,
            grant_heap: &mut self.grant_heap,
            pages: &mut self.pages,
            gaps: &mut self.gaps,
            sizes: &mut self.sizes,
            peer_prio: &mut self.peer_prio,
        }
    }

    fn view(&self) -> View<'_, P> {
        View {
            peers: &self.peers,
            slots: &self.slots,
            peer_index: &self.peer_index,
            slot_index: &self.slot_index,
            tx_heap: &self.tx_heap,
            grant_heap: &self.grant_heap,
            pages: &self.pages,
            gaps: &self.gaps,
            sizes: &self.sizes,
            peer_prio: &self.peer_prio,
        }
    }
}

/// The shape of a page pool: the bytes of a page and the count of pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pages {
    /// Bytes of one page.
    pub(crate) len: u32,
    /// Pages in the pool.
    pub(crate) count: u32,
}

impl Pages {
    /// Returns the pages a message of `len` bytes spans.
    pub(crate) const fn needed(self, len: u32) -> u32 {
        if self.len == 0 {
            return u32::MAX;
        }
        len.div_ceil(self.len)
    }

    /// Returns the bytes of page `page` in `pool`.
    pub(crate) fn page(self, pool: &[u8], page: u16) -> Option<&[u8]> {
        let start = usize::from(page).checked_mul(to_usize(self.len))?;
        pool.get(start..start.checked_add(to_usize(self.len))?)
    }

    /// Returns the bytes of page `page` in `pool` to write.
    pub(crate) fn page_mut(self, pool: &mut [u8], page: u16) -> Option<&mut [u8]> {
        let start = usize::from(page).checked_mul(to_usize(self.len))?;
        pool.get_mut(start..start.checked_add(to_usize(self.len))?)
    }

    /// Returns where the page table entry of `page` starts in the pool.
    fn link_at(self, page: u16) -> Option<usize> {
        let table = to_usize(self.count).checked_mul(to_usize(self.len))?;
        table.checked_add(usize::from(page).checked_mul(LINK_LEN)?)
    }

    /// Returns the page after `page` in its chain, or [`NO_PAGE`].
    pub(crate) fn link(self, pool: &[u8], page: u16) -> u16 {
        let Some(at) = self.link_at(page) else {
            return NO_PAGE;
        };
        match pool.get(at..at.saturating_add(LINK_LEN)) {
            Some(&[high, low]) => u16::from_be_bytes([high, low]),
            _ => NO_PAGE,
        }
    }

    /// Sets the page after `page` in its chain.
    pub(crate) fn set_link(self, pool: &mut [u8], page: u16, next: u16) {
        if let Some(at) = self.link_at(page) {
            if let Some(entry) = pool.get_mut(at..at.saturating_add(LINK_LEN)) {
                entry.copy_from_slice(&next.to_be_bytes());
            }
        }
    }

    /// Returns the page of a chain from `first` that holds byte `offset`, and
    /// where in that page the byte stands.
    pub(crate) fn seek(self, pool: &[u8], first: u16, offset: u32) -> Option<(u16, usize)> {
        let index = offset.checked_div(self.len)?;
        if index >= MAX_PAGES {
            return None;
        }
        let mut page = first;
        for _ in 0..index {
            if page == NO_PAGE {
                return None;
            }
            page = self.link(pool, page);
        }
        if page == NO_PAGE {
            return None;
        }
        Some((page, to_usize(offset % self.len)))
    }

    /// Copies the bytes of the chain from `first` at `offset` into `out` and
    /// returns how many it copied, never past `len`.
    pub(crate) fn read(
        self,
        pool: &[u8],
        first: u16,
        len: u32,
        offset: u32,
        out: &mut [u8],
    ) -> usize {
        let left = to_usize(len.saturating_sub(offset));
        let want = out.len().min(left);
        let mut done = 0;
        while done < want {
            let at = offset.saturating_add(u32::try_from(done).unwrap_or(u32::MAX));
            let Some((page, within)) = self.seek(pool, first, at) else {
                break;
            };
            let Some(bytes) = self.page(pool, page).and_then(|p| p.get(within..)) else {
                break;
            };
            let take = bytes.len().min(want - done);
            let (Some(from), Some(to)) = (bytes.get(..take), out.get_mut(done..done + take)) else {
                break;
            };
            to.copy_from_slice(from);
            done += take;
        }
        done
    }

    /// Copies `bytes` into the chain from `first` at `offset`, and returns
    /// whether the chain held every byte.
    pub(crate) fn write(self, pool: &mut [u8], first: u16, offset: u32, bytes: &[u8]) -> bool {
        let mut done = 0;
        while done < bytes.len() {
            let at = offset.saturating_add(u32::try_from(done).unwrap_or(u32::MAX));
            let Some((page, within)) = self.seek(pool, first, at) else {
                return false;
            };
            let Some(target) = self.page_mut(pool, page).and_then(|p| p.get_mut(within..)) else {
                return false;
            };
            let take = target.len().min(bytes.len() - done);
            let (Some(from), Some(to)) = (bytes.get(done..done + take), target.get_mut(..take))
            else {
                return false;
            };
            to.copy_from_slice(from);
            done += take;
        }
        true
    }

    /// Writes zeros into the chain from `first` from `offset` up to `end`.
    #[cfg(any(feature = "client", feature = "server"))]
    pub(crate) fn zero(self, pool: &mut [u8], first: u16, offset: u32, end: u32) {
        let mut at = offset;
        while at < end {
            let Some((page, within)) = self.seek(pool, first, at) else {
                return;
            };
            let Some(target) = self.page_mut(pool, page).and_then(|p| p.get_mut(within..)) else {
                return;
            };
            let take = target.len().min(to_usize(end - at));
            if let Some(span) = target.get_mut(..take) {
                span.fill(0);
            }
            at = at.saturating_add(u32::try_from(take).unwrap_or(u32::MAX));
        }
    }
}

/// Returns a count as a `usize`, `usize::MAX` where it does not fit.
pub(crate) fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// A message read in order, across the pages that hold it.
#[derive(Clone, Copy, Debug)]
pub struct Reader<'a> {
    /// The page pool with its page table.
    pool: &'a [u8],
    /// The shape of the pool.
    pages: Pages,
    /// The first page of the message.
    first: u16,
    /// The bytes of the message.
    len: u32,
}

#[cfg(any(feature = "client", feature = "server"))]
impl<'a> Reader<'a> {
    /// Returns a reader of the `len` bytes of the chain from `first`.
    pub(crate) fn new(pool: &'a [u8], pages: Pages, first: u16, len: u32) -> Self {
        Self {
            pool,
            pages,
            first,
            len,
        }
    }
}

impl Read for Reader<'_> {
    fn len(&self) -> u32 {
        self.len
    }

    fn as_slice(&self) -> Option<&[u8]> {
        if self.len == 0 {
            return Some(&[]);
        }
        if self.len > self.pages.len {
            return None;
        }
        self.pages
            .page(self.pool, self.first)
            .and_then(|page| page.get(..to_usize(self.len)))
    }

    fn read_at(&self, offset: u32, out: &mut [u8]) -> usize {
        self.pages
            .read(self.pool, self.first, self.len, offset, out)
    }
}

/// Writes a message across the pages of its slot in order, for a length the
/// caller announced.
#[derive(Debug)]
pub struct Writer<'a> {
    /// The page pool with its page table.
    pool: &'a mut [u8],
    /// The shape of the pool.
    pages: Pages,
    /// The first page of the message.
    first: u16,
    /// The bytes the caller announced.
    len: u32,
    /// The bytes written so far.
    written: u32,
}

#[cfg(any(feature = "client", feature = "server"))]
impl<'a> Writer<'a> {
    /// Returns a writer of `len` bytes into the chain from `first`.
    pub(crate) fn new(pool: &'a mut [u8], pages: Pages, first: u16, len: u32) -> Self {
        Self {
            pool,
            pages,
            first,
            len,
            written: 0,
        }
    }
}

impl Writer<'_> {
    /// Appends `bytes`.
    ///
    /// # Errors
    ///
    /// Returns [`NoRoom`] when `bytes` run past the length the caller
    /// announced, and writes none of them.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), NoRoom> {
        let count = u32::try_from(bytes.len()).map_err(|_| NoRoom)?;
        let end = self.written.checked_add(count).ok_or(NoRoom)?;
        if end > self.len {
            return Err(NoRoom);
        }
        if !self.pages.write(self.pool, self.first, self.written, bytes) {
            return Err(NoRoom);
        }
        self.written = end;
        Ok(())
    }

    /// Returns how many bytes were written.
    #[must_use]
    pub fn written(&self) -> u32 {
        self.written
    }

    /// Writes zeros from the bytes written up to the length announced, so no
    /// byte of an earlier message is sent.
    #[cfg(any(feature = "client", feature = "server"))]
    pub(crate) fn finish(&mut self) {
        self.pages
            .zero(self.pool, self.first, self.written, self.len);
    }
}

/// What a reservation needs of its endpoint, whatever the endpoint's types.
pub(crate) trait Reserve {
    /// Copies `bytes` into the reserved pages of `slot` at `at`, and returns
    /// whether they held every byte.
    fn write_reserved(&mut self, slot: u32, at: u32, bytes: &[u8]) -> bool;

    /// Commits the reply of `slot` with `len` bytes and queues it.
    fn commit_reserved(&mut self, slot: u32, len: u32) -> Result<(), CommitError>;

    /// Returns the reserved pages of `slot` and leaves its RPC open.
    fn cancel_reserved(&mut self, slot: u32);
}

/// A reply reserved in the pages of its slot and committed with the length
/// written.
///
/// It borrows its endpoint until it commits or drops, so nothing else runs on
/// the endpoint meanwhile.
pub struct ReplyWriter<'a> {
    /// The endpoint that holds the reservation.
    target: &'a mut dyn Reserve,
    /// The slot of the RPC.
    slot: u32,
    /// The bytes reserved.
    cap: u32,
    /// The bytes written so far.
    written: u32,
    /// Whether the reservation still holds its pages.
    open: bool,
}

#[cfg(feature = "server")]
impl<'a> ReplyWriter<'a> {
    /// Returns the writer of the reservation of `cap` bytes of `slot`.
    pub(crate) fn new(target: &'a mut dyn Reserve, slot: u32, cap: u32) -> Self {
        Self {
            target,
            slot,
            cap,
            written: 0,
            open: true,
        }
    }
}

impl ReplyWriter<'_> {
    /// Appends `bytes`.
    ///
    /// # Errors
    ///
    /// Returns [`NoRoom`] when `bytes` run past the length reserved, and
    /// writes none of them.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), NoRoom> {
        let count = u32::try_from(bytes.len()).map_err(|_| NoRoom)?;
        let end = self.written.checked_add(count).ok_or(NoRoom)?;
        if end > self.cap {
            return Err(NoRoom);
        }
        if !self.target.write_reserved(self.slot, self.written, bytes) {
            return Err(NoRoom);
        }
        self.written = end;
        Ok(())
    }

    /// Returns how many bytes were written.
    #[must_use]
    pub fn written(&self) -> u32 {
        self.written
    }

    /// Commits the reply with the bytes written as its length and queues it.
    ///
    /// The length leads the first DATA packet of the reply, and no packet of
    /// it left before the commit, which keeps Homa's rule that a message
    /// states its length before its first byte. A reservation dropped without
    /// a commit returns its storage, and the RPC stays open.
    ///
    /// # Errors
    ///
    /// Returns [`CommitError::Gone`] when the RPC ended before the commit.
    pub fn commit(mut self) -> Result<(), CommitError> {
        self.open = false;
        self.target.commit_reserved(self.slot, self.written)
    }
}

impl fmt::Debug for ReplyWriter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReplyWriter")
            .field("slot", &self.slot)
            .field("cap", &self.cap)
            .field("written", &self.written)
            .field("open", &self.open)
            .finish_non_exhaustive()
    }
}

impl Drop for ReplyWriter<'_> {
    fn drop(&mut self) {
        if self.open {
            self.open = false;
            self.target.cancel_reserved(self.slot);
        }
    }
}

/// Why a reservation did not commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitError {
    /// The RPC ended before the commit, the peer gone or the link down.
    Gone,
}

impl fmt::Display for CommitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the RPC ended before the reply was committed")
    }
}

impl core::error::Error for CommitError {}
