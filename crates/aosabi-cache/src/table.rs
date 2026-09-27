// SPDX-License-Identifier: MIT
/*
 * aosabi-cache table: the slab, the index and the lists of recency of the
 * request side cache.
 */

//! The table of documents and verdicts.
//!
//! The slab is one `Vec` of entries addressed by a `u32` slot, with a list of
//! free slots through it. An entry holds the hash of its text, its document,
//! the two links of its list, its use stamp, its estimated bytes, its segment
//! and its verdicts, the first inline and more in a `Vec`. The index is one
//! `Vec<u32>` of [`crate::SLOTS_PER_ENTRY`] slots per entry of the bound,
//! rounded up to a power of two, probed linearly over a window of
//! [`crate::WINDOW`] slots from the slot the low bits of a hash name, and an
//! entry leaves it by backward shift, so no tombstone lengthens a probe. A
//! counter of 64 bits rises by one at every insert and every hit, and the
//! entry takes its value as its use stamp, so the oldest entry of a window is
//! the one with the smallest stamp and no two entries share one. The counter
//! does not wrap within the life of a process, since 2^64 steps at one a
//! nanosecond take 584 years.
//!
//! The entries stand in two lists of recency by slot, probation and protected.
//! A new entry enters the head of probation. A hit in probation moves the
//! entry to the head of protected, and a hit in protected moves it to the
//! head. Protected weighs at most `PROTECTED_PERCENT`, 80 percent, of the
//! bound of bytes, and while it weighs more its tail moves to the head of
//! probation. An eviction for the bound of entries or of bytes takes the tail
//! of probation, and the tail of protected only where probation holds no entry
//! to take. An insert into a full window evicts the probation entry of that
//! window with the smallest stamp, and the protected entry with the smallest
//! stamp only where the window holds no probation entry, so a flood of new
//! texts takes out a document hit while it runs only where all the entries of
//! a window are protected. [`Counters::protected_window_evictions`] counts
//! those.
//!
//! The key of a document is its own text: a lookup compares the text of a
//! request with the text the document holds, through
//! [`DocumentHandle::holds_text`], so the table keeps no copy of any text. A
//! verdict is found by its rule list and by the schema it was validated
//! against, alive and the same object as the one the validation names, through
//! [`SchemaHandle`], as Yoga finds it in a `WeakMap` by schema.
//!
//! The record of the last parse names the address of the document a lookup hit
//! or an insert kept or refused, and the slot that holds it or none. A
//! validation reads a verdict only where its document has the address the
//! record names and the recorded slot still holds a document of that address,
//! so a plugin that replaces the document between the parse and the validation
//! meets no verdict of another document. Removing the entry the record names
//! clears its slot. [`Counters::replaced`] counts the validations of another
//! document and [`Counters::not_admitted`] those of a document the table does
//! not hold.
//!
//! What hangs on a kept document beside it, the fragment map and the field
//! plans of the executor, weighs on its entry as the document and its verdicts
//! do. The caller adds their bytes with [`Table::add_weight`] once an
//! execution builds them, and frees them with [`Table::free_weight`] once
//! their schema is gone, each time naming the entry by the hash of its text
//! and the address of its document, so the table finds the entry in the window
//! of the hash and compares no text. They leave with the entry, so an eviction
//! drops them with their document.
//!
//! Every slot the table reads is one it wrote, and every position of the
//! index is the caller's hash or a step after it under the mask, so every
//! index is in range.

use alloc::{
    vec,
    vec::Vec, //
};
use core::mem::size_of;

use crate::{
    CacheBudget,
    MAX_ENTRIES,
    PROTECTED_PERCENT,
    SEGMENTS,
    SLOTS_PER_ENTRY,
    WINDOW,
    rules::{
        RuleList,
        Rules, //
    }, //
};

/// No slot, the end of a list, the end of the free list and an empty slot of the index.
const NIL: u32 = u32::MAX;

/// The segment of an entry in probation, the list a new entry enters.
const PROBATION: u8 = 0;

/// The segment of an entry in protected, the list a hit in probation moves its entry to.
const PROTECTED: u8 = 1;

/// The handle of a parsed document a table keeps.
pub trait DocumentHandle {
    /// Returns the address of the document.
    ///
    /// No other document has it while a handle of the document lives.
    fn address(&self) -> usize;

