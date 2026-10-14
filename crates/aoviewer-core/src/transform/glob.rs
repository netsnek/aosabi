// SPDX-License-Identifier: MIT
/*
 * aoviewer-core transform glob: the matcher of the hiding globs and of the
 * declared clusters.
 *
 * The option `hide=*_aggregate*` hides every type its glob matches, and a
 * declared cluster maps its name to type name globs.
 */

//! The globs.
//!
//! A glob is a pattern over a type name in which `*` matches any run of bytes,
//! the empty run included, `?` matches one byte, and every other byte matches
//! itself. Names in GraphQL are case sensitive, so a glob is as well. A name is
//! ASCII by its grammar, so a byte is a character.
//!
//! [specification]: https://spec.graphql.org/October2021/

use alloc::string::String;

/// The longest glob in bytes.
///
/// The cap is the longest name [`ReadLimits::DEFAULT`] admits, and it bounds
/// the matcher.
///
/// [`ReadLimits::DEFAULT`]: crate::limits::ReadLimits::DEFAULT
pub const MAX_GLOB_BYTES: usize = 1024;

/// The longest name the matcher reads, the most
/// [`ReadLimits::max_name_bytes`](crate::limits::ReadLimits::max_name_bytes)
/// admits.
const MAX_NAME_BYTES: usize = u16::MAX as usize;

/// The most steps one match takes.
///
/// The matcher remembers the last star it passed and the name position that
/// star swallows up to. That position never moves back, so it moves at most
/// once per byte of the name and once per star, and between two of its moves
/// the pattern advances at most once per byte of the glob. A match therefore
/// takes at most `(name + glob + 1) * (glob + 1)` steps.
const MAX_MATCH_STEPS: usize = (MAX_NAME_BYTES + MAX_GLOB_BYTES + 2) * (MAX_GLOB_BYTES + 2);

/// The `*` and `?` matcher of `hide=` and of the declared clusters.
///
/// A glob holds the bytes of a name and the two wildcards and nothing else,
/// so it never carries the `,` or the `&` of the option grammar and writes
/// back unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glob {
    /// The pattern as written.
    pattern: String,
}

impl Glob {
    /// Reads a glob from its pattern.
    ///
    /// Returns `None` for an empty pattern, for one longer than
    /// [`MAX_GLOB_BYTES`], and for one that holds a byte outside
    /// `[_0-9A-Za-z*?]`.
    #[must_use]
    pub fn parse(pattern: &str) -> Option<Glob> {
        let valid = !pattern.is_empty()
            && pattern.len() <= MAX_GLOB_BYTES
            && pattern
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'*' | b'?'));
        valid.then(|| Glob {
            pattern: String::from(pattern),
        })
    }

    /// Returns the pattern as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.pattern
    }

    /// Returns whether the glob matches the whole of `name`.
    ///
    /// A name longer than any model holds matches nothing.
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        let (p, n) = (self.pattern.as_bytes(), name.as_bytes());
        if n.len() > MAX_NAME_BYTES {
            return false;
        }
        let (mut pi, mut ni) = (0, 0);
        // The pattern position after the last star and the name position
        // that star swallows up to.
        let mut star: Option<(usize, usize)> = None;
        for _ in 0..MAX_MATCH_STEPS {
            if ni == n.len() {
                return p[pi..].iter().all(|&b| b == b'*');
            }
            if p.get(pi) == Some(&b'*') {
                pi += 1;
                star = Some((pi, ni));
            } else if p.get(pi).is_some_and(|&b| b == b'?' || b == n[ni]) {
                pi += 1;
                ni += 1;
            } else if let Some((sp, sn)) = star {
                pi = sp;
                ni = sn + 1;
                star = Some((sp, ni));
            } else {
                return false;
            }
        }
        false
    }
}
