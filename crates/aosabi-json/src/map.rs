// SPDX-License-Identifier: MIT
/*
 * aosabi-json map: the insertion ordered map and set every crate of Aosabi carries.
 *
 * Keeps the semantics of indexmap 2.14.2 src/map.rs and src/set.rs,
 * https://docs.rs/crate/indexmap/2.14.2/source/, for the part of its
 * interface Aosabi uses. No code of indexmap is carried. The index table
 * under map/table.rs translates the generic group of hashbrown 0.17.1, and
 * its header names the ranges.
 */

//! The insertion ordered map and set of Aosabi.
//!
//! An [`IndexMap`] keeps its entries in a vector in the order their keys were
//! first inserted. A map that has held no more than `SMALL` entries, twelve,
//! the count the module `table` sets on every target, and reserved room for
//! no more finds a key by comparing it with the keys of its entries in their
//! order, so it hashes no key and allocates nothing beside its vector. A map
//! that grows past that count, or reserves room for more, hashes every key it
//! holds once, keeps the hash of each entry beside it from then on, and finds
//! a key through groups of positions into that vector, probed a group of
//! slots at a time, which it keeps whatever its count of entries, as the
//! module `table` states. Iteration walks the vector, so the order never
//! depends on a hash, which is what the byte contract needs of every object
//! and every keyed collection.
//!
//! The semantics are those of `indexmap` 2.14.2. Inserting a key that exists
//! keeps the key and its position and replaces the value, a new key goes
//! last, and removing a key shifts the entries after it one position down,
//! keeping their relative order. Two maps are equal when they hold the same
//! keys with equal values in any order. The index table holds no deleted
//! marker, so a lookup never passes one. The objects of an answer gain keys
//! and lose none, so a removal is off the path of an answer. A removal from a
//! small map shifts its entries alone, and in groups it empties the slot of
//! its key where no probe passes it and records every entry again from the
//! hashes the entries keep where one may, as the module `table` states. The
//! map answers 100000 operations of `insert`, `get`, `shift_remove`, `entry`
//! with `or_insert`, `contains_key`, `get_index` and `get_index_of` the way a
//! Python `dict` answers them, and ends with the same entries in the same
//! order, with the fixed hasher, with the keyed one and with one under which
//! the keys of one length share their hash, in each of three replays. The
//! first grows a map to hundreds of entries. The second grows a map past
//! `SMALL` entries and shrinks it back hundreds of times, which moves the map
//! to groups in its first few dozen operations and after that grows and
//! shrinks the groups. The third starts a fresh map every 64 operations, so a
//! small map takes thousands of removals and tens of thousands of lookups
//! after them, on the host and under the feature `narrow-group`.
//!
//! Every map carries [`KeyHasherBuilder`] unless it names another builder.
//! That builder hashes with `SipHash-1-3` in every build, as the
//! `RandomState` of the standard library where this crate reaches a source
//! of entropy through it, and elsewhere as [`crate::value::KeyState`], keyed
//! by the platform or with the fixed key, and `crate::value` states which
//! build is which. A lookup is bound on [`Borrow`], so a map keyed by
//! `String` is asked with a `&str`.
//!
//! The module is `no_std` with `alloc`, uses no atomic operation and no
//! `unsafe` of its own, and neither does the builder
//! [`crate::value::KeyState`]. It builds for `riscv32imc-unknown-none-elf`,
//! the target of the ESP32-C3, which has no atomic compare and swap.

mod set;
mod table;

use alloc::vec::{
    self,
    Vec, //
};
use core::{
    borrow::Borrow,
    fmt,
    hash::{
        BuildHasher,
        Hash, //
    },
    slice,
};

pub use self::set::IndexSet;
use self::table::{
    Hashes,
    Table,
    beyond,
    small, //
};
use crate::value::KeyHasherBuilder;