    /// Answers whether the document was parsed from exactly the bytes of `text`.
    fn holds_text(&self, text: &str) -> bool;
}

/// The handle of a schema a verdict was validated against.
///
/// A verdict keeps a handle that keeps no schema alive, as Yoga keeps a
/// verdict in a `WeakMap` by its schema, and matches only while its schema
/// lives.
pub trait SchemaHandle {
    /// Returns the address of the schema, which no other schema shares while a handle of it lives.
    fn address(&self) -> usize;

    /// Answers whether the schema still lives.
    fn is_alive(&self) -> bool;
}

/// The counters of a table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// The lookups that found the text.
    pub hits: u64,
    /// The lookups that did not.
    pub misses: u64,
    /// The entries removed to make room, for the bounds or in a full window.
    pub evictions: u64,
    /// The entries of [`Counters::evictions`] removed from a full window.
    pub window_evictions: u64,
    /// The entries of [`Counters::window_evictions`] taken from protected.
    pub protected_window_evictions: u64,
    /// The entries and the verdicts not kept because they pass the bound of one entry.
    pub refusals: u64,
    /// The validations of a document other than the one the last parse answered.
    pub replaced: u64,
    /// The validations of the document of the last parse where the table holds no entry of it.
    pub not_admitted: u64,
    /// The most positions of the index one lookup read.
    pub most_slots_read: usize,
}

/// What a table holds for a validation.
#[derive(Debug)]
pub enum Kept<'a, V> {
    /// The payload of the verdict of the document under the rule list and the schema.
    Hit(&'a V),
    /// No verdict under the rule list and the schema.
    ///
    /// [`Table::add_verdict`] keeps the verdict once the validation answers.
    Miss(RuleList),
    /// No verdict is read or kept, so the validation runs uncached.
    Uncached,
}

/// A verdict of the validation of a document.
struct Verdict<S, V> {
    /// The list of the names of the rules.
    rules: RuleList,
    /// The schema the document was validated against.
    schema: S,
    /// What the validation answered.
    payload: V,
    /// The estimated bytes of the verdict.
    weight: usize,
}

impl<S: SchemaHandle, V> Verdict<S, V> {
    /// Answers whether the verdict holds for `rules` and the living schema at `schema`.
    fn matches(&self, rules: RuleList, schema: usize) -> bool {
        self.rules == rules && self.schema.address() == schema && self.schema.is_alive()
    }
}

/// One entry of the slab.
struct Entry<D, S, V> {
    /// The hash of the text of the document.
    hash: u64,
    /// The document, none for a free slot.
    document: Option<D>,
    /// The entry used just after it, or no slot.
    prev: u32,
    /// The entry used just before it, or the next free slot of a free slot.
    next: u32,
    /// The value of the counter of use at the last insert or hit of the entry.
    stamp: u64,
    /// The estimated bytes of the document and its verdicts.
    weight: usize,
    /// The list of recency the entry is in, [`PROBATION`] or [`PROTECTED`].
    segment: u8,
    /// The first verdict, the one verdict of a document validated under one key.
    first: Option<Verdict<S, V>>,
    /// The verdicts after the first.
    more: Vec<Verdict<S, V>>,
}

impl<D, S: SchemaHandle, V> Entry<D, S, V> {
    /// Returns the verdicts of the entry.
    fn verdicts(&self) -> impl Iterator<Item = &Verdict<S, V>> {
        self.first.iter().chain(self.more.iter())
    }

    /// Drops the verdicts whose schema no longer lives and answers their estimated bytes.
    fn prune(&mut self) -> usize {
        let mut freed = 0;
        self.more.retain(|verdict| {
            let alive = verdict.schema.is_alive();
            if !alive {
                freed += verdict.weight;
            }
            alive
        });
        if let Some(first) = self.first.take_if(|first| !first.schema.is_alive()) {
            freed += first.weight;
            self.first = self.more.pop();
        }
        freed
    }

    /// Keeps `verdict` beside the verdicts of the entry.
    fn push(&mut self, verdict: Verdict<S, V>) {
        if self.first.is_none() {
            self.first = Some(verdict);
        } else {
            self.more.push(verdict);
        }
    }
}

