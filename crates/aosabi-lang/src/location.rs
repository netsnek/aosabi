// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the line and column of a position in a source.
 *
 * Derived from graphql-js 16.12.0, src/language/location.ts, MIT.
 */

//! The location arithmetic of the language layer.
//!
//! [`get_location`] turns a position into a 1-indexed line and column. The
//! reference matches the line terminator pattern `/\r\n|[\n\r]/g` over the
//! body and counts the matches that start before the position. A JavaScript
//! string index is a UTF-16 code unit index, so the position and the column
//! count code units.
//!
//! The port reads the matches from a table of line starts, the start of
//! every line after a terminator as a byte offset and a code unit offset,
//! which one scan of the WTF-8 bytes of [`Source::bytes`] builds on the first
//! location request of a source and the source keeps. Both terminators are
//! ASCII, so the scan finds them in the bytes and counts the code units of
//! the string the reference matches in as it goes, which makes the offset of
//! every match a code unit offset as in the reference. A location is then a
//! binary search over the table, and the excerpt of an error takes the bytes
//! of its lines from the same table (see [`mod@crate::print_location`]), so no
//! location walks the body from its start.

use alloc::{
    boxed::Box,
    vec::Vec, //
};
use core::ops::Range;

use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::{
    char_utils::code_units_started_by,
    source::{
        Source,
        narrow,
        widen, //
    },
};

/// Represents a location in a Source.
///
/// `line` and `column` are 1-indexed, and the column counts UTF-16 code units.
/// The same shape is the private `Location` interface of source.ts, so the
/// offset of a [`Source`] is this type as well.
///
/// Derived from graphql-js 16.12.0, src/language/location.ts, MIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceLocation {
    /// The 1-indexed line.
    pub line: usize,
    /// The 1-indexed column, in UTF-16 code units.
    pub column: usize,
}

impl From<SourceLocation> for JsObject {
    /// Builds the `{line, column}` object, in that key order.
    fn from(location: SourceLocation) -> Self {
        let mut object = Self::with_capacity(2);
        object.insert("line", js_number(location.line));
        object.insert("column", js_number(location.column));
        object
    }
}

impl From<SourceLocation> for JsValue {
    fn from(location: SourceLocation) -> Self {
        Self::Object(JsObject::from(location))
    }
}

/// Takes a Source and a UTF-16 code unit offset, and returns the
/// corresponding line and column as a [`SourceLocation`].
///
/// `position` is a code unit index into the body of `source`. A position at or
/// past the end of the body lands on the last line, as it does in the
/// reference, because only terminators that start before it count.
///
/// Derived from graphql-js 16.12.0, src/language/location.ts, MIT.
// Out of line the table read is one function a listing can hold to its budget,
// at a call per location beside the tens of thousands of instructions of an
// error.
#[must_use]
#[inline(never)]
pub fn get_location(source: &Source, position: usize) -> SourceLocation {
    match source.built_lines() {
        Some(table) => table.locate(source.bytes(), position),
        None => locate_first(source, position),
    }
}

/// Builds the table of line starts of `source` and locates `position` in it.
///
/// The first location request of a source takes this path, and every later
/// one finds the table built.
#[cold]
#[inline(never)]
fn locate_first(source: &Source, position: usize) -> SourceLocation {
    source.line_table().locate(source.bytes(), position)
}

/// Takes a Source and a UTF-16 code unit offset, and returns the
/// corresponding line and column by a walk of the body from its start.
///
/// The walk stays for one release behind the dev feature `exec-walk-location`
/// as the second judge of [`get_location`], which the notebooks hold to it
/// over every terminator and every class of code unit. No package of a card
/// names the feature.
///
/// Derived from graphql-js 16.12.0, src/language/location.ts, MIT.
#[cfg(feature = "exec-walk-location")]
#[must_use]
pub fn get_location_by_walk(source: &Source, position: usize) -> SourceLocation {
    let body = source.bytes();
    let mut last_line_start = 0;
    let mut line = 1;

    // `matchAll` with the global flag yields non overlapping matches from left
    // to right, each resuming at the end of the previous one, which is the
    // walk below. `index` is the byte offset of the walk and `unit` the code
    // unit index of the same point, and a terminator is as many code units as
    // bytes.
    let mut index = 0;
    let mut unit = 0;
    while let Some(&byte) = body.get(index) {
        let length = line_break_length(body, index);
        if length == 0 {
            index += 1;
            unit += code_units_started_by(byte);
            continue;
        }
        if unit >= position {
            break;
        }
        last_line_start = unit + length;
        line += 1;
        index += length;
        unit += length;
    }

    SourceLocation {
        line,
        column: position + 1 - last_line_start,
    }
}

