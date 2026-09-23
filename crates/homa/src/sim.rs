// SPDX-License-Identifier: MIT
/*
 * homa sim: the simulators the notebooks drive, which no served build links.
 *
 * The simulators are those of the Homa design note, first version,
 * homa/design-v1.md section 2.5, lines 566 to 1088, the loopback, the lossy
 * link and the baseline of AoFrame alone, with the clock in nanoseconds of its
 * second version, homa/design.md section 7.8, lines 2784 to 2815. The
 * generator is xoshiro256** seeded through `SplitMix64`, as the authors of
 * both publish them.
 */

//! The simulated links, clock and generator of the notebooks.
//!
//! A run is deterministic: a [`Clock`] the caller moves, links that move
//! packets at the times their [`LinkModel`] gives, and faults drawn from a
//! seeded [`Rng`], so one seed replays one run byte for byte. The links are
//! [`Loopback`], a perfect link, [`Lossy`], a link with the faults of
//! [`Faults`], and [`Baseline`], whole messages in one `AoFrame` each with no
//! Homa, which is what ADR-0023 measures Homa against. [`Fabric`] is the rack
//! of a data centre, hosts on one switch with priority queues, on which the
//! mechanisms of Network act. [`Radio`] is `LoRa`, half duplex radios with a
//! time on air, channel activity detection and collisions, which the carrier
//! of `LoRa` drives. What a run records goes into a [`Trace`] of
//! [`TraceEvent`]s in the order it happened.

mod fabric;
mod link;
mod radio;

pub use fabric::*;
pub use link::*;
pub use radio::*;

use alloc::vec::Vec;

use crate::{
    ClockBackwards,
    Failure,
    Nanos,
    PeerPort, //
};

/// A virtual monotonic clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    /// The time the clock reads.
    now: Nanos,
}

impl Clock {
    /// Returns a clock at 0.
    #[must_use]
    pub const fn new() -> Self {
        Self { now: 0 }
    }

    /// Returns the time the clock reads.
    #[must_use]
    pub const fn now(&self) -> Nanos {
        self.now
    }

    /// Moves the clock to `at`, which must not lie before `now`.
    ///
    /// # Errors
    ///
    /// Returns [`ClockBackwards`] when `at` lies before the time the clock
    /// reads, and leaves the clock where it stood.
    pub fn advance_to(&mut self, at: Nanos) -> Result<(), ClockBackwards> {
        if at < self.now {
            return Err(ClockBackwards);
        }
        self.now = at;
        Ok(())
    }
}

/// The increment of `SplitMix64`, the golden ratio in 64 bits.
const GOLDEN: u64 = 0x9e37_79b9_7f4a_7c15;

/// Returns the output `SplitMix64` gives for the state `z`, its state already
/// advanced.
const fn mix(z: u64) -> u64 {
    let z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A seeded generator, xoshiro256** written in the crate, whose every use
/// names a purpose.
///
/// The four words of the state are the first four outputs of `SplitMix64` from
/// the seed, as the authors suggest for a seed of 64 bits. A generator for a
/// purpose, [`Rng::stream`], starts from a seed derived from this seed and the
/// bytes of the purpose, so two purposes draw apart and a new draw of one
/// never moves another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng {
    /// The four words of the state.
    state: [u64; 4],
    /// The seed the state came from.
    seed: u64,
}

impl Rng {
    /// Returns the generator of `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        let mut x = seed;
        let mut state = [0u64; 4];
        let mut i = 0;
        while i < 4 {
            x = x.wrapping_add(GOLDEN);
            state[i] = mix(x);
            i += 1;
        }
        Self { state, seed }
    }

    /// Returns the generator of `purpose`, derived from the seed and the
    /// purpose text.
    ///
    /// The derived seed starts at this generator's seed, and each byte `b`
    /// of `purpose` in order turns it from `h` into the `SplitMix64` output of
    /// `(h + 0x9e3779b97f4a7c15) ^ b`.
    #[must_use]
    pub fn stream(&self, purpose: &str) -> Self {
        let mut h = self.seed;
        for &byte in purpose.as_bytes() {
            h = mix(h.wrapping_add(GOLDEN) ^ u64::from(byte));
        }
        Self::new(h)
    }

    /// Returns the next output of xoshiro256**.
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.state;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// Returns a value below `bound`, 0 when `bound` is 0.
    ///
    /// The value is the high word of the product of an output and `bound`,
    /// and an output whose low word falls below `2^64 mod bound` is drawn
    /// again, which is Lemire's method and gives every value below `bound`
    /// the same chance.
    pub fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let product = u128::from(self.next_u64()) * u128::from(bound);
            let low = u64::try_from(product & u128::from(u64::MAX)).unwrap_or(u64::MAX);
            if low >= threshold {
                return u64::try_from(product >> 64).unwrap_or(0);
            }
        }
    }

    /// Returns whether an event of `per_million` chances in a million
    /// happens, one draw either way.
    pub(crate) fn chance(&mut self, per_million: u32) -> bool {
        self.below(1_000_000) < u64::from(per_million)
    }

    /// Returns the seed the generator came from.
    pub(crate) const fn seed(&self) -> u64 {
        self.seed
    }
}