/// One list of recency, the most recent use at its head.
#[derive(Clone, Copy)]
struct List {
    /// The entry used last, or no slot.
    head: u32,
    /// The entry used longest ago, or no slot.
    tail: u32,
}

/// The record of the last parse.
#[derive(Clone, Copy)]
struct Record {
    /// The address of the document the parse answered.
    address: usize,
    /// The slot that holds it, or no slot where the table does not hold it.
    slot: u32,
}

/// The table of parsed documents and of the verdicts of their validations.
///
/// `D` is the handle of a document, `S` the handle of a schema and `V` the
/// payload of a verdict. The table hands out a borrow of a document or of a
/// payload, which the caller clones before it calls anything else, so no
/// borrow of the table outlives a hook.
pub struct Table<D, S, V> {
    /// The entries, free and kept.
    entries: Vec<Entry<D, S, V>>,
    /// The index, a slot of the slab or no slot in each position.
    index: Vec<u32>,
    /// The count of positions of the index less one, a mask of its low bits.
    mask: usize,
    /// The first free slot of the slab, or no slot.
    free: u32,
    /// The lists of recency, probation and protected.
    lists: [List; SEGMENTS],
    /// The entries kept.
    len: usize,
    /// The estimated bytes of the entries kept.
    used: usize,
    /// The estimated bytes of the entries in protected.
    protected: usize,
    /// The estimated bytes protected holds at most.
    protected_bound: usize,
    /// The counter of use, which rises at every insert and every hit.
    clock: u64,
    /// The bounds.
    budget: CacheBudget,
    /// The rule lists.
    rules: Rules,
    /// The record of the last parse.
    record: Option<Record>,
    /// The counters.
    counters: Counters,
}

/// Returns `slot` as an index of the slab.
///
/// Every target of the workspace has pointers of 32 bits or more, so the
/// widening is exact.
const fn widen(slot: u32) -> usize {
    slot as usize
}

/// Returns `at` as a slot of the slab, which [`MAX_ENTRIES`] keeps below [`NIL`].
fn narrow(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(NIL)
}

impl<D, S, V> Table<D, S, V> {
    /// The estimated bytes a table adds to the payload of every verdict it keeps.
    ///
    /// They are the bytes of its own record of the verdict.
    pub const VERDICT_BYTES: usize = size_of::<Verdict<S, V>>();

    /// Returns an empty table under `budget`.
    ///
    /// The table takes `budget.entries` between 1 and [`MAX_ENTRIES`] and
    /// `budget.entry_bytes` at most `budget.bytes`, and allocates its index
    /// at once, [`SLOTS_PER_ENTRY`] slots of 4 bytes for each entry rounded
    /// up to a power of two. Protected holds at most 80 percent of
    /// `budget.bytes`, rounded down.
    #[must_use]
    pub fn new(budget: CacheBudget) -> Self {
        let entries = budget.entries.clamp(1, MAX_ENTRIES);
        let budget = CacheBudget {
            entries,
            bytes: budget.bytes,
            entry_bytes: budget.entry_bytes.min(budget.bytes),
        };
        let positions = (entries * SLOTS_PER_ENTRY).next_power_of_two();
        let protected_bound =
            budget.bytes / 100 * PROTECTED_PERCENT + budget.bytes % 100 * PROTECTED_PERCENT / 100;
        Self {
            entries: Vec::new(),
            index: vec![NIL; positions],
            mask: positions - 1,
            free: NIL,
            lists: [List {
                head: NIL,
                tail: NIL,
            }; SEGMENTS],
            len: 0,
            used: 0,
            protected: 0,
            protected_bound,
            clock: 0,
            budget,
            rules: Rules::new(),
            record: None,
            counters: Counters::default(),
        }
    }

    /// Returns the bounds the table holds.
    #[must_use]
    pub const fn budget(&self) -> CacheBudget {
        self.budget
    }

    /// Returns the count of entries the table holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Answers whether the table holds no entry.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the estimated bytes of the entries the table holds.
    #[must_use]
    pub const fn used(&self) -> usize {
        self.used
    }

    /// Returns the counters of the table.
    #[must_use]
    pub const fn counters(&self) -> Counters {
        self.counters
    }

