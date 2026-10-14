// SPDX-License-Identifier: MIT
/*
 * aoviewer-core commonmark inline: the inline structure of the subset.
 *
 * Emphasis, strong emphasis, code spans, links, autolinks and line breaks are
 * the inlines the subset knows, read with the delimiter stack of the
 * specification.
 */

//! The inlines of the subset.
//!
//! The content of a leaf is read once from left to right. A run of `*` or `_`
//! and a `[` become text nodes whose delimiters go on a stack, a `]` looks
//! back for its `[` and forms a link when an inline destination follows, and
//! the emphasis of a link's text and then of the whole leaf is resolved by
//! the procedure the specification calls process emphasis. The subset has no
//! images, so a link deactivates every `[` before it by dropping the whole
//! stack of brackets.
//!
//! Every loop consumes at least one byte of the leaf, walks the stack of
//! delimiters, whose length the leaf caps, moves the nodes between two
//! delimiters once, or runs under a constant of this file.

use alloc::{
    string::String,
    vec::Vec, //
};

use super::{
    Doc,
    Kind,
    NONE,
    Node,
    Span,
    append,
    block::is_space,
    index,
    insert_after,
    insert_before,
    push,
    unlink, //
};

/// The longest run of backticks that opens a code span.
///
/// The parser remembers the start of the latest run of every length up to
/// this one that a search for a closing run has passed, so a leaf with many
/// unclosed runs is read in linear time, and a longer run is text.
const MAX_BACKTICKS: usize = 1000;

/// The deepest nesting of unescaped parentheses inside a link destination.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
const MAX_LINK_PARENS: usize = 32;

/// The longest scheme of an autolink.
const MAX_SCHEME: usize = 32;

/// The longest label of the domain of an email autolink.
const MAX_LABEL: usize = 63;

/// The schemes a link may name.
const SCHEMES: [&str; 3] = ["http", "https", "mailto"];

/// A delimiter run on the stack.
#[derive(Clone, Copy, Debug)]
struct Delim {
    /// The text node of the run.
    node: u32,
    /// The byte of the run, `*` or `_`.
    byte: u8,
    /// Whether the run can open emphasis.
    open: bool,
    /// Whether the run can close emphasis.
    close: bool,
    /// The length of the run as read, before emphasis took any of it.
    len: usize,
    /// The delimiter below on the stack.
    prev: u32,
    /// The delimiter above on the stack.
    next: u32,
}

/// A `[` on the stack of brackets.
#[derive(Clone, Copy, Debug)]
struct Bracket {
    /// The text node of the bracket.
    node: u32,
    /// The number of delimiters read before it, the bottom of the stack for
    /// the emphasis of the link text.
    bottom: u32,
}

/// The inline parser over the content of one leaf.
struct Inline<'a> {
    /// The buffer of the document.
    text: &'a str,
    /// The nodes of the document.
    nodes: &'a mut Vec<Node>,
    /// The derived text of the document.
    strs: &'a mut String,
    /// The buffer positions of every `<` read as text.
    lts: &'a mut Vec<u32>,
    /// The leaf.
    leaf: u32,
    /// The first byte of the content.
    start: usize,
    /// The byte after the content.
    end: usize,
    /// The byte the parser stands on.
    pos: usize,
    /// Every delimiter read, removed ones included.
    delims: Vec<Delim>,
    /// The delimiter on top of the stack.
    top: u32,
    /// The open brackets, innermost last.
    brackets: Vec<Bracket>,
    /// The start of the latest run of each length of backticks a search has
    /// passed, 0 for none.
    ticks: Vec<u32>,
    /// Whether one search for a closing run has reached the end of the leaf.
    scanned: bool,
}

/// Reads the inlines of the leaf `leaf` of `doc` as its children, adding
/// the buffer position of every `<` read as text to `lts`.
pub(super) fn parse(doc: &mut Doc, leaf: u32, lts: &mut Vec<u32>) {
    let span = doc.nodes[leaf as usize].text;
    let Doc {
        nodes, buf, strs, ..
    } = doc;
    let mut p = Inline {
        text: buf,
        nodes,
        strs,
        lts,
        leaf,
        start: span.start as usize,
        end: (span.end as usize).min(buf.len()),
        pos: span.start as usize,
        delims: Vec::new(),
        top: NONE,
        brackets: Vec::new(),
        ticks: Vec::new(),
        scanned: false,
    };
    p.run();
}

/// Tells whether `b` ends a run of plain text.
fn is_special(b: u8) -> bool {
    matches!(b, b'\n' | b'\\' | b'`' | b'*' | b'_' | b'[' | b']' | b'<')
}

