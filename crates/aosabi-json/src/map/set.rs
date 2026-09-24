// SPDX-License-Identifier: MIT
/*
 * aosabi-json map set: the insertion ordered set, a map whose values are empty.
 *
 * Keeps the semantics of the set of indexmap 2.14.2 src/set.rs,
 * https://docs.rs/crate/indexmap/2.14.2/source/, for the part of its
 * interface Aosabi uses.
 */

//! The insertion ordered set of Aosabi.
//!
//! An [`IndexSet`] is an [`IndexMap`] whose values are `()`. Inserting an
//! item the set holds keeps the item and its position, and removing one
//! shifts the items after it, as the set of indexmap 2.14.2 does.

use core::{
    borrow::Borrow,
    fmt,
    hash::{
        BuildHasher,
        Hash, //
    },
};

use super::{
    IndexMap,
    IntoKeys,
    Keys, //
};
use crate::value::KeyHasherBuilder;

/// A set that iterates its items in the order they were first inserted.
///
/// The type `S` builds the hasher of the items, and it is
/// [`KeyHasherBuilder`] unless a set names another.
pub struct IndexSet<T, S = KeyHasherBuilder> {
    /// The items, as the keys of a map.
    map: IndexMap<T, (), S>,
}

impl<T, S> IndexSet<T, S> {
    /// Returns the item at position `index` of the insertion order.
    #[must_use]
    pub fn get_index(&self, index: usize) -> Option<&T> {
        self.map.get_index(index).map(|(item, ())| item)
    }

    /// Iterates the items in insertion order.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            keys: self.map.keys(),
        }
    }
}

impl<T, S> IndexSet<T, S>
where
    T: Hash + Eq,
    S: BuildHasher,
{
    /// Adds `item` last and tells whether the set did not hold it.
    ///
    /// An item the set holds stays where it is, and `item` is dropped.
    ///
    /// # Panics
    ///
    /// Panics under the conditions of [`IndexMap::insert`].
    pub fn insert(&mut self, item: T) -> bool {
        self.map.insert(item, ()).is_none()
    }

    /// Adds `item` last and returns its position, with whether the set did not hold it.
    ///
    /// An item the set holds stays where it is, and its position is
    /// returned.
    ///
    /// # Panics
    ///
    /// Panics under the conditions of [`IndexMap::insert`].
    pub fn insert_full(&mut self, item: T) -> (usize, bool) {
        let (position, previous) = self.map.insert_full(item, ());
        (position, previous.is_none())
    }

    /// Returns whether the set holds `item`.
    pub fn contains<Q>(&self, item: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.contains_key(item)
    }

    /// Removes `item`, shifting the items after it, and tells whether the set held it.
    pub fn shift_remove<Q>(&mut self, item: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.shift_remove(item).is_some()
    }
}

impl<T, S: Default> Default for IndexSet<T, S> {
    fn default() -> Self {
        Self {
            map: IndexMap::default(),
        }
    }
}

impl<T: Clone, S: Clone> Clone for IndexSet<T, S> {
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
        }
    }
}

impl<T: fmt::Debug, S> fmt::Debug for IndexSet<T, S> {
    /// Writes the items as a set in insertion order, the form of indexmap
    /// 2.14.2.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl<T, S> FromIterator<T> for IndexSet<T, S>
where
    T: Hash + Eq,
    S: BuildHasher + Default,
{
    /// Collects the items into an empty set, as its `extend` inserts them.
    ///
    /// # Panics
    ///
    /// Panics under the conditions of the `extend` of [`IndexMap`].
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            map: iter.into_iter().map(|item| (item, ())).collect(),
        }
    }
}

impl<T, S> Extend<T> for IndexSet<T, S>
where
    T: Hash + Eq,
    S: BuildHasher,
{
    /// Inserts every item in turn, so an item the set holds keeps its position.
    ///
    /// # Panics
    ///
    /// Panics under the conditions of the `extend` of [`IndexMap`].
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.map.extend(iter.into_iter().map(|item| (item, ())));
    }
}

impl<'a, T, S> IntoIterator for &'a IndexSet<T, S> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T, S> IntoIterator for IndexSet<T, S> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            keys: self.map.into_keys(),
        }
    }
}

/// The iterator of [`IndexSet::iter`], over the items in insertion order.
pub struct Iter<'a, T> {
    /// The items left, as the keys of the map.
    keys: Keys<'a, T, ()>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.keys.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.keys.size_hint()
    }
}

/// The iterator of an owned set, over its items in insertion order.
pub struct IntoIter<T> {
    /// The items left, as the keys of the map.
    keys: IntoKeys<T, ()>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.keys.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.keys.size_hint()
    }
}