    /// Returns the position of the index the low bits of `hash` name.
    fn home(&self, hash: u64) -> usize {
        let [a, b, c, d, ..] = hash.to_le_bytes();
        widen(u32::from_le_bytes([a, b, c, d])) & self.mask
    }

    /// Returns the position after `at`, the first after the last.
    const fn after(&self, at: usize) -> usize {
        (at + 1) & self.mask
    }

    /// Takes the entry at `slot` out of its list.
    fn unlink(&mut self, slot: u32) {
        let entry = &self.entries[widen(slot)];
        let (prev, next, segment) = (entry.prev, entry.next, usize::from(entry.segment));
        if prev == NIL {
            self.lists[segment].head = next;
        } else {
            self.entries[widen(prev)].next = next;
        }
        if next == NIL {
            self.lists[segment].tail = prev;
        } else {
            self.entries[widen(next)].prev = prev;
        }
    }

    /// Puts the entry at `slot` at the head of its list.
    fn push_front(&mut self, slot: u32) {
        let segment = usize::from(self.entries[widen(slot)].segment);
        let head = self.lists[segment].head;
        let entry = &mut self.entries[widen(slot)];
        entry.prev = NIL;
        entry.next = head;
        if head == NIL {
            self.lists[segment].tail = slot;
        } else {
            self.entries[widen(head)].prev = slot;
        }
        self.lists[segment].head = slot;
    }

    /// Moves the tail of protected to the head of probation.
    ///
    /// It repeats while protected weighs more than its bound.
    fn balance(&mut self) {
        while self.protected > self.protected_bound {
            let slot = self.lists[usize::from(PROTECTED)].tail;
            if slot == NIL {
                return;
            }
            self.unlink(slot);
            let entry = &mut self.entries[widen(slot)];
            entry.segment = PROBATION;
            self.protected -= entry.weight;
            self.push_front(slot);
        }
    }

    /// Stamps the entry at `slot` of a hit and puts it at the head of protected.
    ///
    /// An entry of probation moves to protected, which then moves its tail
    /// back to probation while it weighs more than its bound.
    fn touch(&mut self, slot: u32) {
        self.clock += 1;
        let entry = &mut self.entries[widen(slot)];
        entry.stamp = self.clock;
        if entry.segment == PROBATION {
            self.unlink(slot);
            let entry = &mut self.entries[widen(slot)];
            entry.segment = PROTECTED;
            self.protected = self.protected.saturating_add(entry.weight);
            self.push_front(slot);
            self.balance();
        } else if self.lists[usize::from(PROTECTED)].head != slot {
            self.unlink(slot);
            self.push_front(slot);
        }
    }

    /// Takes the slot at position `hole` out of the index and shifts the entries after it back.
    ///
    /// An entry after the hole whose home lies at or before the hole moves
    /// into it, and the hole moves to where the entry stood, until an empty
    /// position ends the run, so every entry stays reachable from its home
    /// with no empty position between.
    fn shift_back(&mut self, mut hole: usize) {
        self.index[hole] = NIL;
        let mut at = hole;
        loop {
            at = self.after(at);
            let moved = self.index[at];
            if moved == NIL {
                return;
            }
            let start = self.home(self.entries[widen(moved)].hash);
            if (at.wrapping_sub(start) & self.mask) >= (at.wrapping_sub(hole) & self.mask) {
                self.index[hole] = moved;
                self.index[at] = NIL;
                hole = at;
            }
        }
    }

    /// Removes the entry at `slot` from its list, the index and the slab.
    fn remove(&mut self, slot: u32) {
        self.unlink(slot);
        let mut at = self.home(self.entries[widen(slot)].hash);
        for _ in 0..WINDOW {
            if self.index[at] == slot {
                self.shift_back(at);
                break;
            }
            at = self.after(at);
        }
        let free = self.free;
        let entry = &mut self.entries[widen(slot)];
        let weight = entry.weight;
        if entry.segment == PROTECTED {
            self.protected -= weight;
        }
        entry.document = None;
        entry.first = None;
        entry.more = Vec::new();
        entry.weight = 0;
        entry.prev = NIL;
        entry.next = free;
        self.free = slot;
        self.len -= 1;
        self.used -= weight;
        if let Some(record) = &mut self.record
            && record.slot == slot
        {
            record.slot = NIL;
        }
    }