/// One entry of a map: the hash of its key, the key and the value.
#[derive(Clone)]
struct Bucket<K, V> {
    /// The hash of `key`, as wide as a pointer, which an entry of a small map
    /// takes only when its map moves to groups and holds no hash before.
    hash: usize,
    /// The key.
    key: K,
    /// The value.
    value: V,
}

/// A map that iterates its entries in the order their keys were first inserted.
///
/// The type `S` builds the hasher of the keys, and it is
/// [`KeyHasherBuilder`] unless a map names another. The module documentation
/// states the semantics.
pub struct IndexMap<K, V, S = KeyHasherBuilder> {
    /// The entries in insertion order.
    entries: Vec<Bucket<K, V>>,
    /// The positions of the entries, by hash.
    table: Table,
    /// The builder of the hasher of every key.
    hasher: S,
}

impl<K, V, S> IndexMap<K, V, S> {
    /// Creates an empty map with room for `capacity` entries and `hasher`.
    ///
    /// # Panics
    ///
    /// Panics when `capacity` entries or the index table for them take more
    /// than `isize::MAX` bytes, as [`Vec::with_capacity`] does.
    #[must_use]
    pub fn with_capacity_and_hasher(capacity: usize, hasher: S) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            table: Table::with_capacity(capacity),
            hasher,
        }
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the map has no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the key and the value at position `index` of the insertion order.
    #[must_use]
    pub fn get_index(&self, index: usize) -> Option<(&K, &V)> {
        self.entries
            .get(index)
            .map(|bucket| (&bucket.key, &bucket.value))
    }

    /// Iterates the entries in insertion order.
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            entries: self.entries.iter(),
        }
    }

    /// Iterates the keys in insertion order.
    pub fn keys(&self) -> Keys<'_, K, V> {
        Keys {
            entries: self.entries.iter(),
        }
    }

    /// Iterates the values in insertion order.
    pub fn values(&self) -> Values<'_, K, V> {
        Values {
            entries: self.entries.iter(),
        }
    }

    /// Iterates the values in insertion order, each one mutable.
    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        ValuesMut {
            entries: self.entries.iter_mut(),
        }
    }

    /// Consumes the map and iterates its keys in insertion order.
    pub fn into_keys(self) -> IntoKeys<K, V> {
        IntoKeys {
            entries: self.entries.into_iter(),
        }
    }

    /// Returns a map of the same keys in the same order, whose values `make`
    /// answers from the values of this map in their order.
    ///
    /// Each entry keeps the hash of its key where it holds one, and the new
    /// map copies the index table and the builder of the hasher, so no key
    /// is hashed again.
    ///
    /// # Errors
    ///
    /// Returns the first error `make` answers, which stops the walk.
    pub fn try_map_values<W, E>(
        &self,
        mut make: impl FnMut(&V) -> Result<W, E>,
    ) -> Result<IndexMap<K, W, S>, E>
    where
        K: Clone,
        S: Clone,
    {
        let mut entries = Vec::with_capacity(self.entries.len());
        for bucket in &self.entries {
            entries.push(Bucket {
                hash: bucket.hash,
                key: bucket.key.clone(),
                value: make(&bucket.value)?,
            });
        }
        Ok(IndexMap {
            entries,
            table: self.table.clone(),
            hasher: self.hasher.clone(),
        })
    }
}

