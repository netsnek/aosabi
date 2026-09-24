// SPDX-License-Identifier: MIT
/*
 * aosabi-json map table: the index table of the insertion ordered map, probed a group at a time.
 *
 * The table differs from hashbrown in its layout. A group of control bytes is
 * one integer at an index of its own, and the positions of its slots sit at
 * the same index of a second array, so a group needs no raw pointer and no
 * copy of the first group. The table keeps no tombstone. A removal empties its
 * slot where the group holds another empty slot and records every entry again
 * where the group is full. A map of at most SMALL entries keeps no table, and
 * allocates no group until it holds more entries or reserves room for more.
 */

//! The index table of [`super::IndexMap`].
//!
//! The table maps a hash to the position of an entry in the vector of entries,
//! which is what keeps the insertion order. It is a table of `2^n` slots with
//! one control byte per slot, the scheme of the `SwissTable` that `hashbrown`
//! implements. A control byte is either empty, `0xFF`, or the top seven bits
//! of the hash of the entry in its slot, the tag, and the slot holds the
//! position of the entry as a `u32`, which keeps the positions of a large
//! table at half the size that positions as wide as a pointer take on a 64 bit
//! target.
//!
//! A map that has held no more than [`SMALL`] entries and reserved room for
//! no more has no use for an index. Its table is [`Table::Small`], which
//! records nothing, and the map finds a key by comparing it with the keys of
//! its entries in their order, so it hashes no key, its entries carry no
//! hash, and the growth of such a map allocates nothing beyond its vector.
//! The entry after the last of [`SMALL`] moves the map to [`Table::Groups`],
//! which records every entry, and so does a reserve of room for more entries
//! than [`SMALL`]. The map hashes every key it holds for that move, once. A
//! table with groups keeps them and records every entry of its map whatever
//! the count of entries, as `hashbrown` does, so a map that shrinks to
//! [`SMALL`] entries or fewer goes on finding its keys through its groups.
//!
//! A slot records a position below `2^32`, so the groups record the first
//! `2^32` entries of a map. A map of more entries, which a target of 64 bits
//! can hold, keeps the entries past them in its vector alone and finds a key
//! by comparing every entry, so a map of any size answers every lookup and
//! never panics. The map tests that count before it probes the groups (see
//! [`beyond`]), and on a target of 32 bits every position fits and the test of
//! the count compiles away. No map of Aosabi comes near that count, as a `Map`
//! of the reference ends at `2^24` entries.
//!
//! The slots of [`Table::Groups`] come in groups. The control bytes of a group
//! are one integer of the array `ctrl`, and the positions of its slots are one
//! row of the array `positions` at the same index, so a lookup that misses
//! reads the dense control bytes alone. A lookup starts at the group the low
//! bits of the hash name. The bit trick of the generic group of `hashbrown`
//! finds every control byte that equals the tag in a few integer operations,
//! only those slots compare their key, and a group with an empty slot ends the
//! search. The next group is chosen by triangular numbers, which visits every
//! group of a table of `2^n` groups once. Keys whose hashes agree in the low
//! bits the table masks therefore share a start and fill one cluster of
//! groups, so a probe among them reads one more group for every eight of them,
//! or every four in a group of four slots, whatever their tags, and a map of
//! `n` of them costs time quadratic in `n` to build. A slot the probe passes
//! holds the tag of the key sought with a chance of about one in 128 when the
//! tags of the keys are spread, so a hit compares the key it finds plus about
//! one other key per 128 slots it probes. Keys that agree in the tag as well
//! add on top of the reads of their groups a comparison of the key for each of
//! them on every probe that passes them. The doc of [`crate::value::KeyState`]
//! weighs both costs for the fixed key.
//!
//! A comparison of a key that fails is the cold path of the probe, which
//! `hashbrown` states with its `likely` around the comparison and this table
//! with [`core::hint::cold_path`]. The compiler then lays the probe out for
//! the comparison that finds the key and for the group that ends the search,
//! and keeps the registers of the loop of its caller for that path.
//!
//! The paths that record every entry of a map, the move of a small map to
//! groups, the growth of groups and a reserve, read the hashes of the
//! entries through the trait object [`Hashes`], a chunk of [`CHUNK`] hashes
//! per call. Their code therefore exists once in a binary, whatever the types
//! of key and value of its maps, and a type of entry brings one function of
//! its own, the copy of its hashes. The probes [`Groups::find`] and
//! [`Groups::find_slot`] take the comparison of a key as a generic closure,
//! so a lookup compares its key with no call through an object, and
//! [`Groups::remove`] takes the hashes it records again as a generic
//! iterator (see [`Groups::refill`]).
//!
//! The group is the integer of the target. It is eight bytes on a 64 bit
//! target and on `wasm32`, and four bytes elsewhere, so the probe on a target
//! of 32 bits such as `riscv32imc-unknown-none-elf` runs no emulated 64 bit
//! operation. That is the choice of `hashbrown` for its generic group on every
//! target the workspace builds, and `hashbrown` also takes eight bytes on
//! `aarch64` and `x86_64` with pointers of 32 bits, where this table takes
//! four. The feature `narrow-group` takes the group of four bytes on every
//! target, so a host runs the probe of the ESP32-C3. The table uses no atomic
//! operation and no `unsafe`.

