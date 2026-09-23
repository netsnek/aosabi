// SPDX-License-Identifier: MIT
/*
 * homa pacer: readiness, the rate pacer and the budget pacer.
 *
 * Readiness is the pacer of the first version of the Homa design note,
 * homa/design-v1.md section 1.4, lines 179 to 281, with the length of the
 * packet that its second version adds, homa/design.md section 3.7, lines 621
 * to 640, and the FIFO share of a link by count of its first version, section
 * 1.4. The rate pacer is the first half of HomaModule's pacer, its estimate of
 * when the link has sent everything and its short messages that skip the wait,
 * homa_qdisc.c lines 14 to 60 and 242 to 246 at
 * https://github.com/PlatformLab/HomaModule/tree/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3,
 * read, and the budget pacer is the airtime ledger of the second version,
 * sections 3.7 and 6.3, lines 621 to 640 and 1810 to 1868.
 */

//! The pacers of the engine.
//!
//! **Readiness**, always built. The engine asks the carrier whether one more
//! packet of a given length may be queued for a peer and hands it one packet
//! at a time, so packets never queue below the engine where SRPT cannot
//! reorder them. The carrier answers from its platform's own signal where one
//! exists, and a driver without one counts the packets it holds and not yet
//! sent. The preemption granularity of SRPT is the depth of that queue.
//!
//! **The FIFO share by count.** Every `fifo_every`th packet of a link and
//! every `fifo_every`th grant of a downlink goes to the oldest message rather
//! than the shortest, 1 in 16 on a point to point profile, so the largest
//! messages never starve, and a share of 1 makes every choice FIFO, which is
//! the baseline SRPT is measured against without a switch of its own.
//!
//! **Rate**, feature `pacer-rate`. [`Rate`] keeps an estimate of when the
//! link will have sent everything it was given, at the rate it was built for,
//! and holds a DATA packet of a message of at least `defer_min_bytes` while
//! that estimate lies more than `max_backlog` ahead, so the queue below the
//! engine stays short where the carrier's readiness cannot see it, which is
//! the first half of `HomaModule`'s pacer. A shorter message and every
//! control packet go at once and are charged to the estimate.
//!
//! **Budget**, feature `pacer-budget`. [`Budget`] is a quantity per sliding
//! window kept in buckets the caller owns, which a carrier with a regulated
//! budget, the air of `LoRa`, charges before it sends.

/// Returns whether the carrier takes a packet of `len` bytes for `peer` now.
pub(crate) fn admits<P>(ready: &impl Fn(P, usize) -> bool, peer: P, len: usize) -> bool {
    ready(peer, len)
}

/// Returns whether the next choice of a link whose counter reads `count` is
/// the FIFO share's, one in `every`, and none where `every` is 0.
pub(crate) const fn fifo_turn(count: u8, every: u8) -> bool {
    every > 0 && count % every == every - 1
}

/// Returns the counter of a link after one more choice, which wraps at
/// `every`.
pub(crate) const fn advance(count: u8, every: u8) -> u8 {
    if every == 0 {
        0
    } else {
        (count % every + 1) % every
    }
}

/// Paces packets at a link rate, the first half of `HomaModule`'s pacer.
#[cfg(feature = "pacer-rate")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    /// The rate of the link in bits a second.
    bits_per_s: u64,
    /// The backlog the link may hold before a long message waits.
    max_backlog: crate::Nanos,
    /// The shortest message whose packets wait for the backlog.
    defer_min_bytes: u32,
    /// When the link will have sent everything it was given.
    idle_at: crate::Nanos,
}

#[cfg(feature = "pacer-rate")]
impl Rate {
    /// Builds a pacer for `bits_per_s` that lets at most `max_backlog` of
    /// data wait in the link.
    ///
    /// A message shorter than `defer_min_bytes` never waits. `HomaModule`'s
    /// defaults are a backlog of 5 µs and 1,000 bytes.
    #[must_use]
    pub const fn new(bits_per_s: u64, max_backlog: crate::Nanos, defer_min_bytes: u32) -> Self {
        Self {
            bits_per_s: if bits_per_s == 0 { 1 } else { bits_per_s },
            max_backlog,
            defer_min_bytes,
            idle_at: 0,
        }
    }

    /// Returns the nanoseconds `len` bytes take on the link, rounded up.
    #[must_use]
    pub fn wire_time(&self, len: usize) -> crate::Nanos {
        let bits = u128::try_from(len).unwrap_or(u128::MAX).saturating_mul(8);
        let nanos = bits
            .saturating_mul(1_000_000_000)
            .div_ceil(u128::from(self.bits_per_s));
        crate::Nanos::try_from(nanos).unwrap_or(crate::Nanos::MAX)
    }

    /// Returns whether a packet of `len` bytes of a `msg_len` byte message
    /// may go now, and charges it to the link where it may.
    pub fn admit(&mut self, now: crate::Nanos, len: usize, msg_len: u32) -> bool {
        let backlog = self.idle_at.saturating_sub(now);
        if msg_len >= self.defer_min_bytes && backlog > self.max_backlog {
            return false;
        }
        self.charge(now, len);
        true
    }

    /// Charges a packet of `len` bytes that goes whatever the backlog.
    pub fn charge(&mut self, now: crate::Nanos, len: usize) {
        let time = self.wire_time(len);
        self.idle_at = self.idle_at.max(now).saturating_add(time);
    }