    /// Returns the tail of probation other than the entry at `keep`.
    ///
    /// Without one it returns that of protected, or no slot.
    fn oldest_except(&self, keep: u32) -> u32 {
        for list in &self.lists {
            let mut slot = list.tail;
            if slot == keep && slot != NIL {
                slot = self.entries[widen(slot)].prev;
            }
            if slot != NIL {
                return slot;
            }
        }
        NIL
    }

    /// Evicts the entry [`Table::oldest_except`] names, answering whether it found one.
    fn evict_except(&mut self, keep: u32) -> bool {
        let slot = self.oldest_except(keep);
        if slot == NIL {
            return false;
        }
        self.remove(slot);
        self.counters.evictions += 1;
        true
    }

    /// Returns the first empty position of the window of `hash`, none where all of it is taken.
    fn free_in_window(&self, hash: u64) -> Option<usize> {
        let mut at = self.home(hash);
        for _ in 0..WINDOW {
            if self.index[at] == NIL {
                return Some(at);
            }
            at = self.after(at);
        }
        None
    }

    /// Evicts an entry of the full window of `hash`.
    ///
    /// It takes the probation entry of the window with the smallest stamp,
    /// and the protected entry with the smallest stamp where the window holds
    /// no probation entry.
    fn evict_in_window(&mut self, hash: u64) {
        let mut at = self.home(hash);
        let mut oldest = [NIL; SEGMENTS];
        let mut stamps = [u64::MAX; SEGMENTS];
        for _ in 0..WINDOW {
            let slot = self.index[at];
            let entry = &self.entries[widen(slot)];
            let segment = usize::from(entry.segment);
            if entry.stamp < stamps[segment] {
                stamps[segment] = entry.stamp;
                oldest[segment] = slot;
            }
            at = self.after(at);
        }
        let [probation, protected] = oldest;
        if probation == NIL {
            self.remove(protected);
            self.counters.protected_window_evictions += 1;
        } else {
            self.remove(probation);
        }
        self.counters.evictions += 1;
        self.counters.window_evictions += 1;
    }
}

impl<D: DocumentHandle, S: SchemaHandle, V> Table<D, S, V> {
    /// Returns the document parsed from `text`, whose hash is `hash`, where the table holds it.
    ///
    /// A lookup reads at most [`WINDOW`] positions of the index from the
    /// home of `hash`, compares the hash of each entry before its text, and
    /// stops at the first empty position. A hit stamps the entry, puts it at
    /// the head of protected, which moves the tail of protected back to
    /// probation while protected weighs more than its bound, and records the
    /// document as the one of the last parse. An entry that alone weighs more
    /// than that bound so goes back to the head of probation at once. A miss
    /// clears that record until [`Table::insert`] writes it.
    pub fn lookup(&mut self, hash: u64, text: &str) -> Option<&D> {
        let mut at = self.home(hash);
        let mut read = 0;
        let mut found = NIL;
        while read < WINDOW {
            let slot = self.index[at];
            read += 1;
            if slot == NIL {
                break;
            }
            let entry = &self.entries[widen(slot)];
            if entry.hash == hash
                && entry
                    .document
                    .as_ref()
                    .is_some_and(|document| document.holds_text(text))
            {
                found = slot;
                break;
            }
            at = self.after(at);
        }
        self.counters.most_slots_read = self.counters.most_slots_read.max(read);
        let address = self
            .entries
            .get(widen(found))
            .and_then(|entry| entry.document.as_ref())
            .map(DocumentHandle::address);
        let Some(address) = address else {
            self.counters.misses += 1;
            self.record = None;
            return None;
        };
        self.counters.hits += 1;
        self.touch(found);
        self.record = Some(Record {
            address,
            slot: found,
        });
        self.entries[widen(found)].document.as_ref()
    }