/// Tells whether `c` is Unicode white space: a character of the general
/// category Zs, a tab, a line feed, a form feed or a carriage return.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

/// Tells whether `c` is Unicode punctuation, a character of the general
/// categories P or S.
fn is_punctuation(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_punctuation();
    }
    let code = u32::from(c);
    PUNCTUATION.partition_point(|&b| b <= code) % 2 == 1
}

/// Tells whether `url` names no scheme, or one of [`SCHEMES`].
///
/// A scheme is a letter and then letters, digits, `+`, `.` and `-` up to a
/// colon, so a destination that starts otherwise is relative.
///
/// [URL standard]: https://url.spec.whatwg.org/
fn allowed(url: &str) -> bool {
    let bytes = url.as_bytes();
    let end = bytes
        .iter()
        .position(|&b| !(b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-')));
    let Some(colon) = end else {
        return true;
    };
    if bytes[colon] != b':' || colon == 0 || !bytes[0].is_ascii_alphabetic() {
        return true;
    }
    let scheme = &url[..colon];
    SCHEMES.iter().any(|s| s.eq_ignore_ascii_case(scheme))
}

/// Appends `text` to `out` with every backslash before an ASCII punctuation
/// character dropped.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
pub(super) fn unescape(text: &str, out: &mut String) {
    let mut chars = text.chars().peekable();
    // Each pass takes one or two characters of `text`.
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&next) = chars.peek()
            && next.is_ascii_punctuation()
        {
            out.push(next);
            chars.next();
            continue;
        }
        out.push(c);
    }
}

/// Returns the length of the scheme and of the whole content of a URI
/// autolink after a `<` at `at` of the content `s`, the closing `>` left out.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn uri_autolink(s: &[u8], at: usize) -> Option<(usize, usize)> {
    if !s.get(at)?.is_ascii_alphabetic() {
        return None;
    }
    let scheme = 1 + s[at + 1..]
        .iter()
        .take(MAX_SCHEME)
        .take_while(|&&b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'))
        .count();
    if !(2..=MAX_SCHEME).contains(&scheme) || s.get(at + scheme) != Some(&b':') {
        return None;
    }
    let rest = at + scheme + 1;
    let len = s[rest..]
        .iter()
        .position(|&b| b <= 0x20 || b == 0x7F || b == b'<' || b == b'>')?;
    (s[rest + len] == b'>').then_some((scheme, scheme + 1 + len))
}

/// Returns the length of the content of an email autolink after a `<` at `at`
/// of the content `s`, the closing `>` left out.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn email_autolink(s: &[u8], at: usize) -> Option<usize> {
    let local = s
        .get(at..)?
        .iter()
        .take_while(|&&b| b.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&b))
        .count();
    if local == 0 || s.get(at + local) != Some(&b'@') {
        return None;
    }
    let mut i = at + local + 1;
    // Each pass takes one label of the domain and the dot after it.
    loop {
        let label = s
            .get(i..)?
            .iter()
            .take_while(|&&b| b.is_ascii_alphanumeric() || b == b'-')
            .count();
        if label == 0 || label > MAX_LABEL || s[i] == b'-' || s[i + label - 1] == b'-' {
            return None;
        }
        i += label;
        match s.get(i) {
            Some(b'.') => i += 1,
            Some(b'>') => return Some(i - at),
            _ => return None,
        }
    }
}

