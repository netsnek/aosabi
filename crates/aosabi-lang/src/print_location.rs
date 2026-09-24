// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the excerpt of a source an error prints under its message.
 *
 * Derived from graphql-js 16.12.0, src/language/printLocation.ts, MIT.
 */

//! The location printer of the language layer.
//!
//! [`print_location`] and [`print_source_location`] render the lines around a
//! location with a caret under the column, which is the text the error's
//! `toString` appends to its message. The reference pads, splits and slices
//! JavaScript strings, and each of those counts UTF-16 code units.
//!
//! The port takes each line it prints from the table of line starts of the
//! source (see [`crate::location`]), which gives the bytes of the line and
//! its length in code units, so the body is never split and no line is read
//! that is not printed. A line prints as its text. A line longer than 120
//! code units prints in pieces of 80 units up to the piece after the one
//! that holds the caret, so only that prefix of the line is read: a line of
//! ASCII, which holds as many bytes as code units, is cut by bytes, and any
//! other line has that prefix converted into code units, which are cut and
//! decoded piece by piece. Only the error path reaches this module.
//!
//! One case decodes with a loss. A cut through a surrogate pair leaves a lone
//! surrogate at the edge of two pieces, which a JavaScript string carries and
//! a Rust `String` cannot. Such a unit prints as U+FFFD here. A lone surrogate
//! of the body prints the same way. The message and the locations of an error
//! never pass through this file, so the loss cannot reach a response body.

use alloc::{
    format,
    string::String,
    vec::Vec, //
};
use core::{
    fmt::Write,
    ops::Range, //
};

use crate::{
    ast::Location,
    char_utils::push_utf16,
    location::{
        LineSpan,
        LineTable,
        SourceLocation,
        get_location, //
    },
    source::{
        Source,
        widen, //
    },
};

/// The width of one piece of a minified line.
const SUB_LINE_WIDTH: usize = 80;

/// The length above which a line is printed in pieces.
const MINIFIED_LINE_LENGTH: usize = 120;

/// A prefixed line of the excerpt: the prefix and the text, which is absent
/// where the reference reads past the ends of its arrays.
type PrefixedLine<'a> = (Prefix, Option<Piece<'a>>);

/// Render a helpful description of the location in the GraphQL Source document.
///
/// `location` is the region of a node, and the excerpt is printed around its
/// start.
///
/// Derived from graphql-js 16.12.0, src/language/printLocation.ts, MIT.
#[must_use]
pub fn print_location(location: &Location) -> String {
    print_source_location(
        &location.source,
        get_location(&location.source, location.start),
    )
}

/// Render a helpful description of the location in the GraphQL Source document.
///
/// `source` is the document and `source_location` a 1-indexed line and column
/// in it. The offset of the source is added to the line and column that are
/// printed, and the first line is padded by the column offset, so a document
/// embedded in a file prints its numbers as that file counts them.
///
/// Derived from graphql-js 16.12.0, src/language/printLocation.ts, MIT.
///
/// A line of zero or beyond the last line of the body has no output in the
/// reference, which throws a `TypeError` on it, and this function prints the
/// name, line and column alone for it.
#[must_use]
pub fn print_source_location(source: &Source, source_location: SourceLocation) -> String {
    let first_line_column_offset = source.location_offset().column - 1;

    let line_offset = source.location_offset().line - 1;
    let line_num = source_location.line + line_offset;

    let column_offset = if source_location.line == 1 {
        first_line_column_offset
    } else {
        0
    };
    let column_num = source_location.column + column_offset;
    let mut output = format!("{}:{}:{}\n", source.name(), line_num, column_num);

    let lines = PaddedLines {
        source,
        table: source.line_table(),
        first_line_column_offset,
    };
    let Some(line_index) = source_location.line.checked_sub(1) else {
        return output;
    };
    let Some(location_line) = lines.get(line_index) else {
        return output;
    };

    // Special case for minified documents
    if location_line.units() > MINIFIED_LINE_LENGTH {
        print_minified_line(&mut output, &location_line, line_num, column_num);
    } else {
        print_context_lines(
            &mut output,
            &lines,
            &location_line,
            line_index,
            line_num,
            column_num,
        );
    }
    output
}

/// The lines of `''.padStart(offset) + body`, read from the table of line starts of the body.
///
/// A line terminator never stands inside the padding, so the padded body
/// splits into the lines of the body with the padding in front of the
/// first.
struct PaddedLines<'a> {
    /// The source whose body the lines are read from.
    source: &'a Source,
    /// The table of line starts of the body.
    table: &'a LineTable,
    /// The count of spaces in front of the first line.
    first_line_column_offset: usize,
}