impl<K, V, S> IndexMap<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    /// Returns the hash of `key`, as wide as a pointer.
    #[inline]
    fn hash<Q: ?Sized + Hash>(&self, key: &Q) -> usize {
        hash_with(&self.hasher, key)
    }

    /// Gives every entry the hash of its key, before a small map moves to groups.
    ///
    /// An entry of a small map holds no hash, since such a map compares
    /// keys, and the groups record the hash of every entry, so the move
    /// hashes each key the map holds once.
    #[cold]
    #[inline(never)]
    fn hash_entries(&mut self) {
        let hasher = &self.hasher;
        for bucket in &mut self.entries {
            bucket.hash = hash_with(hasher, &bucket.key);
        }
    }

    /// Returns the hash a new entry of `key` takes and the position of `key` where the map holds it.
    ///
    /// A small map compares `key` with the key of each of its entries and
    /// hashes none, and a new entry of it takes no hash, except the entry
    /// that moves the map to groups, before which every entry takes the hash
    /// of its key. A map with groups hashes `key` and probes for it.
    // The hash of a map with groups is the one hash of this function, so the
    // compiler inlines the write of the keyed hasher into the loop of the
    // caller, as it does for a lookup, while the move of a small map, which
    // hashes as well, stays out of line in `leave_small`.
    #[inline]
    fn locate(&mut self, key: &K) -> (usize, Option<usize>) {
        match self.table {
            Table::Small => self.locate_small(key),
            Table::Groups(_) => {
                let hash = self.hash(key);
                (hash, self.position_of(hash, key))
            }
        }
    }

    /// Returns the hash a new entry of `key` takes in a small map and the position of `key` where the map holds it.
    fn locate_small(&mut self, key: &K) -> (usize, Option<usize>) {
        if let Some(position) = find(&self.entries, key) {
            return (0, Some(position));
        }
        if small(self.entries.len() + 1) {
            return (0, None);
        }
        (self.leave_small(key), None)
    }

    /// Gives every entry the hash of its key and returns the hash of `key`, whose entry moves a small map to groups.
    ///
    /// The move runs once per map, so the hashes stay out of the function
    /// of the caller, whose one hash is the hash of a map with groups.
    #[cold]
    #[inline(never)]
    fn leave_small(&mut self, key: &K) -> usize {
        self.hash_entries();
        self.hash(key)
    }

    /// Returns the position of `key` in the insertion order.
    ///
    /// A small map compares `key` with the key of each of its entries in
    /// their order and hashes none, as `indexmap` 2.14.2 does for a map of
    /// one entry. Groups probe for the hash of `key`, and a map of more
    /// entries than the groups record, `2^32`, compares every entry.
    // Always inlined, so the hash of the key and the probe land in the loop
    // of the caller, where the compiler inlines the write of the keyed
    // hasher as well. A call to this function keeps that write a call of
    // its own.
    #[expect(clippy::inline_always)]
    #[inline(always)]
    pub fn get_index_of<Q>(&self, key: &Q) -> Option<usize>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let entries = self.entries.as_slice();
        match &self.table {
            Table::Groups(groups) if !beyond(entries.len()) => groups
                .find(self.hash(key), |position| {
                    entries[position].key.borrow() == key
                }),
            Table::Groups(_) => scan(entries, key),
            Table::Small => find(entries, key),
        }
    }

    /// Returns the position of `key`, whose hash is `hash`.
    ///
    /// Groups probe for `hash`, a map of more entries than the groups record
    /// compares every entry (see [`beyond`]), and a small map, whose `hash`
    /// is no hash, compares the key of each of its entries.
    #[inline]
    fn position_of<Q>(&self, hash: usize, key: &Q) -> Option<usize>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq,
    {
        let entries = self.entries.as_slice();
        match &self.table {
            Table::Groups(groups) if !beyond(entries.len()) => {
                groups.find(hash, |position| entries[position].key.borrow() == key)
            }
            Table::Groups(_) => scan(entries, key),
            Table::Small => find(entries, key),
        }
    }

    /// Returns the value of `key`.
    // Always inlined, as `get_index_of` is, since a call to this function
    // keeps the lookup out of the loop of its caller.
    #[expect(clippy::inline_always)]
    #[inline(always)]
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let position = self.get_index_of(key)?;
        Some(&self.entries[position].value)
    }

    /// Returns the value of `key` for writing.
    // Always inlined, as `get_index_of` is, since a call to this function
    // keeps the lookup out of the loop of its caller.
    #[expect(clippy::inline_always)]
    #[inline(always)]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let position = self.get_index_of(key)?;
        Some(&mut self.entries[position].value)
    }

    /// Returns whether the map holds `key`.
    // Always inlined, as `get_index_of` is, since a call to this function
    // keeps the lookup out of the loop of its caller.
    #[expect(clippy::inline_always)]
    #[inline(always)]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.get_index_of(key).is_some()
    }

    /// Sets the value of `key`, keeping the key and the position of one that exists.
    ///
    /// The previous value of an existing key is returned, and a new key goes
    /// last.
    ///
    /// # Panics
    ///
    /// Panics when a new key grows the entries or the index table past
    /// `isize::MAX` bytes, as [`Vec::push`] does.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.insert_full(key, value).1
    }

    /// Sets the value of `key` and returns its position with the previous value.
    fn insert_full(&mut self, key: K, value: V) -> (usize, Option<V>) {
        let (hash, found) = self.locate(&key);
        match found {
            Some(position) => (
                position,
                Some(core::mem::replace(&mut self.entries[position].value, value)),
            ),
            None => (
                push(&mut self.entries, &mut self.table, hash, key, value),
                None,
            ),
        }
    }

    /// Returns the entry of `key`, occupied or vacant, for an insertion in place.
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        let (hash, found) = self.locate(&key);
        match found {
            Some(position) => Entry::Occupied(OccupiedEntry {
                entries: &mut self.entries,
                position,
            }),
            None => Entry::Vacant(VacantEntry {
                entries: &mut self.entries,
                table: &mut self.table,
                hash,
                key,
            }),
        }
    }

    /// Removes `key` and returns its value, shifting the entries after it.
    ///
    /// The entries after `key` keep their relative order and move one
    /// position down, which is the `delete` of a JavaScript object and costs
    /// time in proportion to the size of the map.
    pub fn shift_remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let (slot, position) = match &self.table {
            Table::Small => {
                let position = find(&self.entries, key)?;
                return Some(self.entries.remove(position).value);
            }
            Table::Groups(_) if beyond(self.entries.len()) => {
                return self.shift_remove_beyond(key);
            }
            Table::Groups(groups) => {
                let entries = &self.entries;
                groups.find_slot(self.hash(key), |position| {
                    entries[position].key.borrow() == key
                })?
            }
        };
        let removed = self.entries.remove(position);
        if let Table::Groups(groups) = &mut self.table {
            let shifted = self.entries[position..].iter().map(|bucket| bucket.hash);
            let hashes = self.entries.iter().map(|bucket| bucket.hash);
            groups.remove(slot, shifted, hashes);
        }
        Some(removed.value)
    }

    /// Removes `key` from a map of more entries than its groups record.
    ///
    /// The removal moves an entry the groups do not record into the
    /// positions they record, so the groups record every hash again.
    #[cold]
    #[inline(never)]
    fn shift_remove_beyond<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let position = self.get_index_of(key)?;
        let removed = self.entries.remove(position);
        if let Table::Groups(groups) = &mut self.table {
            groups.rebuild(self.entries.len(), &self.entries);
        }
        Some(removed.value)
    }
}