    /// Keeps `document` under `hash` with `weight` estimated bytes and answers whether it is kept.
    ///
    /// A document that weighs more than the bound of one entry is refused and
    /// counted, and the record of the last parse names it with no slot.
    /// Otherwise the insert first evicts the tails of probation, then of
    /// protected, until the bound of entries and the bound of bytes hold with
    /// the document, then, while the window of `hash` is full, the probation
    /// entry of that window with the smallest stamp, or the protected one
    /// where the window holds no probation entry. The backward shift of an
    /// eviction can move another entry into the window, so the eviction
    /// repeats, at most [`WINDOW`] times, since at most `2 * WINDOW - 1`
    /// entries can stand in a window and the `WINDOW - 1` positions after it
    /// and every round removes one of them. So no insert is refused for a full
    /// window. The document then takes the first empty position of its window,
    /// a free slot of the slab and the head of probation, and the record of
    /// the last parse names it.
    ///
    /// The caller inserts a document only after a lookup of its text missed,
    /// and a second entry of one text costs room and never a wrong hit.
    pub fn insert(&mut self, hash: u64, document: D, weight: usize) -> bool {
        let address = document.address();
        if weight > self.budget.entry_bytes {
            self.counters.refusals += 1;
            self.record = Some(Record { address, slot: NIL });
            return false;
        }
        while (self.len >= self.budget.entries
            || self.used.saturating_add(weight) > self.budget.bytes)
            && self.evict_except(NIL)
        {}
        let at = loop {
            if let Some(at) = self.free_in_window(hash) {
                break at;
            }
            self.evict_in_window(hash);
        };
        self.clock += 1;
        let entry = Entry {
            hash,
            document: Some(document),
            prev: NIL,
            next: NIL,
            stamp: self.clock,
            weight,
            segment: PROBATION,
            first: None,
            more: Vec::new(),
        };
        let slot = if self.free == NIL {
            self.entries.push(entry);
            narrow(self.entries.len() - 1)
        } else {
            let slot = self.free;
            self.free = self.entries[widen(slot)].next;
            self.entries[widen(slot)] = entry;
            slot
        };
        self.index[at] = slot;
        self.push_front(slot);
        self.len += 1;
        self.used = self.used.saturating_add(weight);
        self.record = Some(Record { address, slot });
        true
    }

    /// Returns the slot of the entry of the document at `address`, whose text hashes to `hash`, or no slot.
    ///
    /// The entry stands within [`WINDOW`] positions of the home of `hash`,
    /// so the search reads at most that many positions and compares no text.
    fn holding(&self, hash: u64, address: usize) -> u32 {
        let mut at = self.home(hash);
        for _ in 0..WINDOW {
            let slot = self.index[at];
            if slot == NIL {
                break;
            }
            let entry = &self.entries[widen(slot)];
            if entry.hash == hash
                && entry
                    .document
                    .as_ref()
                    .is_some_and(|document| document.address() == address)
            {
                return slot;
            }
            at = self.after(at);
        }
        NIL
    }

    /// Adds `weight` estimated bytes to the entry of the document at `address` and answers whether they are counted.
    ///
    /// The text of the document hashes to `hash`. The bytes are those of
    /// what hangs on the document beside it, the fragment map and the field
    /// plans of the executor. Bytes that would take the entry past the bound
    /// of one entry are refused and counted, and bytes for a document no
    /// entry holds are refused. An entry in protected then moves the tail of
    /// protected back to probation while protected weighs more than its
    /// bound, and the tails of probation, then of protected, other than its
    /// own entry leave until the bound of bytes holds, as after
    /// [`Table::add_verdict`].
    pub fn add_weight(&mut self, hash: u64, address: usize, weight: usize) -> bool {
        let slot = self.holding(hash, address);
        if slot == NIL {
            return false;
        }
        let entry = &mut self.entries[widen(slot)];
        if entry.weight.saturating_add(weight) > self.budget.entry_bytes {
            self.counters.refusals += 1;
            return false;
        }
        entry.weight += weight;
        let protected = entry.segment == PROTECTED;
        self.used = self.used.saturating_add(weight);
        if protected {
            self.protected = self.protected.saturating_add(weight);
            self.balance();
        }
        while self.used > self.budget.bytes && self.evict_except(slot) {}
        true
    }

