// SPDX-License-Identifier: MIT
/*
 * aoviewer-core commonmark: the recognizer of the CommonMark subset of a
 * description and its four writers.
 *
 * Descriptions are Markdown already and go into a page verbatim with their
 * headings demoted, so the subset keeps the source bytes. The block and inline
 * rules are those of the CommonMark specification 0.31.2, and the tables those
 * of the GitHub Flavored Markdown specification.
 */

//! The Markdown subset of a description.
//!
//! [`parse`] recognizes the subset and never validates. It knows paragraphs,
//! ATX headings, thematic breaks, fenced and indented code blocks, block
//! quotes, bullet and ordered lists, GitHub tables, blank lines, emphasis,
//! strong emphasis, code spans, links, autolinks and hard and soft line
//! breaks, and everything else is text.
//!
//! A link or an autolink whose destination names a scheme other than `http`,
//! `https` or `mailto` is no link, and its brackets stay text. A destination
//! without a scheme is relative and stays a link, because it names no scheme
//! at all.
//!
//! The [`Doc`] holds one tree of blocks and inlines. The content of every
//! leaf block is copied into one buffer with the prefixes of its containers
//! removed, which is what the inlines are read from, while the positions of
//! the ATX headings and of every `<` read as text are kept as byte offsets
//! of the source, which is what [`to_markdown`] rewrites.
//!
//! Every loop of the subset either consumes at least one byte of the text,
//! whose length [`ReadLimits::max_description_bytes`] and
//! `MAX_TEXT_BYTES` cap, or walks the open blocks, whose nesting
//! [`ReadLimits::max_depth`] caps, or walks the nodes of the tree, each of
//! which is entered and left once, or runs under a constant of its own
//! module.

mod block;
mod demote;
mod html;
mod inline;

use alloc::{
    string::String,
    vec::Vec, //
};

use crate::limits::ReadLimits;

/// The longest text the subset reads as Markdown, whatever the limits say.
///
/// Offsets of the tree are `u32`, and the buffer of a text can grow past
/// its source by the spaces of a tab, so a quarter of the range keeps every
/// offset exact.
const MAX_TEXT_BYTES: usize = 1 << 30;

/// The index of no node.
const NONE: u32 = u32::MAX;

/// The index of the document node, the root of every tree.
const ROOT: u32 = 0;

/// The deepest nesting of calls [`to_typst`] writes.
///
/// A quote, a list, emphasis or a link deeper than this adds its content to
/// the call around it, so a hostile nesting never reaches the Typst compiler
/// as deep syntax or as deep realization.
const MAX_TYPST_NESTING: usize = 16;

/// What happens to raw HTML in a description.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RawHtml {
    /// The source bytes pass unchanged, and a Markdown host's sanitizer
    /// renders the markup safe.
    #[default]
    Verbatim,
    /// Every `<` the subset reads as text is written `&lt;`, so no markup of
    /// the author reaches the host.
    Escaped,
}

/// A description read as the subset.
///
/// The document keeps a tree of blocks and inlines over the content of its
/// leaf blocks, and the byte offsets of the source that [`to_markdown`]
/// rewrites. It is built by [`parse`] alone, and every writer reads it
/// together with the source it was built from.
#[derive(Clone, Debug)]
pub struct Doc {
    /// The nodes of the tree, the document node first.
    nodes: Vec<Node>,
    /// The content of the leaf blocks, each line ended by a line feed.
    buf: String,
    /// The text that no leaf holds as written: the content of code spans and
    /// the destinations and titles of links.
    strs: String,
    /// The ATX headings, by the source offset of their first `#`, in source
    /// order.
    headings: Vec<Heading>,
    /// The source offsets of every `<` the subset reads as text, ascending.
    lts: Vec<u32>,
    /// The limits the text was read under, or none when it was read as plain
    /// text, which [`first_paragraph_cell`] reads a first paragraph under.
    limits: Option<ReadLimits>,
}

