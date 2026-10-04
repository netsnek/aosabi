// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-lora counter: the counters of the frames a link sends,
 * reserved in blocks in the storage of the card.
 *
 * The rule is that of the Homa design note, second version, homa/design.md
 * section 6.5, lines 1928 to 1979: a device reserves counters in blocks of
 * 1,024 in its storage capability and starts after the last reserved block at
 * boot, so a restart never reuses a nonce.
 */

//! The counters of a link and the storage that keeps them across restarts.

use crate::Direction;

/// The counters a reservation takes at once.
pub const BLOCK: u32 = 1_024;

/// The storage capability for counters, reserved in blocks of 1,024.
///
/// The store keeps, for each link and direction, the first counter no
/// reservation has taken yet, 0 before the first, and keeps it across a
/// restart of the card.
pub trait CounterStore {
    /// A failure of the storage.
    type Error;

    /// Returns the first counter of `link` and `direction` no reservation
    /// has taken.
    ///
    /// # Errors
    ///
    /// Returns the storage's error where it cannot be read.
    fn reserved(&mut self, link: u8, direction: Direction) -> Result<u32, Self::Error>;

    /// Records that every counter of `link` and `direction` below `up_to`
    /// is reserved, and returns once the record is durable.
    ///
    /// # Errors
    ///
    /// Returns the storage's error where the record did not become durable.
    fn reserve(&mut self, link: u8, direction: Direction, up_to: u32) -> Result<(), Self::Error>;
}

/// The counters one link hands its frames of one direction.
///
/// A counter is handed out only after the block that holds it is reserved in
/// the store, so a card that restarts starts after every counter it may have
/// sent. Once the counters reach `u32::MAX` they are spent, [`Counters::next`]
/// hands out `u32::MAX`, and [`crate::seal`] refuses it, so no nonce repeats.
#[derive(Debug)]
pub struct Counters<S: CounterStore> {
    /// The storage.
    store: S,
    /// The link.
    link: u8,
    /// The direction.
    direction: Direction,
    /// The next counter to hand out.
    next: u32,
    /// The first counter not reserved.
    reserved: u32,
}

impl<S: CounterStore> Counters<S> {
    /// Starts after the last reserved block, so a restart never reuses a
    /// nonce, and reserves the next block before the first counter.
    ///
    /// # Errors
    ///
    /// Returns the storage's error, and hands out no counter then.
    pub fn boot(mut store: S, link: u8, direction: Direction) -> Result<Self, S::Error> {
        let next = store.reserved(link, direction)?;
        let reserved = next.saturating_add(BLOCK);
        store.reserve(link, direction, reserved)?;
        Ok(Self {
            store,
            link,
            direction,
            next,
            reserved,
        })
    }

    /// Starts again after the last reserved block, as [`Counters::boot`]
    /// does after a restart of the card.
    ///
    /// # Errors
    ///
    /// Returns the storage's error, and the counters stay spent until a
    /// later call succeeds.
    pub fn reboot(&mut self) -> Result<(), S::Error> {
        self.next = u32::MAX;
        self.reserved = u32::MAX;
        let next = self.store.reserved(self.link, self.direction)?;
        let reserved = next.saturating_add(BLOCK);
        self.store.reserve(self.link, self.direction, reserved)?;
        self.next = next;
        self.reserved = reserved;
        Ok(())
    }

    /// Returns the next counter, reserving the next block first where the
    /// reserved ones are used up.
    ///
    /// # Errors
    ///
    /// Returns the storage's error, and hands out no counter then.
    // The name is the one of the plan's interface, and a counter whose store
    // can fail answers a result and is no iterator.
    #[expect(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<u32, S::Error> {
        if self.next == u32::MAX {
            return Ok(u32::MAX);
        }
        if self.next >= self.reserved {
            let up_to = self.reserved.saturating_add(BLOCK);
            self.store.reserve(self.link, self.direction, up_to)?;
            self.reserved = up_to;
        }
        let counter = self.next;
        self.next += 1;
        Ok(counter)
    }

    /// Returns the counter [`Counters::next`] hands out next.
    #[must_use]
    pub const fn peek(&self) -> u32 {
        self.next
    }

    /// Returns the storage.
    #[must_use]
    pub const fn store(&self) -> &S {
        &self.store
    }

    /// Returns the storage, which the card keeps for its next boot.
    #[must_use]
    pub fn into_store(self) -> S {
        self.store
    }
}
