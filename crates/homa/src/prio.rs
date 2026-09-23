// SPDX-License-Identifier: MIT
/*
 * homa prio: the priority levels, the unscheduled cutoffs and the scheduled
 * levels.
 *
 * The shapes are those of the Homa design note, second version,
 * homa/design.md sections 3.5 and 7.6, lines 583 to 606 and 2630 to 2690, and
 * the rules are HomaModule's at commit 50d65305: its default levels and
 * cutoffs, homa_utils.c lines 66 to 84, the level of an unscheduled message,
 * homa_peer.c lines 700 to 710, and the level of a grant, homa_grant_priority
 * of homa_grant.c lines 282 to 303, at
 * https://github.com/PlatformLab/HomaModule/tree/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3,
 * read. The computed cutoffs follow the rule of SIGCOMM 2018 section 3.4,
 * equal bytes per unscheduled level and shorter messages higher.
 */

//! The priority machinery of Network.
//!
//! A switch serves up to eight priority queues on each port, and the
//! receiver of a message decides the level of every packet of it. The
//! levels split in two: the lower ones carry scheduled bytes, whose level
//! each GRANT names, and the upper ones carry unscheduled bytes, whose level
//! the receiver's cutoffs give by the length of the message, shorter
//! messages higher. [`Levels`] says how many levels are in use and where
//! they split, [`Cutoffs`] holds the cutoffs a receiver announces in CUTOFFS,
//! and [`scheduled`] gives the level of a grant as `homa_grant_priority`
//! does. Under the feature `prio-compute` a receiver counts the unscheduled
//! bytes of what it receives in [`Sizes`] and computes cutoffs from them.
//!
//! A level is a number from 0, the lowest, to 7. [`Levels::external`] maps it
//! to the level the network sees, which the carrier writes into the TOS byte
//! or the traffic class.

use crate::Priority;

/// The most levels a switch port serves, `HOMA_MAX_PRIORITIES`.
pub const MAX_LEVELS: u8 = 8;

/// The longest message Homa carries, which the lowest unscheduled level
/// covers.
pub const MAX_MESSAGE: u32 = 1_000_000;

/// The levels in use and their map to what the network sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Levels {
    /// Levels in use, from 1 to 8.
    pub num: u8,
    /// The highest level of scheduled bytes, every level above it carries
    /// unscheduled bytes.
    pub max_scheduled: u8,
    /// The level the network sees for each level of the engine.
    pub map: [u8; 8],
}

impl Levels {
    /// Returns `HomaModule`'s defaults at the pinned commit: 8 levels, the
    /// scheduled bytes on levels 0 to 3 and the identity map.
    #[must_use]
    pub const fn module_default() -> Self {
        Self::split(MAX_LEVELS)
    }

    /// Returns `num` levels, clamped to 1 to 8, the lower half scheduled and
    /// the upper half unscheduled with the identity map.
    ///
    /// Eight levels give `HomaModule`'s split of four and four, four give two
    /// and two, two give one and one, and one level carries every byte, so
    /// its one level is scheduled and unscheduled at once.
    #[must_use]
    pub const fn split(num: u8) -> Self {
        let num = if num == 0 {
            1
        } else if num > MAX_LEVELS {
            MAX_LEVELS
        } else {
            num
        };
        let max_scheduled = if num >= 2 { num / 2 - 1 } else { 0 };
        Self {
            num,
            max_scheduled,
            map: [0, 1, 2, 3, 4, 5, 6, 7],
        }
    }

    /// Returns the level the network sees for `level`, which is clamped to
    /// the levels in use first.
    #[must_use]
    pub const fn external(&self, level: u8) -> Priority {
        let top = self.top();
        let level = if level > top { top } else { level };
        let mapped = self.map[level as usize];
        match Priority::new(mapped) {
            Some(priority) => priority,
            None => Priority::HIGHEST,
        }
    }

    /// Returns the highest level in use, which every control packet takes, as
    /// `__homa_xmit_control` sends at `num_priorities - 1`.
    #[must_use]
    pub const fn top(&self) -> u8 {
        let num = if self.num == 0 {
            1
        } else if self.num > MAX_LEVELS {
            MAX_LEVELS
        } else {
            self.num
        };
        num - 1
    }