/// An ATX heading of the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Heading {
    /// The source offset of the first `#` of the opening sequence.
    at: u32,
    /// The level, the number of `#` of the opening sequence.
    level: u8,
}

/// A range of bytes of [`Doc::buf`] or [`Doc::strs`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Span {
    /// The first byte.
    start: u32,
    /// The byte after the last.
    end: u32,
}

impl Span {
    /// Returns the span from `start` to `end`.
    fn new(start: usize, end: usize) -> Span {
        Span {
            start: index(start),
            end: index(end),
        }
    }

    /// Returns the text of the span inside `text`, or the empty text when the
    /// span does not fall on characters of it.
    fn of(self, text: &str) -> &str {
        text.get(self.start as usize..self.end as usize)
            .unwrap_or_default()
    }

    /// Tells whether the span holds no byte.
    fn is_empty(self) -> bool {
        self.end <= self.start
    }
}

/// The opening fence of a fenced code block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fence {
    /// The byte of the fence, a backtick or a tilde.
    byte: u8,
    /// The length of the opening run.
    len: u32,
    /// The indentation of the opening fence, which every content line loses
    /// up to that many spaces of.
    indent: u32,
}

/// The alignment of a table column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    /// No colon in the delimiter row.
    None,
    /// A colon on the left.
    Left,
    /// Colons on both sides.
    Center,
    /// A colon on the right.
    Right,
}

/// What a node of the tree is.
///
/// The text of a leaf block, of a text node and of a break is the span of
/// its node inside [`Doc::buf`], and the text of a code span and the
/// destination and title of a link are spans inside [`Doc::strs`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// The root.
    Document,
    /// A block quote.
    Quote,
    /// A bullet or an ordered list.
    List {
        /// Whether the markers are numbers.
        ordered: bool,
        /// The bullet, or the byte after the number, `.` or `)`.
        marker: u8,
        /// The number of the first item.
        start: u32,
        /// Whether no blank line separates the items or the blocks of one
        /// item.
        tight: bool,
    },
    /// A list item.
    Item {
        /// The columns a line indents to continue the item, the indentation
        /// of the marker, its width and the spaces after it.
        content: u32,
    },
    /// A paragraph.
    Paragraph,
    /// An ATX heading.
    Heading {
        /// The level from 1 to 6.
        level: u8,
    },
    /// A thematic break.
    Rule,
    /// A fenced or an indented code block.
    Code {
        /// The opening fence, or none for an indented block.
        fence: Option<Fence>,
        /// The info string inside [`Doc::buf`], with its backslash escapes
        /// still in place.
        info: Span,
    },
    /// A GitHub table.
    Table,
    /// A row of a table.
    Row {
        /// Whether the row is the header row.
        header: bool,
    },
    /// A cell of a row.
    Cell {
        /// The alignment of the cell's column.
        align: Align,
    },
    /// Text.
    Text,
    /// A soft line break.
    Soft,
    /// A hard line break.
    Hard,
    /// A code span.
    CodeSpan,
    /// Emphasis.
    Emph,
    /// Strong emphasis.
    Strong,
    /// A link or an autolink.
    Link {
        /// The destination inside [`Doc::strs`].
        url: Span,
        /// The title inside [`Doc::strs`], empty when there is none.
        title: Span,
        /// Whether the link is an email autolink, whose destination is
        /// written after `mailto:`.
        mailto: bool,
    },
}

/// A node of the tree.
#[derive(Clone, Copy, Debug)]
struct Node {
    /// What the node is.
    kind: Kind,
    /// The parent, or [`NONE`] for the root and a node taken out.
    parent: u32,
    /// The first child.
    first: u32,
    /// The last child.
    last: u32,
    /// The sibling before.
    prev: u32,
    /// The sibling after.
    next: u32,
    /// The text of the node, see [`Kind`].
    text: Span,
}

impl Node {
    /// Returns a node of `kind` over `text` that hangs nowhere yet.
    fn new(kind: Kind, text: Span) -> Node {
        Node {
            kind,
            parent: NONE,
            first: NONE,
            last: NONE,
            prev: NONE,
            next: NONE,
            text,
        }
    }
}

