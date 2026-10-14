// SPDX-License-Identifier: MIT
/*
 * aoviewer-core commonmark block: the block structure of the subset.
 *
 * Paragraphs, ATX headings, thematic breaks, code blocks, block quotes, lists
 * and GitHub tables are the blocks the subset knows, read line by line in the
 * two steps of the specification, where a line first continues the open blocks
 * and then opens new ones.
 */

//! The blocks of the subset.
//!
//! A line walks the open blocks from the document down and continues each one
//! its prefix matches, then opens the blocks whose starts follow, and then
//! adds its rest to the deepest block, or to an open paragraph as a lazy
//! continuation line. Tabs count to the next multiple of four columns wherever
//! columns decide the structure, and a tab split between two containers gives
//! its remaining columns to the content as spaces.
//!
//! A list is tight unless a blank line stands between two of its items or
//! between two blocks of one item. Every block records the first and the last
//! line that gave it content, and the list compares them when it closes. A
//! line that holds only the marker of a block quote gives the quote content,
//! and a blank line inside a fenced code block gives the code block content,
//! while a blank line anywhere else gives no block content.
//!
//! A table opens on a delimiter row directly under a paragraph whose last line
//! has as many cells, and it takes every following line that is not blank and
//! starts no other block as a row, with as many cells as the header. The
//! delimiter row is tried after every block start of the [CommonMark
//! specification], so a line that starts another block never opens a table,
//! and a paragraph whose last line failed as a header is not tried again.
//!
//! [CommonMark specification]: https://spec.commonmark.org/0.31.2/
//! [GitHub Flavored Markdown specification]: https://github.github.com/gfm/

use alloc::{
    string::String,
    vec::Vec, //
};

use super::{
    Align,
    Doc,
    Fence,
    Heading,
    Kind,
    NONE,
    ROOT,
    Span,
    append,
    index,
    insert_before,
    push, //
};

/// The tab stop of the block structure.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
const TAB_STOP: usize = 4;

/// The indentation of an indented code block, one more than the most a block
/// start may be indented.
const CODE_INDENT: usize = 4;

/// The digits an ordered list marker carries at most.
const MAX_MARKER_DIGITS: usize = 9;

/// The columns after a list marker beyond which the content starts one space
/// after the marker and the rest is indented code.
const MAX_MARKER_SPACES: usize = 4;

/// The most `#` an ATX heading opens with.
const MAX_LEVEL: usize = 6;

/// The shortest run of a code fence.
const MIN_FENCE: usize = 3;

/// A copy of source bytes into the buffer of a document.
///
/// The copy of `len` bytes starts at `buf` in the buffer and at `src` in the
/// source, so a position of the buffer that falls inside it has a position
/// in the source.
#[derive(Clone, Copy, Debug)]
pub(super) struct Map {
    /// The first byte in the buffer.
    buf: u32,
    /// The first byte in the source.
    src: u32,
    /// The number of bytes.
    len: u32,
}

/// What the parser knows of a block while it reads.
#[derive(Clone, Copy, Debug)]
struct State {
    /// Whether the block takes more lines.
    open: bool,
    /// The number of blocks around it, the document counting as none.
    depth: u32,
    /// The first line that gave it content.
    first_line: u32,
    /// The last line that gave it content.
    last_line: u32,
    /// Whether a delimiter row found no header in the paragraph.
    no_table: bool,
}

/// The block parser over the lines of one text.
struct Parser<'s, 'd> {
    /// The source.
    src: &'s str,
    /// The document being built.
    doc: &'d mut Doc,
    /// The copies of source bytes into the buffer.
    maps: Vec<Map>,
    /// One state per block node.
    state: Vec<State>,
    /// The deepest nesting a block may have.
    max_depth: u32,
    /// The bytes of the current line without its line ending.
    line: &'s [u8],
    /// The source offset of the current line.
    line_start: usize,
    /// The number of the current line, counted from 1.
    line_number: u32,
    /// The byte of the line the parser stands on.
    offset: usize,
    /// The column the parser stands on.
    column: usize,
    /// The first byte at or after `offset` that is no space or tab.
    first_nonspace: usize,
    /// The column of `first_nonspace`.
    first_nonspace_column: usize,
    /// The columns from `column` to `first_nonspace_column`.
    indent: usize,
    /// Whether nothing but spaces and tabs follows `offset`.
    blank: bool,
    /// Whether the tab at `offset` has given away some of its columns.
    partial_tab: bool,
    /// The byte before which no thematic break can start on this line.
    no_break_before: usize,
    /// The deepest open block.
    current: u32,
}

