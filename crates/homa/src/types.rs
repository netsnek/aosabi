// SPDX-License-Identifier: MIT
/*
 * homa types: the items every engine module shares, re-exported at the crate
 * root.
 *
 * The shapes are those of the Homa design note, first version, homa/design-v1.md
 * section 2.5, lines 566 to 1088, with the additions of its second version,
 * homa/design.md sections 7.2 and 7.4, lines 2113 to 2326 and 2438 to 2539. Three
 * items take their shape here: the report of RPC_UNKNOWN to a request that asks
 * for it, after the outcome the second version calls unknown in section 4.7,
 * lines 1314 to 1406, the Homa port of a peer, after its sections 4.3 and 7.3,
 * lines 1036 to 1206 and 2327 to 2437, and the state of an RPC that `rpc_info`
 * writes, after its section 7.4.
 */

//! The types, the traits and the errors every engine module shares.
//!
//! Configuration types exist in every build, since they are data, and the
//! code that acts on them sits behind the features. A profile that asks for a
//! mechanism the build left out fails `Endpoint::new` with
//! [`ConfigError::Missing`], so a mismatch between a card and its features is
//! an error at start and never a silent fallback.
//!
//! Every duration is a count of [`Nanos`] and every share a count per mille,
//! so no item of the engine needs floating point.

use core::{
    convert::Infallible,
    fmt,
    hash::Hash,
    net::SocketAddr, //
};

use crate::wire::DecodeError;

/// Nanoseconds on a monotonic clock the caller owns.
pub type Nanos = u64;

/// One millisecond.
const MS: Nanos = 1_000_000;

/// One second.
const SECOND: Nanos = 1_000 * MS;

/// An RPC as the local endpoint names it, even on the client and odd on the
/// server.
///
/// Both ends name one RPC by the client's identifier, and the server sets the low
/// bit, as `HomaModule` does. The identifier is 64 bits in the engine and in the
/// full form, and the compact form carries its low 32 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RpcId(u64);

impl RpcId {
    /// Returns the RPC that `id` names, as an event, a trace or a socket gave
    /// it.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns whether the local endpoint serves the RPC.
    #[must_use]
    pub const fn is_server(self) -> bool {
        self.0 & 1 == 1
    }

    /// Returns the identifier the peer uses for the same RPC.
    #[must_use]
    pub const fn remote(self) -> u64 {
        self.0 ^ 1
    }

    /// Returns the identifier as a number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A switch priority level, 0 the lowest and 7 the highest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Priority(u8);

impl Priority {
    /// The level every control packet and every packet without priorities
    /// uses.
    pub const HIGHEST: Self = Self(7);

    /// Returns the level, or `None` above 7.
    #[must_use]
    pub const fn new(level: u8) -> Option<Self> {
        if level <= Self::HIGHEST.0 {
            Some(Self(level))
        } else {
            None
        }
    }

    /// Returns the level as a number from 0 to 7.
    #[must_use]
    pub const fn level(self) -> u8 {
        self.0
    }

    /// Returns the TOS or traffic class byte, the level shifted left by five
    /// bits.
    #[must_use]
    pub const fn tos(self) -> u8 {
        self.0 << 5
    }
}

/// How a message longer than the unscheduled bytes starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheduling {
    /// The first unscheduled bytes leave at once and GRANTs release the rest.
    Prefix,
    /// A longer message announces itself with `START_MSG` and waits for GRANTs.
    AllOrNothing,
}

/// Which peers share the receiver's downlink and the sender's uplink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every peer is its own link, as on a point to point carrier.
    PerPeer,
    /// All peers share one link to a switch, as on Network.
    Shared,
}

/// Which peers the endpoint accepts packets from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// Only peers the carrier reported with `link_up`.
    Linked,
    /// Any peer, which gets a place on its first packet by evicting an idle
    /// one.
    Open,
}

/// What one end accepts from its peer, the body of a LIMITS packet on the
/// compact form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Bytes of the largest packet this end accepts, header included.
    pub max_packet: u16,
    /// Bytes of the largest message this end accepts.
    pub max_message: u32,
    /// Bytes of a message the peer may send before the first GRANT.
    pub unsched_bytes: u32,
    /// RPCs the peer may have open at this end at once.
    pub max_rpcs: u8,
}

/// How much a receiver lets one message have granted and not received.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// A fixed number of bytes.
    Fixed(u32),
    /// `max_incoming` divided by one more than the grantable messages.
    Dynamic,
}

/// The durations a link runs with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timers {
    /// Silence after which a receiver that expects data sends its first
    /// RESEND.
    pub resend_after: Nanos,
    /// The time between two RESENDs of one silence after the first.
    pub resend_interval: Nanos,
    /// The same as `resend_after` for the response of a patient RPC.
    pub patient_after: Nanos,
    /// Silence after which a server that sent a whole response asks for its
    /// acknowledgement with `NEED_ACK`.
    pub need_ack_after: Nanos,
    /// Silence of a peer after which it is dead, once `timeout_resends` went
    /// unanswered as well.
    pub rpc_timeout: Nanos,
    /// Unanswered RESEND and `NEED_ACK` packets after which the peer is dead.
    pub timeout_resends: u8,
}

