// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the grouping of array items by a key, which the
 * uniqueness rules find their duplicates with.
 *
 * Derived from graphql-js 16.12.0, src/jsutils/groupBy.ts, MIT.
 */

//! The `groupBy` of the validation layer.

use alloc::{
    vec,
    vec::Vec, //
};
use core::hash::Hash;

use aosabi_json::{
    map::IndexMap,
    value::KeyHasherBuilder, //
};

/// The groups of [`group_by()`], the `Map<K, ReadonlyArray<T>>` of the
/// reference.
///
/// A `Map` iterates in the order its keys were first set, and a rule
/// reports one error for each group in that order, so the carrier keeps
/// insertion order.
pub type Groups<K, T> = IndexMap<K, Vec<T>, KeyHasherBuilder>;

/// Groups array items into a Map, given a function to produce grouping key.
///
/// `list` yields the items, which are borrows of nodes in every caller
/// and therefore `Copy`, and `key_fn` produces the key of an item. The
/// reference finds a key as `SameValueZero` does, which for the string
/// keys of every caller is equality of the text, and `Eq` on a `&str` is
/// that.
///
/// Derived from graphql-js 16.12.0, src/jsutils/groupBy.ts, MIT.
pub fn group_by<K, T>(
    list: impl IntoIterator<Item = T>,
    mut key_fn: impl FnMut(T) -> K,
) -> Groups<K, T>
where
    K: Hash + Eq,
    T: Copy,
{
    let mut result: Groups<K, T> = Groups::default();
    for item in list {
        let key = key_fn(item);
        match result.get_mut(&key) {
            None => {
                result.insert(key, vec![item]);
            }
            Some(group) => group.push(item),
        }
    }
    result
}