/// Reads the blocks of `text` into `doc`, and returns the copies of source
/// bytes into its buffer.
pub(super) fn parse(doc: &mut Doc, text: &str, max_depth: u16) -> Vec<Map> {
    let mut p = Parser {
        src: text,
        doc,
        maps: Vec::new(),
        state: Vec::new(),
        max_depth: u32::from(max_depth),
        line: &[],
        line_start: 0,
        line_number: 0,
        offset: 0,
        column: 0,
        first_nonspace: 0,
        first_nonspace_column: 0,
        indent: 0,
        blank: false,
        partial_tab: false,
        no_break_before: 0,
        current: ROOT,
    };
    p.state.push(State {
        open: true,
        depth: 0,
        first_line: 0,
        last_line: 0,
        no_table: false,
    });
    let bytes = text.as_bytes();
    let mut at = 0;
    // Each pass consumes one line and its line ending, at least one byte.
    while at < bytes.len() {
        let end = bytes[at..]
            .iter()
            .position(|&b| b == b'\n' || b == b'\r')
            .map_or(bytes.len(), |i| at + i);
        p.process_line(at, end);
        at = match bytes.get(end) {
            Some(b'\r') if bytes.get(end + 1) == Some(&b'\n') => end + 2,
            Some(_) => end + 1,
            None => end,
        };
    }
    p.finish();
    p.maps
}

/// Returns the source offsets of the buffer positions `positions`, in
/// ascending order, through the copies `maps`.
pub(super) fn sources(maps: &[Map], positions: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = positions
        .iter()
        .filter_map(|&pos| {
            let i = maps.partition_point(|m| m.buf <= pos).checked_sub(1)?;
            let m = maps[i];
            (pos < m.buf + m.len).then(|| m.src + (pos - m.buf))
        })
        .collect();
    out.sort_unstable();
    out
}

/// Tells whether `b` is a space or a tab.
fn is_space_or_tab(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Tells whether `b` is ASCII white space: a space, a tab, a line ending, a
/// line tabulation or a form feed.
pub(super) fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C)
}

/// Returns the byte `i` of `line`, or a line feed at and after its end.
fn byte(line: &[u8], i: usize) -> u8 {
    line.get(i).copied().unwrap_or(b'\n')
}

/// Returns the number of `#` and the bytes of the opening sequence of an
/// ATX heading at `at`, its white space after included.
fn atx_start(line: &[u8], at: usize) -> Option<(u8, usize)> {
    let hashes = line[at.min(line.len())..]
        .iter()
        .take_while(|&&b| b == b'#')
        .count();
    if hashes == 0 || hashes > MAX_LEVEL {
        return None;
    }
    let after = at + hashes;
    if after >= line.len() {
        return Some((u8::try_from(hashes).unwrap_or(1), hashes + 1));
    }
    let spaces = line[after..]
        .iter()
        .take_while(|&&b| is_space_or_tab(b))
        .count();
    (spaces > 0).then(|| (u8::try_from(hashes).unwrap_or(1), hashes + spaces))
}

/// Returns the byte and the length of an opening code fence at `at`.
///
/// A backtick fence refuses a backtick in its info string.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn open_fence(line: &[u8], at: usize) -> Option<(u8, usize)> {
    let c = byte(line, at);
    if c != b'`' && c != b'~' {
        return None;
    }
    let len = line[at..].iter().take_while(|&&b| b == c).count();
    if len < MIN_FENCE || (c == b'`' && line[at + len..].contains(&b'`')) {
        return None;
    }
    Some((c, len))
}

/// Returns the length of a closing fence of `c` at `at`, a run followed by
/// nothing but spaces and tabs.
fn close_fence(line: &[u8], at: usize, c: u8) -> usize {
    let len = line[at.min(line.len())..]
        .iter()
        .take_while(|&&b| b == c)
        .count();
    let rest_blank = line[at + len..].iter().all(|&b| is_space_or_tab(b));
    if len >= MIN_FENCE && rest_blank {
        len
    } else {
        0
    }
}

/// A list marker.
#[derive(Clone, Copy, Debug)]
struct Marker {
    /// Whether the marker is a number.
    ordered: bool,
    /// The bullet, or the byte after the number.
    byte: u8,
    /// The number.
    start: u32,
}

/// Returns the length of a list marker at `at` and the marker.
///
/// A marker that interrupts a paragraph needs content after it, and an ordered
/// one needs the number 1.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn list_marker(line: &[u8], at: usize, interrupts: bool) -> Option<(usize, Marker)> {
    let c = byte(line, at);
    let (len, marker) = if matches!(c, b'*' | b'-' | b'+') {
        let marker = Marker {
            ordered: false,
            byte: c,
            start: 0,
        };
        (1, marker)
    } else if c.is_ascii_digit() {
        let digits = line[at..]
            .iter()
            .take(MAX_MARKER_DIGITS)
            .take_while(|b| b.is_ascii_digit())
            .count();
        let start = line[at..at + digits]
            .iter()
            .fold(0u32, |n, &d| n * 10 + u32::from(d - b'0'));
        let delimiter = byte(line, at + digits);
        if (delimiter != b'.' && delimiter != b')') || (interrupts && start != 1) {
            return None;
        }
        let marker = Marker {
            ordered: true,
            byte: delimiter,
            start,
        };
        (digits + 1, marker)
    } else {
        return None;
    };
    if !is_space(byte(line, at + len)) {
        return None;
    }
    if interrupts
        && line[(at + len).min(line.len())..]
            .iter()
            .all(|&b| is_space_or_tab(b))
    {
        return None;
    }
    Some((len, marker))
}