impl<'a> PaddedLines<'a> {
    /// Returns the 0-indexed line `index`, or `None` past the last line.
    fn get(&self, index: usize) -> Option<PaddedLine<'a>> {
        let LineSpan { bytes, units } = self.table.line(self.source.bytes(), index)?;
        let padding = if index == 0 {
            self.first_line_column_offset
        } else {
            0
        };
        Some(PaddedLine {
            source: self.source,
            bytes,
            units,
            padding,
        })
    }
}

/// One line of the padded body: its padding of spaces, then a line of the body.
#[derive(Clone)]
struct PaddedLine<'a> {
    /// The source the line belongs to.
    source: &'a Source,
    /// The byte range of the line in the body.
    bytes: Range<usize>,
    /// The code units of the line of the body, without the padding.
    units: usize,
    /// The spaces in front of the line, which only the first line has.
    padding: usize,
}

impl PaddedLine<'_> {
    /// Returns `locationLine.length`, the code units of the padded line.
    ///
    /// `padStart` fills with spaces, and a space is one code unit.
    fn units(&self) -> usize {
        self.padding + self.units
    }

    /// Returns whether every code unit of the line is a byte of ASCII.
    ///
    /// A character outside ASCII takes more bytes than code units, two bytes
    /// for one unit, three for one, or four for two, so a line holds as many
    /// bytes as code units exactly when it is ASCII.
    fn is_ascii(&self) -> bool {
        self.bytes.len() == self.units
    }

    /// Returns the WTF-8 bytes of the line of the body.
    fn wtf8(&self) -> &[u8] {
        self.source
            .bytes()
            .get(self.bytes.clone())
            .unwrap_or_default()
    }

    /// Appends the text of the padded line to `output`.
    ///
    /// A line of a body of text answers through [`Source::slice`], which
    /// checks the two ends of its span, and a line that holds a lone
    /// surrogate is decoded with the loss the module documentation names.
    fn write(&self, output: &mut String) {
        push_spaces(output, self.padding);
        let text = self.source.slice(self.bytes.start, self.bytes.end);
        if text.len() == self.bytes.len() {
            output.push_str(text);
        } else {
            push_lossy(output, self.wtf8());
        }
    }

    /// Returns the pieces of the padded line up to its code unit `read`.
    ///
    /// A line of ASCII takes its text up to that unit, which a body of text
    /// answers with a check of the two ends of the span, and any other line
    /// takes its first `read` code units.
    fn sub_lines(&self, read: usize) -> SubLines<'_> {
        if self.is_ascii() {
            let end = self.bytes.start + read.saturating_sub(self.padding).min(self.units);
            SubLines::Ascii {
                padding: self.padding,
                text: self.source.slice(self.bytes.start, end),
            }
        } else {
            SubLines::Units(self.code_units(read))
        }
    }

    /// Returns the first `count` code units of the padded line.
    ///
    /// A code unit takes at most three bytes, so the units lie inside the
    /// first `3 * count` bytes of the line, taken on to the end of the
    /// character they cut. Those bytes are converted and the units past
    /// `count` dropped, which leaves a leading surrogate alone where the
    /// count cuts an astral character, as `slice` of the reference does.
    fn code_units(&self, count: usize) -> Vec<u16> {
        let wtf8 = self.wtf8();
        let spaces = self.padding.min(count);
        let mut end = wtf8.len().min(3 * (count - spaces));
        while wtf8.get(end).is_some_and(|&byte| byte & 0xc0 == 0x80) {
            end += 1;
        }
        let mut units = Vec::with_capacity(spaces + end);
        units.resize(spaces, u16::from(b' '));
        push_utf16(&mut units, wtf8.get(..end).unwrap_or_default());
        units.truncate(count);
        units
    }
}

/// The pieces a long line prints, read up to the end of the piece after the caret's.
enum SubLines<'a> {
    /// A line of ASCII, whose code units are its bytes: its padding and its text.
    Ascii {
        /// The spaces in front of the line.
        padding: usize,
        /// The text of the line of the body up to the last unit read.
        text: &'a str,
    },
    /// Any other line, the code units read of it.
    Units(Vec<u16>),
}