/// Returns the position of the entry whose key equals `key`, by comparing every entry.
///
/// This is the lookup of a map of more entries than its groups record (see
/// [`beyond`]), which is slow and which no map of Aosabi reaches, so it
/// stays out of the way of the probe.
#[cold]
#[inline(never)]
fn scan<K, V, Q>(entries: &[Bucket<K, V>], key: &Q) -> Option<usize>
where
    K: Borrow<Q>,
    Q: ?Sized + Eq,
{
    entries.iter().position(|bucket| bucket.key.borrow() == key)
}

/// Returns the position of the entry of a small map whose key equals `key`, comparing the keys in their order.
///
/// A small map holds at most `SMALL` entries, twelve, so the lookup compares
/// at most twelve keys, most of which differ from `key` in their length or
/// their first bytes, and hashes none.
// Always inlined, as the lookups of the map that call it are, so the
// comparison lands in the loop of the caller.
#[expect(clippy::inline_always)]
#[inline(always)]
fn find<K, V, Q>(entries: &[Bucket<K, V>], key: &Q) -> Option<usize>
where
    K: Borrow<Q>,
    Q: ?Sized + Eq,
{
    entries.iter().position(|bucket| bucket.key.borrow() == key)
}

/// Returns the hash `hasher` gives `key`, as wide as a pointer.
// A pointer wide hash is what the index table masks and tags, as it is for
// indexmap 2.14.2, so the cast drops the high half on a 32 bit target on
// purpose.
#[expect(clippy::cast_possible_truncation)]
#[inline]
fn hash_with<S: BuildHasher, Q: ?Sized + Hash>(hasher: &S, key: &Q) -> usize {
    hasher.hash_one(key) as usize
}