impl Inline<'_> {
    /// Returns the byte `i` of the content, or 0 outside it.
    fn byte(&self, i: usize) -> u8 {
        if i < self.end {
            self.text.as_bytes()[i]
        } else {
            0
        }
    }

    /// Adds a text node over the buffer bytes from `start` to `end`.
    fn text_node(&mut self, start: usize, end: usize) -> u32 {
        push(self.nodes, Kind::Text, Span::new(start, end))
    }

    /// Reads the content and resolves its emphasis.
    fn run(&mut self) {
        // Each pass reads one inline, at least one byte.
        while self.pos < self.end {
            let node = match self.byte(self.pos) {
                b'\n' => Some(self.newline()),
                b'`' => Some(self.code_span()),
                b'\\' => Some(self.backslash()),
                b'<' => Some(self.pointy()),
                b'*' | b'_' => Some(self.delimiters()),
                b'[' => Some(self.open_bracket()),
                b']' => self.close_bracket(),
                _ => self.plain(),
            };
            if let Some(node) = node {
                append(self.nodes, self.leaf, node);
            }
        }
        self.process_emphasis(0);
    }

    /// Reads plain text up to the next byte that may start an inline,
    /// dropping the white space before a line ending.
    fn plain(&mut self) -> Option<u32> {
        let start = self.pos;
        let mut end = start + 1;
        while end < self.end && !is_special(self.byte(end)) {
            end += 1;
        }
        self.pos = end;
        let mut stop = end;
        if self.byte(end) == b'\n' {
            while stop > start && is_space(self.byte(stop - 1)) {
                stop -= 1;
            }
        }
        (stop > start).then(|| self.text_node(start, stop))
    }

    /// Reads a line ending as a hard break after two spaces, else as a soft
    /// one, and skips the white space that starts the next line.
    ///
    /// The node spans the white space before the line ending as well, which
    /// the cell of [`super::first_paragraph_cell`] replaces with its break.
    ///
    /// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
    fn newline(&mut self) -> u32 {
        let at = self.pos;
        let mut from = at;
        while from > self.start && matches!(self.byte(from - 1), b' ' | b'\t') {
            from -= 1;
        }
        let hard = at >= self.start + 2 && self.byte(at - 1) == b' ' && self.byte(at - 2) == b' ';
        self.pos = at + 1;
        while self.pos < self.end && matches!(self.byte(self.pos), b' ' | b'\t') {
            self.pos += 1;
        }
        let kind = if hard { Kind::Hard } else { Kind::Soft };
        push(self.nodes, kind, Span::new(from, at + 1))
    }

    /// Reads a backslash: an escaped punctuation character as text, a hard
    /// break before a line ending, or else the backslash itself.
    fn backslash(&mut self) -> u32 {
        let at = self.pos;
        let next = self.byte(at + 1);
        if next.is_ascii_punctuation() {
            self.pos = at + 2;
            self.text_node(at + 1, at + 2)
        } else if next == b'\n' {
            self.pos = at + 2;
            push(self.nodes, Kind::Hard, Span::new(at, at + 2))
        } else {
            self.pos = at + 1;
            self.text_node(at, at + 1)
        }
    }

    /// Reads a code span, or the run of backticks as text when no run of the
    /// same length closes it.
    fn code_span(&mut self) -> u32 {
        let open = self.pos;
        let mut after = open;
        while self.byte(after) == b'`' {
            after += 1;
        }
        let len = after - open;
        let Some(close) = self.closing_ticks(after, len) else {
            self.pos = after;
            return self.text_node(open, after);
        };
        self.pos = close + len;
        let start = self.strs.len();
        let content = self.text.get(after..close).unwrap_or_default();
        let mut non_space = false;
        for c in content.chars() {
            let c = if c == '\n' { ' ' } else { c };
            non_space |= c != ' ';
            self.strs.push(c);
        }
        let bytes = &self.strs.as_bytes()[start..];
        let strip = non_space
            && bytes.len() >= 2
            && bytes.first() == Some(&b' ')
            && bytes.last() == Some(&b' ');
        let span = if strip {
            Span::new(start + 1, self.strs.len() - 1)
        } else {
            Span::new(start, self.strs.len())
        };
        push(self.nodes, Kind::CodeSpan, span)
    }

    /// Returns where the next run of exactly `len` backticks after `from`
    /// starts, or none when `len` is longer than [`MAX_BACKTICKS`].
    ///
    /// Every run the search passes raises the position remembered for its
    /// length and never lowers it. Once one search has reached the end of
    /// the leaf, a length whose remembered position lies before `from`
    /// therefore has no run after `from`, and the search is skipped.
    fn closing_ticks(&mut self, from: usize, len: usize) -> Option<usize> {
        if len > MAX_BACKTICKS {
            return None;
        }
        if self.ticks.is_empty() {
            self.ticks.resize(MAX_BACKTICKS + 1, 0);
        }
        if self.scanned && (self.ticks[len] as usize) < from {
            return None;
        }
        let mut at = from;
        // Each pass takes a run of other bytes and a run of backticks.
        while at < self.end {
            while at < self.end && self.byte(at) != b'`' {
                at += 1;
            }
            let run = at;
            while self.byte(at) == b'`' {
                at += 1;
            }
            if at == run {
                break;
            }
            if at - run <= MAX_BACKTICKS {
                self.ticks[at - run] = self.ticks[at - run].max(index(run));
            }
            if at - run == len {
                return Some(run);
            }
        }
        self.scanned = true;
        None
    }

    /// Reads an autolink after a `<`, or the `<` as text.
    ///
    /// A URI autolink of a scheme outside [`SCHEMES`] is text.
    fn pointy(&mut self) -> u32 {
        let at = self.pos;
        let content = &self.text.as_bytes()[..self.end];
        let uri = uri_autolink(content, at + 1).filter(|&(scheme, _)| {
            let name = self.text.get(at + 1..at + 1 + scheme).unwrap_or_default();
            SCHEMES.iter().any(|s| s.eq_ignore_ascii_case(name))
        });
        let (len, mailto) = match uri {
            Some((_, len)) => (Some(len), false),
            None => (email_autolink(content, at + 1), true),
        };
        let Some(len) = len else {
            self.lts.push(index(at));
            self.pos = at + 1;
            return self.text_node(at, at + 1);
        };
        self.pos = at + len + 2;
        let content = Span::new(at + 1, at + 1 + len);
        let url_start = self.strs.len();
        self.strs.push_str(content.of(self.text));
        let link = Kind::Link {
            url: Span::new(url_start, self.strs.len()),
            title: Span::default(),
            mailto,
        };
        let node = push(self.nodes, link, Span::default());
        let words = push(self.nodes, Kind::Text, content);
        append(self.nodes, node, words);
        node
    }

    /// Reads a run of `*` or `_` as text and puts it on the stack when it can
    /// open or close emphasis.
    fn delimiters(&mut self) -> u32 {
        let start = self.pos;
        let byte = self.byte(start);
        while self.byte(self.pos) == byte {
            self.pos += 1;
        }
        let before = if start == self.start {
            '\n'
        } else {
            self.text
                .get(..start)
                .and_then(|t| t.chars().next_back())
                .unwrap_or('\n')
        };
        let after = if self.pos >= self.end {
            '\n'
        } else {
            self.text
                .get(self.pos..)
                .and_then(|t| t.chars().next())
                .unwrap_or('\n')
        };
        let left = !is_whitespace(after)
            && (!is_punctuation(after) || is_whitespace(before) || is_punctuation(before));
        let right = !is_whitespace(before)
            && (!is_punctuation(before) || is_whitespace(after) || is_punctuation(after));
        let (open, close) = if byte == b'_' {
            (
                left && (!right || is_punctuation(before)),
                right && (!left || is_punctuation(after)),
            )
        } else {
            (left, right)
        };
        let node = self.text_node(start, self.pos);
        if open || close {
            let i = index(self.delims.len());
            self.delims.push(Delim {
                node,
                byte,
                open,
                close,
                len: self.pos - start,
                prev: self.top,
                next: NONE,
            });
            if self.top != NONE {
                self.delims[self.top as usize].next = i;
            }
            self.top = i;
        }
        node
    }

    /// Reads a `[` as text and puts it on the stack of brackets.
    fn open_bracket(&mut self) -> u32 {
        let node = self.text_node(self.pos, self.pos + 1);
        self.pos += 1;
        self.brackets.push(Bracket {
            node,
            bottom: index(self.delims.len()),
        });
        node
    }

    /// Reads a `]`, which closes a link when the innermost `[` is followed by
    /// an inline destination this `]` ends, and is text otherwise.
    fn close_bracket(&mut self) -> Option<u32> {
        let at = self.pos;
        self.pos += 1;
        let Some(&opener) = self.brackets.last() else {
            return Some(self.text_node(at, at + 1));
        };
        let mark = self.strs.len();
        if let Some((url, title, after)) = self.inline_link(at + 1)
            && allowed(url.of(self.strs))
        {
            self.pos = after;
            let link = Kind::Link {
                url,
                title,
                mailto: false,
            };
            let node = push(self.nodes, link, Span::default());
            insert_before(self.nodes, opener.node, node);
            let mut child = self.nodes[opener.node as usize].next;
            while child != NONE {
                let next = self.nodes[child as usize].next;
                unlink(self.nodes, child);
                append(self.nodes, node, child);
                child = next;
            }
            unlink(self.nodes, opener.node);
            self.process_emphasis(opener.bottom);
            self.brackets.clear();
            return None;
        }
        self.strs.truncate(mark);
        self.brackets.pop();
        Some(self.text_node(at, at + 1))
    }

    /// Reads an inline destination and title after a `]`, from the `(` at
    /// `at` to its `)`, and returns the destination and the title written
    /// into the derived text, and the byte after the `)`.
    fn inline_link(&mut self, at: usize) -> Option<(Span, Span, usize)> {
        if self.byte(at) != b'(' {
            return None;
        }
        let dest_at = self.skip_space(at + 1);
        let (dest, after_dest) = self.destination(dest_at)?;
        let title_at = self.skip_space(after_dest);
        let (title, after_title) = if title_at > after_dest {
            self.title(title_at)
                .map_or((None, title_at), |(t, a)| (Some(t), a))
        } else {
            (None, title_at)
        };
        let close = self.skip_space(after_title);
        if self.byte(close) != b')' {
            return None;
        }
        let raw = self.text.get(dest.0..dest.1).unwrap_or_default();
        let raw = raw.trim_matches(|c: char| u8::try_from(c).is_ok_and(is_space));
        let url_start = self.strs.len();
        unescape(raw, self.strs);
        let url = Span::new(url_start, self.strs.len());
        let title_start = self.strs.len();
        if let Some((a, b)) = title {
            unescape(self.text.get(a..b).unwrap_or_default(), self.strs);
        }
        let title = Span::new(title_start, self.strs.len());
        Some((url, title, close + 1))
    }

    /// Returns the first byte at or after `at` that is no white space.
    fn skip_space(&self, at: usize) -> usize {
        let mut i = at;
        while i < self.end && is_space(self.byte(i)) {
            i += 1;
        }
        i
    }

    /// Returns the range of a link destination at `at`, without its angle
    /// brackets, and the byte after it.
    fn destination(&self, at: usize) -> Option<((usize, usize), usize)> {
        if self.byte(at) == b'<' {
            let mut i = at + 1;
            // Each pass takes one byte, or a backslash and the byte after it.
            while i < self.end {
                match self.byte(i) {
                    b'>' => return (i + 1 < self.end).then_some(((at + 1, i), i + 1)),
                    b'\\' => i += 2,
                    b'\n' | b'<' => return None,
                    _ => i += 1,
                }
            }
            return None;
        }
        let mut i = at;
        let mut depth = 0;
        while i < self.end {
            let b = self.byte(i);
            if b == b'\\' && self.byte(i + 1).is_ascii_punctuation() {
                i += 2;
            } else if b == b'(' {
                depth += 1;
                if depth > MAX_LINK_PARENS {
                    return None;
                }
                i += 1;
            } else if b == b')' {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                i += 1;
            } else if is_space(b) || b.is_ascii_control() {
                break;
            } else {
                i += 1;
            }
        }
        (i < self.end && depth == 0).then_some(((at, i), i))
    }

    /// Returns the range of a link title at `at`, without its delimiters,
    /// and the byte after it.
    fn title(&self, at: usize) -> Option<((usize, usize), usize)> {
        let close = match self.byte(at) {
            b'"' => b'"',
            b'\'' => b'\'',
            b'(' => b')',
            _ => return None,
        };
        let mut i = at + 1;
        while i < self.end {
            let b = self.byte(i);
            if b == b'\\' && self.byte(i + 1).is_ascii_punctuation() {
                i += 2;
            } else if b == close {
                return Some(((at + 1, i), i + 1));
            } else if close == b')' && b == b'(' {
                return None;
            } else {
                i += 1;
            }
        }
        None
    }

    /// Takes the delimiter `i` off the stack.
    fn remove_delim(&mut self, i: u32) {
        let Delim { prev, next, .. } = self.delims[i as usize];
        if prev != NONE {
            self.delims[prev as usize].next = next;
        }
        if next == NONE {
            self.top = prev;
        } else {
            self.delims[next as usize].prev = prev;
        }
    }

    /// Resolves the emphasis of the delimiters from `bottom` up, and takes
    /// them off the stack.
    ///
    /// The lowest opener a closer may reach is kept per byte, per whether
    /// the closer can open and per its length modulo 3, so no search repeats
    /// a search that failed.
    fn process_emphasis(&mut self, bottom: u32) {
        let mut openers_bottom = [bottom; 12];
        let mut closer = self.top;
        let mut first = NONE;
        while closer != NONE && closer >= bottom {
            first = closer;
            closer = self.delims[closer as usize].prev;
        }
        closer = first;
        while closer != NONE {
            let c = self.delims[closer as usize];
            if !c.close {
                closer = c.next;
                continue;
            }
            let slot = usize::from(c.byte == b'*') * 6 + usize::from(c.open) * 3 + c.len % 3;
            let mut opener = c.prev;
            let mut found = false;
            while opener != NONE && opener >= bottom && opener >= openers_bottom[slot] {
                let o = self.delims[opener as usize];
                let odd = (c.open || o.close)
                    && !c.len.is_multiple_of(3)
                    && (o.len + c.len).is_multiple_of(3);
                if o.open && o.byte == c.byte && !odd {
                    found = true;
                    break;
                }
                opener = o.prev;
            }
            if found {
                closer = self.insert_emph(opener, closer);
            } else {
                openers_bottom[slot] = closer;
                if !c.open {
                    self.remove_delim(closer);
                }
                closer = c.next;
            }
        }
        while self.top != NONE && self.top >= bottom {
            self.remove_delim(self.top);
        }
    }

    /// Wraps the nodes between the runs of `opener` and `closer` in
    /// emphasis, or strong emphasis when both have two delimiters left, and
    /// returns the closer to go on with.
    fn insert_emph(&mut self, opener: u32, closer: u32) -> u32 {
        let o = self.delims[opener as usize];
        let c = self.delims[closer as usize];
        let o_left = self.nodes[o.node as usize].text;
        let c_left = self.nodes[c.node as usize].text;
        let o_len = (o_left.end - o_left.start) as usize;
        let c_len = (c_left.end - c_left.start) as usize;
        let used = if o_len >= 2 && c_len >= 2 { 2 } else { 1 };
        self.nodes[o.node as usize].text.end -= index(used);
        self.nodes[c.node as usize].text.start += index(used);
        let mut between = c.prev;
        while between != NONE && between != opener {
            let prev = self.delims[between as usize].prev;
            self.remove_delim(between);
            between = prev;
        }
        let kind = if used == 1 { Kind::Emph } else { Kind::Strong };
        let emph = push(self.nodes, kind, Span::default());
        let mut child = self.nodes[o.node as usize].next;
        while child != NONE && child != c.node {
            let next = self.nodes[child as usize].next;
            unlink(self.nodes, child);
            append(self.nodes, emph, child);
            child = next;
        }
        insert_after(self.nodes, o.node, emph);
        if o_len == used {
            unlink(self.nodes, o.node);
            self.remove_delim(opener);
        }
        if c_len == used {
            unlink(self.nodes, c.node);
            let next = self.delims[closer as usize].next;
            self.remove_delim(closer);
            return next;
        }
        closer
    }
}