/// Incast control, used when the build has the feature `incast`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Incast {
    /// Outstanding client RPCs of the endpoint above which a new request is
    /// marked.
    pub threshold: u16,
    /// Bytes of the response to a marked request a server sends unscheduled.
    pub unsched_bytes: u32,
}

/// The values of a carrier's links.
///
/// A card writes its own values into the fields where it knows better, with their
/// provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    /// How a message longer than `unsched_bytes` starts.
    pub scheduling: Scheduling,
    /// Whether the peers share one downlink and one uplink.
    pub scope: Scope,
    /// Which peers the endpoint accepts packets from.
    pub admission: Admission,
    /// Bytes of the largest packet the carrier moves, header included.
    pub max_packet: u16,
    /// Bytes of a message that leave before the first GRANT.
    pub unsched_bytes: u32,
    /// Bytes one message may have granted and not received.
    pub window: Window,
    /// Bytes the receiver expects on its downlink at most, unscheduled and
    /// granted.
    pub max_incoming: u32,
    /// Messages the receiver grants to at once.
    pub max_overcommit: u8,
    /// The share of the link per mille that the oldest message gets through
    /// FIFO grants, 0 without them.
    pub fifo_permille: u16,
    /// Bytes one FIFO grant gives the oldest message.
    pub fifo_grant_increment: u32,
    /// One DATA packet and one grant in this many go to the oldest message on
    /// a point to point link, 0 for none and 1 for FIFO order alone.
    pub fifo_every: u8,
    /// Whether a receiver grants at half a window rather than as data arrives.
    pub batch_grants: bool,
    /// Whether a RESEND raises the granted offset of the range it names.
    pub resend_raises_grant: bool,
    /// The durations of the link.
    pub timers: Timers,
    /// Incast control, `None` where it does not act.
    pub incast: Option<Incast>,
}

/// The planning rate of the L2CAP LE credit based channel in bytes a second.
const CHANNEL_BYTES_PER_S: u64 = 36_000;

/// The supervision timeout the Bluetooth rule assumes of a link whose parameters
/// a side knows.
const BLUETOOTH_SUPERVISION: Nanos = 6 * SECOND;

/// The bytes the `LoRa` frame adds to one compact packet.
const LORA_FRAME: u32 = 21;

/// The bytes of the header of a compact DATA packet that carries a part.
const COMPACT_PART_HEADER: u32 = 14;

/// The bytes of a RESEND packet on the compact form.
const COMPACT_RESEND: u32 = 14;

/// The longest response a `LoRa` link carries in any band.
const LORA_LONGEST_RESPONSE: u32 = 2_048;

impl Profile {
    /// Returns the values of a Network path of `mtu` bytes, `HomaModule`'s
    /// defaults at the pinned commit where the module has the value.
    ///
    /// The module counts its timers in ticks of 1 ms. Its defaults are 40,000
    /// unscheduled bytes, 400,000 bytes of `max_incoming`, a dynamic window, an
    /// overcommitment of 8 and FIFO grants of 50 per mille in steps of 50,000
    /// bytes, and a RESEND after 5 ticks and every 5, a silence of 100 ticks, 5
    /// RESENDs before a peer is dead and a `NEED_ACK` after 2 ticks, so 5 ms, 100
    /// ms and 2 ms here. The packet is the path's MTU less the IP header of
    /// `encap`, so 1,480 bytes on IPv4 at an MTU of 1,500, of which 1,424 carry
    /// payload behind the 56 bytes of the DATA header. A RESEND raises the
    /// granted offset to the end of the range it names, since the module takes
    /// such a RESEND as the GRANT that was lost. Each file is read at <https://github.com/PlatformLab/HomaModule/tree/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3>.
    ///
    /// Two values are the design note's, since the module has none. A patient
    /// request waits 5 s before its first RESEND, because the module has no
    /// patient RPCs. Incast control marks a request above 8 outstanding RPCs and
    /// lets 300 bytes of its response go unscheduled.
    #[must_use]
    pub const fn network(mtu: u16, encap: Encap) -> Self {
        Self {
            scheduling: Scheduling::AllOrNothing,
            scope: Scope::Shared,
            admission: Admission::Open,
            max_packet: mtu.saturating_sub(encap.header_len()),
            unsched_bytes: 40_000,
            window: Window::Dynamic,
            max_incoming: 400_000,
            max_overcommit: 8,
            fifo_permille: 50,
            fifo_grant_increment: 50_000,
            fifo_every: 0,
            batch_grants: false,
            resend_raises_grant: true,
            timers: Timers {
                resend_after: 5 * MS,
                resend_interval: 5 * MS,
                patient_after: 5 * SECOND,
                need_ack_after: 2 * MS,
                rpc_timeout: 100 * MS,
                timeout_resends: 5,
            },
            incast: Some(Incast {
                threshold: 8,
                unsched_bytes: 300,
            }),
        }
    }

