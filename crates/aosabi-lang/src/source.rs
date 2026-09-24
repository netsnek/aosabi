// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the source input to GraphQL, carried as the JavaScript string
 * it is in the reference.
 *
 * Derived from graphql-js 16.12.0, src/language/source.ts, MIT.
 */

//! The source of a GraphQL document.
//!
//! graphql-js keeps the body as a JavaScript string, and a JavaScript string
//! is a sequence of UTF-16 code units, so every position the lexer, the parser
//! and the error carry is a code unit index. The body here is WTF-8 bytes, and
//! a byte offset is that code unit index for as long as every byte before it
//! is ASCII. The lexer therefore walks the bytes and converts every position
//! it reports into the code unit index graphql-js reports for the same point
//! of the same text.
//!
//! A character outside ASCII is one code unit when its code point is below
//! U+10000, which a lone surrogate in its three WTF-8 bytes is as well, and
//! two code units when it takes four bytes, which are the two halves of a
//! surrogate pair. The code unit index of a byte offset is therefore the
//! offset less the bytes the body holds beyond its code units before it,
//! one for every continuation byte less one for every lead of four bytes.
//! The lexer walks the body in order and keeps that count as it reads, so it
//! converts a position by a subtraction and at most a walk over the bytes of
//! the token it reads (see [`crate::lexer`]). A source carries its body, its
//! name and its offset, and one thing derived from them, the table of the
//! starts of its lines, which the first location request builds by one scan
//! of the body and which [`get_location`](crate::location::get_location) and
//! the location printer read, so that no location walks the body from its
//! start (see [`crate::location`]).
//!
//! A token keeps its positions, its line and its column as `u32`. Every one
//! of them fits for a body of at most `u32::MAX - 1` bytes, and the lexer
//! refuses a longer body before its first token, which every entry point of
//! the parser reads first.
//!
//! Any sequence of code units is a JavaScript string, a surrogate without its
//! partner included, and the reference lexer names such a code unit in a
//! syntax error at the position it stands at. A Rust `String` cannot hold one,
//! so the body is a [`SourceBody`]: the `String` of a body that carries no
//! lone surrogate, and the [`JsString`] of one that does, the WTF-8 carrier of
//! the workspace. A `&str` and a `String` are text, and a `JsString` becomes
//! text once, when [`JsString::as_str`] answers. A body that carries a lone
//! surrogate therefore reaches the lexer as the three bytes WTF-8 writes for
//! it, and every other body as its UTF-8, which answers a span of its text
//! with a check of the two ends of the span alone.
//!
//! The reference's `isSource` guard has no Rust reading beyond the type
//! itself. The parser's `isSource(source) ? source : new Source(source)`
//! reads as `impl Into<Source>` here, which the three `From` impls of
//! [`Source`] provide.

use alloc::{
    borrow::Cow,
    boxed::Box,
    string::String, //
};
use core::fmt;

use aosabi_json::JsString;
use once_cell::race::OnceBox;

use crate::{
    char_utils::slice,
    location::{
        LineTable,
        SourceLocation, //
    },
};

/// The longest body the lexer reads, in bytes.
///
/// A position is a code unit index at most the length of the body, since every
/// code unit takes a byte or more. A line is one more than the count of line
/// terminators before it and a column one more than a distance of code units,
/// so both are at most one more than the length. Every token but `<SOF>` and
/// `<EOF>` takes a byte or more, so the arena of a body holds at most two
/// tokens more than its length. For a body of at most this many bytes, one
/// below `u32::MAX`, every position, line and column fits a `u32` and every
/// token has an id of its own. The lexer refuses a longer body before its
/// first token, which the entry points of [`crate::parser`] all pass through.
pub(crate) const MAX_BODY_LENGTH: usize = u32::MAX as usize - 1;

