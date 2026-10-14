// SPDX-License-Identifier: MIT
/*
 * aoviewer-core commonmark html: the HTML writer of the subset.
 *
 * Raw HTML is escaped and only http, https and mailto are linked, which the
 * examples of the CommonMark specification judge.
 */

//! The HTML writer of the subset.
//!
//! The markup is the one the examples of the specification expect: a block
//! starts on a line of its own, a paragraph of a tight list item loses its
//! `<p>`, a soft break is a line feed and a hard one `<br />`, and a table is
//! the markup of the GitHub specification. Every text is escaped, so the only
//! `<` of the output is markup this writer writes.
//!
//! The walk enters and leaves every node once, and every other loop runs
//! over the bytes of one text.
//!
//! [CommonMark specification]: https://spec.commonmark.org/0.31.2/
//! [GitHub Flavored Markdown specification]: https://github.github.com/gfm/

use alloc::string::String;

use super::{
    Align,
    Doc,
    Kind,
    NONE,
    inline::unescape, //
};

/// Writes the HTML of `doc` to `out`.
pub(super) fn write(doc: &Doc, out: &mut String) {
    let base = out.len();
    for (node, entering) in doc.walk() {
        let n = doc.node(node);
        match n.kind {
            Kind::Document | Kind::Row { .. } | Kind::Cell { .. } | Kind::Table => {
                table_part(doc, node, entering, base, out);
            }
            Kind::Quote if entering => {
                line(out, base);
                out.push_str("<blockquote>\n");
            }
            Kind::Quote => {
                line(out, base);
                out.push_str("</blockquote>\n");
            }
            Kind::List { .. } => list(n.kind, entering, base, out),
            Kind::Item { .. } if entering => {
                line(out, base);
                out.push_str("<li>");
            }
            Kind::Item { .. } => out.push_str("</li>\n"),
            Kind::Paragraph if tight(doc, node) => {}
            Kind::Paragraph if entering => {
                line(out, base);
                out.push_str("<p>");
            }
            Kind::Paragraph => out.push_str("</p>\n"),
            Kind::Heading { level } => {
                if entering {
                    line(out, base);
                }
                out.push_str(if entering { "<h" } else { "</h" });
                out.push(char::from(b'0' + level.min(6)));
                out.push_str(if entering { ">" } else { ">\n" });
            }
            Kind::Rule if entering => {
                line(out, base);
                out.push_str("<hr />\n");
            }
            Kind::Code { info, .. } if entering => {
                line(out, base);
                out.push_str("<pre><code");
                let mut word = String::new();
                info_word(info.of(&doc.buf), &mut word);
                if !word.is_empty() {
                    out.push_str(" class=\"language-");
                    escape(&word, out);
                    out.push('"');
                }
                out.push('>');
                escape(n.text.of(&doc.buf), out);
                out.push_str("</code></pre>\n");
            }
            _ => inline(doc, node, entering, out),
        }
    }
}

/// Writes the part of an inline node that `entering` asks for.
fn inline(doc: &Doc, node: u32, entering: bool, out: &mut String) {
    let n = doc.node(node);
    match n.kind {
        Kind::Text if entering => escape(n.text.of(&doc.buf), out),
        Kind::Soft if entering => out.push('\n'),
        Kind::Hard if entering => out.push_str("<br />\n"),
        Kind::CodeSpan if entering => {
            out.push_str("<code>");
            escape(n.text.of(&doc.strs), out);
            out.push_str("</code>");
        }
        Kind::Emph => out.push_str(if entering { "<em>" } else { "</em>" }),
        Kind::Strong => out.push_str(if entering { "<strong>" } else { "</strong>" }),
        Kind::Link { url, title, mailto } if entering => {
            out.push_str("<a href=\"");
            if mailto {
                out.push_str("mailto:");
            }
            href(url.of(&doc.strs), out);
            if !title.is_empty() {
                out.push_str("\" title=\"");
                escape(title.of(&doc.strs), out);
            }
            out.push_str("\">");
        }
        Kind::Link { .. } => out.push_str("</a>"),
        _ => {}
    }
}

