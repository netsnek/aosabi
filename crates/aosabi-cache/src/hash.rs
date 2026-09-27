// SPDX-License-Identifier: MIT
/*
 * aosabi-cache hash: the keyed hash of the text of a request.
 */

//! The keyed hash a caller hashes the text of a request with.
//!
//! A [`crate::Table`] takes the hash of a text from its caller, so one table
//! serves whichever keyed hasher the build of its caller holds. The hash only
//! chooses where the table looks, and the whole text decides every hit, so a
//! collision of hashes costs a compare and never hands a document to another
//! text.

use aosabi_json::value::KeyState;

/// A keyed hash of a whole text.
///
/// The hash is keyed with a key a client does not know, so a client cannot
/// choose texts that share a window of the index.
pub trait TextHash {
    /// Returns the hash of `text`.
    fn hash_text(&self, text: &[u8]) -> u64;
}

/// The one call `SipHash-1-3` of [`KeyState::hash_bytes`] under the key of the state.
///
/// The state takes the key of `aosabi_json::value::seed` or the fixed key
/// when it is made, and a build without the standard library takes the fixed
/// key, which a Worker keeps until its boot calls `seed`. The one call keeps
/// at opt-level z the speed of opt-level 3, which the streaming hasher does
/// not.
impl TextHash for KeyState {
    #[inline]
    fn hash_text(&self, text: &[u8]) -> u64 {
        self.hash_bytes(text)
    }
}
