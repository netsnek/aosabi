// SPDX-License-Identifier: MIT
/*
 * aoviewer-core commonmark demote: the two Markdown writers of a description,
 * the verbatim copy with its headings demoted and the first paragraph of a
 * table cell.
 *
 * A heading inside a description sits below the level that encloses it, and a
 * description of one paragraph sits in its table cell.
 */

//! The heading demotion and the first paragraph of a cell.
//!
//! Every loop runs over the edits of one source, which the source bounds,
//! over the bytes of one leaf, or over the nodes of the tree once.

use alloc::{
    string::String,
    vec::Vec, //
};
use core::fmt::Write;

use super::{
    Doc,
    Kind,
    NONE,
    RawHtml,
    Walk, //
};

/// The deepest level of an ATX heading.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
const MAX_LEVEL: u8 = 6;

/// Writes `source` with the headings of `doc` demoted by `demote` levels
/// and, under [`RawHtml::Escaped`], every `<` read as text written `&lt;`.
pub(super) fn to_markdown(
    doc: &Doc,
    source: &str,
    demote: u8,
    raw_html: RawHtml,
    out: &mut String,
) {
    let lts: &[u32] = if raw_html == RawHtml::Escaped {
        &doc.lts
    } else {
        &[]
    };
    let mut headings = doc.headings.iter().peekable();
    let mut lts = lts.iter().peekable();
    let mut at = 0;
    // Each pass writes the source up to one edit and the edit.
    loop {
        let heading = headings.peek().map(|h| h.at as usize);
        let lt = lts.peek().map(|&&l| l as usize);
        let next = match (heading, lt) {
            (Some(h), Some(l)) => h.min(l),
            (Some(h), None) => h,
            (None, Some(l)) => l,
            (None, None) => break,
        };
        let Some(before) = source.get(at..next) else {
            break;
        };
        out.push_str(before);
        at = next;
        if heading == Some(next) {
            let level = headings.next().map_or(MAX_LEVEL, |h| h.level);
            let extra = demote.min(MAX_LEVEL.saturating_sub(level));
            for _ in 0..extra {
                out.push('#');
            }
        } else {
            lts.next();
            out.push_str("&lt;");
            at += 1;
        }
    }
    out.push_str(source.get(at..).unwrap_or_default());
}

/// Writes every block of `doc` for a table cell, one line of the cell per
/// block, the lines joined by `<br>`.
///
/// A cell holds inlines alone, so the containers flatten into lines. A
/// paragraph or a heading writes its inline source, a code block its code as
/// one code span, a row of a table its cells joined by an escaped pipe, and a
/// thematic break nothing. The first line of a list item starts with the
/// bullet or the number the item shows, while a block quote and a heading
/// lose their markers.
///
/// [GitHub Flavored Markdown specification]: https://github.github.com/gfm/
pub(super) fn cell(doc: &Doc, out: &mut String) {
    // Whether a line has been written.
    let mut lines = false;
    // The markers of the open items whose first line is still to come.
    let mut markers = String::new();
    // For every open list, innermost last, the number of its next item and
    // the length of `markers` before the marker of its open item.
    let mut lists: Vec<(u32, usize)> = Vec::new();
    for (node, entering) in doc.walk() {
        let kind = doc.node(node).kind;
        match kind {
            Kind::List { start, .. } if entering => lists.push((start, 0)),
            Kind::List { .. } => {
                lists.pop();
            }
            Kind::Item { .. } if entering => {
                if let Some((number, before)) = lists.last_mut() {
                    *before = markers.len();
                    let list = doc.node(doc.node(node).parent).kind;
                    item_marker(list, *number, &mut markers);
                }
            }
            Kind::Item { .. } => {
                if let Some((number, before)) = lists.last_mut() {
                    markers.truncate((*before).min(markers.len()));
                    *number = number.saturating_add(1);
                }
            }
            Kind::Paragraph | Kind::Heading { .. } | Kind::Code { .. } | Kind::Row { .. }
                if entering =>
            {
                let mark = out.len();
                if lines {
                    out.push_str("<br>");
                }
                out.push_str(&markers);
                let content = out.len();
                match kind {
                    Kind::Code { .. } => code_cell(doc.node(node).text.of(&doc.buf), out),
                    Kind::Row { .. } => row_cell(doc, node, out),
                    _ => inline_cell(doc, node, out),
                }
                if out.len() == content {
                    out.truncate(mark);
                } else {
                    lines = true;
                    markers.clear();
                }
            }
            _ => {}
        }
    }
}