/// Splits a table row into the ranges of its cells, untrimmed.
///
/// A leading and a trailing pipe are optional, a pipe after a backslash
/// belongs to its cell, and a row of no cell is none.
///
/// [GitHub Flavored Markdown specification]: https://github.github.com/gfm/
fn row_cells(row: &[u8]) -> Option<Vec<(usize, usize)>> {
    let mut cells = Vec::new();
    let mut at = pipe_end(row, 0);
    // Each pass takes a cell or a pipe, at least one byte, or stops.
    while at < row.len() {
        let mut end = at;
        while end < row.len() && row[end] != b'|' {
            end += if row[end] == b'\\' && row.get(end + 1) == Some(&b'|') {
                2
            } else {
                1
            };
        }
        let end = end.min(row.len());
        let pipe = pipe_end(row, end);
        if end > at || pipe > 0 {
            cells.push((at, end));
        }
        if pipe == 0 {
            break;
        }
        at = end + pipe;
    }
    (!cells.is_empty()).then_some(cells)
}

/// Returns the length of a pipe and the white space after it at `at`, or 0.
fn pipe_end(row: &[u8], at: usize) -> usize {
    if row.get(at) != Some(&b'|') {
        return 0;
    }
    1 + row[at + 1..]
        .iter()
        .take_while(|&&b| matches!(b, b' ' | b'\t' | 0x0B | 0x0C))
        .count()
}

/// Returns `cell` without the white space around it.
fn trim(cell: &[u8]) -> &[u8] {
    let start = cell
        .iter()
        .position(|&b| !is_space(b))
        .unwrap_or(cell.len());
    let end = cell
        .iter()
        .rposition(|&b| !is_space(b))
        .map_or(start, |i| i + 1);
    &cell[start..end.max(start)]
}

/// Returns the alignments of a delimiter row, or none when a cell is no
/// delimiter.
fn delimiter_row(row: &[u8]) -> Option<Vec<Align>> {
    let cells = row_cells(row)?;
    let mut aligns = Vec::with_capacity(cells.len());
    for (start, end) in cells {
        let cell = trim(&row[start..end]);
        let left = cell.first() == Some(&b':');
        let right = cell.len() > 1 && cell.last() == Some(&b':');
        let dashes = &cell[usize::from(left)..cell.len() - usize::from(right)];
        if dashes.is_empty() || dashes.iter().any(|&b| b != b'-') {
            return None;
        }
        aligns.push(match (left, right) {
            (true, true) => Align::Center,
            (true, false) => Align::Left,
            (false, true) => Align::Right,
            (false, false) => Align::None,
        });
    }
    Some(aligns)
}

/// Tells whether `row` is a setext heading underline of dashes, a run of `-`
/// with nothing after it but spaces and tabs.
///
/// Such a line under a paragraph is a heading to a GitHub host, which tries it
/// before the table extension, so it never opens a table, and the subset keeps
/// it as the text of the paragraph.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
/// [blocks.c]: https://github.com/github/cmark-gfm/blob/27d942c8b0a62d192f616e5bf3578f4b6a89e180/src/blocks.c
fn setext_dashes(row: &[u8]) -> bool {
    let dashes = row.iter().take_while(|&&b| b == b'-').count();
    dashes > 0 && row[dashes..].iter().all(|&b| is_space_or_tab(b))
}

/// Tells whether a block of `parent` may hold a block of `child`.
fn can_contain(parent: Kind, child: Kind) -> bool {
    match parent {
        Kind::Document | Kind::Quote | Kind::Item { .. } => !matches!(child, Kind::Item { .. }),
        Kind::List { .. } => matches!(child, Kind::Item { .. }),
        Kind::Table => matches!(child, Kind::Row { .. }),
        Kind::Row { .. } => matches!(child, Kind::Cell { .. }),
        _ => false,
    }
}

impl Parser<'_, '_> {
    /// Returns the kind of the node `i`.
    fn kind(&self, i: u32) -> Kind {
        self.doc.nodes[i as usize].kind
    }

    /// Returns the parent of the node `i`.
    fn parent(&self, i: u32) -> u32 {
        self.doc.nodes[i as usize].parent
    }