    /// Returns when a packet the pacer held may go, the time the backlog
    /// falls to `max_backlog`.
    #[must_use]
    pub fn next_admit(&self, now: crate::Nanos) -> crate::Nanos {
        self.idle_at.saturating_sub(self.max_backlog).max(now)
    }

    /// Returns the backlog of the link at `now`.
    #[must_use]
    pub fn backlog(&self, now: crate::Nanos) -> crate::Nanos {
        self.idle_at.saturating_sub(now)
    }
}

/// A charge that the budget has no room for.
#[cfg(feature = "pacer-budget")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverBudget;

#[cfg(feature = "pacer-budget")]
impl core::fmt::Display for OverBudget {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("the charge does not fit the budget")
    }
}

#[cfg(feature = "pacer-budget")]
impl core::error::Error for OverBudget {}

/// A budget of a quantity over a sliding window, in buckets the caller owns.
///
/// `n` buckets each hold what was charged in one slice of `window / (n - 1)`,
/// so the sum of all of them covers the whole last `window` and the slice
/// before it, which never lets more than `limit` into any sliding window, as
/// the airtime ledger of 61 buckets of a minute over an hour does. A share of
/// `reserve_permille` of `limit` is kept for control, which data never takes.
#[cfg(feature = "pacer-budget")]
#[derive(Debug)]
pub struct Budget<'b> {
    /// What each slice holds, the slice of time `t` at `t / width % n`.
    buckets: &'b mut [u32],
    /// The time one bucket covers.
    width: crate::Nanos,
    /// What any sliding window may hold.
    limit: u32,
    /// What control may hold beyond data.
    reserve: u32,
    /// The slice the newest charge fell into.
    newest: u64,
}

#[cfg(feature = "pacer-budget")]
impl<'b> Budget<'b> {
    /// Builds a budget of `limit` per `window`, kept in `buckets` equal
    /// slices of it.
    ///
    /// The buckets are cleared. Fewer than two buckets hold the whole window
    /// in one, and a window of 0 takes one nanosecond.
    pub fn new(
        buckets: &'b mut [u32],
        window: crate::Nanos,
        limit: u32,
        reserve_permille: u16,
    ) -> Self {
        buckets.fill(0);
        let slices = u64::try_from(buckets.len().saturating_sub(1).max(1)).unwrap_or(u64::MAX);
        let width = (window.max(1)).div_ceil(slices);
        let reserve = u64::from(limit) * u64::from(reserve_permille.min(1_000)) / 1_000;
        Self {
            buckets,
            width,
            limit,
            reserve: u32::try_from(reserve).unwrap_or(limit),
            newest: 0,
        }
    }

    /// Returns the slice of time `now`.
    fn slice(&self, now: crate::Nanos) -> u64 {
        now / self.width
    }

    /// Returns what the buckets hold that still counts at `now`.
    fn used(&self, now: crate::Nanos) -> u64 {
        let n = u64::try_from(self.buckets.len()).unwrap_or(u64::MAX);
        let current = self.slice(now).max(self.newest);
        let mut used = 0u64;
        for (i, &amount) in self.buckets.iter().enumerate() {
            let i = u64::try_from(i).unwrap_or(u64::MAX);
            // The newest slice at or before `newest` that bucket `i` holds.
            let back = (self.newest % n + n - i) % n;
            let Some(held) = self.newest.checked_sub(back) else {
                continue;
            };
            if held + n > current {
                used = used.saturating_add(u64::from(amount));
            }
        }
        used
    }

    /// Returns whether `amount` fits now, for data or for control, without
    /// charging it.
    #[must_use]
    pub fn fits(&self, now: crate::Nanos, amount: u32, control: bool) -> bool {
        let limit = if control {
            self.limit
        } else {
            self.limit.saturating_sub(self.reserve)
        };
        self.used(now).saturating_add(u64::from(amount)) <= u64::from(limit)
    }

    /// Charges `amount` at `now`.
    ///
    /// # Errors
    ///
    /// Returns [`OverBudget`] where `amount` does not fit the whole budget,
    /// control's reserve included, and charges nothing.
    pub fn charge(&mut self, now: crate::Nanos, amount: u32) -> Result<(), OverBudget> {
        if !self.fits(now, amount, true) || self.buckets.is_empty() {
            return Err(OverBudget);
        }
        let n = u64::try_from(self.buckets.len()).unwrap_or(u64::MAX);
        let slice = self.slice(now);
        if slice > self.newest {
            let stale = (slice - self.newest).min(n);
            for step in 1..=stale {
                let at = usize::try_from((self.newest + step) % n).unwrap_or(0);
                if let Some(bucket) = self.buckets.get_mut(at) {
                    *bucket = 0;
                }
            }
            self.newest = slice;
        }
        let at = usize::try_from(self.newest % n).unwrap_or(0);
        if let Some(bucket) = self.buckets.get_mut(at) {
            *bucket = bucket.saturating_add(amount);
        }
        Ok(())
    }

    /// Returns when `amount` of data fits at the earliest, `crate::Nanos::MAX`
    /// where it never does.
    #[must_use]
    pub fn next_fit(&self, now: crate::Nanos, amount: u32) -> crate::Nanos {
        if self.fits(now, amount, false) {
            return now;
        }
        let n = u64::try_from(self.buckets.len()).unwrap_or(u64::MAX);
        let first = self.slice(now).saturating_add(1);
        for step in 0..=n {
            let at = first.saturating_add(step).saturating_mul(self.width);
            if self.fits(at, amount, false) {
                return at;
            }
        }
        crate::Nanos::MAX
    }
}