use alloc::{
    boxed::Box,
    vec, //
};

/// The integer one group of control bytes is.
#[cfg(all(
    any(target_pointer_width = "64", target_arch = "wasm32"),
    not(feature = "narrow-group")
))]
type Word = u64;

/// The integer one group of control bytes is.
#[cfg(not(all(
    any(target_pointer_width = "64", target_arch = "wasm32"),
    not(feature = "narrow-group")
)))]
type Word = u32;

/// The count of slots in one group, one control byte each.
const WIDTH: usize = size_of::<Word>();

/// Every control byte of a group empty.
const EMPTY: Word = Word::MAX;

/// The byte `0x01` in every lane of a group.
const LOW: Word = Word::MAX / 0xFF;

/// The byte `0x80` in every lane of a group, the bit that marks an empty slot.
const HIGH: Word = LOW << 7;

/// The share of the slots, one in `SWEEP`, beyond which a removal moves the positions in one pass.
///
/// A pass over every slot and a probe for every moved entry cost the same at
/// about one moved entry in 16 to 32 slots in tables of 128 and 2,048 slots
/// and at about one in 200 in a table of 131,072 slots, so one in 128 keeps
/// the choice within about one and a half times the cheaper of the two.
const SWEEP: usize = 128;

/// The most entries a map holds with no groups.
///
/// A map that has held no more than `SMALL` entries and reserved room for no
/// more is [`Table::Small`] and compares keys, on every target, and the groups
/// record the entries of a map that has held more. Twelve is the count the map
/// family of check 9 of `tests/notebooks/20_exec_asm.ipynb` reads. Natively a
/// hit or a miss of a key in a map of 9 and 12 keys takes 8.63 and 10.70 ns by
/// the scan against 13.34 and 13.07 ns through groups, and a build with a hit
/// and a miss per key 28.84 and 32.01 ns per lookup against 31.00 and 31.35
/// ns, while a scan of 20 keys takes 17.75 ns against 13.43 ns. In the Worker
/// module under V8 a lookup in a map of 9 and 12 keys takes 8.09 and 9.37 ns
/// by the scan against 47.65 and 47.84 ns through groups at opt-level z and
/// 7.95 and 9.65 ns against 19.05 and 18.95 ns at opt-level 3. At five to
/// eight keys the run of `tests/notebooks/01_json_writer.ipynb` with
/// `map_measure_limits` and the counts 0, 4, 8, 12 and 16 reads a build by the
/// scan in 0.481 and 0.486 times the time of groups with the fixed and the
/// keyed hasher and a hit in 0.580 and 0.474 times.
pub(super) const SMALL: usize = 12;