    /// Returns the byte `i` of the current line.
    fn peek(&self, i: usize) -> u8 {
        byte(self.line, i)
    }

    /// Reads the line from `start` to `end` of the source.
    fn process_line(&mut self, start: usize, end: usize) {
        let src: &str = self.src;
        self.line = &src.as_bytes()[start..end];
        self.line_start = start;
        self.line_number += 1;
        self.offset = 0;
        self.column = 0;
        self.first_nonspace = 0;
        self.first_nonspace_column = 0;
        self.indent = 0;
        self.blank = false;
        self.partial_tab = false;
        self.no_break_before = 0;
        let Some(last_matched) = self.check_open() else {
            return;
        };
        let container = self.open_new(last_matched);
        self.add_text(container, last_matched);
    }

    /// Moves `count` bytes on, or `count` columns when `columns` is set, in
    /// which case a tab may give away only some of its columns.
    fn advance(&mut self, count: usize, columns: bool) {
        let mut count = count;
        // Each pass takes at least one column off `count`.
        while count > 0 && self.offset <= self.line.len() {
            if self.peek(self.offset) == b'\t' {
                let to_tab = TAB_STOP - self.column % TAB_STOP;
                if columns {
                    self.partial_tab = to_tab > count;
                    let step = count.min(to_tab);
                    self.column += step;
                    if !self.partial_tab {
                        self.offset += 1;
                    }
                    count -= step;
                } else {
                    self.partial_tab = false;
                    self.column += to_tab;
                    self.offset += 1;
                    count -= 1;
                }
            } else {
                self.partial_tab = false;
                self.offset += 1;
                self.column += 1;
                count -= 1;
            }
        }
    }

    /// Finds the first byte at or after the offset that is no space or tab,
    /// its column, the indentation and whether the line is blank from here.
    fn find_first_nonspace(&mut self) {
        let mut to_tab = TAB_STOP - self.column % TAB_STOP;
        if self.first_nonspace <= self.offset {
            self.first_nonspace = self.offset;
            self.first_nonspace_column = self.column;
            // Each pass takes one byte of the line.
            while let Some(&b) = self.line.get(self.first_nonspace) {
                if b == b' ' {
                    self.first_nonspace += 1;
                    self.first_nonspace_column += 1;
                    to_tab -= 1;
                    if to_tab == 0 {
                        to_tab = TAB_STOP;
                    }
                } else if b == b'\t' {
                    self.first_nonspace += 1;
                    self.first_nonspace_column += to_tab;
                    to_tab = TAB_STOP;
                } else {
                    break;
                }
            }
        }
        self.indent = self.first_nonspace_column.saturating_sub(self.column);
        self.blank = self.first_nonspace >= self.line.len();
    }

    /// Continues the open blocks the line matches, and returns the last one,
    /// or none when the line closed a fenced code block and is done.
    fn check_open(&mut self) -> Option<u32> {
        let mut container = ROOT;
        // Each pass descends one open block, at most the depth of the tree.
        loop {
            let child = self.doc.nodes[container as usize].last;
            if child == NONE || !self.state[child as usize].open {
                return Some(container);
            }
            container = child;
            self.find_first_nonspace();
            let matched = match self.kind(container) {
                Kind::Quote => self.quote_prefix(),
                Kind::Item { content } => self.item_prefix(container, content as usize),
                Kind::Code {
                    fence: Some(fence), ..
                } => {
                    if self.closes(fence) {
                        self.touch(container);
                        self.current = self.finalize(container);
                        return None;
                    }
                    self.fence_prefix(fence);
                    true
                }
                Kind::Code { fence: None, .. } => self.indented_prefix(),
                Kind::Heading { .. } => false,
                Kind::Paragraph => !self.blank,
                Kind::Table => {
                    !self.blank && row_cells(&self.line[self.first_nonspace..]).is_some()
                }
                _ => true,
            };
            if !matched {
                return Some(self.parent(container));
            }
        }
    }

    /// Takes the marker of a block quote.
    fn quote_prefix(&mut self) -> bool {
        if self.indent > 3 || self.peek(self.first_nonspace) != b'>' {
            return false;
        }
        self.advance(self.indent + 1, true);
        if is_space_or_tab(self.peek(self.offset)) {
            self.advance(1, true);
        }
        true
    }

    /// Takes the indentation of a list item whose content starts at column
    /// `content`, or a blank line of an item that has content.
    fn item_prefix(&mut self, item: u32, content: usize) -> bool {
        if self.indent >= content {
            self.advance(content, true);
            true
        } else if self.blank && self.doc.nodes[item as usize].first != NONE {
            self.advance(self.first_nonspace - self.offset, false);
            true
        } else {
            false
        }
    }