/// Appends a new entry and records it in `table`, and returns its position.
fn push<K, V>(
    entries: &mut Vec<Bucket<K, V>>,
    table: &mut Table,
    hash: usize,
    key: K,
    value: V,
) -> usize {
    let position = entries.len();
    table.insert(hash, position, &*entries);
    entries.push(Bucket { hash, key, value });
    position
}

/// The entries of a map hand the index table the hash each of them keeps.
impl<K, V> Hashes for Vec<Bucket<K, V>> {
    fn read(&self, from: usize, into: &mut [usize]) -> usize {
        let entries = self.get(from..).unwrap_or_default();
        for (hash, bucket) in into.iter_mut().zip(entries) {
            *hash = bucket.hash;
        }
        into.len().min(entries.len())
    }
}

impl<K, V, S: Default> Default for IndexMap<K, V, S> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            table: Table::new(),
            hasher: S::default(),
        }
    }
}

impl<K: Clone, V: Clone, S: Clone> Clone for IndexMap<K, V, S> {
    /// Copies the entries, the index table and the builder of the hasher.
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            table: self.table.clone(),
            hasher: self.hasher.clone(),
        }
    }
}

impl<K: fmt::Debug, V: fmt::Debug, S> fmt::Debug for IndexMap<K, V, S> {
    /// Writes the entries as a map in insertion order, the form of indexmap
    /// 2.14.2.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K, V1, S1, V2, S2> PartialEq<IndexMap<K, V2, S2>> for IndexMap<K, V1, S1>
where
    K: Hash + Eq,
    V1: PartialEq<V2>,
    S1: BuildHasher,
    S2: BuildHasher,
{
    /// Tells whether both maps hold the same keys with equal values, in any order.
    fn eq(&self, other: &IndexMap<K, V2, S2>) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .all(|(key, value)| other.get(key).is_some_and(|theirs| *value == *theirs))
    }
}

impl<K, V, S> FromIterator<(K, V)> for IndexMap<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher + Default,
{
    /// Collects the pairs into an empty map, as its `extend` inserts them.
    ///
    /// # Panics
    ///
    /// Panics under the conditions of the `extend` of the map.
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = Self::default();
        map.extend(iter);
        map
    }
}

impl<K, V, S> Extend<(K, V)> for IndexMap<K, V, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    /// Inserts every pair in turn, so a repeated key keeps its first
    /// position and its last value.
    ///
    /// # Panics
    ///
    /// Panics when the lower bound of the size hint of `iter`, or the pairs
    /// it yields, grow the entries or the index table past `isize::MAX`
    /// bytes, as [`Vec::reserve`] and [`Vec::push`] do.
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        let wanted = self.len().saturating_add(iter.size_hint().0);
        self.entries.reserve(iter.size_hint().0);
        if let Table::Small = self.table
            && !small(wanted)
        {
            self.hash_entries();
        }
        self.table.reserve(wanted, &self.entries);
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl<'a, K, V, S> IntoIterator for &'a IndexMap<K, V, S> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<K, V, S> IntoIterator for IndexMap<K, V, S> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            entries: self.entries.into_iter(),
        }
    }
}

