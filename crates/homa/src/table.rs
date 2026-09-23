// SPDX-License-Identifier: MIT
/*
 * homa table: open addressing over caller owned buckets with linear probing.
 *
 * The engine finds a peer and an RPC slot through such a table, so an endpoint
 * with thousands of peers pays a constant per packet for the lookup, since a
 * table at most half full keeps every run of linear probing short.
 */

//! Open addressing over caller owned buckets.
//!
//! A table is a slice of [`Bucket`]s whose length is a power of two at least
//! twice the entries it holds. A bucket is vacant or holds the index of an
//! entry in a slice the caller owns beside the table, a peer or an RPC slot,
//! so the table stores no key and asks the caller whether an entry matches. A
//! key starts at its hash modulo the length and moves on one bucket at a time
//! while the bucket holds another entry, which is linear probing. A removal
//! shifts the entries behind it back into the gap, so the table keeps no
//! tombstone and a lookup never reads past the run its hash starts in.
//!
//! The hash is `SipHash-1-3` keyed with `Config::hash_key`, which comes from
//! the caller's randomness, because on Network a sender chooses its address
//! and an unkeyed hash would let it choose addresses that collide and turn
//! every lookup into a scan.
//!
//! The walks of [`find`], [`insert`] and [`remove`] are the lookups. Each adds
//! one to its `probes` for every bucket it reads beyond the first, the vacant
//! bucket that ends it included, so a walk over `k` buckets adds `k - 1`. The
//! shift that follows a removal is no lookup and adds nothing. The endpoint
//! reports the sum as
//! [`Stats::probes_beyond_first`](crate::Stats::probes_beyond_first).

use core::{
    fmt,
    hash::{
        Hash,
        Hasher, //
    },
};

use crate::siphash::{
    self,
    Sip13, //
};

/// One bucket: the index of a peer or a slot, or vacant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bucket(u32);

impl Bucket {
    /// A bucket that holds no entry.
    pub const VACANT: Self = Self(u32::MAX);

    /// Returns a bucket that holds `index`, or `None` for `u32::MAX`, which
    /// marks a vacant bucket.
    #[must_use]
    pub const fn new(index: u32) -> Option<Self> {
        if index == u32::MAX {
            None
        } else {
            Some(Self(index))
        }
    }

    /// Returns the index the bucket holds, or `None` when it is vacant.
    #[must_use]
    pub const fn index(self) -> Option<u32> {
        if self.0 == u32::MAX {
            None
        } else {
            Some(self.0)
        }
    }
}

/// Why a table operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The length of the buckets is no power of two.
    Length,
    /// Every bucket is taken.
    Full,
    /// No bucket of the run holds the index.
    Missing,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Length => "the length of the buckets is no power of two",
            Self::Full => "every bucket is taken",
            Self::Missing => "no bucket holds the index",
        })
    }
}

impl core::error::Error for Error {}

/// Returns the buckets a table of `entries` entries takes, the smallest power
/// of two at least twice `entries`, or `None` where that does not fit a
/// `u32`.
#[must_use]
pub const fn capacity(entries: u32) -> Option<u32> {
    match entries.checked_mul(2) {
        Some(0) => Some(1),
        Some(twice) => twice.checked_next_power_of_two(),
        None => None,
    }
}

/// Returns the keyed hash of `key` as the tables compute it.
///
/// The value is `SipHash-1-3` under `hash_key` over the bytes that the
/// [`Hash`] implementation of `key` writes, so a peer is hashed as its carrier
/// names it, on Network the address and the port together. The walks of this
/// module take the hash as an argument, so a caller that drives a table
/// outside the engine computes it here and with [`hash_bytes`]. A caller
/// that needs a [`Hasher`] of its own takes [`Sip13`] of the module
/// `siphash`.
pub fn hash<K: Hash + ?Sized>(hash_key: &[u8; 16], key: &K) -> u64 {
    let mut hasher = Sip13::new(hash_key);
    key.hash(&mut hasher);
    hasher.finish()
}

/// Returns the keyed hash of `bytes`, `SipHash-1-3` under `hash_key` as the
/// standard library's default hasher computes it over the same bytes.
#[must_use]
pub fn hash_bytes(hash_key: &[u8; 16], bytes: &[u8]) -> u64 {
    siphash::hash(hash_key, bytes)
}