/// The first position a slot does not record, `2^32`, where a position can reach it.
const UNRECORDED: Option<usize> = 1usize.checked_shl(u32::BITS);

/// The count of hashes one call of [`Hashes::read`] copies on a path that records entries.
///
/// The chunk sits on the stack of the path, which calls through the object
/// once for every `CHUNK` entries it records.
const CHUNK: usize = 16;

/// The hashes of the entries of a map, in the order of the entries.
///
/// The growth of [`Table::insert`], [`Table::reserve`] and
/// [`Groups::rebuild`] take the entries of their map as this trait object, so
/// the code of those paths is made once and not for every type of key and
/// value, and only [`Hashes::read`] is made for each type of entry.
pub(super) trait Hashes {
    /// Copies the hashes of the entries from `from` on into `into` and returns their count.
    ///
    /// It copies as many as `into` takes, or as many as the entries from
    /// `from` on hold where they are fewer, so an answer below the length of
    /// `into` says that the entries end there.
    fn read(&self, from: usize, into: &mut [usize]) -> usize;
}

/// One slot of the groups, the group it lies in and its lane there.
#[derive(Debug, Clone, Copy)]
pub(super) struct Slot {
    /// The index of the group in `ctrl` and in `positions`.
    at: usize,
    /// The lane of the slot in its group.
    lane: usize,
}

/// The index table: none for a small map, or the groups of a larger one.
#[derive(Debug, Clone)]
pub(super) enum Table {
    /// The table of a map of up to [`SMALL`] entries, which records nothing,
    /// since such a map compares the keys of its entries in their order.
    Small,
    /// The groups of a map that grew past [`SMALL`] entries or reserved
    /// room for more, which record every entry of their map.
    Groups(Groups),
}

/// The control bytes and the positions of the groups of a table.
///
/// The groups hold exactly the first `2^32` entries of their map, because
/// they keep no tombstone, so the count of entries says how full they are
/// and they store no count of their own.
#[derive(Debug, Clone)]
pub(super) struct Groups {
    /// The control bytes of every group, a power of two of them.
    ctrl: Box<[Word]>,
    /// The position every slot of a group holds, as many as `ctrl`.
    positions: Box<[[u32; WIDTH]]>,
}

impl Table {
    /// Creates the table of an empty map, which records nothing and allocates nothing.
    pub(super) const fn new() -> Self {
        Self::Small
    }

    /// Creates a table that takes `capacity` entries before it grows.
    ///
    /// A capacity of up to [`SMALL`] entries allocates nothing, since a
    /// small map needs no table.
    pub(super) fn with_capacity(capacity: usize) -> Self {
        if small(capacity) {
            Self::new()
        } else {
            Self::Groups(Groups::sized(groups_for(capacity)))
        }
    }

    /// Records the entry at `position` with `hash`, which the table does not hold.
    ///
    /// The position `position` is the count of entries so far, and `hashes`
    /// reads the hash of every entry, in their order. A small map records
    /// nothing, and `hash` is no hash there. The entry `SMALL + 1` moves the
    /// map to groups that record `hashes` first, which the map hashes for
    /// the move, and groups that take no more entries grow and record
    /// `hashes` again. A position a slot does not record, `2^32` or more,
    /// stays with the vector of entries alone.
    pub(super) fn insert(&mut self, hash: usize, position: usize, hashes: &dyn Hashes) {
        let Ok(slot) = u32::try_from(position) else {
            return;
        };
        match self {
            Self::Small if small(position + 1) => {}
            Self::Small => {
                let mut groups = Groups::recording(position + 1, hashes);
                groups.place(hash, slot);
                *self = Self::Groups(groups);
            }
            Self::Groups(groups) => {
                if position >= capacity_of(groups.ctrl.len()) {
                    groups.rebuild(position + 1, hashes);
                }
                groups.place(hash, slot);
            }
        }
    }