/// Returns `i` as an offset of the tree.
fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// Adds a node of `kind` over `text` to `nodes` and returns its index.
fn push(nodes: &mut Vec<Node>, kind: Kind, text: Span) -> u32 {
    nodes.push(Node::new(kind, text));
    index(nodes.len() - 1)
}

/// Makes `child` the last child of `parent`.
fn append(nodes: &mut [Node], parent: u32, child: u32) {
    let last = nodes[parent as usize].last;
    let c = &mut nodes[child as usize];
    c.parent = parent;
    c.prev = last;
    c.next = NONE;
    if last == NONE {
        nodes[parent as usize].first = child;
    } else {
        nodes[last as usize].next = child;
    }
    nodes[parent as usize].last = child;
}

/// Puts `node` directly after its sibling `at`.
fn insert_after(nodes: &mut [Node], at: u32, node: u32) {
    let parent = nodes[at as usize].parent;
    let next = nodes[at as usize].next;
    let n = &mut nodes[node as usize];
    n.parent = parent;
    n.prev = at;
    n.next = next;
    nodes[at as usize].next = node;
    if next != NONE {
        nodes[next as usize].prev = node;
    } else if parent != NONE {
        nodes[parent as usize].last = node;
    }
}

/// Puts `node` directly before its sibling `at`.
fn insert_before(nodes: &mut [Node], at: u32, node: u32) {
    let parent = nodes[at as usize].parent;
    let prev = nodes[at as usize].prev;
    let n = &mut nodes[node as usize];
    n.parent = parent;
    n.prev = prev;
    n.next = at;
    nodes[at as usize].prev = node;
    if prev != NONE {
        nodes[prev as usize].next = node;
    } else if parent != NONE {
        nodes[parent as usize].first = node;
    }
}

/// Takes `node` out of the children of its parent, keeping its own
/// children.
fn unlink(nodes: &mut [Node], node: u32) {
    let Node {
        parent, prev, next, ..
    } = nodes[node as usize];
    if prev != NONE {
        nodes[prev as usize].next = next;
    } else if parent != NONE {
        nodes[parent as usize].first = next;
    }
    if next != NONE {
        nodes[next as usize].prev = prev;
    } else if parent != NONE {
        nodes[parent as usize].last = prev;
    }
    let n = &mut nodes[node as usize];
    n.parent = NONE;
    n.prev = NONE;
    n.next = NONE;
}

/// A walk over a subtree that enters every node and then leaves it.
///
/// The walk keeps no stack, since every node knows its parent and its next
/// sibling, so the depth of a tree never costs memory or recursion.
struct Walk<'d> {
    /// The nodes of the tree.
    nodes: &'d [Node],
    /// The root of the subtree.
    root: u32,
    /// The node of the next event, or [`NONE`] after the last.
    at: u32,
    /// Whether the next event enters its node.
    entering: bool,
}

impl<'d> Walk<'d> {
    /// Returns the walk over the subtree of `root`.
    fn new(nodes: &'d [Node], root: u32) -> Walk<'d> {
        let at = if (root as usize) < nodes.len() {
            root
        } else {
            NONE
        };
        Walk {
            nodes,
            root,
            at,
            entering: true,
        }
    }
}

impl Iterator for Walk<'_> {
    type Item = (u32, bool);

    fn next(&mut self) -> Option<(u32, bool)> {
        if self.at == NONE {
            return None;
        }
        let event = (self.at, self.entering);
        let node = &self.nodes[self.at as usize];
        if self.entering && node.first != NONE {
            self.at = node.first;
        } else if self.entering {
            self.entering = false;
        } else if self.at == self.root {
            self.at = NONE;
        } else if node.next != NONE {
            self.at = node.next;
            self.entering = true;
        } else {
            self.at = node.parent;
        }
        Some(event)
    }
}

impl Doc {
    /// Returns the node `i`.
    fn node(&self, i: u32) -> &Node {
        &self.nodes[i as usize]
    }