    /// Takes `weight` estimated bytes off the entry of the document at `address`, whose text hashes to `hash`.
    ///
    /// The bytes are those [`Table::add_weight`] added for what hangs on the
    /// document and what frees them, a plan whose schema is gone. An entry
    /// keeps at least nothing, and a document no entry holds frees nothing.
    pub fn free_weight(&mut self, hash: u64, address: usize, weight: usize) {
        let slot = self.holding(hash, address);
        if slot == NIL {
            return;
        }
        let entry = &mut self.entries[widen(slot)];
        let weight = weight.min(entry.weight);
        entry.weight -= weight;
        self.used -= weight;
        if entry.segment == PROTECTED {
            self.protected -= weight;
        }
    }

    /// Returns the recorded slot of the document at `address`, where that slot holds it.
    ///
    /// With `count` the validation is counted as one of a replaced document
    /// where the record names another address or none, and as one of a
    /// document not admitted where it names no slot.
    fn recorded(&mut self, address: usize, count: bool) -> Option<u32> {
        let record = self.record.filter(|record| record.address == address);
        let Some(record) = record else {
            self.counters.replaced += u64::from(count);
            return None;
        };
        let holds = self
            .entries
            .get(widen(record.slot))
            .and_then(|entry| entry.document.as_ref())
            .is_some_and(|document| document.address() == address);
        if !holds {
            self.counters.not_admitted += u64::from(count);
            return None;
        }
        Some(record.slot)
    }

    /// Returns what the table holds for a validation of the document at `address`.
    ///
    /// The validation runs the rules `names` against the schema at `schema`.
    /// It reads a verdict only where the record of the last
    /// parse names the document at `address` with a slot that still holds
    /// it, and counts it otherwise as one of a replaced document or of a
    /// document not admitted, and it runs uncached under a rule list the
    /// table has no room for. A verdict matches where its rule list holds
    /// the same names in the same order and its schema lives and lies at
    /// `schema`.
    pub fn verdict<I>(&mut self, address: usize, names: I, schema: usize) -> Kept<'_, V>
    where
        I: IntoIterator<Item = &'static str>,
        I::IntoIter: ExactSizeIterator + Clone,
    {
        let Some(slot) = self.recorded(address, true) else {
            return Kept::Uncached;
        };
        let Some(rules) = self.rules.intern(names) else {
            return Kept::Uncached;
        };
        match self.entries[widen(slot)]
            .verdicts()
            .find(|verdict| verdict.matches(rules, schema))
        {
            Some(verdict) => Kept::Hit(&verdict.payload),
            None => Kept::Miss(rules),
        }
    }

    /// Keeps `payload` as a verdict of the document at `address` and answers whether it is kept.
    ///
    /// The verdict holds for `rules` and `schema` and weighs `weight`
    /// estimated bytes.
    ///
    /// The verdict is kept only where the record of the last parse still
    /// names the document at `address` with a slot that holds it. The
    /// verdicts of the entry whose schema no longer lives leave first. A
    /// verdict the entry already holds under the same rule list and schema
    /// stays, and the new one is not kept. A verdict that would take its
    /// entry past the bound of one entry is refused and counted. A kept
    /// verdict adds [`Table::VERDICT_BYTES`] to `weight`. An entry in
    /// protected then moves the tail of protected back to probation while
    /// protected weighs more than its bound, and the tails of probation, then
    /// of protected, other than its own entry leave until the bound of bytes
    /// holds.
    pub fn add_verdict(
        &mut self,
        address: usize,
        rules: RuleList,
        schema: S,
        payload: V,
        weight: usize,
    ) -> bool {
        let Some(slot) = self.recorded(address, false) else {
            return false;
        };
        let weight = weight.saturating_add(Self::VERDICT_BYTES);
        let entry = &mut self.entries[widen(slot)];
        let freed = entry.prune();
        entry.weight -= freed;
        let protected = entry.segment == PROTECTED;
        if protected {
            self.protected -= freed;
        }
        self.used -= freed;
        let held = schema.address();
        if entry.verdicts().any(|verdict| verdict.matches(rules, held)) {
            return false;
        }
        if entry.weight.saturating_add(weight) > self.budget.entry_bytes {
            self.counters.refusals += 1;
            return false;
        }
        entry.push(Verdict {
            rules,
            schema,
            payload,
            weight,
        });
        entry.weight += weight;
        self.used = self.used.saturating_add(weight);
        if protected {
            self.protected = self.protected.saturating_add(weight);
            self.balance();
        }
        while self.used > self.budget.bytes && self.evict_except(slot) {}
        true
    }
}