    /// Returns the values of a point to point link.
    ///
    /// The link has its own downlink with one sender, admits the peers the
    /// carrier reported, grants to one message at a time with a `window` of fixed
    /// bytes, batches its grants at half a window, raises the granted offset on a
    /// RESEND and gives one DATA packet and one grant in 16 to the oldest
    /// message. The profile leaves `max_incoming` unbounded, since its bound is
    /// the sum of the slots a card gives the link, which the card writes into the
    /// field.
    #[must_use]
    pub const fn link(max_packet: u16, unsched_bytes: u32, window: u32, timers: Timers) -> Self {
        Self {
            scheduling: Scheduling::Prefix,
            scope: Scope::PerPeer,
            admission: Admission::Linked,
            max_packet,
            unsched_bytes,
            window: Window::Fixed(window),
            max_incoming: u32::MAX,
            max_overcommit: 1,
            fifo_permille: 0,
            fifo_grant_increment: 0,
            fifo_every: 16,
            batch_grants: true,
            resend_raises_grant: true,
            timers,
            incast: None,
        }
    }

    /// Returns the values of a Unix Socket, a `SOCK_SEQPACKET` socket or a
    /// stream socket with prefixed `AoFrame`, whose receiver accepts messages
    /// of `max_message` bytes.
    ///
    /// Packets of 16,384 bytes, the whole message unscheduled, so grants stay
    /// inert, and a RESEND after 1,000 ms, which on a link that loses nothing is
    /// a probe of liveness alone, with 5 before the peer is dead.
    #[must_use]
    pub const fn unix_socket(max_message: u32) -> Self {
        Self::link(16_384, max_message, max_message, link_timers(1_000 * MS, 5))
    }

    /// Returns the values of USB bulk, at high speed or at full speed.
    ///
    /// Packets of 16,384 bytes, 32,768 unscheduled and a window of 65,536 at high
    /// speed, and 4,096, 4,096 and 8,192 at full speed, a RESEND after 50 ms and
    /// 20 before the peer is dead.
    #[must_use]
    pub const fn usb_bulk(high_speed: bool) -> Self {
        let timers = link_timers(50 * MS, 20);
        if high_speed {
            Self::link(16_384, 32_768, 65_536, timers)
        } else {
            Self::link(4_096, 4_096, 8_192, timers)
        }
    }

    /// Returns the values of USB serial, a CDC-ACM tty with delimited
    /// `AoFrame`.
    ///
    /// Packets of 512 bytes, 2,048 unscheduled, a window of 4,096, a RESEND after
    /// 100 ms, since the host tty adds latency, and 10 before the peer is dead.
    #[must_use]
    pub const fn usb_serial() -> Self {
        Self::link(512, 2_048, 4_096, link_timers(100 * MS, 10))
    }

    /// Returns the values of the L2CAP LE credit based channel to a peer whose
    /// MPS is `peer_mps`, at a connection `interval`.
    ///
    /// A packet fills whole K-frames: the largest whole k for which k times the
    /// MPS less 6 stays at or under 512 gives a packet of k times the MPS less 6,
    /// which is 488 at an MPS of 247 and 454 at 230, and a peer whose MPS is 518
    /// or more takes a packet of 512 in one K-frame. The unscheduled bytes are
    /// the bandwidth delay product of two connection intervals at the planning
    /// rate of 36 kB/s in whole packets, the window twice that, and the RESEND
    /// four times the round trip and the time of one window, rounded up to 10 ms,
    /// so a 30 ms interval and an MPS of 247 give 5 packets, 10 and 790 ms. The
    /// RESENDs before the peer is dead follow the Bluetooth rule against a
    /// supervision timeout of 6 s. An `interval` of 0 is a side that does not
    /// know the connection parameters, which sizes its unscheduled bytes and its
    /// window at a 30 ms interval and takes 2,000 ms and 18 RESENDs, while its
    /// packet follows the MPS alone.
    #[must_use]
    pub const fn bluetooth_channel(peer_mps: u16, interval: Nanos) -> Self {
        let max_packet = channel_packet(peer_mps);
        let packet = max_packet as u64;
        let sized = if interval == 0 { 30 * MS } else { interval };
        let rtt = sized.saturating_mul(2);
        let bdp = CHANNEL_BYTES_PER_S.saturating_mul(rtt) / SECOND;
        let unsched_packets = max_u64(bdp.div_ceil(packet), 1);
        let window = unsched_packets.saturating_mul(2).saturating_mul(packet);
        let window_time = window.saturating_mul(SECOND) / CHANNEL_BYTES_PER_S;
        let timers = if interval == 0 {
            link_timers(2_000 * MS, 18)
        } else {
            bluetooth_timers(rtt.saturating_add(window_time).saturating_mul(4))
        };
        Self::link(
            max_packet,
            saturate_u32(unsched_packets.saturating_mul(packet)),
            saturate_u32(window),
            timers,
        )
    }