/// Returns the length of the line terminator that starts at the byte `index`, or zero.
///
/// This is the pattern `/\r\n|[\n\r]/` of the reference: the alternation tries
/// `\r\n` first, so a carriage return followed by a line feed is one
/// terminator of two code units, and either character alone is one of one.
/// Both characters are ASCII, one byte and one code unit each, and no byte of
/// a character outside ASCII is either of them, so the length is the same in
/// bytes. The scan of the table of line starts reads every terminator with it.
pub(crate) fn line_break_length(body: &[u8], index: usize) -> usize {
    match body.get(index) {
        Some(&CARRIAGE_RETURN) if body.get(index + 1) == Some(&LINE_FEED) => 2,
        Some(&CARRIAGE_RETURN | &LINE_FEED) => 1,
        _ => 0,
    }
}

/// The line feed, one of the two characters of a line terminator.
const LINE_FEED: u8 = 0x0A;

/// The carriage return, the other character of a line terminator.
const CARRIAGE_RETURN: u8 = 0x0D;

/// The bytes the scan of a body reads at a time, one word of 64 bits.
const WORD: usize = 8;

/// The byte 0x01 in every lane of a word.
const ONES: u64 = 0x0101_0101_0101_0101;

/// The top bit of every lane of a word.
const HIGHS: u64 = 0x8080_8080_8080_8080;

/// The line feed in every lane of a word.
const LINE_FEEDS: u64 = 0x0A0A_0A0A_0A0A_0A0A;

/// The carriage return in every lane of a word.
const CARRIAGE_RETURNS: u64 = 0x0D0D_0D0D_0D0D_0D0D;

/// The starts of the lines of a body, which a location is a binary search over.
///
/// The first line starts at offset 0 and has no entry, and every terminator
/// of the pattern `/\r\n|[\n\r]/` adds the start of the line after it, its
/// code unit offset to `units` and its byte offset to `bytes` at the same
/// index, 8 bytes per line. The offsets rise strictly, since every
/// terminator takes a code unit or two. A body of at most `u32::MAX - 1`
/// bytes, which every body the lexer reads is, gives every offset exactly
/// (see [`crate::source`]). The code unit offsets stand in an array of their
/// own, so a step of the search loads one by its index scaled by 4.
#[derive(Debug, Clone)]
pub(crate) struct LineTable {
    /// The code unit offset of the start of every line after the first, in the order of the body.
    units: Box<[u32]>,
    /// The byte offset of the same starts.
    bytes: Box<[u32]>,
    /// The code units of the whole body, where its last line ends.
    total: u32,
}

/// The bytes and the code units of one line of a body, without its terminator.
#[derive(Debug, Clone)]
pub(crate) struct LineSpan {
    /// The byte range of the line in the body.
    pub(crate) bytes: Range<usize>,
    /// The code units of the line, the length the reference measures it by.
    pub(crate) units: usize,
}

impl LineTable {
    /// Returns the table of line starts of the WTF-8 bytes `body`, by one scan of them.
    ///
    /// The scan reads a word of eight bytes at a time and tests all of its
    /// lanes for a line feed or a carriage return at once, and adds the
    /// code units a word without one starts in one step, so a body of ASCII
    /// costs a few instructions a word. The lowest lane a test marks is the
    /// first terminator of the word, which the step at that lane reads with
    /// the byte after it, so a carriage return that ends a word joins the
    /// line feed that starts the next. The bytes after the last whole word
    /// are read one at a time.
    // The scan is the one loop over the body a location reaches, and out of
    // line it stays out of `get_location`, whose listing a notebook holds to a
    // loop of the binary search alone.
    #[inline(never)]
    pub(crate) fn build(body: &[u8]) -> Self {
        let mut unit_starts = Vec::new();
        let mut byte_starts = Vec::new();
        let mut units = 0;
        let mut index = 0;
        while let Some(at) = scan_to_break(body, index, &mut units) {
            let length = line_break_length(body, at);
            index = at + length;
            units += length;
            unit_starts.push(narrow(units));
            byte_starts.push(narrow(index));
        }
        Self {
            units: unit_starts.into_boxed_slice(),
            bytes: byte_starts.into_boxed_slice(),
            total: narrow(units),
        }
    }

    /// Returns the line and column of the code unit offset `position` in `body`.
    ///
    /// A terminator counts when it starts before the position, as the
    /// reference counts it. A line start at or before the position follows
    /// such a terminator, and so does the start one unit past the position
    /// when a carriage return and a line feed end there, whose terminator
    /// starts at the unit before the position. The column counts from the
    /// start of the last line counted, as the reference counts it, so a
    /// position between the two characters of `\r\n` answers column 0 of
    /// the next line, as the walk of the reference does.
    pub(crate) fn locate(&self, body: &[u8], position: usize) -> SourceLocation {
        // A `get_unchecked` would skip the bounds checks of the search.
        let mut lines = self.units.partition_point(|&unit| widen(unit) <= position);
        if self
            .units
            .get(lines)
            .is_some_and(|&next| widen(next) - 1 == position && self.break_before(body, lines) == 2)
        {
            lines += 1;
        }
        let last_line_start = lines
            .checked_sub(1)
            .and_then(|last| self.units.get(last))
            .map_or(0, |&unit| widen(unit));
        SourceLocation {
            line: lines + 1,
            column: position + 1 - last_line_start,
        }
    }