/// Writes the start or the end of a list of `kind`.
fn list(kind: Kind, entering: bool, base: usize, out: &mut String) {
    let Kind::List { ordered, start, .. } = kind else {
        return;
    };
    if !entering {
        out.push_str(if ordered { "</ol>\n" } else { "</ul>\n" });
        return;
    }
    line(out, base);
    if !ordered {
        out.push_str("<ul>\n");
    } else if start == 1 {
        out.push_str("<ol>\n");
    } else {
        out.push_str("<ol start=\"");
        super::push_number(start, out);
        out.push_str("\">\n");
    }
}

/// Writes the part of a table, a row or a cell that `entering` asks for,
/// with the head around the header row and the body around the rest.
fn table_part(doc: &Doc, node: u32, entering: bool, base: usize, out: &mut String) {
    let n = doc.node(node);
    match (n.kind, entering) {
        (Kind::Table, true) => {
            line(out, base);
            out.push_str("<table>");
        }
        (Kind::Table, false) => {
            let header = n.first;
            if header != NONE && doc.node(header).next != NONE {
                line(out, base);
                out.push_str("</tbody>");
            }
            line(out, base);
            out.push_str("</table>\n");
        }
        (Kind::Row { header }, true) => {
            line(out, base);
            if header {
                out.push_str("<thead>\n");
            } else if doc.node(n.parent).first != NONE
                && doc.node(doc.node(n.parent).first).next == node
            {
                out.push_str("<tbody>\n");
            }
            out.push_str("<tr>");
        }
        (Kind::Row { header }, false) => {
            line(out, base);
            out.push_str("</tr>");
            if header {
                out.push_str("\n</thead>");
            }
        }
        (Kind::Cell { align }, true) => {
            line(out, base);
            out.push_str(if header_cell(doc, node) { "<th" } else { "<td" });
            match align {
                Align::Left => out.push_str(" align=\"left\""),
                Align::Center => out.push_str(" align=\"center\""),
                Align::Right => out.push_str(" align=\"right\""),
                Align::None => {}
            }
            out.push('>');
        }
        (Kind::Cell { .. }, false) => {
            out.push_str(if header_cell(doc, node) {
                "</th>"
            } else {
                "</td>"
            });
        }
        _ => {}
    }
}

/// Tells whether the cell `node` belongs to the header row.
fn header_cell(doc: &Doc, node: u32) -> bool {
    matches!(
        doc.node(doc.node(node).parent).kind,
        Kind::Row { header: true }
    )
}

/// Tells whether the paragraph `node` sits directly in an item of a tight list,
/// which writes it without `<p>`.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn tight(doc: &Doc, node: u32) -> bool {
    let item = doc.node(node).parent;
    if item == NONE || !matches!(doc.node(item).kind, Kind::Item { .. }) {
        return false;
    }
    let list = doc.node(item).parent;
    list != NONE && matches!(doc.node(list).kind, Kind::List { tight: true, .. })
}

/// Starts a line, unless the output of this document is empty or already
/// ends one.
fn line(out: &mut String, base: usize) {
    if out.len() > base && !out.ends_with('\n') {
        out.push('\n');
    }
}

/// Writes the first word of the info string `info` with its backslash escapes
/// resolved, the word that names the language of a code block.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
pub(super) fn info_word(info: &str, out: &mut String) {
    let mut resolved = String::new();
    unescape(info, &mut resolved);
    let word = resolved
        .split(|c: char| c.is_ascii_whitespace() || c == '\u{0B}')
        .next()
        .unwrap_or_default();
    out.push_str(word);
}

/// Writes `text` escaped for HTML text and attribute values.
///
/// `&`, `<`, `>` and `"` take their entities, and U+0000 becomes U+FFFD.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
pub(super) fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\0' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

/// Writes the destination `url` for an `href` attribute.
///
/// Letters, digits and ``-_.+!*(),%#@?=;:/$~`` pass, `&` becomes `&amp;` and
/// `'` `&#x27;`, and every other byte is percent encoded, a UTF-8 character
/// byte by byte.
fn href(url: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &b in url.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.+!*(),%#@?=;:/$~".contains(&b) {
            out.push(char::from(b));
        } else if b == b'&' {
            out.push_str("&amp;");
        } else if b == b'\'' {
            out.push_str("&#x27;");
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xF)]));
        }
    }
}