/// Returns whether the value `key` falls among `per_million` in a million
/// under `seed`, the same answer every time it is asked.
pub(crate) const fn picked(seed: u64, key: u64, per_million: u32) -> bool {
    mix(seed.wrapping_add(GOLDEN) ^ key) % 1_000_000 < per_million as u64
}

/// How a simulated link moves packets, the first version's profile with
/// nanoseconds.
///
/// With `event` 0 the link sends continuously: a packet takes its bytes at
/// `bytes_per_s` on the wire, 0 for no time, one after the other, and at
/// most `packets_per_event` packets, 1 where it is 0, wait for the wire or
/// ride it, which is the depth below the engine. With `event` above 0 the
/// link sends only at every multiple of `event`, as a connection event of
/// Bluetooth does, at most `packets_per_event` packets at each, and holds at
/// most that many, and `bytes_per_s` paces [`Baseline`] alone. Every packet
/// arrives `latency` after it leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkModel {
    /// Bytes a second on the wire, 0 for a wire that takes no time.
    pub bytes_per_s: u32,
    /// Packets at each event, and the packets the link holds.
    pub packets_per_event: u8,
    /// The time between two events, 0 for a link without events.
    pub event: Nanos,
    /// The time from leaving to arriving.
    pub latency: Nanos,
}

/// Faults a lossy link injects, drawn per packet from the stream "faults".
///
/// Every count is per million. `drop_per_million`, `drop_limits_per_million`,
/// `duplicate_per_million` and `corrupt_per_million` are drawn once each for
/// every packet in that order, whatever the packet is, so a seed replays the
/// same faults. `drop_grants_per_million` picks the messages whose every
/// GRANT is lost, by the identifier of the GRANT, and `link_down_at` loses
/// every packet from that time on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Faults {
    /// Packets lost.
    pub drop_per_million: u32,
    /// Messages whose every GRANT is lost.
    pub drop_grants_per_million: u32,
    /// LIMITS packets lost.
    pub drop_limits_per_million: u32,
    /// Packets delivered twice.
    pub duplicate_per_million: u32,
    /// Packets with one bit flipped in their delimited `AoFrame`.
    pub corrupt_per_million: u32,
    /// The time from which the link loses every packet.
    pub link_down_at: Option<Nanos>,
}

/// One end of a simulated two ended link.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// The first end.
    A,
    /// The second end.
    B,
}

impl Side {
    /// Returns the other end.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }

    /// Returns the end as an index, 0 for A and 1 for B.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

/// An end of a simulated link has no Homa port.
impl PeerPort for Side {}

/// What a simulation records, in the order it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceEvent {
    /// A DATA packet left, with the bytes left of every message that could
    /// have gone instead.
    Transmit {
        /// When it left.
        at: Nanos,
        /// The end it left from.
        from: Side,
        /// The RPC of its message, as the sender names it.
        id: u64,
        /// The bytes its message had left before it.
        bytes_left: u32,
        /// Each message that could have gone, its RPC and its bytes left.
        candidates: Vec<(u64, u32)>,
        /// Whether the FIFO share chose it.
        fifo: bool,
    },
    /// A GRANT left.
    Grant {
        /// When it left.
        at: Nanos,
        /// The end it left from.
        from: Side,
        /// The RPC of its message, as the receiver names it.
        id: u64,
        /// The first byte the sender may not send yet.
        offset: u32,
        /// The level of the granted bytes.
        priority: u8,
    },
    /// A whole message arrived.
    Deliver {
        /// When it arrived.
        at: Nanos,
        /// The end it arrived at.
        to: Side,
        /// The RPC of the message, or the message of the baseline.
        id: u64,
        /// The bytes of the message.
        length: u32,
    },
    /// An RPC failed.
    Fail {
        /// When it failed.
        at: Nanos,
        /// The end whose RPC failed.
        side: Side,
        /// The RPC.
        id: u64,
        /// Why it failed.
        why: Failure,
    },
    /// A packet or a message was lost on its way.
    Drop {
        /// When it was lost.
        at: Nanos,
        /// The end it was going to.
        to: Side,
        /// Why it was lost.
        reason: &'static str,
    },
    /// The switch of a fabric dropped a packet for want of buffer.
    Overflow {
        /// When it was dropped.
        at: Nanos,
        /// The host it was going to.
        to: u16,
        /// Its level.
        level: u8,
    },
    /// A transmission of a radio ended.
    Air {
        /// When its first symbol went on the air.
        at: Nanos,
        /// The radio that sent it.
        radio: u8,
        /// Its time on air.
        air: Nanos,
        /// The bytes of its payload.
        len: u16,
        /// Whether another transmission overlapped it.
        collided: bool,
    },
    /// A radio lost a transmission it would have heard.
    Lost {
        /// When the transmission ended.
        at: Nanos,
        /// The radio that lost it.
        radio: u8,
        /// The radio that sent it.
        from: u8,
        /// Why it was lost: "collision", "half duplex" or "noise".
        reason: &'static str,
    },
}

/// The events of one run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace {
    /// The events in the order they happened.
    pub events: Vec<TraceEvent>,
}
