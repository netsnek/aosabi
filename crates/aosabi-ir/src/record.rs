// SPDX-License-Identifier: MIT
/*
 * aosabi-ir: the JavaScript object of the model, a map from a type name to a
 * value that keeps the key order a JavaScript object keeps.
 *
 * No file of the reference is translated here.
 */

//! The record of the model.
//!
//! The reference keeps the six maps of an IR as plain objects and reads them
//! through `Object.values`. The order of those reads is the order of the
//! blocks in the printed schema, so it is part of the byte contract.
//! [`Record`] reproduces it: keys that are array indices come first in
//! ascending numeric order, then every other key in insertion order.
//! `Object.keys`, `Object.values` and `Object.entries` all read through that
//! one clause.
//!
//! [own-keys]: https://tc39.es/ecma262/#sec-ordinaryownpropertykeys
//! [enumerable]: https://tc39.es/ecma262/#sec-enumerableownproperties

use alloc::{
    string::String,
    vec::Vec, //
};

use aosabi_json::{
    map::IndexMap,
    value::{
        KeyHasherBuilder,
        array_index, //
    }, //
};

/// A JavaScript object with string keys, which carries its properties in
/// insertion order and reads them in `OrdinaryOwnPropertyKeys` order.
///
/// The carrier is an [`IndexMap`], the only accepted one, with the hasher of
/// the JSON writer. Two records are equal when [`Record::entries`] yields the
/// same pairs in the same order, because that order is what the reference can
/// observe.
#[derive(Debug, Clone)]
pub struct Record<T> {
    /// The properties in insertion order.
    entries: IndexMap<String, T, KeyHasherBuilder>,
}

impl<T> Record<T> {
    /// Creates an empty record, the `{}` of the reference.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: IndexMap::default(),
        }
    }

    /// Sets a property, which is `obj[key] = value`.
    ///
    /// A `key` that exists keeps its place and the previous value is
    /// returned, which is what assigning an existing property does in
    /// JavaScript.
    pub fn insert(&mut self, key: String, value: T) -> Option<T> {
        self.entries.insert(key, value)
    }

    /// Returns the value of the property named `key`, which is `obj[key]`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&T> {
        self.entries.get(key)
    }

    /// Returns the value of the property named `key` for writing.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut T> {
        self.entries.get_mut(key)
    }

    /// Returns whether a property named `key` exists, which is `key in obj`
    /// for an object with no prototype member of that name.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Removes the property named `key`, which is `delete obj[key]`.
    ///
    /// The remaining properties keep their relative order, so the removal
    /// shifts and never swaps.
    pub fn remove(&mut self, key: &str) -> Option<T> {
        self.entries.shift_remove(key)
    }

    /// Returns the number of properties.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the record has no property.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates the keys the way `Object.keys` lists them.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries().map(|(key, _)| key)
    }

    /// Iterates the values the way `Object.values` lists them.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.entries().map(|(_, value)| value)
    }

    /// Iterates the properties the way `Object.entries` lists them.
    ///
    /// A record with no array index key is walked in place, so the common
    /// record of type names costs no allocation.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &T)> {
        let order = self.own_property_keys();
        let mut next = 0;
        core::iter::from_fn(move || {
            let position = match &order {
                Some(order) => *order.get(next)?,
                None => next,
            };
            let (key, value) = self.entries.get_index(position)?;
            next += 1;
            Some((key.as_str(), value))
        })
    }

    /// Returns the positions of the properties in `OrdinaryOwnPropertyKeys`
    /// order, or `None` when that order is the insertion order.
    ///
    /// The array index keys are sorted by their numeric value and every other
    /// key follows in insertion order. What counts as an array index is
    /// decided by [`array_index`], the rule the JSON writer applies to every
    /// object it writes.
    ///
    /// [own-keys]: https://tc39.es/ecma262/#sec-ordinaryownpropertykeys
    fn own_property_keys(&self) -> Option<Vec<usize>> {
        let mut indexed: Vec<(u32, usize)> = Vec::new();
        for (position, key) in self.entries.keys().enumerate() {
            if let Some(index) = array_index(key.as_bytes()) {
                indexed.push((index, position));
            }
        }
        if indexed.is_empty() {
            return None;
        }
        indexed.sort_unstable();
        let mut order: Vec<usize> = Vec::with_capacity(self.entries.len());
        order.extend(indexed.iter().map(|(_, position)| *position));
        order.extend(
            self.entries
                .keys()
                .enumerate()
                .filter(|(_, key)| array_index(key.as_bytes()).is_none())
                .map(|(position, _)| position),
        );
        Some(order)
    }
}

impl<T> Default for Record<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: PartialEq> PartialEq for Record<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.entries().eq(other.entries())
    }
}