    /// Makes room for `capacity` entries in all, of which `hashes` are recorded.
    ///
    /// A capacity of up to [`SMALL`] entries needs no group. A small map
    /// that reserves room for more moves to groups that record `hashes`, the
    /// hash of every entry the map holds, which the map hashes for the move.
    pub(super) fn reserve(&mut self, capacity: usize, hashes: &dyn Hashes) {
        match self {
            Self::Small => {
                if !small(capacity) {
                    *self = Self::Groups(Groups::recording(capacity, hashes));
                }
            }
            Self::Groups(groups) => {
                if capacity > capacity_of(groups.ctrl.len()) {
                    groups.rebuild(capacity, hashes);
                }
            }
        }
    }
}

impl Groups {
    /// Creates `groups` empty groups.
    fn sized(groups: usize) -> Self {
        Self {
            ctrl: vec![EMPTY; groups].into_boxed_slice(),
            positions: vec![[0; WIDTH]; groups].into_boxed_slice(),
        }
    }

    /// Creates the groups that take `capacity` entries and records the hashes `hashes` reads.
    ///
    /// A map moves to groups when it grows past [`SMALL`] entries or reserves
    /// room for more, never on a lookup, so the function stays out of line, as
    /// the growth of `hashbrown` does.
    #[cold]
    #[inline(never)]
    fn recording(capacity: usize, hashes: &dyn Hashes) -> Self {
        let mut groups = Self::sized(groups_for(capacity));
        groups.record(hashes);
        groups
    }

    /// Returns the position whose entry has `hash` and satisfies `eq`.
    ///
    /// The closure `eq` is asked only for the positions of slots whose tag
    /// matches, and the search ends at the first group that holds an empty
    /// slot. A position no slot records is not found (see [`beyond`]).
    #[inline]
    pub(super) fn find(&self, hash: usize, eq: impl FnMut(usize) -> bool) -> Option<usize> {
        self.find_slot(hash, eq).map(|(_, position)| position)
    }

    /// Returns the slot and the position whose entry has `hash` and satisfies `eq`.
    ///
    /// This is the probe of [`Groups::find`], which also names the slot.
    #[inline]
    pub(super) fn find_slot(
        &self,
        hash: usize,
        mut eq: impl FnMut(usize) -> bool,
    ) -> Option<(Slot, usize)> {
        let ctrls = &self.ctrl[..];
        let positions = &self.positions[..ctrls.len()];
        let mask = ctrls.len().wrapping_sub(1);
        let tag = tag(hash);
        let mut at = hash & mask;
        let mut stride = 0;
        loop {
            let ctrl = *ctrls.get(at)?;
            let mut found = match_tag(ctrl, tag);
            while found != 0 {
                let lane = lowest(found);
                let position = positions[at][lane] as usize;
                if eq(position) {
                    return Some((Slot { at, lane }, position));
                }
                core::hint::cold_path();
                found &= found - 1;
            }
            if match_empty(ctrl) != 0 {
                return None;
            }
            stride += 1;
            at = (at + stride) & mask;
        }
    }