/// The boundaries of the ranges of the general categories P and S above
/// U+007F, each range from a boundary at an even index up to the next one.
///
/// [Unicode character database 16.0.0]: https://www.unicode.org/Public/16.0.0/ucd/UnicodeData.txt
const PUNCTUATION: [u32; 690] = [
    0xA1, 0xAA, 0xAB, 0xAD, 0xAE, 0xB2, 0xB4, 0xB5, 0xB6, 0xB9, 0xBB, 0xBC, 0xBF, 0xC0, 0xD7, 0xD8,
    0xF7, 0xF8, 0x2C2, 0x2C6, 0x2D2, 0x2E0, 0x2E5, 0x2EC, 0x2ED, 0x2EE, 0x2EF, 0x300, 0x375, 0x376,
    0x37E, 0x37F, 0x384, 0x386, 0x387, 0x388, 0x3F6, 0x3F7, 0x482, 0x483, 0x55A, 0x560, 0x589,
    0x58B, 0x58D, 0x590, 0x5BE, 0x5BF, 0x5C0, 0x5C1, 0x5C3, 0x5C4, 0x5C6, 0x5C7, 0x5F3, 0x5F5,
    0x606, 0x610, 0x61B, 0x61C, 0x61D, 0x620, 0x66A, 0x66E, 0x6D4, 0x6D5, 0x6DE, 0x6DF, 0x6E9,
    0x6EA, 0x6FD, 0x6FF, 0x700, 0x70E, 0x7F6, 0x7FA, 0x7FE, 0x800, 0x830, 0x83F, 0x85E, 0x85F,
    0x888, 0x889, 0x964, 0x966, 0x970, 0x971, 0x9F2, 0x9F4, 0x9FA, 0x9FC, 0x9FD, 0x9FE, 0xA76,
    0xA77, 0xAF0, 0xAF2, 0xB70, 0xB71, 0xBF3, 0xBFB, 0xC77, 0xC78, 0xC7F, 0xC80, 0xC84, 0xC85,
    0xD4F, 0xD50, 0xD79, 0xD7A, 0xDF4, 0xDF5, 0xE3F, 0xE40, 0xE4F, 0xE50, 0xE5A, 0xE5C, 0xF01,
    0xF18, 0xF1A, 0xF20, 0xF34, 0xF35, 0xF36, 0xF37, 0xF38, 0xF39, 0xF3A, 0xF3E, 0xF85, 0xF86,
    0xFBE, 0xFC6, 0xFC7, 0xFCD, 0xFCE, 0xFDB, 0x104A, 0x1050, 0x109E, 0x10A0, 0x10FB, 0x10FC,
    0x1360, 0x1369, 0x1390, 0x139A, 0x1400, 0x1401, 0x166D, 0x166F, 0x169B, 0x169D, 0x16EB, 0x16EE,
    0x1735, 0x1737, 0x17D4, 0x17D7, 0x17D8, 0x17DC, 0x1800, 0x180B, 0x1940, 0x1941, 0x1944, 0x1946,
    0x19DE, 0x1A00, 0x1A1E, 0x1A20, 0x1AA0, 0x1AA7, 0x1AA8, 0x1AAE, 0x1B4E, 0x1B50, 0x1B5A, 0x1B6B,
    0x1B74, 0x1B80, 0x1BFC, 0x1C00, 0x1C3B, 0x1C40, 0x1C7E, 0x1C80, 0x1CC0, 0x1CC8, 0x1CD3, 0x1CD4,
    0x1FBD, 0x1FBE, 0x1FBF, 0x1FC2, 0x1FCD, 0x1FD0, 0x1FDD, 0x1FE0, 0x1FED, 0x1FF0, 0x1FFD, 0x1FFF,
    0x2010, 0x2028, 0x2030, 0x205F, 0x207A, 0x207F, 0x208A, 0x208F, 0x20A0, 0x20C1, 0x2100, 0x2102,
    0x2103, 0x2107, 0x2108, 0x210A, 0x2114, 0x2115, 0x2116, 0x2119, 0x211E, 0x2124, 0x2125, 0x2126,
    0x2127, 0x2128, 0x2129, 0x212A, 0x212E, 0x212F, 0x213A, 0x213C, 0x2140, 0x2145, 0x214A, 0x214E,
    0x214F, 0x2150, 0x218A, 0x218C, 0x2190, 0x242A, 0x2440, 0x244B, 0x249C, 0x24EA, 0x2500, 0x2776,
    0x2794, 0x2B74, 0x2B76, 0x2B96, 0x2B97, 0x2C00, 0x2CE5, 0x2CEB, 0x2CF9, 0x2CFD, 0x2CFE, 0x2D00,
    0x2D70, 0x2D71, 0x2E00, 0x2E2F, 0x2E30, 0x2E5E, 0x2E80, 0x2E9A, 0x2E9B, 0x2EF4, 0x2F00, 0x2FD6,
    0x2FF0, 0x3000, 0x3001, 0x3005, 0x3008, 0x3021, 0x3030, 0x3031, 0x3036, 0x3038, 0x303D, 0x3040,
    0x309B, 0x309D, 0x30A0, 0x30A1, 0x30FB, 0x30FC, 0x3190, 0x3192, 0x3196, 0x31A0, 0x31C0, 0x31E6,
    0x31EF, 0x31F0, 0x3200, 0x321F, 0x322A, 0x3248, 0x3250, 0x3251, 0x3260, 0x3280, 0x328A, 0x32B1,
    0x32C0, 0x3400, 0x4DC0, 0x4E00, 0xA490, 0xA4C7, 0xA4FE, 0xA500, 0xA60D, 0xA610, 0xA673, 0xA674,
    0xA67E, 0xA67F, 0xA6F2, 0xA6F8, 0xA700, 0xA717, 0xA720, 0xA722, 0xA789, 0xA78B, 0xA828, 0xA82C,
    0xA836, 0xA83A, 0xA874, 0xA878, 0xA8CE, 0xA8D0, 0xA8F8, 0xA8FB, 0xA8FC, 0xA8FD, 0xA92E, 0xA930,
    0xA95F, 0xA960, 0xA9C1, 0xA9CE, 0xA9DE, 0xA9E0, 0xAA5C, 0xAA60, 0xAA77, 0xAA7A, 0xAADE, 0xAAE0,
    0xAAF0, 0xAAF2, 0xAB5B, 0xAB5C, 0xAB6A, 0xAB6C, 0xABEB, 0xABEC, 0xFB29, 0xFB2A, 0xFBB2, 0xFBC3,
    0xFD3E, 0xFD50, 0xFDCF, 0xFDD0, 0xFDFC, 0xFE00, 0xFE10, 0xFE1A, 0xFE30, 0xFE53, 0xFE54, 0xFE67,
    0xFE68, 0xFE6C, 0xFF01, 0xFF10, 0xFF1A, 0xFF21, 0xFF3B, 0xFF41, 0xFF5B, 0xFF66, 0xFFE0, 0xFFE7,
    0xFFE8, 0xFFEF, 0xFFFC, 0xFFFE, 0x10100, 0x10103, 0x10137, 0x10140, 0x10179, 0x1018A, 0x1018C,
    0x1018F, 0x10190, 0x1019D, 0x101A0, 0x101A1, 0x101D0, 0x101FD, 0x1039F, 0x103A0, 0x103D0,
    0x103D1, 0x1056F, 0x10570, 0x10857, 0x10858, 0x10877, 0x10879, 0x1091F, 0x10920, 0x1093F,
    0x10940, 0x10A50, 0x10A59, 0x10A7F, 0x10A80, 0x10AC8, 0x10AC9, 0x10AF0, 0x10AF7, 0x10B39,
    0x10B40, 0x10B99, 0x10B9D, 0x10D6E, 0x10D6F, 0x10D8E, 0x10D90, 0x10EAD, 0x10EAE, 0x10F55,
    0x10F5A, 0x10F86, 0x10F8A, 0x11047, 0x1104E, 0x110BB, 0x110BD, 0x110BE, 0x110C2, 0x11140,
    0x11144, 0x11174, 0x11176, 0x111C5, 0x111C9, 0x111CD, 0x111CE, 0x111DB, 0x111DC, 0x111DD,
    0x111E0, 0x11238, 0x1123E, 0x112A9, 0x112AA, 0x113D4, 0x113D6, 0x113D7, 0x113D9, 0x1144B,
    0x11450, 0x1145A, 0x1145C, 0x1145D, 0x1145E, 0x114C6, 0x114C7, 0x115C1, 0x115D8, 0x11641,
    0x11644, 0x11660, 0x1166D, 0x116B9, 0x116BA, 0x1173C, 0x11740, 0x1183B, 0x1183C, 0x11944,
    0x11947, 0x119E2, 0x119E3, 0x11A3F, 0x11A47, 0x11A9A, 0x11A9D, 0x11A9E, 0x11AA3, 0x11B00,
    0x11B0A, 0x11BE1, 0x11BE2, 0x11C41, 0x11C46, 0x11C70, 0x11C72, 0x11EF7, 0x11EF9, 0x11F43,
    0x11F50, 0x11FD5, 0x11FF2, 0x11FFF, 0x12000, 0x12470, 0x12475, 0x12FF1, 0x12FF3, 0x16A6E,
    0x16A70, 0x16AF5, 0x16AF6, 0x16B37, 0x16B40, 0x16B44, 0x16B46, 0x16D6D, 0x16D70, 0x16E97,
    0x16E9B, 0x16FE2, 0x16FE3, 0x1BC9C, 0x1BC9D, 0x1BC9F, 0x1BCA0, 0x1CC00, 0x1CCF0, 0x1CD00,
    0x1CEB4, 0x1CF50, 0x1CFC4, 0x1D000, 0x1D0F6, 0x1D100, 0x1D127, 0x1D129, 0x1D165, 0x1D16A,
    0x1D16D, 0x1D183, 0x1D185, 0x1D18C, 0x1D1AA, 0x1D1AE, 0x1D1EB, 0x1D200, 0x1D242, 0x1D245,
    0x1D246, 0x1D300, 0x1D357, 0x1D6C1, 0x1D6C2, 0x1D6DB, 0x1D6DC, 0x1D6FB, 0x1D6FC, 0x1D715,
    0x1D716, 0x1D735, 0x1D736, 0x1D74F, 0x1D750, 0x1D76F, 0x1D770, 0x1D789, 0x1D78A, 0x1D7A9,
    0x1D7AA, 0x1D7C3, 0x1D7C4, 0x1D800, 0x1DA00, 0x1DA37, 0x1DA3B, 0x1DA6D, 0x1DA75, 0x1DA76,
    0x1DA84, 0x1DA85, 0x1DA8C, 0x1E14F, 0x1E150, 0x1E2FF, 0x1E300, 0x1E5FF, 0x1E600, 0x1E95E,
    0x1E960, 0x1ECAC, 0x1ECAD, 0x1ECB0, 0x1ECB1, 0x1ED2E, 0x1ED2F, 0x1EEF0, 0x1EEF2, 0x1F000,
    0x1F02C, 0x1F030, 0x1F094, 0x1F0A0, 0x1F0AF, 0x1F0B1, 0x1F0C0, 0x1F0C1, 0x1F0D0, 0x1F0D1,
    0x1F0F6, 0x1F10D, 0x1F1AE, 0x1F1E6, 0x1F203, 0x1F210, 0x1F23C, 0x1F240, 0x1F249, 0x1F250,
    0x1F252, 0x1F260, 0x1F266, 0x1F300, 0x1F6D8, 0x1F6DC, 0x1F6ED, 0x1F6F0, 0x1F6FD, 0x1F700,
    0x1F777, 0x1F77B, 0x1F7DA, 0x1F7E0, 0x1F7EC, 0x1F7F0, 0x1F7F1, 0x1F800, 0x1F80C, 0x1F810,
    0x1F848, 0x1F850, 0x1F85A, 0x1F860, 0x1F888, 0x1F890, 0x1F8AE, 0x1F8B0, 0x1F8BC, 0x1F8C0,
    0x1F8C2, 0x1F900, 0x1FA54, 0x1FA60, 0x1FA6E, 0x1FA70, 0x1FA7D, 0x1FA80, 0x1FA8A, 0x1FA8F,
    0x1FAC7, 0x1FACE, 0x1FADD, 0x1FADF, 0x1FAEA, 0x1FAF0, 0x1FAF9, 0x1FB00, 0x1FB93, 0x1FB94,
    0x1FBF0,
];