    /// Returns the values of the GATT data path at an `ATT_MTU` of `att_mtu`, at
    /// a connection `interval`.
    ///
    /// One Homa packet per ATT value of `ATT_MTU` less 3 bytes, never below the
    /// 20 bytes of the default `ATT_MTU`, 4 packets unscheduled and a window of
    /// 8, since two packets per connection event and a round trip of two events
    /// make the bandwidth delay product four packets at any `ATT_MTU`, and a
    /// RESEND four times the round trip and the four events of a window, 720 ms
    /// at a 30 ms interval. The RESENDs before the peer is dead follow the
    /// Bluetooth rule against a supervision timeout of 6 s, which gives 10 at 30
    /// ms. An `interval` of 0 is a side that does not know the connection
    /// parameters, which takes 2,000 ms and 18 RESENDs.
    #[must_use]
    pub const fn bluetooth_gatt(att_mtu: u16, interval: Nanos) -> Self {
        let max_packet = max_u16(att_mtu.saturating_sub(3), 20);
        let packet = max_packet as u32;
        let timers = if interval == 0 {
            link_timers(2_000 * MS, 18)
        } else {
            bluetooth_timers(interval.saturating_mul(24))
        };
        Self::link(
            max_packet,
            packet.saturating_mul(4),
            packet.saturating_mul(8),
            timers,
        )
    }

    /// Returns the values of a serial line with delimited `AoFrame` at `baud`.
    ///
    /// Packets of 256 bytes, 512 unscheduled and a window of 1,024, a RESEND
    /// after 550 ms at 115,200 baud that scales inversely with the rate and never
    /// goes below 50 ms, and 5 RESENDs before the peer is dead.
    #[must_use]
    pub const fn serial_line(baud: u32) -> Self {
        let rate = max_u64(baud as u64, 1);
        let resend = max_u64((550 * MS).saturating_mul(115_200) / rate, 50 * MS);
        Self::link(256, 512, 1_024, link_timers(resend, 5))
    }

    /// Returns the values of a `LoRa` link at the spreading factor `sf` in a band
    /// whose duty cycle is `band_permille` per mille.
    ///
    /// One compact packet per `LoRa` packet sealed in the frame of 21 bytes, so a
    /// packet is 234 bytes in the 10 percent band and in the 1 percent band the
    /// `LoRaWAN` ceiling of the spreading factor less 21, 234 at SF7 and SF8, 107
    /// at SF9 and 43 above. The whole message is unscheduled, so no grant
    /// travels, and the longest message is at most 2,048 bytes and takes at most
    /// a quarter of the hour's air the duty cycle allows. The RESEND waits twice
    /// a burst of at most 2 s of air, at least one packet, and two RESENDs,
    /// rounded to 100 ms, 5 RESENDs go before the peer is dead, and a patient RPC
    /// waits 600 s. A spreading factor outside 7 to 12 takes the nearest, and a
    /// band of 100 per mille or more is the 10 percent band.
    #[must_use]
    pub const fn lora(sf: u8, band_permille: u16) -> Self {
        let sf = if sf < 7 {
            7
        } else if sf > 12 {
            12
        } else {
            sf
        };
        let max_packet: u16 = if band_permille >= 100 || sf <= 8 {
            234
        } else if sf == 9 {
            107
        } else {
            43
        };
        let full_air = lora_air(sf, max_packet as u32 + LORA_FRAME);
        let longest = lora_longest(sf, max_packet as u32, full_air, band_permille);
        let burst = max_u64((2 * SECOND) / max_u64(full_air, 1), 1);
        let resend_air = lora_air(sf, COMPACT_RESEND + LORA_FRAME);
        let wait = burst
            .saturating_mul(full_air)
            .saturating_add(resend_air.saturating_mul(2))
            .saturating_mul(2);
        let resend = wait.saturating_add(50 * MS) / (100 * MS) * (100 * MS);
        let mut profile = Self::link(max_packet, longest, longest, link_timers(resend, 5));
        profile.timers.patient_after = 600 * SECOND;
        profile.batch_grants = false;
        profile
    }
}

/// Returns the timers of a point to point link from its RESEND and its count.
///
/// The `NEED_ACK` waits twice the RESEND, a patient RPC waits the larger of 5,000
/// ms and the RESEND, the RESENDs of one silence repeat at the RESEND, and the
/// silence that ends the peer is the RESEND times its count.
const fn link_timers(resend_after: Nanos, timeout_resends: u8) -> Timers {
    Timers {
        resend_after,
        resend_interval: resend_after,
        patient_after: max_u64(5 * SECOND, resend_after),
        need_ack_after: resend_after.saturating_mul(2),
        rpc_timeout: resend_after.saturating_mul(timeout_resends as u64),
        timeout_resends,
    }
}

