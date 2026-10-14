// SPDX-License-Identifier: MIT
/*
 * aoviewer-core escape: the escapes of a schema string in XML and in Markdown.
 *
 * The Markdown writer writes titles, crumbs, the sentences of the tool and the
 * text of a table cell through md_text and md_cell, and a name as a code span
 * of its own. The SVG writer escapes through its own text and attr, whose
 * rules xml_text and xml_attr copy.
 */

//! The escapes.
//!
//! [`xml_text`] and [`xml_attr`] write a string for XML 1.0, where `&`, `<`
//! and `>` become references, a double quote inside an attribute does too, and
//! a character the `Char` production of XML 1.0 excludes becomes U+FFFD, since
//! no reference can carry it. A tab, a line feed or a carriage return inside
//! an attribute is written as a reference, which the value normalization of a
//! parser keeps, and a carriage return in text is one as well, since the line
//! end handling of a parser turns a literal one into a line feed. The rules
//! are the ones of the SVG writer, byte for byte.
//!
//! [`md_text`] and [`md_cell`] write a string so that a reader of the [GitHub
//! Flavored Markdown specification], the [CommonMark specification] with the
//! table, strikethrough and autolink extensions, reads no markup and the same
//! characters back, but for the spaces and tabs it strips at the end of a
//! paragraph or a heading and at both ends of a cell.
//!
//! Every ASCII punctuation character may be escaped with a backslash, so the
//! characters that open or close an inline construct are escaped wherever they
//! stand: the backslash, the backtick of a code span, `*`, `_` and the `~` of
//! strikethrough, the brackets of a link, `<` and `>` of raw HTML and
//! autolinks, `&` of an entity reference, `#` of a heading and its closing
//! sequence, and `$` of the math GitHub renders. U+0000 becomes U+FFFD, as the
//! reader replaces it.
//!
//! The autolink extension links bare text that holds no bracket: `www.` and a
//! domain, `http://`, `https://`, `mailto:` or `xmpp:` and what follows, and
//! an email address. Every `:` and every `@` is escaped, and so is the `.`
//! after a run of three `w` in either case, so none of these reads as a link
//! and the escaped character still reads as itself. The filters GitHub runs
//! over the HTML after the Markdown, such as emoji short codes and mentions,
//! are no Markdown and stay out of reach of an escape.
//!
//! The two differ where the text stands. Text of a paragraph, a heading or a
//! link label turns every line ending into a space, since a sentence of the
//! tool is one line, escapes a leading `-`, `+` or `=` and the `.` or `)`
//! after leading digits, which would open a list or a setext underline, and
//! writes leading spaces, tabs and line endings as character references, since
//! a reader counts a leading space or tab as indentation, which may open an
//! indented code block, and a line ending becomes a space there, which may let
//! the character after it open a block. A cell of a table holds inlines alone,
//! so no block can open there, and it writes a line ending as `<br>` and
//! escapes `|`, which would end the cell.
//!
//! Every loop runs over the characters of one string, a schema string whose
//! length the limits of the reader cap,
//! [`max_name_bytes`](crate::limits::ReadLimits::max_name_bytes) for a name
//! and [`max_description_bytes`](crate::limits::ReadLimits::max_description_bytes)
//! for a description, a deprecation reason, a default value or a
//! specification URL, or a title or a sentence the tool builds from a fixed
//! text and such strings.
//!
//! [CommonMark specification]: https://spec.commonmark.org/0.31.2/
//! [GitHub Flavored Markdown specification]: https://github.github.com/gfm/

use alloc::string::String;

/// The longest run of leading digits a list marker takes, which is where an
/// ordered list stops being one.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
const MAX_MARKER_DIGITS: usize = 9;

/// The run of `w` before the `.` of an autolink of the form `www.`.
///
/// [GitHub Flavored Markdown specification]: https://github.github.com/gfm/
const WWW: usize = 3;

/// Where a Markdown string stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    /// A paragraph, a heading or the label of a link.
    Text,
    /// A cell of a table.
    Cell,
}

