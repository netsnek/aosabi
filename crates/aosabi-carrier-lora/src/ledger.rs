// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-lora ledger: the airtime ledger that keeps a radio inside
 * the duty cycle of its band.
 *
 * The ledger is that of the Homa design note, second version, homa/design.md
 * section 6.3, lines 1810 to 1868: the air of one transmitter in 61 buckets of
 * one minute, a packet admitted only while the last 61 buckets plus the packet
 * stay within the duty cycle's share of an hour, and 90 percent of the budget
 * for DATA, so the control packets still leave when a long response has used
 * its share. The duty cycles are 10 percent in the band 869.4 to 869.65 MHz
 * and 1 percent in the band 868.0 to 868.6 MHz.
 */

//! The airtime ledger.
//!
//! The ledger runs over [`homa::pacer::Budget`] with 61 buckets over an
//! hour, whose sum covers the whole last hour and the minute before it, and
//! counts the air of a packet in microseconds at the time the packet leaves
//! the air, its end. So every packet whose air lies in any continuous hour
//! is counted when the last packet of that hour is admitted, and the air of
//! one transmitter in any continuous hour never exceeds the share of its
//! band: where the hour cuts the first packet in two, the part outside it is
//! at least as long as the air of the packets that ended too early to be
//! counted, since they lie in that part.

use homa::{
    Nanos,
    pacer::{
        Budget,
        OverBudget, //
    }, //
};

use crate::Band;

/// The buckets of the ledger.
pub const BUCKETS: usize = 61;

/// An hour in nanoseconds, the window of the duty cycle.
pub const HOUR: Nanos = 3_600_000_000_000;

/// The share of the budget per mille control keeps for itself, so DATA takes
/// 90 percent of it.
pub const CONTROL_PERMILLE: u16 = 100;

/// The nanoseconds in a microsecond, the unit the ledger counts air in.
const MICRO: Nanos = 1_000;

/// The airtime ledger, 61 buckets of one minute over
/// [`homa::pacer::Budget`], 90 percent for DATA.
#[derive(Debug)]
pub struct Ledger<'b> {
    /// The budget of the band in microseconds of air per hour.
    budget: Budget<'b>,
    /// The band.
    band: Band,
}

impl<'b> Ledger<'b> {
    /// Returns a ledger of `band` over `buckets`, which it clears.
    pub fn new(buckets: &'b mut [u32; BUCKETS], band: Band) -> Self {
        let limit = micros(band.air_per_hour());
        Self {
            budget: Budget::new(buckets, HOUR, limit, CONTROL_PERMILLE),
            band,
        }
    }

    /// Returns the band of the ledger.
    #[must_use]
    pub const fn band(&self) -> Band {
        self.band
    }

    /// Returns whether a packet of `air` that leaves now fits, from the share
    /// of DATA or, for `control`, from the whole budget, without charging
    /// it.
    #[must_use]
    pub fn fits(&self, now: Nanos, air: Nanos, control: bool) -> bool {
        self.budget
            .fits(now.saturating_add(air), micros(air), control)
    }

    /// Charges a packet of `air` that leaves now.
    ///
    /// # Errors
    ///
    /// Returns [`OverBudget`] where the packet does not fit the whole budget,
    /// and charges nothing.
    pub fn charge(&mut self, now: Nanos, air: Nanos) -> Result<(), OverBudget> {
        self.budget.charge(now.saturating_add(air), micros(air))
    }

    /// Returns the earliest time a packet of DATA of `air` fits, `now` where
    /// it fits now and `Nanos::MAX` where it never does.
    #[must_use]
    pub fn next_fit(&self, now: Nanos, air: Nanos) -> Nanos {
        let end = self.budget.next_fit(now.saturating_add(air), micros(air));
        if end == Nanos::MAX {
            return Nanos::MAX;
        }
        end.saturating_sub(air).max(now)
    }
}

/// Returns `air` in microseconds, rounded up, or `u32::MAX` where it does
/// not fit.
fn micros(air: Nanos) -> u32 {
    u32::try_from(air.div_ceil(MICRO)).unwrap_or(u32::MAX)
}