/// Returns the timers of a Bluetooth link whose RESEND is at least `resend`.
///
/// The RESEND is rounded up to 10 ms with a floor of 50 ms, and the count is the
/// smallest one whose RESENDs outlast the supervision timeout plus one RESEND, at
/// least 5, so the carrier reports a dropped link before the protocol declares a
/// live one dead.
const fn bluetooth_timers(resend: Nanos) -> Timers {
    let step = 10 * MS;
    let resend = max_u64(resend.div_ceil(step).saturating_mul(step), 50 * MS);
    let count = BLUETOOTH_SUPERVISION.saturating_add(resend) / resend + 1;
    let count = if count < 5 { 5 } else { count };
    link_timers(resend, saturate_u8(count))
}

/// Returns the largest packet of the L2CAP LE credit based channel to a peer
/// whose MPS is `mps`.
const fn channel_packet(mps: u16) -> u16 {
    let mps = max_u16(mps, 23);
    let frames = 518 / mps;
    if frames == 0 {
        512
    } else {
        frames * mps - 6
    }
}

/// Returns the time on air of a `LoRa` packet of `bytes` bytes at `sf`.
///
/// The symbols are the preamble of 8, 4.25 and 8 of the header, and the payload's
/// `ceil(max(8 * PL - 4 * SF + 44, 0) / (4 * (SF - 2 * LDRO))) * 5`, with the CRC
/// and the explicit header in the 44 and the low data rate optimisation on at
/// SF11 and SF12, and a symbol at 125 kHz lasts `2^SF * 8` microseconds. The
/// count is kept in quarter symbols, so the arithmetic stays whole.
const fn lora_air(sf: u8, bytes: u32) -> Nanos {
    let sf = sf as u64;
    let ldro = if sf >= 11 { 1 } else { 0 };
    let bits = (bytes as u64 * 8 + 44).saturating_sub(4 * sf);
    let per_block = 4 * (sf - 2 * ldro);
    let payload_symbols = bits.div_ceil(per_block) * 5;
    let quarters = 81 + 4 * payload_symbols;
    quarters * (1 << sf) * 2_000
}

/// Returns the longest message of a `LoRa` link, at most 2,048 bytes and at most
/// a quarter of the hour's air of the band.
///
/// A message goes in full packets of `max_packet` bytes, each `full_air` long
/// on air and carrying `max_packet` less the 14 bytes of a part's header, and
/// one last packet with the rest, so the longest message fills as many full
/// packets as fit and then the largest last packet that fits the air left.
const fn lora_longest(sf: u8, max_packet: u32, full_air: Nanos, band_permille: u16) -> u32 {
    let quarter = (900 * MS).saturating_mul(band_permille as u64);
    let per_packet = max_packet.saturating_sub(COMPACT_PART_HEADER);
    let full = quarter / max_u64(full_air, 1);
    let whole = full.saturating_mul(per_packet as u64);
    if whole >= LORA_LONGEST_RESPONSE as u64 {
        return LORA_LONGEST_RESPONSE;
    }
    let left = quarter - full * full_air;
    let mut rest = per_packet;
    while rest > 0 && lora_air(sf, rest + COMPACT_PART_HEADER + LORA_FRAME) > left {
        rest -= 1;
    }
    let longest = whole + rest as u64;
    if longest > LORA_LONGEST_RESPONSE as u64 {
        LORA_LONGEST_RESPONSE
    } else {
        saturate_u32(longest)
    }
}

/// Returns the larger of two durations.
const fn max_u64(a: u64, b: u64) -> u64 {
    if a > b {
        a
    } else {
        b
    }
}

/// Returns the larger of two sizes.
const fn max_u16(a: u16, b: u16) -> u16 {
    if a > b {
        a
    } else {
        b
    }
}

/// Returns `value`, or `u32::MAX` where it does not fit.
// The value is at most u32::MAX where the cast runs, so it truncates nothing.
#[expect(clippy::cast_possible_truncation)]
const fn saturate_u32(value: u64) -> u32 {
    if value > u32::MAX as u64 {
        u32::MAX
    } else {
        value as u32
    }
}

/// Returns `value`, or `u8::MAX` where it does not fit.
// The value is at most u8::MAX where the cast runs, so it truncates nothing.
#[expect(clippy::cast_possible_truncation)]
const fn saturate_u8(value: u64) -> u8 {
    if value > u8::MAX as u64 {
        u8::MAX
    } else {
        value as u8
    }
}

/// What a Network packet of the crate's engine travels in, which fixes the
/// header bytes below Homa.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encap {
    /// IPv4, whose header without options is 20 bytes.
    Ip4,
    /// IPv6, whose header without extensions is 40 bytes, 20 more than that of
    /// IPv4.
    Ip6,
}

impl Encap {
    /// Returns the bytes of the IP header below every Homa packet.
    const fn header_len(self) -> u16 {
        match self {
            Self::Ip4 => 20,
            Self::Ip6 => 40,
        }
    }
}