/// Writes the marker of the item numbered `number` of `list` as text, with a
/// `*` bullet escaped, since emphasis could take it inside the cell.
fn item_marker(list: Kind, number: u32, out: &mut String) {
    let Kind::List {
        ordered, marker, ..
    } = list
    else {
        return;
    };
    if ordered {
        let _ = write!(out, "{number}");
    } else if marker == b'*' {
        out.push('\\');
    }
    out.push(char::from(marker));
    out.push(' ');
}

/// Writes the cells of the table row `row` as one line of a cell, joined by
/// an escaped pipe.
fn row_cell(doc: &Doc, row: u32, out: &mut String) {
    let mut cell = doc.node(row).first;
    // Each pass writes one cell of the row, each of which is visited once.
    while cell != NONE {
        if cell != doc.node(row).first {
            out.push_str(" \\| ");
        }
        inline_cell(doc, cell, out);
        cell = doc.node(cell).next;
    }
}

/// Writes the inline source of the paragraph or heading `leaf` for a cell,
/// with every hard break written `<br>`, every other line ending a space
/// and every `|` escaped.
fn inline_cell(doc: &Doc, leaf: u32, out: &mut String) {
    let span = doc.node(leaf).text;
    let mut at = span.start as usize;
    for (node, entering) in Walk::new(&doc.nodes, leaf) {
        let kind = doc.node(node).kind;
        if !entering || !matches!(kind, Kind::Soft | Kind::Hard) {
            continue;
        }
        let brk = doc.node(node).text;
        cell_text(doc.buf.get(at..brk.start as usize).unwrap_or_default(), out);
        out.push_str(if kind == Kind::Hard { "<br>" } else { " " });
        at = brk.end as usize;
    }
    cell_text(doc.buf.get(at..span.end as usize).unwrap_or_default(), out);
}

/// Writes `text` for a cell, every line ending a space and every `|` not
/// yet escaped escaped.
///
/// A pipe inside a cell of a GitHub table escapes as `\|`, inside a code span
/// as well.
///
/// [GitHub Flavored Markdown specification]: https://github.github.com/gfm/
fn cell_text(text: &str, out: &mut String) {
    let mut backslashes = 0;
    for c in text.chars() {
        match c {
            '|' if backslashes % 2 == 0 => out.push_str("\\|"),
            '\n' => out.push(' '),
            c => out.push(c),
        }
        backslashes = if c == '\\' { backslashes + 1 } else { 0 };
    }
}

/// Writes the code of a code block as one code span of a cell.
///
/// The fence is one backtick longer than the longest run inside, and a space
/// pads both sides when the code starts or ends with a backtick or a space,
/// which the code span takes off again.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn code_cell(code: &str, out: &mut String) {
    let code = code.trim_end_matches('\n');
    if code.trim_matches([' ', '\n']).is_empty() {
        return;
    }
    let mut longest = 0;
    let mut run = 0;
    for b in code.bytes() {
        run = if b == b'`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let pad = code.starts_with(['`', ' ', '\n']) || code.ends_with(['`', ' ']);
    for _ in 0..=longest {
        out.push('`');
    }
    if pad {
        out.push(' ');
    }
    cell_text(code, out);
    if pad {
        out.push(' ');
    }
    for _ in 0..=longest {
        out.push('`');
    }
}