/// Writes `text` as XML character data.
pub fn xml_text(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            '\t' | '\n' => out.push(c),
            c if excluded(c) => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

/// Writes `text` as the value of an XML attribute in double quotes.
pub fn xml_attr(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c if excluded(c) => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

/// Writes `text`, a schema name or a sentence of the tool, as Markdown text
/// that reads back as the same characters.
///
/// The text stands at the start of a paragraph, a heading or a link label,
/// on one line. Every line ending becomes a space, and one before the first
/// other character is written as a character reference.
pub fn md_text(text: &str, out: &mut String) {
    md(text, Place::Text, out);
}

/// Writes `text` as the content of a cell of a GitHub table that reads back
/// as the same characters.
///
/// Every `|` is escaped and every line ending is written `<br>`, so the text
/// stays in its cell and on its row.
pub fn md_cell(text: &str, out: &mut String) {
    md(text, Place::Cell, out);
}

/// Returns whether the `Char` production of XML 1.0 excludes `c`, a control
/// character other than the tab, the line feed and the carriage return, or one
/// of the two noncharacters U+FFFE and U+FFFF.
fn excluded(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}')
        || matches!(c, '\u{FFFE}' | '\u{FFFF}')
}

/// Writes `text` for `place`.
fn md(text: &str, place: Place, out: &mut String) {
    let rest = match place {
        Place::Text => block_start(text, out),
        Place::Cell => text,
    };
    // The run of `w` in either case just before `c`, up to `WWW`.
    let mut ws = 0;
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' | '`' | '*' | '_' | '~' | '[' | ']' | '<' | '>' | '#' | '&' | '$' | ':' | '@' => {
                out.push('\\');
                out.push(c);
            }
            '.' if ws == WWW => out.push_str("\\."),
            '|' if place == Place::Cell => out.push_str("\\|"),
            '\r' | '\n' => {
                // A carriage return and a line feed are one line ending.
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str(match place {
                    Place::Text => " ",
                    Place::Cell => "<br>",
                });
            }
            '\0' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
        ws = if matches!(c, 'w' | 'W') {
            (ws + 1).min(WWW)
        } else {
            0
        };
    }
}

/// Writes what would open a block at the start of `text` so that it opens
/// none, and returns the rest of `text`.
///
/// Leading spaces and tabs are written as character references, which a reader
/// does not count as indentation, and so is a leading line ending, as the
/// reference of the space `md` makes of it elsewhere. A leading `-`, `+` or
/// `=` and the `.` or `)` after at most nine leading digits are escaped, which
/// leaves no list item and no setext underline. Every other opening character
/// is one the inline escapes cover, such as `#`, `>`, `*`, `_`, `~` and the
/// backtick.
///
/// [CommonMark specification]: https://spec.commonmark.org/0.31.2/
fn block_start<'t>(text: &'t str, out: &mut String) -> &'t str {
    let rest = text.trim_start_matches([' ', '\t', '\r', '\n']);
    let mut after_cr = false;
    for b in text[..text.len() - rest.len()].bytes() {
        match b {
            b'\t' => out.push_str("&#9;"),
            // A carriage return and a line feed are one line ending.
            b'\n' if after_cr => {}
            _ => out.push_str("&#32;"),
        }
        after_cr = b == b'\r';
    }
    let bytes = rest.as_bytes();
    match bytes.first() {
        Some(b'-' | b'+' | b'=') => {
            out.push('\\');
            out.push(char::from(bytes[0]));
            &rest[1..]
        }
        Some(b'0'..=b'9') => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            match bytes.get(digits) {
                Some(&p @ (b'.' | b')')) if digits <= MAX_MARKER_DIGITS => {
                    out.push_str(&rest[..digits]);
                    out.push('\\');
                    out.push(char::from(p));
                    &rest[digits + 1..]
                }
                _ => rest,
            }
        }
        _ => rest,
    }
}