/// Returns the index `matches` accepts among the run that `hash` starts in.
///
/// The walk stops at the first vacant bucket and after one pass over the
/// table, and every bucket it reads beyond the first, the vacant bucket that
/// ends a miss included, adds one to `probes`. A table whose length is no
/// power of two holds nothing.
pub fn find(
    buckets: &[Bucket],
    hash: u64,
    probes: &mut u64,
    mut matches: impl FnMut(u32) -> bool,
) -> Option<u32> {
    let mask = mask(buckets.len())?;
    let mut at = home(hash, mask);
    for step in 0..buckets.len() {
        if step > 0 {
            *probes = probes.saturating_add(1);
        }
        let index = buckets.get(at)?.index()?;
        if matches(index) {
            return Some(index);
        }
        at = (at + 1) & mask;
    }
    None
}

/// Places `index` in the first vacant bucket of the run that `hash` starts in.
///
/// Every bucket the walk reads beyond the first, the vacant bucket it fills
/// included, adds one to `probes`. The caller has checked that no bucket
/// holds the same key already.
///
/// # Errors
///
/// Returns [`Error::Length`] when the length of `buckets` is no power of two,
/// and [`Error::Full`] when every bucket is taken or `index` is `u32::MAX`.
pub fn insert(
    buckets: &mut [Bucket],
    hash: u64,
    index: u32,
    probes: &mut u64,
) -> Result<(), Error> {
    let entry = Bucket::new(index).ok_or(Error::Full)?;
    let mask = mask(buckets.len()).ok_or(Error::Length)?;
    let mut at = home(hash, mask);
    for step in 0..buckets.len() {
        if step > 0 {
            *probes = probes.saturating_add(1);
        }
        let bucket = buckets.get_mut(at).ok_or(Error::Length)?;
        if bucket.index().is_none() {
            *bucket = entry;
            return Ok(());
        }
        at = (at + 1) & mask;
    }
    Err(Error::Full)
}

/// Removes `index` from the run that `hash` starts in, and shifts back every
/// entry behind it whose own run allows, so no gap splits a run.
///
/// The walk that finds `index` counts into `probes` as [`find`] does, and the
/// shift counts nothing. `hash_of` returns the hash of the entry an index
/// names, which the shift needs to know where each entry behind the gap
/// starts.
///
/// # Errors
///
/// Returns [`Error::Length`] when the length of `buckets` is no power of two,
/// and [`Error::Missing`] when no bucket of the run holds `index`.
pub fn remove(
    buckets: &mut [Bucket],
    hash: u64,
    index: u32,
    probes: &mut u64,
    mut hash_of: impl FnMut(u32) -> u64,
) -> Result<(), Error> {
    let mask = mask(buckets.len()).ok_or(Error::Length)?;
    let mut gap = locate(buckets, hash, index, mask, probes).ok_or(Error::Missing)?;
    buckets[gap] = Bucket::VACANT;
    let mut at = gap;
    for _ in 1..buckets.len() {
        at = (at + 1) & mask;
        let Some(moving) = buckets[at].index() else {
            break;
        };
        let start = home(hash_of(moving), mask);
        if !within(start, gap, at) {
            buckets[gap] = buckets[at];
            buckets[at] = Bucket::VACANT;
            gap = at;
        }
    }
    Ok(())
}

/// Returns the bucket that holds `index` in the run that `hash` starts in,
/// counting into `probes` as [`find`] does.
fn locate(
    buckets: &[Bucket],
    hash: u64,
    index: u32,
    mask: usize,
    probes: &mut u64,
) -> Option<usize> {
    let mut at = home(hash, mask);
    for step in 0..buckets.len() {
        if step > 0 {
            *probes = probes.saturating_add(1);
        }
        let held = buckets.get(at)?.index()?;
        if held == index {
            return Some(at);
        }
        at = (at + 1) & mask;
    }
    None
}

/// Returns whether `start` lies cyclically after `gap` and at or before `at`,
/// where an entry that starts there may not move back into the gap.
fn within(start: usize, gap: usize, at: usize) -> bool {
    if gap <= at {
        gap < start && start <= at
    } else {
        gap < start || start <= at
    }
}

/// Returns the mask of a table of `len` buckets, or `None` when `len` is no
/// power of two.
fn mask(len: usize) -> Option<usize> {
    if len.is_power_of_two() {
        Some(len - 1)
    } else {
        None
    }
}

/// Returns the bucket a run of `hash` starts in.
fn home(hash: u64, mask: usize) -> usize {
    let wide = u64::try_from(mask).unwrap_or(u64::MAX);
    usize::try_from(hash & wide).unwrap_or(0)
}
