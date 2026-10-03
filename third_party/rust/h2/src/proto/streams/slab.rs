// SPDX-License-Identifier: MIT
/*
 * h2: the slab that holds the streams and the frames they buffer, in place
 * of the crate slab.
 */
//! A vector of vacant and occupied entries whose keys stay.
//!
//! An entry keeps its key until it is removed, and a removed entry joins a
//! free list that the next insert takes its key from.

use std::ops;

/// Values stored at keys that stay, with the vacant keys reused.
#[derive(Debug)]
pub(crate) struct Slab<T> {
    entries: Vec<Entry<T>>,
    len: usize,
    // The first key of the free list, the length of `entries` when it is empty.
    next: usize,
}

#[derive(Debug)]
enum Entry<T> {
    // A vacant entry names the next key of the free list.
    Vacant(usize),
    Occupied(T),
}

impl<T> Slab<T> {
    pub(crate) fn new() -> Slab<T> {
        Slab {
            entries: Vec::new(),
            len: 0,
            next: 0,
        }
    }

    // A count of the occupied entries, which the feature unstable reads.
    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn get(&self, key: usize) -> Option<&T> {
        match self.entries.get(key) {
            Some(Entry::Occupied(value)) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn get_mut(&mut self, key: usize) -> Option<&mut T> {
        match self.entries.get_mut(key) {
            Some(Entry::Occupied(value)) => Some(value),
            _ => None,
        }
    }

    /// Stores `value` and returns its key, the last key removed if any.
    pub(crate) fn insert(&mut self, value: T) -> usize {
        let key = self.next;
        if key == self.entries.len() {
            self.entries.push(Entry::Occupied(value));
            self.next = key + 1;
        } else {
            match std::mem::replace(&mut self.entries[key], Entry::Occupied(value)) {
                Entry::Vacant(next) => self.next = next,
                Entry::Occupied(_) => unreachable!("the free list names an occupied entry"),
            }
        }
        self.len += 1;
        key
    }

    /// Removes the value at `key` and returns it.
    ///
    /// # Panics
    ///
    /// Panics when `key` holds no value.
    pub(crate) fn remove(&mut self, key: usize) -> T {
        match self.entries.get_mut(key) {
            Some(entry @ Entry::Occupied(_)) => {
                let value = std::mem::replace(entry, Entry::Vacant(self.next));
                self.next = key;
                self.len -= 1;
                match value {
                    Entry::Occupied(value) => value,
                    Entry::Vacant(_) => unreachable!("the entry was occupied"),
                }
            }
            _ => panic!("invalid key"),
        }
    }
}

impl<T> ops::Index<usize> for Slab<T> {
    type Output = T;

    fn index(&self, key: usize) -> &T {
        self.get(key).expect("invalid key")
    }
}

impl<T> ops::IndexMut<usize> for Slab<T> {
    fn index_mut(&mut self, key: usize) -> &mut T {
        self.get_mut(key).expect("invalid key")
    }
}