/// What the full form needs of a peer: the Homa port of the other end, 0
/// where the carrier has no ports.
///
/// The engine writes it as the destination port of every packet to the
/// peer. The compact form writes and reads no port, so every peer type of a
/// point to point carrier answers 0 through the provided method.
pub trait PeerPort {
    /// Returns the Homa port of the peer.
    fn homa_port(&self) -> u16 {
        0
    }

    /// Returns whether `other` is the host of this peer on another Homa
    /// port, which never holds for a carrier without ports.
    ///
    /// A server takes the acknowledgement of an RPC from any port of the client's
    /// host, as `HomaModule` finds the RPC of an acknowledgement by the address
    /// of its sender and the identifier alone.
    fn same_host(&self, _other: &Self) -> bool {
        false
    }
}

/// The peer of a Unix Socket, USB and a serial line, one per link.
impl PeerPort for () {}

/// The link of `LoRa`.
impl PeerPort for u8 {}

/// A host of the fabric simulator.
impl PeerPort for u16 {}

/// A central of the GATT data path.
impl PeerPort for u32 {}

/// Network on both paths, an address with its Homa port.
impl PeerPort for SocketAddr {
    fn homa_port(&self) -> u16 {
        self.port()
    }

    fn same_host(&self, other: &Self) -> bool {
        self.ip() == other.ip()
    }
}

/// A link that moves whole packets.
pub trait Carrier {
    /// The other end: an address with its Homa port, a socket, a USB
    /// function, a connection, a line or a link.
    type Peer: Copy + Eq + Hash + PeerPort;
    /// A failure of the link itself.
    type Error;

    /// Returns whether one more packet of `len` bytes for `peer` can be queued
    /// now.
    fn ready(&self, peer: Self::Peer, len: usize) -> bool;

    /// Queues one packet for `peer` at `priority`, the header followed by the
    /// payload.
    ///
    /// A carrier without priorities ignores the level.
    ///
    /// # Errors
    ///
    /// Returns the carrier's error when the link itself fails.
    fn send(
        &mut self,
        peer: Self::Peer,
        priority: Priority,
        header: &[u8],
        payload: &[u8],
    ) -> Result<(), Self::Error>;

    /// Takes one received packet into `buf`, or `None` when none is waiting.
    ///
    /// # Errors
    ///
    /// Returns the carrier's error when the link itself fails.
    fn recv(&mut self, buf: &mut [u8]) -> Result<Option<(Self::Peer, usize)>, Self::Error>;
}

/// What `GraphTP` needs from Homa, the crate's engine with both roles in the
/// process or the crate's kernel module.
///
/// `Endpoint` implements it where the build has both roles, and the socket of
/// the crate's kernel module in every build that has it, so one loop serves
/// both. A build with the role `server` alone takes the inherent methods of
/// `Endpoint` instead.
pub trait Rpcs {
    /// The other end of an RPC.
    type Peer: Copy + Eq + Hash + PeerPort;
    /// A failure of the implementation.
    type Error;
    /// A received message, borrowed until it is released.
    type Message<'a>: Read
    where
        Self: 'a;

    /// Queues a request and returns its identifier.
    ///
    /// # Errors
    ///
    /// Returns the implementation's error when the request cannot be queued.
    fn request(
        &mut self,
        peer: Self::Peer,
        message: &[u8],
        options: RequestOptions,
    ) -> Result<RpcId, Self::Error>;

    /// Queues the response of a server RPC.
    ///
    /// # Errors
    ///
    /// Returns the implementation's error when the response cannot be queued.
    fn reply(&mut self, id: RpcId, message: &[u8]) -> Result<(), Self::Error>;

    /// Returns the next event, a request, a response or a failure.
    ///
    /// # Errors
    ///
    /// Returns the implementation's error when it cannot read its events.
    fn next_event(&mut self) -> Result<Option<Event<Self::Peer>>, Self::Error>;

    /// Returns a message an event reported.
    fn message(&self, id: RpcId) -> Option<Self::Message<'_>>;

    /// Frees a message and its storage.
    ///
    /// # Errors
    ///
    /// Returns the implementation's error when `id` names no message.
    fn release(&mut self, id: RpcId) -> Result<(), Self::Error>;

    /// Ends a client RPC locally.
    ///
    /// # Errors
    ///
    /// Returns the implementation's error when `id` names no client RPC.
    fn abort(&mut self, id: RpcId) -> Result<(), Self::Error>;
}

/// A received message read in order, across the pages that hold it.
pub trait Read {
    /// Returns the length of the message in bytes.
    fn len(&self) -> u32;

    /// Returns whether the message is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the message as one slice when one page holds it.
    fn as_slice(&self) -> Option<&[u8]>;

    /// Copies the bytes from `offset` into `out` and returns how many it
    /// copied.
    fn read_at(&self, offset: u32, out: &mut [u8]) -> usize;
}