impl SubLines<'_> {
    /// Appends the code units `units` of the line to `output`.
    ///
    /// A piece of code units is decoded with the loss the module
    /// documentation names, as `String::from_utf16_lossy` decodes it.
    fn write(&self, output: &mut String, units: Range<usize>) {
        match self {
            Self::Ascii { padding, text } => {
                push_spaces(
                    output,
                    (*padding).min(units.end).saturating_sub(units.start),
                );
                let from = units.start.max(*padding) - padding;
                let to = units.end.max(*padding) - padding;
                output.push_str(text.get(from..to).unwrap_or_default());
            }
            Self::Units(read) => output.extend(
                char::decode_utf16(read.get(units).unwrap_or_default().iter().copied())
                    .map(|decoded| decoded.unwrap_or(char::REPLACEMENT_CHARACTER)),
            ),
        }
    }
}

/// The prefix of a line of the excerpt.
#[derive(Clone, Copy)]
enum Prefix {
    /// `${n} |`, the number of a line of the source.
    Line(usize),
    /// `|`, the bar of a piece of a long line and of the caret.
    Bar,
}

impl Prefix {
    /// Returns the length of the prefix, which is ASCII, so its bytes are its code units.
    fn len(self) -> usize {
        match self {
            Self::Line(number) => {
                number
                    .checked_ilog10()
                    .map_or(1, |digits| widen(digits) + 1)
                    + 2
            }
            Self::Bar => 1,
        }
    }

    /// Appends the prefix padded to `pad_len`, `prefix.padStart(padLen)`, to `output`.
    fn write(self, output: &mut String, pad_len: usize) {
        push_spaces(output, pad_len.saturating_sub(self.len()));
        match self {
            // Writing into a `String` cannot fail.
            Self::Line(number) => {
                let _ = write!(output, "{number} |");
            }
            Self::Bar => output.push('|'),
        }
    }
}

/// The text of a prefixed line of the context of an excerpt.
#[derive(Clone)]
enum Piece<'a> {
    /// A whole line of the padded body.
    Line(&'a PaddedLine<'a>),
    /// `'^'.padStart(column)`, a caret under the given column.
    Caret(usize),
}

impl Piece<'_> {
    /// Returns whether the piece is the empty string.
    fn is_empty(&self) -> bool {
        match self {
            Self::Line(line) => line.units() == 0,
            Self::Caret(_) => false,
        }
    }

    /// Returns the most bytes the text of the piece takes.
    fn capacity(&self) -> usize {
        match self {
            Self::Line(line) => line.padding + line.bytes.len(),
            Self::Caret(column) => (*column).max(1),
        }
    }

    /// Appends the text of the piece to `output`.
    fn write(&self, output: &mut String) {
        match self {
            Self::Line(line) => line.write(output),
            Self::Caret(column) => push_caret(output, *column),
        }
    }
}

/// Prints the pieces of a line longer than [`MINIFIED_LINE_LENGTH`].
///
/// `location_line` is the line, `line_num` its printed number and `column_num`
/// the printed column, from which the piece that carries the caret follows.
/// The reference cuts the whole line into pieces and prints the first ones up
/// to the piece after the caret's, so the units past that piece are never
/// read. Every printed piece holds units and the caret line a caret, so every
/// line of this excerpt exists and takes the space before its text, and the
/// first piece, which a line longer than 120 units always has, carries the
/// widest prefix, `${lineNum} |`, so each prefix is padded to its length.
///
/// Derived from graphql-js 16.12.0, src/language/printLocation.ts, MIT.
fn print_minified_line(
    output: &mut String,
    location_line: &PaddedLine<'_>,
    line_num: usize,
    column_num: usize,
) {
    let sub_line_index = column_num / SUB_LINE_WIDTH;
    let sub_line_column_num = column_num % SUB_LINE_WIDTH;
    let units = location_line.units();
    let sub_lines = units.div_ceil(SUB_LINE_WIDTH);
    let read = units.min(
        sub_line_index
            .saturating_add(2)
            .saturating_mul(SUB_LINE_WIDTH),
    );
    let pieces = location_line.sub_lines(read);
    let piece = |at: usize| at * SUB_LINE_WIDTH..((at + 1) * SUB_LINE_WIDTH).min(units);

    let pad_len = Prefix::Line(line_num).len();
    // Every line after the first starts a line of its own with the bar padded
    // to that width and the space before its text.
    let mut bar = String::with_capacity(pad_len + 2);
    bar.push('\n');
    Prefix::Bar.write(&mut bar, pad_len);
    bar.push(' ');
    // `slice(1, subLineIndex + 1)` clamps its end to the length of the array.
    let end = (sub_line_index + 1).min(sub_lines);
    output.reserve((end + 3) * bar.len() + 3 * read + SUB_LINE_WIDTH);
    Prefix::Line(line_num).write(output, pad_len);
    output.push(' ');
    pieces.write(output, piece(0));
    for at in 1..end {
        output.push_str(&bar);
        pieces.write(output, piece(at));
    }
    output.push_str(&bar);
    push_caret(output, sub_line_column_num);
    // `subLines[subLineIndex + 1]` is undefined past the last piece, and
    // that line drops out.
    if sub_line_index + 1 < sub_lines {
        output.push_str(&bar);
        pieces.write(output, piece(sub_line_index + 1));
    }
}