    /// Takes the indentation of an indented code block, or a blank line.
    fn indented_prefix(&mut self) -> bool {
        if self.indent >= CODE_INDENT {
            self.advance(CODE_INDENT, true);
            true
        } else if self.blank {
            self.advance(self.first_nonspace - self.offset, false);
            true
        } else {
            false
        }
    }

    /// Tells whether the line closes the code block of `fence`, taking the
    /// closing fence when it does.
    fn closes(&mut self, fence: Fence) -> bool {
        if self.indent > 3 || self.peek(self.first_nonspace) != fence.byte {
            return false;
        }
        let len = close_fence(self.line, self.first_nonspace, fence.byte);
        if len < fence.len as usize {
            return false;
        }
        self.advance(self.first_nonspace + len - self.offset, false);
        true
    }

    /// Takes up to the indentation of the opening fence from a content line.
    fn fence_prefix(&mut self, fence: Fence) {
        let mut spaces = fence.indent;
        while spaces > 0 && is_space_or_tab(self.peek(self.offset)) {
            self.advance(1, true);
            spaces -= 1;
        }
    }

    /// Opens the blocks whose starts follow on the line under `container`,
    /// and returns the deepest.
    fn open_new(&mut self, container: u32) -> u32 {
        let mut container = container;
        let mut maybe_lazy = self.kind(self.current) == Kind::Paragraph;
        // Each pass opens a block and takes at least its marker, so the line
        // ends the loop, and the depth cap ends it sooner.
        loop {
            let kind = self.kind(container);
            if matches!(kind, Kind::Code { .. })
                || self.state[container as usize].depth >= self.max_depth
            {
                break;
            }
            self.find_first_nonspace();
            match self.open_one(container, kind, maybe_lazy) {
                Some(opened) => container = opened,
                None => break,
            }
            if matches!(
                self.kind(container),
                Kind::Paragraph | Kind::Heading { .. } | Kind::Code { .. } | Kind::Table
            ) {
                break;
            }
            maybe_lazy = false;
        }
        container
    }

    /// Opens the block whose start stands at the first nonspace byte, and
    /// returns it, or none when no block starts there.
    fn open_one(&mut self, container: u32, kind: Kind, maybe_lazy: bool) -> Option<u32> {
        let indented = self.indent >= CODE_INDENT;
        let at = self.first_nonspace;
        if !indented && self.peek(at) == b'>' {
            self.advance(at + 1 - self.offset, false);
            if is_space_or_tab(self.peek(self.offset)) {
                self.advance(1, true);
            }
            return Some(self.add_child(container, Kind::Quote));
        }
        if !indented && let Some((level, len)) = atx_start(self.line, at) {
            self.advance(at + len - self.offset, false);
            self.doc.headings.push(Heading {
                at: index(self.line_start + at),
                level,
            });
            return Some(self.add_child(container, Kind::Heading { level }));
        }
        if !indented && let Some((c, len)) = open_fence(self.line, at) {
            let fence = Fence {
                byte: c,
                len: index(len),
                indent: index(at - self.offset),
            };
            let code = Kind::Code {
                fence: Some(fence),
                info: Span::default(),
            };
            let node = self.add_child(container, code);
            self.advance(at + len - self.offset, false);
            return Some(node);
        }
        if !indented && self.no_break_before <= at && self.thematic_break(at) {
            let node = self.add_child(container, Kind::Rule);
            self.advance(self.line.len() - self.offset, false);
            return Some(node);
        }
        if self.indent < CODE_INDENT
            && let Some((len, marker)) = list_marker(self.line, at, kind == Kind::Paragraph)
        {
            return Some(self.open_item(container, kind, len, marker));
        }
        if indented && !maybe_lazy && !self.blank {
            self.advance(CODE_INDENT, true);
            let code = Kind::Code {
                fence: None,
                info: Span::default(),
            };
            return Some(self.add_child(container, code));
        }
        if !indented && kind == Kind::Paragraph {
            return self.open_table(container);
        }
        None
    }

    /// Tells whether a thematic break stands at `at`, and records where the
    /// search failed when none does.
    fn thematic_break(&mut self, at: usize) -> bool {
        let c = self.peek(at);
        if !matches!(c, b'*' | b'_' | b'-') {
            self.no_break_before = at;
            return false;
        }
        let mut count = 0;
        let mut i = at;
        while let Some(&b) = self.line.get(i) {
            if b == c {
                count += 1;
            } else if !is_space_or_tab(b) {
                break;
            }
            i += 1;
        }
        if count >= 3 && i >= self.line.len() {
            true
        } else {
            self.no_break_before = i;
            false
        }
    }