    /// Empties `slot` and moves every position after the one it held one down.
    ///
    /// The map calls this after it took the entry of `slot` out of its vector,
    /// so `shifted` hands the hash of every entry that moved one position down
    /// and `hashes` the hash of every entry left, in their order. The slot
    /// turns empty when its group holds another empty slot, the rule by which
    /// `hashbrown` marks an erased slot empty. No probe passes such a group,
    /// because a probe passes a group only when it finds it full, and a group
    /// turns full only by an insertion and back only by [`Groups::rebuild`] or
    /// [`Groups::refill`]. When the group of the slot is full, a probe may
    /// pass it to reach a later key, and where `hashbrown` marks the slot
    /// deleted the groups record `hashes` again, so no probe ever reads a
    /// deleted marker.
    ///
    /// The positions after the slot move down one entry of `shifted` at a time
    /// when they are few, and by one pass over every slot beyond one moved
    /// entry in [`SWEEP`] slots, the choice `indexmap` 2.14.2 makes at half
    /// its capacity.
    pub(super) fn remove(
        &mut self,
        slot: Slot,
        shifted: impl ExactSizeIterator<Item = usize>,
        hashes: impl Iterator<Item = usize>,
    ) {
        if match_empty(self.ctrl[slot.at]) == 0 {
            self.refill(hashes);
            return;
        }
        self.ctrl[slot.at] |= 0xFF << (slot.lane * 8);
        let gone = self.positions[slot.at][slot.lane];
        if shifted.len() > self.ctrl.len() * WIDTH / SWEEP {
            for recorded in self.positions.as_flattened_mut() {
                *recorded -= u32::from(*recorded > gone);
            }
            return;
        }
        for (hash, lower) in shifted.zip(gone..) {
            let Some(held) = self.slot_of(hash, lower + 1) else {
                self.refill(hashes);
                return;
            };
            self.positions[held.at][held.lane] = lower;
        }
    }

    /// Drops every slot and records `hashes` again at their positions, keeping the count of groups.
    ///
    /// This is the rebuild of [`Groups::remove`], whose groups held one entry
    /// more and keep their count. A removal from a full group records every
    /// entry, which the rows of `shift_remove` time in
    /// `tests/notebooks/01_json_writer.ipynb`, section The cost of the
    /// insertion ordered map. A record through [`Hashes`] adds a call through
    /// the object and a copy for every [`CHUNK`] entries to that time, so this
    /// reads each hash in place from a generic iterator and is made for every
    /// type of key and value whose map removes a key. A growth records
    /// through [`Groups::rebuild`].
    // Erased storage would compile this once.
    #[cold]
    #[inline(never)]
    fn refill(&mut self, hashes: impl Iterator<Item = usize>) {
        self.ctrl.fill(EMPTY);
        for (position, hash) in hashes.enumerate() {
            let Ok(slot) = u32::try_from(position) else {
                return;
            };
            self.place(hash, slot);
        }
    }

    /// Returns the slot that holds `position` for `hash`.
    fn slot_of(&self, hash: usize, position: u32) -> Option<Slot> {
        self.find_slot(hash, |held| held == position as usize)
            .map(|(slot, _)| slot)
    }

    /// Drops every slot and records the hashes `hashes` reads again, at their positions.
    ///
    /// The groups keep their count, or grow to the count that takes `capacity`
    /// entries when they are fewer, and they record the first `2^32` of
    /// `hashes`. The growth of [`Table::insert`] and of [`Table::reserve`]
    /// rebuilds through this, and so does a removal from a map of more entries
    /// than the groups record (see [`beyond`]), never a lookup, so the
    /// function stays out of line, as the growth of `hashbrown` does. A
    /// removal from a full group records through [`Groups::refill`] instead.
    #[cold]
    #[inline(never)]
    pub(super) fn rebuild(&mut self, capacity: usize, hashes: &dyn Hashes) {
        let groups = groups_for(capacity).max(self.ctrl.len());
        if groups == self.ctrl.len() {
            self.ctrl.fill(EMPTY);
        } else {
            *self = Self::sized(groups);
        }
        self.record(hashes);
    }

    /// Records the hashes `hashes` reads at their positions, the first `2^32` of them.
    ///
    /// The hashes arrive a chunk of [`CHUNK`] at a time, and a chunk shorter
    /// than that is the last.
    fn record(&mut self, hashes: &dyn Hashes) {
        let mut chunk = [0; CHUNK];
        let mut from = 0;
        loop {
            let read = hashes.read(from, &mut chunk);
            for (&hash, position) in chunk.iter().take(read).zip(from..) {
                let Ok(slot) = u32::try_from(position) else {
                    return;
                };
                self.place(hash, slot);
            }
            if read < CHUNK {
                return;
            }
            from += CHUNK;
        }
    }

