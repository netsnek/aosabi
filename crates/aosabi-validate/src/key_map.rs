// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the keyed lookup over an array, which the argument and
 * input field rules find a node by its name with.
 *
 * Derived from graphql-js 16.12.0, src/jsutils/keyMap.ts, MIT.
 */

//! The `keyMap` of the validation layer.

use alloc::string::String;

use aosabi_type::ObjMap;

/// Creates a keyed JS object from an array, given a function to produce the
/// keys for each value in the array.
///
/// This provides a convenient lookup for the array items if the key function
/// produces unique results. `list` yields the items and `key_fn` produces
/// the key of an item. Of two items with one key the later one is the value,
/// and the key keeps the position of the earlier one, as an assignment to an
/// existing property replaces its value.
///
/// Derived from graphql-js 16.12.0, src/jsutils/keyMap.ts, MIT.
///
/// The reference returns a null prototype object, whose keys enumerate with
/// the array indices first. Every caller keys by a GraphQL name, which
/// starts with a letter or an underscore and is never an array index, so
/// insertion order is the enumeration order here.
pub fn key_map<T>(
    list: impl IntoIterator<Item = T>,
    mut key_fn: impl FnMut(&T) -> String,
) -> ObjMap<T> {
    let mut result: ObjMap<T> = ObjMap::default();
    for item in list {
        result.insert(key_fn(&item), item);
    }
    result
}