    /// Opens a list item after the marker of `len` bytes, and its list when
    /// `container` is no list of the same marker.
    fn open_item(&mut self, container: u32, kind: Kind, len: usize, marker: Marker) -> u32 {
        let marker_indent = self.indent;
        self.advance(self.first_nonspace + len - self.offset, false);
        let saved = (self.partial_tab, self.offset, self.column);
        // Each pass takes a column of white space, at most six.
        while self.column - saved.2 <= MAX_MARKER_SPACES + 1
            && is_space_or_tab(self.peek(self.offset))
        {
            self.advance(1, true);
        }
        let spaces = self.column - saved.2;
        let padding =
            if !(1..=MAX_MARKER_SPACES).contains(&spaces) || self.offset >= self.line.len() {
                (self.partial_tab, self.offset, self.column) = saved;
                if spaces > 0 {
                    self.advance(1, true);
                }
                len + 1
            } else {
                len + spaces
            };
        let same = matches!(kind, Kind::List { ordered, marker: m, .. }
            if ordered == marker.ordered && m == marker.byte);
        let list = if same {
            container
        } else {
            let list = Kind::List {
                ordered: marker.ordered,
                marker: marker.byte,
                start: marker.start,
                tight: true,
            };
            self.add_child(container, list)
        };
        let content = index(marker_indent + padding);
        self.add_child(list, Kind::Item { content })
    }

    /// Opens a table on a delimiter row under the paragraph `paragraph`, and
    /// returns it, or none when the line is no delimiter row, is a setext
    /// heading underline or the last line of the paragraph has another
    /// number of cells.
    fn open_table(&mut self, paragraph: u32) -> Option<u32> {
        let row = &self.line[self.first_nonspace..];
        if self.state[paragraph as usize].no_table || setext_dashes(row) {
            return None;
        }
        let aligns = delimiter_row(row)?;
        let span = self.doc.nodes[paragraph as usize].text;
        let content = &self.doc.buf.as_bytes()[span.start as usize..span.end as usize];
        let body = content.strip_suffix(b"\n").unwrap_or(content);
        let last = body.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let header_start = span.start as usize + last;
        let header: String = self
            .doc
            .buf
            .get(header_start..span.start as usize + body.len())
            .unwrap_or_default()
            .into();
        let cells = row_cells(header.as_bytes()).filter(|c| c.len() == aligns.len());
        let Some(cells) = cells else {
            self.state[paragraph as usize].no_table = true;
            return None;
        };
        let header_src = sources(&self.maps, &[index(header_start)]).first().copied();
        if last > 0 {
            self.split_paragraph(paragraph, header_start);
        }
        // The paragraph node becomes the table, so it stays the block the
        // line matched, and its header is the line before this one.
        let table = paragraph;
        self.doc.nodes[table as usize].kind = Kind::Table;
        self.doc.nodes[table as usize].text = Span::default();
        self.state[table as usize].first_line = self.line_number.saturating_sub(1);
        self.touch(table);
        let row = self.add_child(table, Kind::Row { header: true });
        for (&(start, end), &align) in cells.iter().zip(&aligns) {
            let src = header_src.map(|s| s as usize + start);
            self.add_cell(row, &header.as_bytes()[start..end], src, align);
        }
        self.state[row as usize].open = false;
        self.advance(self.line.len() - self.offset, false);
        Some(table)
    }

    /// Puts the lines of `paragraph` before its last one, which starts at
    /// `header_start` in the buffer, into a closed paragraph before it.
    fn split_paragraph(&mut self, paragraph: u32, header_start: usize) {
        let span = self.doc.nodes[paragraph as usize].text;
        let lines = Span::new(span.start as usize, header_start);
        let before = push(&mut self.doc.nodes, Kind::Paragraph, lines);
        insert_before(&mut self.doc.nodes, paragraph, before);
        let state = self.state[paragraph as usize];
        self.state.push(State {
            open: false,
            last_line: self.line_number.saturating_sub(2),
            ..state
        });
        self.trim_leaf(before);
    }

    /// Adds the row of the current line to `table`, with the cells of its
    /// header, empty ones where the line has fewer and none of the rest.
    fn add_row(&mut self, table: u32) {
        let row_bytes = &self.line[self.first_nonspace.min(self.line.len())..];
        let cells = row_cells(row_bytes).unwrap_or_default();
        let header = self.doc.nodes[table as usize].first;
        let mut aligns = Vec::new();
        let mut cell = if header == NONE {
            NONE
        } else {
            self.doc.nodes[header as usize].first
        };
        while cell != NONE {
            if let Kind::Cell { align } = self.kind(cell) {
                aligns.push(align);
            }
            cell = self.doc.nodes[cell as usize].next;
        }
        let row = self.add_child(table, Kind::Row { header: false });
        for (i, &align) in aligns.iter().enumerate() {
            let (start, end) = cells.get(i).copied().unwrap_or((0, 0));
            let src = self.line_start + self.first_nonspace + start;
            self.add_cell(row, &row_bytes[start..end], Some(src), align);
        }
        self.state[row as usize].open = false;
    }