    /// Puts the entry at `position` with `hash` into the first empty slot of its probe.
    ///
    /// The groups always keep an empty slot, because their capacity is below
    /// their count of slots, so the probe ends.
    fn place(&mut self, hash: usize, position: u32) {
        let mask = self.ctrl.len() - 1;
        let mut at = hash & mask;
        let mut stride = 0;
        loop {
            let ctrl = &mut self.ctrl[at];
            let empty = match_empty(*ctrl);
            if empty != 0 {
                let lane = lowest(empty);
                let shift = lane * 8;
                *ctrl = (*ctrl & !(0xFF << shift)) | (Word::from(tag(hash)) << shift);
                self.positions[at][lane] = position;
                return;
            }
            stride += 1;
            at = (at + stride) & mask;
        }
    }
}

/// Returns whether a map of `len` entries is small, [`Table::Small`].
#[inline]
pub(super) const fn small(len: usize) -> bool {
    len <= SMALL
}

/// Returns whether a map of `len` entries holds entries the groups do not record.
///
/// Those are the entries past the first `2^32`, so the answer is `false` at
/// compile time on a target of 32 bits.
#[inline]
pub(super) const fn beyond(len: usize) -> bool {
    match UNRECORDED {
        Some(first) => len > first,
        None => false,
    }
}

/// Returns the tag of `hash`, its top seven bits.
// The shift leaves seven bits, so the cast drops only bits that are zero.
#[expect(clippy::cast_possible_truncation)]
#[inline]
const fn tag(hash: usize) -> u8 {
    (hash >> (usize::BITS - 7)) as u8
}

/// Returns a mask with the high bit set in every lane of `ctrl` that may equal `tag`.
///
/// A lane that differs from `tag` in its lowest bit alone can be reported as
/// well, but only above a lane that equals it, so the lowest lane reported
/// always equals it, and the comparison of the key discards the others.
// An `unsafe` SIMD group would match faster.
#[inline]
fn match_tag(ctrl: Word, tag: u8) -> Word {
    let cmp = ctrl ^ (LOW * Word::from(tag));
    cmp.wrapping_sub(LOW) & !cmp & HIGH
}

/// Returns a mask with the high bit set in every empty lane of `ctrl`.
///
/// A full slot holds a tag below `0x80`, so the high bit marks an empty
/// one alone.
#[inline]
const fn match_empty(ctrl: Word) -> Word {
    ctrl & HIGH
}

/// Returns the lane of the lowest bit set in `mask`.
///
/// The lane is below [`WIDTH`] for every mask, which the last operation
/// states to the compiler, so an index by it needs no bounds check.
#[inline]
const fn lowest(mask: Word) -> usize {
    (mask.trailing_zeros() / 8) as usize & (WIDTH - 1)
}

/// Returns the count of entries a table of `groups` groups takes.
///
/// A table of eight slots or fewer keeps one slot empty, and a larger one
/// keeps an eighth of its slots empty.
const fn capacity_of(groups: usize) -> usize {
    let buckets = groups * WIDTH;
    if buckets <= 8 {
        buckets.saturating_sub(1)
    } else {
        buckets / 8 * 7
    }
}

/// Returns the count of groups of the smallest table that takes `capacity` entries.
///
/// The count of slots is a power of two and at least one group, and a table
/// takes at most the positions a slot records. Every caller asks for at most
/// one entry more than the vector of entries holds or has reserved, and a
/// vector holds at most `isize::MAX` bytes of entries of at least a pointer
/// each. Eight times `capacity` therefore stays below `usize::MAX` on a target
/// of 64 bits and saturates at most on a target of 32 bits, where the power of
/// two that results still takes `capacity` entries.
fn groups_for(capacity: usize) -> usize {
    let capacity = UNRECORDED.map_or(capacity, |first| capacity.min(first));
    let buckets = if capacity < 8 {
        (capacity + 1).next_power_of_two()
    } else {
        (capacity.saturating_mul(8) / 7).next_power_of_two()
    };
    buckets.max(WIDTH) / WIDTH
}
