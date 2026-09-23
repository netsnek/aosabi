// SPDX-License-Identifier: MIT
/*
 * homa heap: binary heaps over caller owned items, ties to the older message.
 *
 * The engine keeps its transmit order and its grant order in such heaps, so
 * choosing the message with the fewest bytes left, and keeping the order as
 * its bytes fall, costs a logarithm of the messages in flight.
 */

//! Binary heaps over caller owned items.
//!
//! A heap orders the messages the engine chooses among, the transmit order by
//! the bytes a message has left to send and the grant order by the bytes a
//! message has not yet received. The fewest bytes come first, which is SRPT,
//! and a tie goes to the older message, as `HomaModule` breaks a tie of its
//! grant order. The items live in a slice the caller owns and the count of
//! them in a number the caller keeps beside it, so the storage of an endpoint
//! stays typed and allocated once.
//!
//! The heap reports every item it moves to a callback with the item's slot
//! and its new position, so the caller keeps the position of each slot and a
//! key that falls or an item that leaves costs a logarithm and no search.

use core::fmt;

/// One item of a heap: a slot and its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    /// The bytes that order the item, the fewest first.
    key: u32,
    /// The age of the message, the lower the older, which breaks a tie.
    age: u64,
    /// The RPC slot the item stands for.
    slot: u32,
}

impl Item {
    /// An item that stands for no slot.
    pub const VACANT: Self = Self {
        key: u32::MAX,
        age: u64::MAX,
        slot: u32::MAX,
    };

    /// Returns the item of `slot` with `key` bytes and the age `age`.
    #[must_use]
    pub const fn new(slot: u32, key: u32, age: u64) -> Self {
        Self { key, age, slot }
    }

    /// Returns the slot the item stands for.
    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }

    /// Returns the bytes that order the item.
    #[must_use]
    pub const fn key(self) -> u32 {
        self.key
    }

    /// Returns the age of the message, the lower the older.
    #[must_use]
    pub const fn age(self) -> u64 {
        self.age
    }

    /// Returns whether the item goes before `other`: fewer bytes first, and on
    /// a tie the older message.
    #[must_use]
    pub const fn precedes(self, other: Self) -> bool {
        self.key < other.key || (self.key == other.key && self.age < other.age)
    }
}

/// Why a heap operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Every item of the storage is taken.
    Full,
    /// The position lies beyond the items of the heap.
    Missing,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Full => "every item of the heap is taken",
            Self::Missing => "the position lies beyond the items of the heap",
        })
    }
}

impl core::error::Error for Error {}

/// A binary heap over items the caller owns and a count the caller keeps.
#[derive(Debug)]
pub struct Heap<'h> {
    /// The storage, of which the first `len` items are the heap.
    items: &'h mut [Item],
    /// The count of items in the heap.
    len: &'h mut u32,
}

impl<'h> Heap<'h> {
    /// Returns the heap over `items` whose first `len` items are in heap
    /// order, and clamps a count beyond the storage to the storage.
    pub fn new(items: &'h mut [Item], len: &'h mut u32) -> Self {
        let room = u32::try_from(items.len()).unwrap_or(u32::MAX);
        if *len > room {
            *len = room;
        }
        Self { items, len }
    }

    /// Returns the count of items in the heap.
    #[must_use]
    pub fn len(&self) -> u32 {
        *self.len
    }

    /// Returns whether the heap holds no item.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self.len == 0
    }

    /// Returns the first item, the fewest bytes and on a tie the oldest.
    #[must_use]
    pub fn peek(&self) -> Option<Item> {
        self.get(0)
    }

    /// Returns the item at `position`.
    #[must_use]
    pub fn get(&self, position: u32) -> Option<Item> {
        if position < *self.len {
            self.items.get(to_index(position)).copied()
        } else {
            None
        }
    }

    /// Adds `item`, reporting every item that moves, the new one among them,
    /// to `moved` with its slot and its position.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Full`] when every item of the storage is taken.
    pub fn push(&mut self, item: Item, mut moved: impl FnMut(u32, u32)) -> Result<(), Error> {
        let at = *self.len;
        let next = at.checked_add(1).ok_or(Error::Full)?;
        let slot = self.items.get_mut(to_index(at)).ok_or(Error::Full)?;
        *slot = item;
        *self.len = next;
        self.sift_up(at, &mut moved);
        Ok(())
    }

    /// Removes and returns the first item, reporting every item that moves to
    /// `moved`.
    pub fn pop(&mut self, moved: impl FnMut(u32, u32)) -> Option<Item> {
        self.remove(0, moved)
    }

    /// Removes and returns the item at `position`, reporting every item that
    /// moves to `moved`.
    pub fn remove(&mut self, position: u32, mut moved: impl FnMut(u32, u32)) -> Option<Item> {
        let removed = self.get(position)?;
        let last = *self.len - 1;
        *self.len = last;
        if position != last {
            let tail = self.items[to_index(last)];
            self.place(position, tail, &mut moved);
            self.restore(position, &mut moved);
        }
        self.items[to_index(last)] = Item::VACANT;
        Some(removed)
    }

    /// Gives the item at `position` the bytes `key` and moves it to its place,
    /// reporting every item that moves to `moved`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Missing`] when `position` lies beyond the heap.
    pub fn rekey(
        &mut self,
        position: u32,
        key: u32,
        mut moved: impl FnMut(u32, u32),
    ) -> Result<(), Error> {
        let item = self.get(position).ok_or(Error::Missing)?;
        self.items[to_index(position)] = Item { key, ..item };
        self.restore(position, &mut moved);
        Ok(())
    }

    /// Moves the item at `position` up or down until the heap order holds.
    fn restore(&mut self, position: u32, moved: &mut impl FnMut(u32, u32)) {
        if position > 0
            && self.items[to_index(position)].precedes(self.items[to_index((position - 1) / 2)])
        {
            self.sift_up(position, moved);
        } else {
            self.sift_down(position, moved);
        }
    }

    /// Moves the item at `position` towards the root while it precedes its
    /// parent.
    fn sift_up(&mut self, mut position: u32, moved: &mut impl FnMut(u32, u32)) {
        let item = self.items[to_index(position)];
        while position > 0 {
            let parent = (position - 1) / 2;
            let above = self.items[to_index(parent)];
            if !item.precedes(above) {
                break;
            }
            self.place(position, above, moved);
            position = parent;
        }
        self.place(position, item, moved);
    }

    /// Moves the item at `position` towards the leaves while a child precedes
    /// it.
    fn sift_down(&mut self, mut position: u32, moved: &mut impl FnMut(u32, u32)) {
        let item = self.items[to_index(position)];
        let len = *self.len;
        loop {
            let left = position.saturating_mul(2).saturating_add(1);
            if left >= len {
                break;
            }
            let right = left + 1;
            let mut child = left;
            if right < len && self.items[to_index(right)].precedes(self.items[to_index(left)]) {
                child = right;
            }
            let below = self.items[to_index(child)];
            if !below.precedes(item) {
                break;
            }
            self.place(position, below, moved);
            position = child;
        }
        self.place(position, item, moved);
    }

    /// Writes `item` at `position` and reports it to `moved`.
    fn place(&mut self, position: u32, item: Item, moved: &mut impl FnMut(u32, u32)) {
        self.items[to_index(position)] = item;
        moved(item.slot, position);
    }
}

/// Returns a position as an index of the storage.
fn to_index(position: u32) -> usize {
    usize::try_from(position).unwrap_or(usize::MAX)
}