/// How to send a request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RequestOptions {
    /// The response may take long, as for a parked request, a PULL of `GraphTP`
    /// among them.
    pub patient: bool,
    /// After `RPC_UNKNOWN` on an RPC that has received no byte of its answer,
    /// report [`Failure::RpcUnknown`] in place of Homa's silent restart.
    ///
    /// False keeps the restart, which the derived `Default` gives every existing
    /// request.
    pub report_unknown: bool,
    /// The local Homa port the request leaves from, 0 for `Config::port`.
    ///
    /// It lies in the ports the endpoint owns, and the full form writes it as
    /// the source port, so one endpoint can be several peers to a server.
    pub source_port: u16,
}

/// What the endpoint reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event<P> {
    /// A whole request arrived, read it with `Endpoint::request_of`.
    Request {
        /// The client that sent it.
        peer: P,
        /// The server RPC it opened.
        id: RpcId,
    },
    /// A whole response arrived, read it with `Endpoint::response`.
    Response {
        /// The client RPC it answers.
        id: RpcId,
    },
    /// A client RPC ended without a response.
    Failed {
        /// The client RPC that ended.
        id: RpcId,
        /// Why it ended.
        why: Failure,
    },
    /// A server RPC was freed without a reply, because its peer is gone.
    Dropped {
        /// The server RPC that was freed.
        id: RpcId,
    },
    /// The peer's LIMITS arrived or changed.
    Limits {
        /// The peer that sent them.
        peer: P,
        /// What the peer accepts now.
        limits: Limits,
    },
    /// The peer is gone and every RPC with it has ended.
    PeerGone {
        /// The peer that is gone.
        peer: P,
        /// Why it is gone.
        why: Gone,
    },
    /// The peer's unscheduled cutoffs changed.
    Cutoffs {
        /// The peer whose cutoffs changed.
        peer: P,
    },
}

/// Why a client RPC ended without a response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The carrier reported the link down.
    LinkDown,
    /// The peer left `timeout_resends` RESEND and `NEED_ACK` packets unanswered.
    PeerTimeout,
    /// The peer's LIMITS names a `max_message` below the request's length.
    TooLong,
    /// The application aborted it.
    Aborted,
    /// The server answered `RPC_UNKNOWN` before a byte of the answer arrived,
    /// and the request asked for the report with
    /// [`RequestOptions::report_unknown`].
    RpcUnknown,
}

/// Why a peer is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gone {
    /// The carrier reported the link down.
    LinkDown,
    /// The peer left `timeout_resends` RESEND and `NEED_ACK` packets unanswered.
    Timeout,
}

/// Why a message could not be queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendError<E = Infallible> {
    /// Every slot of the role is taken.
    NoSlot,
    /// The message is longer than the slot or than the peer's `max_message`.
    TooLong,
    /// The peer's `max_rpcs` are open.
    Full,
    /// The peer has no link up.
    NoPeer,
    /// The identifier names no server RPC waiting for a reply.
    NoRequest,
    /// The source port of the request lies outside the ports the endpoint
    /// owns.
    Port,
    /// The writer of the message failed.
    Write(E),
}

impl<E: fmt::Display> fmt::Display for SendError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSlot => f.write_str("every slot of the role is taken"),
            Self::TooLong => f.write_str("the message is longer than the slot or the peer accepts"),
            Self::Full => f.write_str("the peer's max_rpcs are open"),
            Self::NoPeer => f.write_str("the peer has no link up"),
            Self::NoRequest => {
                f.write_str("the identifier names no server RPC waiting for a reply")
            }
            Self::Port => f.write_str("the source port lies outside the ports the endpoint owns"),
            Self::Write(e) => write!(f, "the writer of the message failed: {e}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> core::error::Error for SendError<E> {}

/// Why a packet was not taken.
///
/// The endpoint counts every refusal in [`Stats::refused`] and changes
/// nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The packet does not decode.
    Malformed(DecodeError),
    /// The peer has no link up.
    NoPeer,
    /// The packet breaks this end's limits, and LIMITS is queued in answer.
    OverLimits,
    /// No slot is free for a new RPC.
    NoSlot,
    /// The packet opens one gap more than a message may have while it
    /// arrives, two on a point to point link.
    ThirdGap,
    /// `now` is earlier than a time passed before.
    ClockBackwards,
    /// The packet is a FREEZE, which the crate drops and counts in
    /// [`Stats::freezes_dropped`].
    Freeze,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(e) => write!(f, "the packet does not decode: {e}"),
            Self::NoPeer => f.write_str("the peer has no link up"),
            Self::OverLimits => f.write_str("the packet breaks the limits of this end"),
            Self::NoSlot => f.write_str("no slot is free for a new RPC"),
            Self::ThirdGap => f.write_str("the packet opens one gap too many"),
            Self::ClockBackwards => f.write_str("the clock went backwards"),
            Self::Freeze => f.write_str("the packet is a FREEZE, which is dropped"),
        }
    }
}