    /// Returns the bytes and the code units of the 0-indexed line `index` of `body`, or `None` past the last line.
    ///
    /// The lines are those `body.split(/\r\n|[\n\r]/g)` answers, the text
    /// between the terminators and the empty line after a trailing one.
    pub(crate) fn line(&self, body: &[u8], index: usize) -> Option<LineSpan> {
        let (start, start_unit) = match index.checked_sub(1) {
            None => (0, 0),
            Some(before) => (
                widen(*self.bytes.get(before)?),
                widen(*self.units.get(before)?),
            ),
        };
        let (end, end_unit) = match (self.bytes.get(index), self.units.get(index)) {
            (Some(&byte), Some(&unit)) => {
                let length = self.break_before(body, index);
                (widen(byte) - length, widen(unit) - length)
            }
            _ => (body.len(), widen(self.total)),
        };
        Some(LineSpan {
            bytes: start..end,
            units: end_unit - start_unit,
        })
    }

    /// Returns the length of the terminator that ends right before the line start `index` of the table.
    ///
    /// A line start follows `\r\n` exactly when the two bytes before it are
    /// those, since a carriage return before a line feed always matches with
    /// it, and any other line start follows a terminator of one byte.
    fn break_before(&self, body: &[u8], index: usize) -> usize {
        let crlf = self.bytes.get(index).is_some_and(|&start| {
            let start = widen(start);
            start
                .checked_sub(2)
                .and_then(|from| body.get(from..start))
                .is_some_and(|pair| *pair == [CARRIAGE_RETURN, LINE_FEED])
        });
        if crlf { 2 } else { 1 }
    }
}

/// Returns the byte offset of the first terminator of `body` at or after `from`, or `None`.
///
/// The code units of the bytes it passes are added to `units`, all of them
/// up to the end of the body when it finds no terminator. The bytes from
/// `from` are read as whole words while words remain, and a lane is tested by
/// the zero byte test of each terminator over the word with that terminator
/// cleared by an exclusive or. The lowest lane the test marks is exact, since
/// a borrow of the test runs toward higher lanes alone. The bytes after the
/// last whole word are read one at a time.
// An `unsafe` vector scan would test 16 bytes a step.
fn scan_to_break(body: &[u8], from: usize, units: &mut usize) -> Option<usize> {
    let rest = body.get(from..).unwrap_or_default();
    let (words, tail) = rest.as_chunks::<WORD>();
    for (at, word) in words.iter().enumerate() {
        let word = u64::from_le_bytes(*word);
        let breaks = zero_lanes(word ^ LINE_FEEDS) | zero_lanes(word ^ CARRIAGE_RETURNS);
        if breaks != 0 {
            let lanes = breaks.trailing_zeros() / 8;
            *units += units_in(word, lanes);
            return Some(from + at * WORD + widen(lanes));
        }
        *units += units_in(word, 8);
    }
    let start = from + words.len() * WORD;
    for (at, &byte) in tail.iter().enumerate() {
        if byte == LINE_FEED || byte == CARRIAGE_RETURN {
            return Some(start + at);
        }
        *units += code_units_started_by(byte);
    }
    None
}

/// Returns the top bit of every lane of `word` that is zero, and possibly of lanes above the lowest of them.
///
/// The test subtracts one from every lane and keeps the top bit of a lane
/// that was below 0x80 and borrowed. A lane below the lowest zero lane never
/// borrows, so the lowest bit the test sets marks the lowest zero lane
/// exactly.
const fn zero_lanes(word: u64) -> u64 {
    word.wrapping_sub(ONES) & !word & HIGHS
}

/// Returns the UTF-16 code units the lowest `lanes` bytes of the WTF-8 word `word` start.
///
/// A lane of ASCII and the lead of a character of two or three bytes start
/// one unit each, a continuation byte none and the lead of a character of
/// four bytes two, which is [`code_units_started_by`] over each lane. A
/// continuation byte holds the top bit and not the next, and a lead of four
/// bytes the four top bits, so the two classes are counted over all lanes
/// at once.
fn units_in(word: u64, lanes: u32) -> usize {
    let kept = if lanes >= 8 {
        u64::MAX
    } else {
        (1 << (8 * lanes)) - 1
    };
    let word = word & kept;
    if word & HIGHS == 0 {
        return widen(lanes);
    }
    let continuation = word & !(word << 1) & HIGHS;
    let four = word & (word << 1) & (word << 2) & (word << 3) & HIGHS;
    widen(lanes) - widen(continuation.count_ones()) + widen(four.count_ones())
}

/// Turns a position or a count into the JavaScript number it is on the wire.
///
/// A position is bounded by the length of a document, which stays far below
/// 2^53, so the widening to a double is exact.
pub(crate) fn js_number(value: usize) -> JsValue {
    JsValue::from(value as u64)
}