    /// Returns the walk over the whole tree.
    fn walk(&self) -> Walk<'_> {
        Walk::new(&self.nodes, ROOT)
    }

    /// Returns the document of a text too long for the subset, one paragraph
    /// of plain text.
    ///
    /// Every `<` of such a text is text, and it holds no heading, so the
    /// writers still escape what [`RawHtml::Escaped`] asks and never read
    /// markup from it.
    fn plain(text: &str) -> Doc {
        let mut doc = Doc::empty();
        if u32::try_from(text.len()).is_err() {
            return doc;
        }
        let span = Span::new(0, text.len());
        doc.buf.push_str(text);
        let paragraph = push(&mut doc.nodes, Kind::Paragraph, span);
        append(&mut doc.nodes, ROOT, paragraph);
        let words = push(&mut doc.nodes, Kind::Text, span);
        append(&mut doc.nodes, paragraph, words);
        doc.lts = text
            .bytes()
            .enumerate()
            .filter(|&(_, b)| b == b'<')
            .map(|(i, _)| index(i))
            .collect();
        doc
    }

    /// Returns the document of the empty text.
    fn empty() -> Doc {
        let mut nodes = Vec::new();
        push(&mut nodes, Kind::Document, Span::default());
        Doc {
            nodes,
            buf: String::new(),
            strs: String::new(),
            headings: Vec::new(),
            lts: Vec::new(),
            limits: None,
        }
    }
}

/// Reads `text` as the subset.
///
/// A block opens on its marker only while the block that would hold it is
/// shallower than [`ReadLimits::max_depth`], and the rest of the line is
/// text otherwise. A list start opens a list and its item at once, and a
/// paragraph goes under the deepest open block, so blocks nest at most two
/// levels deeper than `max_depth`. A text longer than
/// [`ReadLimits::max_description_bytes`] is read as one paragraph of plain
/// text, which every writer still handles, because the model builder already
/// refuses a longer description and a caller that passes one gets text back
/// rather than a partial reading.
#[must_use]
pub fn parse(text: &str, limits: &ReadLimits) -> Doc {
    let cap = MAX_TEXT_BYTES.min(limits.max_description_bytes as usize);
    if text.len() > cap {
        return Doc::plain(text);
    }
    let mut doc = Doc::empty();
    doc.limits = Some(*limits);
    let maps = block::parse(&mut doc, text, limits.max_depth);
    let mut lts = Vec::new();
    let leaves = doc.nodes.len();
    for i in 0..leaves {
        if matches!(
            doc.nodes[i].kind,
            Kind::Paragraph | Kind::Heading { .. } | Kind::Cell { .. }
        ) {
            inline::parse(&mut doc, index(i), &mut lts);
        }
    }
    doc.lts = block::sources(&maps, &lts);
    doc
}

/// Writes the source text back byte for byte, with every ATX heading
/// outside code demoted by `demote` levels and, under [`RawHtml::Escaped`],
/// raw HTML written as text.
///
/// `source` is the text `doc` was read from. A heading gains `demote` more `#`
/// and stops at level 6, the deepest one the specification knows, so it stays
/// a heading. Under [`RawHtml::Escaped`] every `<` the subset reads as text is
/// written `&lt;`, while a `<` of a code span, a code block, an autolink, a
/// link destination or a backslash escape stays, since none of those is markup
/// of the author.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
pub fn to_markdown(doc: &Doc, source: &str, demote: u8, raw_html: RawHtml, out: &mut String) {
    demote::to_markdown(doc, source, demote, raw_html, out);
}