/// Returns `value`, a position, a line, a column or a count of a body the
/// lexer reads, as the `u32` a token keeps.
///
/// Every such value fits for a body of at most [`MAX_BODY_LENGTH`] bytes,
/// so the conversion is exact for every value a lexer of such a body
/// narrows. A value past `u32::MAX` answers `u32::MAX`, and only a caller of
/// the public [`create_token`](crate::lexer::create_token) or
/// [`read_name`](crate::lexer::read_name), or of
/// [`TokenArena::push`](crate::ast::TokenArena::push) past `u32::MAX + 1`
/// tokens, can pass one.
pub(crate) fn narrow(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Returns the `u32` a token keeps as the `usize` the crate counts in.
///
/// Every target of the workspace, the host, `wasm32` and `riscv32`, has
/// pointers of 32 bits or more, so the widening is exact.
pub(crate) const fn widen(value: u32) -> usize {
    value as usize
}

/// The name a source carries when the caller gives none.
///
/// Every error printed without a named source starts with this string.
pub const DEFAULT_NAME: &str = "GraphQL request";

/// A representation of source input to GraphQL.
///
/// The `name` and `location_offset` are optional in the reference, but they
/// are useful for clients who store GraphQL documents in source files. For
/// example, if the GraphQL input starts at line 40 in a file named
/// `Foo.graphql`, it might be useful for `name` to be `"Foo.graphql"` and the
/// location offset to be line 40, column 1. The `line` and `column` of the
/// offset are 1-indexed.
///
/// The fields are private so that every location offset passes the two checks
/// of the reference constructor. The table of line starts is derived from the
/// body, so two sources are equal when their body, their name and their offset
/// are, whether either has built its table or not.
///
/// Derived from graphql-js 16.12.0, src/language/source.ts, MIT.
#[derive(Clone)]
pub struct Source {
    /// The body, its UTF-8 text unless it carries a lone surrogate.
    body: SourceBody,
    /// The name of the source, borrowed for [`DEFAULT_NAME`].
    name: Cow<'static, str>,
    /// The 1-indexed line and column at which the body starts in its file.
    location_offset: SourceLocation,
    /// The starts of the lines of the body, built on the first location request.
    lines: OnceBox<LineTable>,
}

impl Source {
    /// Creates a source named [`DEFAULT_NAME`] that starts at line 1, column 1.
    ///
    /// `body` is the text of the document, a `&str`, a `String`, a
    /// [`JsString`] or a [`SourceBody`]. This is the reference constructor
    /// with both optional parameters left at their defaults. The name borrows
    /// the constant, so the source allocates for its body alone.
    #[must_use]
    pub fn new(body: impl Into<SourceBody>) -> Self {
        Self::named(body.into(), Cow::Borrowed(DEFAULT_NAME))
    }

    /// Creates a source named `name` that starts at line 1, column 1.
    ///
    /// `body` is the text of the document and `name` is what an error prints
    /// before the line and column.
    #[must_use]
    pub fn with_name(body: impl Into<SourceBody>, name: impl Into<String>) -> Self {
        Self::named(body.into(), Cow::Owned(name.into()))
    }

    /// Creates a source of `body` named `name` that starts at line 1, column 1.
    fn named(body: SourceBody, name: Cow<'static, str>) -> Self {
        // The two assertions of the reference hold trivially for the default
        // offset, so this path cannot fail.
        Self {
            body,
            name,
            location_offset: SourceLocation { line: 1, column: 1 },
            lines: OnceBox::new(),
        }
    }

    /// Creates a source named `name` that starts at `location_offset` in its file.
    ///
    /// `body` is the text of the document and `location_offset` is the
    /// 1-indexed line and column of its first character, which the location
    /// printer adds to every line and column it prints.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::LineNotPositive`] when the line of the offset is
    /// zero and [`SourceError::ColumnNotPositive`] when its column is zero,
    /// which are the two `devAssert` calls of the reference constructor.
    pub fn with_location_offset(
        body: impl Into<SourceBody>,
        name: impl Into<String>,
        location_offset: SourceLocation,
    ) -> Result<Self, SourceError> {
        if location_offset.line == 0 {
            return Err(SourceError::LineNotPositive);
        }
        if location_offset.column == 0 {
            return Err(SourceError::ColumnNotPositive);
        }
        let mut source = Self::with_name(body, name);
        source.location_offset = location_offset;
        Ok(source)
    }

    /// Returns the bytes of the body, its UTF-8 or its WTF-8.
    ///
    /// The lexer walks these bytes, and a byte offset into them is the
    /// position the lexer reads at.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match &self.body {
            SourceBody::Text(text) => text.as_bytes(),
            SourceBody::Wtf8(wtf8) => wtf8.as_wtf8(),
        }
    }

    /// Returns whether the body holds the same bytes as `text`.
    ///
    /// A body that carries a lone surrogate holds bytes no `&str` holds, so
    /// it equals no text, not even the text that replaces each lone surrogate
    /// with U+FFFD, which is the text the served path parses (see
    /// [`to_string_lossy`](crate::js_string::to_string_lossy)).
    ///
    /// On `wasm32` the slice equality of `core` calls the `memcmp` of the
    /// compiler builtins, which compares one byte a step, so the body is
    /// compared there by words of eight bytes, in 509 ns where the slice
    /// equality takes 3,736 ns for 9,920 bytes at opt-level z under V8, while
    /// on the host with the C library of GNU the slice equality takes 182 ns
    /// for those bytes and the words 462 ns. The `memcmp` of musl compares one
    /// byte a step as well, and on the player the words take a hit of the
    /// request cache of a document of 1,905 bytes from 56,326 instructions to
    /// 34,900, so a target of musl compares by words too. Every other target
    /// takes the slice equality.
    #[must_use]
    pub fn body_eq(&self, text: &str) -> bool {
        if cfg!(any(target_arch = "wasm32", target_env = "musl")) {
            eq_words(self.bytes(), text.as_bytes())
        } else {
            self.bytes() == text.as_bytes()
        }
    }

    /// Returns the text of the body between the bytes `start` and `end`.
    ///
    /// This is `body.slice(start, end)` where both ends stand on a character
    /// boundary and the text between them holds no lone surrogate, which the
    /// span of every token meets (see [`crate::ast::TokenValue::Span`]). A
    /// body of text answers through `str::get`, which checks the two ends
    /// alone, and a body that carries a lone surrogate checks the bytes
    /// between them as well. The end is clamped to the length of the body
    /// and the start to the end, and a span that cuts a character or holds a
    /// lone surrogate answers the empty string, which no span of a token
    /// does.
    #[must_use]
    pub fn slice(&self, start: usize, end: usize) -> &str {
        match &self.body {
            SourceBody::Text(text) => {
                let end = end.min(text.len());
                text.get(start.min(end)..end).unwrap_or_default()
            }
            SourceBody::Wtf8(wtf8) => slice(wtf8.as_wtf8(), start, end),
        }
    }

    /// Returns the name of the source.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the 1-indexed line and column at which the body starts.
    #[must_use]
    pub fn location_offset(&self) -> SourceLocation {
        self.location_offset
    }

    /// Returns the table of line starts of the body, which the first call builds.
    ///
    /// Two threads that ask at once may each build a table, and both keep
    /// the one stored first, which reads the same.
    pub(crate) fn line_table(&self) -> &LineTable {
        self.lines
            .get_or_init(|| Box::new(LineTable::build(self.bytes())))
    }

    /// Returns the table of line starts of the body once a location request has built it.
    pub(crate) fn built_lines(&self) -> Option<&LineTable> {
        self.lines.get()
    }
}