impl core::error::Error for Refused {}

/// `now` is earlier than a time passed before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockBackwards;

impl fmt::Display for ClockBackwards {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the clock went backwards")
    }
}

impl core::error::Error for ClockBackwards {}

/// Why an endpoint could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// A part of the storage is shorter than the configuration needs.
    TooSmall,
    /// No slot and no peer, or a capacity of zero.
    Empty,
    /// `first_id` is odd or 0.
    BadFirstId,
    /// The profile asks for a mechanism the build left out, named by its
    /// feature.
    Missing(&'static str),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooSmall => f.write_str("the storage is shorter than the configuration needs"),
            Self::Empty => f.write_str("the configuration holds no slot or no peer"),
            Self::BadFirstId => f.write_str("the first identifier is odd or 0"),
            Self::Missing(feature) => write!(f, "the profile needs the feature {feature}"),
        }
    }
}

impl core::error::Error for ConfigError {}

/// Why a link operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// Every peer place is taken.
    NoPeerSlot,
    /// The peer is up already.
    Up,
    /// The peer has no link up.
    NoPeer,
    /// A packet below 20 bytes, or a window smaller than a packet.
    Params,
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoPeerSlot => "every peer place is taken",
            Self::Up => "the peer is up already",
            Self::NoPeer => "the peer has no link up",
            Self::Params => "the limits of the link are out of range",
        })
    }
}

impl core::error::Error for LinkError {}

/// The identifier names no RPC of the role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownRpc;

impl fmt::Display for UnknownRpc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the identifier names no RPC of the role")
    }
}

impl core::error::Error for UnknownRpc {}

/// The output is shorter than what has to be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoRoom;

impl fmt::Display for NoRoom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the output is shorter than what has to be written")
    }
}

impl core::error::Error for NoRoom {}

/// One packet ready for the carrier, borrowed from the endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transmit<'a, P> {
    /// The peer the packet goes to.
    pub peer: P,
    /// The level the packet leaves at.
    pub priority: Priority,
    /// The header of the packet.
    pub header: &'a [u8],
    /// The payload of the packet, borrowed from the pages of its message.
    pub payload: &'a [u8],
}

/// Counters the notebooks read, all monotonic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Packets sent.
    pub sent: u64,
    /// Packets taken.
    pub received: u64,
    /// Packets refused, each for a reason `Refused` names.
    pub refused: u64,
    /// RESENDs sent.
    pub resends: u64,
    /// Requests sent again from offset 0 after `RPC_UNKNOWN`.
    pub restarts: u64,
    /// LIMITS sent.
    pub limits_sent: u64,
    /// Peers declared dead.
    pub peer_deaths: u64,
    /// Packets sent at each priority level, the lowest first.
    pub per_priority: [u64; 8],
    /// GRANTs sent.
    pub grants_sent: u64,
    /// `START_MSG` packets sent.
    pub start_msg_sent: u64,
    /// Requests sent with the incast mark.
    pub incast_marked: u64,
    /// Marked requests whose response a server kept unscheduled up to the
    /// incast bytes alone.
    pub incast_honoured: u64,
    /// CUTOFFS sent.
    pub cutoffs_sent: u64,
    /// Probes of a table lookup beyond the first.
    pub probes_beyond_first: u64,
    /// FREEZE packets taken and dropped.
    pub freezes_dropped: u64,
    /// Client RPCs ended with [`Failure::RpcUnknown`] in place of a restart.
    pub unknown_reported: u64,
    /// Packets dropped because their destination port is not one the
    /// endpoint owns.
    pub wrong_port: u64,
}

/// The state of one RPC as `Endpoint::rpc_info` reports it, as HOMAIOCINFO
/// does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RpcInfo<P> {
    /// The RPC as the local endpoint names it.
    pub id: RpcId,
    /// The other end of the RPC.
    pub peer: P,
    /// Where the RPC stands.
    pub state: RpcState,
    /// The local Homa port the RPC belongs to.
    pub local_port: u16,
    /// Bytes of the message the RPC moves now, the request until the server
    /// holds it whole and the response after.
    pub length: u32,
    /// Bytes of the outgoing message sent.
    pub sent: u32,
    /// Bytes of the outgoing message the receiver granted.
    pub granted: u32,
    /// Bytes of the incoming message received.
    pub received: u32,
}

/// Where an RPC stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcState {
    /// A client RPC that sends its request or waits for the first byte of its
    /// response.
    Outgoing,
    /// A server RPC whose request arrives, or a client RPC whose response
    /// arrives.
    Incoming,
    /// A server RPC that holds its whole request and has not queued its
    /// response.
    Executing,
    /// A server RPC that sends its response or keeps it until the client
    /// acknowledges it.
    Answered,
    /// A client RPC of a patient request that has sent the whole request and
    /// waits for the first byte of its response.
    Parked,
    /// A server RPC whose response has pages reserved and is not committed.
    Reserved,
}