/// The entry of one key of a map, returned by [`IndexMap::entry`].
pub enum Entry<'a, K, V> {
    /// The map holds the key.
    Occupied(OccupiedEntry<'a, K, V>),
    /// The map does not hold the key.
    Vacant(VacantEntry<'a, K, V>),
}

/// The entry of a key a map holds.
pub struct OccupiedEntry<'a, K, V> {
    /// The entries of the map.
    entries: &'a mut Vec<Bucket<K, V>>,
    /// The position of the key.
    position: usize,
}

/// The entry of a key a map does not hold, with the key and its hash.
pub struct VacantEntry<'a, K, V> {
    /// The entries of the map.
    entries: &'a mut Vec<Bucket<K, V>>,
    /// The index table of the map.
    table: &'a mut Table,
    /// The hash of `key`.
    hash: usize,
    /// The key.
    key: K,
}

impl<'a, K, V> Entry<'a, K, V> {
    /// Returns the value of the key, inserting `value` last when the map
    /// does not hold it.
    ///
    /// # Panics
    ///
    /// Panics under the conditions of [`IndexMap::insert`].
    pub fn or_insert(self, value: V) -> &'a mut V {
        self.or_insert_with(|| value)
    }

    /// Returns the value of the key, inserting the value `make` returns
    /// last when the map does not hold it.
    ///
    /// # Panics
    ///
    /// Panics under the conditions of [`IndexMap::insert`].
    pub fn or_insert_with<F: FnOnce() -> V>(self, make: F) -> &'a mut V {
        match self {
            Self::Occupied(entry) => &mut entry.entries[entry.position].value,
            Self::Vacant(VacantEntry {
                entries,
                table,
                hash,
                key,
            }) => {
                let position = push(entries, table, hash, key, make());
                &mut entries[position].value
            }
        }
    }

    /// Returns the value of the key, inserting the default value last when
    /// the map does not hold it.
    ///
    /// # Panics
    ///
    /// Panics under the conditions of [`IndexMap::insert`].
    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
}

/// The iterator of [`IndexMap::iter`], over the entries in insertion order.
pub struct Iter<'a, K, V> {
    /// The entries left.
    entries: slice::Iter<'a, Bucket<K, V>>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.entries
            .next()
            .map(|bucket| (&bucket.key, &bucket.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}

/// The iterator of [`IndexMap::keys`], over the keys in insertion order.
pub struct Keys<'a, K, V> {
    /// The entries left.
    entries: slice::Iter<'a, Bucket<K, V>>,
}

impl<'a, K, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(|bucket| &bucket.key)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}

/// The iterator of [`IndexMap::values`], over the values in insertion order.
pub struct Values<'a, K, V> {
    /// The entries left.
    entries: slice::Iter<'a, Bucket<K, V>>,
}

impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(|bucket| &bucket.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}

/// The iterator of [`IndexMap::values_mut`], over the values in insertion
/// order, each one mutable.
pub struct ValuesMut<'a, K, V> {
    /// The entries left.
    entries: slice::IterMut<'a, Bucket<K, V>>,
}

impl<'a, K, V> Iterator for ValuesMut<'a, K, V> {
    type Item = &'a mut V;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(|bucket| &mut bucket.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}

/// The iterator of an owned map, over its entries in insertion order.
pub struct IntoIter<K, V> {
    /// The entries left.
    entries: vec::IntoIter<Bucket<K, V>>,
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(|bucket| (bucket.key, bucket.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}

/// The iterator of [`IndexMap::into_keys`], over the keys in insertion order.
pub struct IntoKeys<K, V> {
    /// The entries left.
    entries: vec::IntoIter<Bucket<K, V>>,
}

impl<K, V> Iterator for IntoKeys<K, V> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(|bucket| bucket.key)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}