    /// Returns the lowest level of unscheduled bytes, 0 where one level
    /// carries everything.
    #[must_use]
    pub const fn lowest_unscheduled(&self) -> u8 {
        let top = self.top();
        if top == 0 {
            0
        } else if self.max_scheduled >= top {
            top
        } else {
            self.max_scheduled + 1
        }
    }

    /// Returns whether the levels are in range: 1 to 8 of them, the
    /// scheduled ones below the top unless one level carries everything,
    /// and every mapped level at most 7.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        if self.num == 0 || self.num > MAX_LEVELS {
            return false;
        }
        if self.num > 1 && self.max_scheduled >= self.num - 1 {
            return false;
        }
        let mut i = 0;
        while i < self.map.len() {
            if self.map[i] > 7 {
                return false;
            }
            i += 1;
        }
        true
    }
}

/// A receiver's unscheduled cutoffs and their version.
///
/// `bytes[i]` is the longest unscheduled message that goes at level `i`, and
/// a level whose value is 0 carries no unscheduled message, which is how the
/// scheduled levels read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cutoffs {
    /// The longest unscheduled message of each level, the lowest first.
    pub bytes: [u32; 8],
    /// The version every DATA packet to the receiver names.
    pub version: u16,
}

impl Cutoffs {
    /// Returns `HomaModule`'s defaults at the pinned commit: 200 bytes at
    /// level 7, 2,800 at 6, 15,000 at 5 and every longer message at 4, in
    /// version 1.
    #[must_use]
    pub const fn module_default() -> Self {
        Self {
            bytes: [0, 0, 0, 0, MAX_MESSAGE, 15_000, 2_800, 200],
            version: 1,
        }
    }

    /// Returns the level of an unscheduled message of `length` bytes: the
    /// highest level whose cutoff is at least `length`, and level 0 where
    /// none is, as `homa_unsched_priority` gives it.
    #[must_use]
    pub fn unscheduled(&self, length: u32) -> Priority {
        let level = self
            .bytes
            .iter()
            .rposition(|&cutoff| cutoff >= length)
            .and_then(|level| u8::try_from(level).ok())
            .unwrap_or(0);
        Priority::new(level).unwrap_or(Priority::HIGHEST)
    }
}

/// Returns the level of the granted message of `rank` among `active`, as
/// `homa_grant_priority` gives it.
///
/// The message with no other active message ahead of it has rank 0. Where
/// fewer messages are active than there are scheduled levels, the grants take
/// the lowest of them, so a new shorter message finds a higher level free and
/// passes the others without waiting. The level is that of the engine, before
/// [`Levels::external`] maps it.
// The level is clamped to 0 up to the scheduled level first, so the cast
// keeps it whole.
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[must_use]
pub const fn scheduled(levels: Levels, rank: u8, active: u8) -> Priority {
    let max = levels.max_scheduled as i32;
    let mut level = max - rank as i32;
    let extra = max + 1 - active as i32;
    if extra >= 0 {
        level -= extra;
    }
    let level = if level < 0 { 0 } else { level as u8 };
    match Priority::new(level) {
        Some(priority) => priority,
        None => Priority::HIGHEST,
    }
}

/// The fixed point factors of the fifth octaves, `2^(-m/5)` times `2^16` for
/// `m` from 0 to 4, rounded.
#[cfg(feature = "prio-compute")]
const FIFTHS: [u64; 5] = [65_536, 57_052, 49_667, 43_238, 37_641];

/// Incoming message bytes in 32 logarithmic buckets, the input of
/// `Endpoint::recompute_cutoffs`.
///
/// The buckets count the unscheduled bytes of the messages a receiver takes
/// by the length of the message, in steps of a fifth of an octave down from
/// the unscheduled bytes of the receiver: bucket `i` from 1 to 31 holds the
/// lengths above [`Sizes::edge`] of `i` up to the edge of `i + 1`, the edge
/// of 32 being the unscheduled bytes themselves, and bucket 0 every shorter
/// length. A message longer than the unscheduled bytes is not counted in
/// either scheduling mode. Under all or nothing scheduling it carries no
/// unscheduled byte, and in the prefix mode the cutoffs computed here put
/// its unscheduled bytes at the lowest unscheduled level whatever the other
/// levels share, since every other cutoff is at most the unscheduled bytes.
#[cfg(feature = "prio-compute")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sizes {
    /// The unscheduled bytes counted in each bucket.
    pub bytes: [u64; 32],
}

#[cfg(feature = "prio-compute")]
impl Sizes {
    /// The count of buckets.
    pub const BUCKETS: usize = 32;

