// SPDX-License-Identifier: MIT
/*
 * aosabi-cache budget: the bounds of a table and the estimated bytes it
 * counts against them.
 */

//! The budget of a table and the estimate it weighs an entry by.
//!
//! A table counts no allocation, since a counting allocator needs `unsafe`. It
//! weighs a document by its count of tokens and the bytes of its text, which
//! the parser keeps on the document, so the weight costs no walk of the tree,
//! and it weighs a verdict by the shape of its errors.

/// The three bounds of a table.
///
/// A table holds at most `entries` entries and at most `bytes` estimated bytes,
/// and it keeps no entry whose estimated bytes, its document and its verdicts,
/// pass `entry_bytes`, so the text of such an entry parses every time, as with
/// no cache. The table takes `entries` between 1 and [`crate::MAX_ENTRIES`] and
/// `entry_bytes` at most `bytes`, and [`crate::Table::budget`] answers the
/// bounds it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheBudget {
    /// The entries a table holds at most.
    pub entries: usize,
    /// The estimated bytes of all entries at most.
    pub bytes: usize,
    /// The estimated bytes of one entry at most.
    pub entry_bytes: usize,
}

/// The constants of the estimated bytes of a document and of a verdict on one width of pointer.
///
/// A parsed document weighs `base` bytes, `per_token` bytes for each of its
/// tokens and twice the bytes of its text. An error of a verdict weighs its own
/// size, the bytes of its message, `per_error` bytes and its locations, and
/// each node it blames weighs its own size and `per_token` bytes for each token
/// it spans, as if it held a copy of its subtree, and `per_unit` bytes for each
/// UTF-16 code unit of the text it spans, since the copy holds the string
/// values of its subtree, which no charge per token bounds.
/// [`crate::ESTIMATE_64`] and [`crate::ESTIMATE_32`] are the values, and
/// [`crate::ESTIMATE`] is the one of the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Estimate {
    /// The bytes of a parsed document beside its tokens and its text.
    pub base: usize,
    /// The bytes of one token of a document or of a blamed node.
    pub per_token: usize,
    /// The bytes of an error beside its own size, its message and its locations.
    pub per_error: usize,
    /// The bytes of a blamed node for each UTF-16 code unit of the text it spans.
    pub per_unit: usize,
}

impl Estimate {
    /// Returns the estimated bytes of a document of `tokens` tokens and `text` bytes of text.
    #[must_use]
    pub const fn document(&self, tokens: usize, text: usize) -> usize {
        self.base
            .saturating_add(self.per_token.saturating_mul(tokens))
            .saturating_add(text.saturating_mul(2))
    }

    /// Returns the estimated bytes of an error of `size` bytes.
    ///
    /// Its message holds `message` bytes and its locations `locations` bytes.
    #[must_use]
    pub const fn error(&self, size: usize, message: usize, locations: usize) -> usize {
        size.saturating_add(message)
            .saturating_add(self.per_error)
            .saturating_add(locations)
    }

    /// Returns the estimated bytes of a blamed node of `size` bytes.
    ///
    /// The node spans `tokens` tokens and `units` UTF-16 code units of text.
    #[must_use]
    pub const fn node(&self, size: usize, tokens: usize, units: usize) -> usize {
        size.saturating_add(self.per_token.saturating_mul(tokens))
            .saturating_add(self.per_unit.saturating_mul(units))
    }
}