/// Writes the first paragraph alone, for a table cell, with `|` escaped and
/// a line break written `<br>`, and returns true when more follows.
///
/// The first paragraph is the one the page model cuts from `source`, the
/// text `doc` was read from: its lines from the first that is not blank to
/// the next blank line outside a fenced code block, with more following
/// when anything but white space comes after them. The cell and the page
/// model share that one rule, so a cell that returns false has written every
/// block of the text and a cell that returns true has a `Details` entry to
/// link.
/// A first paragraph shorter than `source` is read again under the limits
/// of `doc`, since the blocks of `doc` may reach past it, so the cell
/// depends on the first paragraph alone.
///
/// Every block of the first paragraph is a line of the cell, and the lines
/// are joined by `<br>`. A paragraph or a heading writes its inline source,
/// in which a hard line break becomes `<br>` and a soft one a space. A code
/// block writes its code as one code span, a row of a table its cells
/// joined by an escaped pipe, and a thematic break nothing. The first line
/// of a list item starts with the bullet or the number the item shows, and
/// a block quote and a heading lose their markers.
pub fn first_paragraph_cell(doc: &Doc, source: &str, out: &mut String) -> bool {
    let (first, more) = crate::page::first_paragraph(source);
    if first.len() == source.len() {
        demote::cell(doc, out);
    } else if let Some(limits) = &doc.limits {
        demote::cell(&parse(first, limits), out);
    } else {
        demote::cell(&Doc::plain(first), out);
    }
    more
}

/// Writes every construct as a call of `sabi-theme.typ` with string
/// arguments, nothing in markup mode.
///
/// The text is one code expression, an array of blocks, and the theme's
/// functions receive every text as a string or an array of inlines,
/// written in these calls:
///
/// - a paragraph `sv-p(inlines)`, an ATX heading `sv-h(level, inlines)`
///   with the level of the source,
/// - a code block `sv-codeblock(lang, code)`, with the first word of the
///   info string or `""` and the code without its last line feed, and a
///   thematic break `sv-rule()`,
/// - a block quote `sv-quote(blocks)`, a list `sv-list(start, tight,
///   items)` with `start` the number of the first item or `none` for a
///   bullet list and each item an array of blocks,
/// - a table `sv-table(aligns, header, rows)`, each alignment `"left"`,
///   `"center"`, `"right"` or `none`, the header an array of cells, each
///   row an array of cells and each cell an array of inlines,
/// - a text a string, emphasis `sv-em(inlines)`, strong emphasis
///   `sv-strong(inlines)`, a code span `sv-code(code)`, a link
///   `sv-link(url, inlines)`, whose text alone is written when its
///   destination is empty, and a hard line break `sv-br()`.
///
/// Every text enters as a string literal in code mode, whose escapes run
/// nothing. A wrapper or a container nested deeper than `MAX_TYPST_NESTING`
/// calls adds its content to the one around it. `source` is the text `doc` was
/// read from, which the calls do not need, since the document carries the
/// content of its leaves.
pub fn to_typst(doc: &Doc, source: &str, out: &mut String) {
    let _ = source;
    Typst::new(doc, out).write();
}

/// The HTML is the markup the examples of that specification expect for the
/// constructs of the subset, and a table the markup of the [GitHub Flavored
/// Markdown specification]. `source` is the text `doc` was read from, which
/// the HTML does not need, since the document carries the content of its
/// leaves.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
/// [GitHub Flavored Markdown specification]: https://github.github.com/gfm/
pub fn to_html(doc: &Doc, source: &str, out: &mut String) {
    let _ = source;
    html::write(doc, out);
}

/// The writer of [`to_typst`].
struct Typst<'d, 'o> {
    /// The document.
    doc: &'d Doc,
    /// The text written.
    out: &'o mut String,
    /// The element count of every array open in `out`, innermost last.
    arrays: Vec<u32>,
    /// Whether every open node was written as a call, innermost last.
    written: Vec<bool>,
    /// The number of open nodes written as a call.
    depth: usize,
    /// Whether a string literal of text is open.
    text: bool,
}

impl<'d, 'o> Typst<'d, 'o> {
    /// Returns the writer of `doc` into `out`.
    fn new(doc: &'d Doc, out: &'o mut String) -> Typst<'d, 'o> {
        Typst {
            doc,
            out,
            arrays: Vec::new(),
            written: Vec::new(),
            depth: 0,
            text: false,
        }
    }