    /// Adds a cell of `bytes`, which start at the source offset `src`, to
    /// `row`, trimmed and with every `\|` written `|`.
    fn add_cell(&mut self, row: u32, bytes: &[u8], src: Option<usize>, align: Align) {
        let lead = bytes
            .iter()
            .position(|&b| !is_space(b))
            .unwrap_or(bytes.len());
        let cell = trim(bytes);
        let start = self.doc.buf.len();
        let mut from = 0;
        // Each pass copies up to the next escaped pipe, at least one byte.
        while from < cell.len() {
            let to = cell[from..]
                .windows(2)
                .position(|w| w == b"\\|")
                .map_or(cell.len(), |i| from + i);
            self.copy(&cell[from..to], src.map(|s| s + lead + from));
            if to < cell.len() {
                self.copy(b"|", src.map(|s| s + lead + to + 1));
            }
            from = to + 2;
        }
        let span = Span::new(start, self.doc.buf.len());
        let node = self.add_child(row, Kind::Cell { align });
        self.doc.nodes[node as usize].text = span;
        self.state[node as usize].open = false;
    }

    /// Appends `bytes`, which start at the source offset `src` when they
    /// have one, to the buffer, recording the copy.
    fn copy(&mut self, bytes: &[u8], src: Option<usize>) {
        let Ok(text) = core::str::from_utf8(bytes) else {
            return;
        };
        if let Some(src) = src
            && !text.is_empty()
        {
            self.maps.push(Map {
                buf: index(self.doc.buf.len()),
                src: index(src),
                len: index(text.len()),
            });
        }
        self.doc.buf.push_str(text);
    }

    /// Adds a block of `kind` under `parent`, closing the open blocks that
    /// cannot hold it, and returns it.
    fn add_child(&mut self, parent: u32, kind: Kind) -> u32 {
        let mut parent = parent;
        // Each pass closes one block toward the document, which holds every
        // block but an item, and an item is added to a list alone.
        while !can_contain(self.kind(parent), kind) && parent != ROOT {
            parent = self.finalize(parent);
        }
        let start = self.doc.buf.len();
        let node = push(&mut self.doc.nodes, kind, Span::new(start, start));
        append(&mut self.doc.nodes, parent, node);
        self.state.push(State {
            open: true,
            depth: self.state[parent as usize].depth + 1,
            first_line: self.line_number,
            last_line: self.line_number,
            no_table: false,
        });
        self.touch(parent);
        node
    }

    /// Records that the current line gave `node` and every block around it
    /// content.
    fn touch(&mut self, node: u32) {
        let mut at = node;
        // Each pass climbs one block toward the document.
        while at != NONE && self.state[at as usize].last_line != self.line_number {
            self.state[at as usize].last_line = self.line_number;
            at = self.parent(at);
        }
    }

    /// Adds the rest of the line to `container`, or to the open paragraph
    /// as a lazy continuation line.
    fn add_text(&mut self, container: u32, last_matched: u32) {
        self.find_first_nonspace();
        if self.current != last_matched
            && container == last_matched
            && !self.blank
            && self.kind(self.current) == Kind::Paragraph
        {
            let paragraph = self.current;
            self.advance(self.first_nonspace - self.offset, false);
            self.add_line(paragraph, self.line.len(), true);
            self.touch(paragraph);
            return;
        }
        // Each pass closes one open block below the last one matched.
        while self.current != last_matched && self.current != NONE {
            self.current = self.finalize(self.current);
        }
        let kind = self.kind(container);
        let mut container = container;
        match kind {
            Kind::Code { .. } => self.add_line(container, self.line.len(), false),
            _ if self.blank => {}
            Kind::Heading { .. } => {
                let end = self.heading_end();
                self.advance(self.first_nonspace - self.offset, false);
                self.add_line(container, end, true);
            }
            Kind::Paragraph => {
                self.advance(self.first_nonspace - self.offset, false);
                self.add_line(container, self.line.len(), true);
            }
            Kind::Table => self.add_row(container),
            _ => {
                container = self.add_child(container, Kind::Paragraph);
                self.advance(self.first_nonspace - self.offset, false);
                self.add_line(container, self.line.len(), true);
            }
        }
        let fenced = matches!(kind, Kind::Code { fence: Some(_), .. });
        if !self.blank || kind == Kind::Quote || fenced {
            self.touch(container);
        }
        self.current = container;
    }