impl PartialEq for Source {
    fn eq(&self, other: &Self) -> bool {
        self.body == other.body
            && self.name == other.name
            && self.location_offset == other.location_offset
    }
}

impl Eq for Source {}

impl fmt::Debug for Source {
    /// Writes the body, the name and the offset, the fields a source is equal
    /// by, and marks the table of line starts derived from them as left out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Source")
            .field("body", &self.body)
            .field("name", &self.name)
            .field("location_offset", &self.location_offset)
            .finish_non_exhaustive()
    }
}

/// Returns whether `left` and `right` hold the same bytes, compared eight bytes a step.
///
/// The words are read by index over `as_chunks`, a form that runs as fast at
/// opt-level z as at opt-level 3, where a loop over the iterator of
/// `chunks_exact` takes 28 to 36 times as long at opt-level z. Both loops test
/// both bounds, so no index reads past either slice.
fn eq_words(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let (left_words, left_tail) = left.as_chunks::<8>();
    let (right_words, right_tail) = right.as_chunks::<8>();
    let mut at = 0;
    while at < left_words.len() && at < right_words.len() {
        if u64::from_ne_bytes(left_words[at]) != u64::from_ne_bytes(right_words[at]) {
            return false;
        }
        at += 1;
    }
    let mut at = 0;
    while at < left_tail.len() && at < right_tail.len() {
        if left_tail[at] != right_tail[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// The body of a [`Source`], its text unless it carries a lone surrogate.
///
/// A JavaScript string may carry a surrogate without its partner, which a
/// Rust `String` cannot hold (see [`crate::source`]). A body without one is
/// `Text`, and [`Source::slice`] answers a span of it with a check of the
/// two ends of the span. A body with one is `Wtf8`, and a span of it is
/// checked as UTF-8 before it is answered. The three conversions below
/// choose the form, and a `Wtf8` built by hand of a body without a lone
/// surrogate reads the same text through the longer check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceBody {
    /// The UTF-8 text of a body that carries no lone surrogate.
    Text(String),
    /// The WTF-8 of a body that carries a lone surrogate.
    Wtf8(JsString),
}

impl From<&str> for SourceBody {
    fn from(body: &str) -> Self {
        Self::Text(String::from(body))
    }
}

impl From<String> for SourceBody {
    fn from(body: String) -> Self {
        Self::Text(body)
    }
}

impl From<JsString> for SourceBody {
    /// Keeps `body` as text when [`JsString::as_str`] answers.
    ///
    /// A `JsString` gives no way to take its bytes, so the text is copied
    /// once, after the one check of `as_str`, and a body that carries a lone
    /// surrogate is kept as it is.
    fn from(body: JsString) -> Self {
        match body.as_str() {
            Some(text) => Self::Text(String::from(text)),
            None => Self::Wtf8(body),
        }
    }
}

impl From<&str> for Source {
    fn from(body: &str) -> Self {
        Self::new(body)
    }
}

impl From<String> for Source {
    fn from(body: String) -> Self {
        Self::new(body)
    }
}

impl From<JsString> for Source {
    fn from(body: JsString) -> Self {
        Self::new(body)
    }
}

/// The reasons a [`Source`] refuses a location offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceError {
    /// The line of the offset is zero.
    LineNotPositive,
    /// The column of the offset is zero.
    ColumnNotPositive,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LineNotPositive => {
                f.write_str("line in locationOffset is 1-indexed and must be positive.")
            }
            Self::ColumnNotPositive => {
                f.write_str("column in locationOffset is 1-indexed and must be positive.")
            }
        }
    }
}

impl core::error::Error for SourceError {}