    /// Returns an empty histogram.
    #[must_use]
    pub const fn new() -> Self {
        Self { bytes: [0; 32] }
    }

    /// Returns the lower edge of bucket `i` for a receiver of `unsched`
    /// unscheduled bytes, `unsched` times `2^(-(32 - i)/5)`, and 0 for
    /// bucket 0.
    // The edge is at most `unsched` where the cast runs, so it truncates
    // nothing.
    #[expect(clippy::cast_possible_truncation)]
    #[must_use]
    pub const fn edge(i: usize, unsched: u32) -> u32 {
        if i == 0 {
            return 0;
        }
        if i >= Self::BUCKETS {
            return unsched;
        }
        let k = Self::BUCKETS - i;
        let scaled = (unsched as u64 * FIFTHS[k % 5]) >> 16;
        let edge = scaled >> (k / 5);
        if edge > u32::MAX as u64 {
            u32::MAX
        } else {
            edge as u32
        }
    }

    /// Returns the bucket of a message of `length` bytes for a receiver of
    /// `unsched` unscheduled bytes, or `None` for a message longer than
    /// them.
    #[must_use]
    pub fn bucket(length: u32, unsched: u32) -> Option<usize> {
        if length > unsched {
            return None;
        }
        Some(
            (1..Self::BUCKETS)
                .rev()
                .find(|&i| length > Self::edge(i, unsched))
                .unwrap_or(0),
        )
    }

    /// Counts `bytes` unscheduled bytes of a message of `length` bytes.
    pub fn count(&mut self, length: u32, unsched: u32, bytes: u32) {
        if let Some(slot) = Self::bucket(length, unsched).and_then(|i| self.bytes.get_mut(i)) {
            *slot = slot.saturating_add(u64::from(bytes));
        }
    }

    /// Returns the cutoffs that give each unscheduled level of `levels` an
    /// equal share of the bytes counted, shorter messages higher, in
    /// `version`, or `None` where nothing was counted.
    ///
    /// The cutoff between two levels is the length at which the bytes
    /// counted below it reach the share of the levels above, read inside its
    /// bucket as if the bucket's messages spread evenly over its lengths. The
    /// lowest unscheduled level takes every longer message up to the longest
    /// Homa carries, and the scheduled levels take none.
    #[must_use]
    pub fn cutoffs(&self, levels: Levels, unsched: u32, version: u16) -> Option<Cutoffs> {
        let total: u64 = self
            .bytes
            .iter()
            .fold(0u64, |sum, &b| sum.saturating_add(b));
        if total == 0 {
            return None;
        }
        let top = levels.top();
        let lowest = levels.lowest_unscheduled();
        let shares = u64::from(top - lowest) + 1;
        let mut out = [0u32; 8];
        out[usize::from(lowest)] = MAX_MESSAGE.max(unsched);
        for j in 1..shares {
            let share = u128::from(total) * u128::from(j) / u128::from(shares);
            let target = u64::try_from(share).unwrap_or(u64::MAX);
            let level = usize::from(top) - usize::try_from(j).unwrap_or(0) + 1;
            if let Some(slot) = out.get_mut(level) {
                *slot = self.length_at(target, unsched);
            }
        }
        Some(Cutoffs {
            bytes: out,
            version,
        })
    }

    /// Returns the length at which the bytes counted in the buckets below it
    /// reach `target`.
    fn length_at(&self, target: u64, unsched: u32) -> u32 {
        let mut below = 0u64;
        for (i, &bytes) in self.bytes.iter().enumerate() {
            if bytes > 0 && below.saturating_add(bytes) >= target {
                // Messages spread evenly over the lengths of a bucket hold
                // bytes in proportion to their length, so the bytes below a
                // length grow with its square.
                let low = u128::from(Self::edge(i, unsched));
                let high = u128::from(Self::edge(i + 1, unsched));
                let into = u128::from(target.saturating_sub(below));
                let area = (high * high).saturating_sub(low * low);
                let square = low * low + area * into / u128::from(bytes);
                let length = square.isqrt().max(1);
                return u32::try_from(length).unwrap_or(u32::MAX);
            }
            below = below.saturating_add(bytes);
        }
        unsched
    }
}

#[cfg(feature = "prio-compute")]
impl Default for Sizes {
    fn default() -> Self {
        Self::new()
    }
}