    /// Returns the end of the content of an ATX heading line, before its
    /// closing sequence and the white space around it.
    ///
    /// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
    fn heading_end(&self) -> usize {
        let line = self.line;
        let mut end = line.len();
        while end > 0 && is_space(line[end - 1]) {
            end -= 1;
        }
        let mut n = end;
        while n > 0 && line[n - 1] == b'#' {
            n -= 1;
        }
        if n < end && n > 0 && is_space_or_tab(line[n - 1]) {
            end = n;
            while end > 0 && is_space(line[end - 1]) {
                end -= 1;
            }
        }
        end
    }

    /// Appends the line from the offset to `end` and a line feed to the
    /// content of the leaf `leaf`, recording the copy when `map` is set.
    ///
    /// A tab the containers took some columns of gives the rest to the content
    /// as spaces.
    ///
    /// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
    fn add_line(&mut self, leaf: u32, end: usize, map: bool) {
        if self.partial_tab {
            self.offset += 1;
            let spaces = TAB_STOP - self.column % TAB_STOP;
            for _ in 0..spaces {
                self.doc.buf.push(' ');
            }
        }
        if self.offset < end {
            let src: &str = self.src;
            let at = self.line_start + self.offset;
            let bytes = &src.as_bytes()[at..self.line_start + end];
            self.copy(bytes, map.then_some(at));
        }
        self.doc.buf.push('\n');
        self.doc.nodes[leaf as usize].text.end = index(self.doc.buf.len());
    }

    /// Drops the white space around the content of the paragraph or
    /// heading `node`.
    fn trim_leaf(&mut self, node: u32) {
        let span = self.doc.nodes[node as usize].text;
        let text = span.of(&self.doc.buf).as_bytes();
        let start = text
            .iter()
            .position(|&b| !is_space(b))
            .unwrap_or(text.len());
        let end = text
            .iter()
            .rposition(|&b| !is_space(b))
            .map_or(start, |i| i + 1);
        let base = span.start as usize;
        self.doc.nodes[node as usize].text = Span::new(base + start, base + end);
    }

    /// Closes `node` and returns its parent.
    fn finalize(&mut self, node: u32) -> u32 {
        self.state[node as usize].open = false;
        match self.kind(node) {
            Kind::Paragraph | Kind::Heading { .. } => self.trim_leaf(node),
            Kind::Code { fence: None, .. } => {
                let span = self.doc.nodes[node as usize].text;
                let text = span.of(&self.doc.buf).as_bytes();
                let end = text
                    .iter()
                    .rposition(|&b| !is_space(b))
                    .and_then(|i| {
                        text[i..]
                            .iter()
                            .position(|&b| b == b'\n')
                            .map(|j| i + j + 1)
                    })
                    .unwrap_or(0);
                self.doc.nodes[node as usize].text.end = span.start + index(end);
            }
            Kind::Code { fence, .. } => self.split_info(node, fence),
            Kind::List {
                ordered,
                marker,
                start,
                ..
            } => {
                let tight = self.tight(node);
                self.doc.nodes[node as usize].kind = Kind::List {
                    ordered,
                    marker,
                    start,
                    tight,
                };
            }
            _ => {}
        }
        self.parent(node)
    }

    /// Makes the first line of the fenced code block `node` its info string,
    /// trimmed, and the rest its content.
    ///
    /// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
    fn split_info(&mut self, node: u32, fence: Option<Fence>) {
        let span = self.doc.nodes[node as usize].text;
        let text = span.of(&self.doc.buf).as_bytes();
        let newline = text.iter().position(|&b| b == b'\n').unwrap_or(text.len());
        let first = &text[..newline];
        let lead = first.iter().position(|&b| !is_space(b)).unwrap_or(0);
        let start = span.start as usize + lead;
        let info = Span::new(start, start + trim(first).len());
        let body = (span.start as usize + newline + 1).min(span.end as usize);
        let n = &mut self.doc.nodes[node as usize];
        n.kind = Kind::Code { fence, info };
        n.text.start = index(body);
    }

    /// Tells whether no blank line stands between two items of `list` or
    /// between two blocks of one item.
    ///
    /// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
    fn tight(&self, list: u32) -> bool {
        let nodes = &self.doc.nodes;
        let gap = |a: u32, b: u32| {
            self.state[b as usize].first_line > self.state[a as usize].last_line + 1
        };
        let mut item = nodes[list as usize].first;
        while item != NONE {
            let next = nodes[item as usize].next;
            if next != NONE && gap(item, next) {
                return false;
            }
            let mut child = nodes[item as usize].first;
            while child != NONE {
                let after = nodes[child as usize].next;
                if after != NONE && gap(child, after) {
                    return false;
                }
                child = after;
            }
            item = next;
        }
        true
    }

    /// Closes every open block at the end of the text.
    fn finish(&mut self) {
        // Each pass closes one open block toward the document.
        while self.current != ROOT && self.current != NONE {
            self.current = self.finalize(self.current);
        }
    }
}
