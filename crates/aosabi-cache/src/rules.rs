// SPDX-License-Identifier: MIT
/*
 * aosabi-cache rules: the interned lists of the names of the rules a
 * validation runs.
 */

//! The interned lists of rule names.
//!
//! Yoga keys a verdict by the names of its rules joined by commas. The
//! table keeps each list of names once, at most [`crate::RULE_LISTS`] of
//! them, and finds a list by comparing its names in order, the address and
//! the length of each name first and its bytes only where they differ, so
//! two lists whose names are equal are one list, as their joined keys are,
//! and nothing is joined or allocated once a list is known.

use alloc::vec::Vec;

use crate::RULE_LISTS;

/// A list of rule names the table knows, by its place among them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleList(u8);

/// The rule lists of a table.
pub(crate) struct Rules {
    /// The names of each list, in the order of its rules.
    lists: Vec<Vec<&'static str>>,
}

/// Answers whether two names are equal, by their address and length first.
fn same_name(known: &str, name: &str) -> bool {
    (core::ptr::eq(known.as_ptr(), name.as_ptr()) && known.len() == name.len()) || known == name
}

impl Rules {
    /// Returns no rule list.
    pub(crate) const fn new() -> Self {
        Self { lists: Vec::new() }
    }

    /// Returns the list of `names`, keeping it where the table has room for one more.
    ///
    /// Answers `None` for a list the table does not know when it knows
    /// [`RULE_LISTS`] lists already, and a validation under it runs uncached.
    pub(crate) fn intern<I>(&mut self, names: I) -> Option<RuleList>
    where
        I: IntoIterator<Item = &'static str>,
        I::IntoIter: ExactSizeIterator + Clone,
    {
        let names = names.into_iter();
        let known = self.lists.iter().position(|list| {
            list.len() == names.len()
                && list
                    .iter()
                    .zip(names.clone())
                    .all(|(known, name)| same_name(known, name))
        });
        if let Some(at) = known {
            return u8::try_from(at).ok().map(RuleList);
        }
        if self.lists.len() >= RULE_LISTS {
            return None;
        }
        let at = u8::try_from(self.lists.len()).ok()?;
        self.lists.push(names.collect());
        Some(RuleList(at))
    }
}