    /// Writes the document.
    fn write(&mut self) {
        for (node, entering) in self.doc.walk() {
            let kind = self.doc.node(node).kind;
            match kind {
                Kind::Text | Kind::Soft => {
                    if entering {
                        self.words(node, kind);
                    }
                }
                Kind::Rule | Kind::Hard | Kind::Code { .. } | Kind::CodeSpan => {
                    self.end_words();
                    if entering {
                        self.leaf(node, kind);
                    }
                }
                _ if entering => {
                    self.end_words();
                    self.enter(node, kind);
                }
                _ => {
                    self.end_words();
                    self.leave(kind);
                }
            }
        }
        self.end_words();
    }

    /// Starts the next element of the innermost array.
    fn element(&mut self) {
        if let Some(count) = self.arrays.last_mut() {
            if *count > 0 {
                self.out.push_str(", ");
            }
            *count += 1;
        }
    }

    /// Opens an array.
    fn open(&mut self) {
        self.out.push('(');
        self.arrays.push(0);
    }

    /// Closes the innermost array, with the comma that keeps an array of one
    /// element an array.
    fn close(&mut self) {
        if self.arrays.pop() == Some(1) {
            self.out.push(',');
        }
        self.out.push(')');
    }

    /// Writes the text of a text node or of a soft break into the open
    /// string literal.
    fn words(&mut self, node: u32, kind: Kind) {
        if !self.text {
            self.element();
            self.out.push('"');
            self.text = true;
        }
        if kind == Kind::Soft {
            self.out.push(' ');
        } else {
            literal_body(self.doc.node(node).text.of(&self.doc.buf), self.out);
        }
    }

    /// Closes the open string literal of text.
    fn end_words(&mut self) {
        if self.text {
            self.out.push('"');
            self.text = false;
        }
    }

    /// Writes a node without children as the next element.
    fn leaf(&mut self, node: u32, kind: Kind) {
        self.element();
        match kind {
            Kind::Rule => self.out.push_str("sv-rule()"),
            Kind::Hard => self.out.push_str("sv-br()"),
            Kind::Code { info, .. } => {
                self.out.push_str("sv-codeblock(");
                let mut lang = String::new();
                html::info_word(info.of(&self.doc.buf), &mut lang);
                literal(&lang, self.out);
                self.out.push_str(", ");
                let code = self.doc.node(node).text.of(&self.doc.buf);
                literal(code.strip_suffix('\n').unwrap_or(code), self.out);
                self.out.push(')');
            }
            _ => {
                self.out.push_str("sv-code(");
                literal(self.doc.node(node).text.of(&self.doc.strs), self.out);
                self.out.push(')');
            }
        }
    }

    /// Tells whether a node of `kind` opened now is written as a call.
    ///
    /// Quotes, lists, emphasis and links give way below `MAX_TYPST_NESTING`
    /// calls, a list item follows its list, and every other node is written,
    /// since a paragraph, a heading and a table nest a bounded number of calls
    /// of their own. A link to the empty destination writes its text alone,
    /// since Typst refuses a link to no URL.
    ///
    /// [Typst link]: https://typst.app/docs/reference/model/link/
    fn writes(&self, kind: Kind) -> bool {
        match kind {
            Kind::Link { url, mailto, .. } if url.is_empty() && !mailto => false,
            Kind::Quote | Kind::List { .. } | Kind::Emph | Kind::Strong | Kind::Link { .. } => {
                self.depth < MAX_TYPST_NESTING
            }
            Kind::Item { .. } => self.written.last() == Some(&true),
            _ => true,
        }
    }

    /// Writes the start of `node`, a node whose content follows.
    fn enter(&mut self, node: u32, kind: Kind) {
        let call = self.writes(kind);
        self.written.push(call);
        if !call {
            return;
        }
        if kind != Kind::Document {
            self.depth += 1;
        }
        match kind {
            Kind::Document | Kind::Row { header: true } => {}
            Kind::Item { .. } | Kind::Row { .. } | Kind::Cell { .. } => self.element(),
            _ => self.start_call(node, kind),
        }
        if kind != Kind::Table {
            self.open();
        }
    }