/// Prints the line before, the line, the caret and the line after.
///
/// `lines` are the lines of the padded body, `location_line` the located line,
/// `line_index` its index, `line_num` its printed number and `column_num` the
/// printed column.
///
/// Derived from graphql-js 16.12.0, src/language/printLocation.ts, MIT.
fn print_context_lines(
    output: &mut String,
    lines: &PaddedLines<'_>,
    location_line: &PaddedLine<'_>,
    line_index: usize,
    line_num: usize,
    column_num: usize,
) {
    // A negative index and an index past the end both read `undefined` in
    // the reference, which the checked arithmetic and `get` reproduce.
    let previous = line_index.checked_sub(1).and_then(|i| lines.get(i));
    let next = lines.get(line_index + 1);
    print_prefixed_lines(
        output,
        &[
            // Lines specified like this: ["prefix", "string"],
            (
                Prefix::Line(line_num - 1),
                previous.as_ref().map(Piece::Line),
            ),
            (Prefix::Line(line_num), Some(Piece::Line(location_line))),
            (Prefix::Bar, Some(Piece::Caret(column_num))),
            (Prefix::Line(line_num + 1), next.as_ref().map(Piece::Line)),
        ],
    );
}

/// Pads every prefix to the widest one and joins the lines that exist.
///
/// Derived from graphql-js 16.12.0, src/language/printLocation.ts, MIT.
fn print_prefixed_lines(output: &mut String, lines: &[PrefixedLine<'_>]) {
    // `line !== undefined` keeps an empty string, so only an absent piece
    // drops out.
    let existing_lines = || {
        lines
            .iter()
            .filter_map(|(prefix, line)| line.as_ref().map(|line| (*prefix, line)))
    };

    let pad_len = existing_lines()
        .map(|(prefix, _)| prefix.len())
        .max()
        .unwrap_or(0);
    output.reserve(
        existing_lines()
            .map(|(_, line)| pad_len + 2 + line.capacity())
            .sum(),
    );
    for (i, (prefix, line)) in existing_lines().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        prefix.write(output, pad_len);
        // `(line ? ' ' + line : '')`: ToBoolean of a string is false for the
        // empty string, so an empty piece gets no separating space.
        if !line.is_empty() {
            output.push(' ');
            line.write(output);
        }
    }
}

/// Appends `'^'.padStart(column)`, a caret under the given column, to `output`.
///
/// `padStart` fills with spaces up to the requested length and leaves a string
/// that is already that long alone, so a column of one or zero gives the bare
/// caret.
fn push_caret(output: &mut String, column: usize) {
    push_spaces(output, column.saturating_sub(1));
    output.push('^');
}

/// Appends `count` spaces to `output`.
///
/// The spaces are copied from a run of spaces, as many as it holds a step.
fn push_spaces(output: &mut String, count: usize) {
    const SPACES: &str = "                                                                ";
    let mut left = count;
    while left > 0 {
        let step = left.min(SPACES.len());
        output.push_str(SPACES.get(..step).unwrap_or_default());
        left -= step;
    }
}

/// Appends the WTF-8 bytes `wtf8` to `output` with U+FFFD in place of each lone surrogate.
///
/// This is the text `String::from_utf16_lossy` makes of the code units of the
/// bytes. A lone surrogate is the three bytes WTF-8 gives it, and a well
/// formed WTF-8 never writes the two halves of a pair that way, so the three
/// bytes UTF-8 refuses are always one lone surrogate, which no unit next to it
/// pairs with.
// A `from_utf8_unchecked` would skip the recheck.
fn push_lossy(output: &mut String, wtf8: &[u8]) {
    let mut rest = wtf8;
    loop {
        let error = match core::str::from_utf8(rest) {
            Ok(text) => {
                output.push_str(text);
                return;
            }
            Err(error) => error,
        };
        let (text, tail) = rest.split_at(error.valid_up_to());
        // The stretch before the error is UTF-8 by the contract of
        // `valid_up_to`, so the default is never taken.
        output.push_str(core::str::from_utf8(text).unwrap_or_default());
        output.push(char::REPLACEMENT_CHARACTER);
        rest = tail.get(3..).unwrap_or_default();
    }
}