    /// Writes the name and the leading arguments of the call of `node`.
    fn start_call(&mut self, node: u32, kind: Kind) {
        self.element();
        match kind {
            Kind::Heading { level } => {
                self.out.push_str("sv-h(");
                push_number(u32::from(level), self.out);
                self.out.push_str(", ");
            }
            Kind::Quote => self.out.push_str("sv-quote("),
            Kind::List {
                ordered,
                start,
                tight,
                ..
            } => {
                self.out.push_str("sv-list(");
                if ordered {
                    push_number(start, self.out);
                } else {
                    self.out.push_str("none");
                }
                self.out
                    .push_str(if tight { ", true, " } else { ", false, " });
            }
            Kind::Emph => self.out.push_str("sv-em("),
            Kind::Strong => self.out.push_str("sv-strong("),
            Kind::Link { url, mailto, .. } => {
                self.out.push_str("sv-link(\"");
                if mailto {
                    self.out.push_str("mailto:");
                }
                literal_body(url.of(&self.doc.strs), self.out);
                self.out.push_str("\", ");
            }
            Kind::Table => self.start_table(node),
            _ => self.out.push_str("sv-p("),
        }
    }

    /// Writes the name of the call of a table and its alignments.
    fn start_table(&mut self, table: u32) {
        self.out.push_str("sv-table(");
        self.open();
        let header = self.doc.node(table).first;
        let mut cell = if header == NONE {
            NONE
        } else {
            self.doc.node(header).first
        };
        while cell != NONE {
            self.element();
            let align = match self.doc.node(cell).kind {
                Kind::Cell { align: Align::Left } => "\"left\"",
                Kind::Cell {
                    align: Align::Center,
                } => "\"center\"",
                Kind::Cell {
                    align: Align::Right,
                } => "\"right\"",
                _ => "none",
            };
            self.out.push_str(align);
            cell = self.doc.node(cell).next;
        }
        self.close();
        self.out.push_str(", ");
    }

    /// Writes the end of a node of `kind` whose content was written.
    ///
    /// The header row of a table opens the array of the body rows after it,
    /// which the table closes.
    fn leave(&mut self, kind: Kind) {
        if self.written.pop() != Some(true) {
            return;
        }
        if kind != Kind::Document {
            self.depth -= 1;
        }
        match kind {
            Kind::Row { header: true } => {
                self.close();
                self.out.push_str(", ");
                self.open();
            }
            Kind::Document | Kind::Item { .. } | Kind::Row { .. } | Kind::Cell { .. } => {
                self.close();
            }
            _ => {
                self.close();
                self.out.push(')');
            }
        }
    }
}

/// Writes `n` in decimal.
fn push_number(n: u32, out: &mut String) {
    let mut digits = [0u8; 10];
    let mut len = 0;
    let mut rest = n;
    for d in &mut digits {
        *d = b'0' + u8::try_from(rest % 10).unwrap_or(0);
        len += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    for &d in digits[..len].iter().rev() {
        out.push(char::from(d));
    }
}

/// Writes `text` as a Typst string literal in code mode.
fn literal(text: &str, out: &mut String) {
    out.push('"');
    literal_body(text, out);
    out.push('"');
}

/// Writes `text` as the inside of a Typst string literal.
///
/// The backslash, the quote, the line feed, the carriage return and the tab
/// take their escapes, and every other control character and the two Unicode
/// separators of lines and paragraphs take `\u{...}`, so the literal holds no
/// character that ends it or starts a line.
///
/// [Typst string]: https://typst.app/docs/reference/foundations/str/
fn literal_body(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str("\\u{");
                let code = u32::from(c);
                for shift in [20u32, 16, 12, 8, 4, 0] {
                    if code >> shift != 0 || shift == 0 {
                        let digit = (code >> shift) & 0xF;
                        out.push(char::from(b"0123456789abcdef"[digit as usize]));
                    }
                }
                out.push('}');
            }
            c => out.push(c),
        }
    }
}
