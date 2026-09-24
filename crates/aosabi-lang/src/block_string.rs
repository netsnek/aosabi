// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the block string value algorithm and the block string printer.
 *
 * Derived from graphql-js 16.12.0, src/language/blockString.ts, MIT.
 */

//! The block string algorithms of the language layer.
//!
//! [`dedent_block_string_lines`] is the `BlockStringValue()` static algorithm
//! of the GraphQL specification, which the lexer applies to the raw lines of
//! a block string. [`is_printable_as_block_string`] and
//! [`print_block_string`] serve the printer, which writes a string value in
//! block form when the value survives the round trip.
//!
//! The lines and values are Rust strings. Every line the lexer hands over
//! ends on a code point boundary, and the two characters the algorithms test
//! for, horizontal tab and space, are ASCII, so a byte index into a leading
//! run of white space is the code unit index the reference uses. The one
//! place where a length is compared against a number, the readability
//! threshold of the printer, counts code units the way `length` does.

use alloc::{
    format,
    string::String,
    vec::Vec, //
};

use crate::char_utils::is_white_space;

/// Produces the value of a block string from its parsed raw value, similar to
/// CoffeeScript's block string, Python's docstring trim or Ruby's `strip_heredoc`.
///
/// This implements the GraphQL spec's `BlockStringValue()` static algorithm.
/// `lines` are the raw lines of the block string, split at every line
/// terminator, and the result carries the common indentation removed from
/// every line but the first and the leading and trailing blank lines dropped.
///
/// Derived from graphql-js 16.12.0, src/language/blockString.ts, MIT.
///
/// Every line of the result is a slice of a line of `lines`, because the
/// dedent only cuts a prefix off a line, so the result borrows them and
/// copies no text.
#[must_use]
pub fn dedent_block_string_lines<S: AsRef<str>>(lines: &[S]) -> Vec<&str> {
    // Number.MAX_SAFE_INTEGER exceeds every line, and so does usize::MAX, so
    // a block whose lines after the first are all blank keeps the sentinel
    // and slices each of them to nothing, as the reference does.
    let mut common_indent = usize::MAX;
    let mut first_non_empty_line = None;
    let mut last_non_empty_line = None;

    for (i, line) in lines.iter().enumerate() {
        let line = line.as_ref();
        let indent = leading_whitespace(line);

        if indent == line.len() {
            continue; // skip empty lines
        }

        first_non_empty_line = first_non_empty_line.or(Some(i));
        last_non_empty_line = Some(i);

        if i != 0 && indent < common_indent {
            common_indent = indent;
        }
    }

    // `slice(firstNonEmptyLine ?? 0, lastNonEmptyLine + 1)` with the initial
    // `-1` of the reference reads as an empty range when no line is non
    // empty.
    let first = first_non_empty_line.unwrap_or(0);
    let end = last_non_empty_line.map_or(0, |last| last + 1);

    lines
        .iter()
        .enumerate()
        // Remove common indentation from all lines but first.
        .map(|(i, line)| {
            if i == 0 {
                line.as_ref()
            } else {
                slice_from(line.as_ref(), common_indent)
            }
        })
        // Remove leading and trailing blank lines.
        .skip(first)
        .take(end.saturating_sub(first))
        .collect()
}

/// Returns `line.slice(start)` for a start inside the leading white space.
///
/// `String.prototype.slice` clamps a start past the length to the length and
/// yields an empty string there. The only start the dedent passes is a common
/// indentation, which is a count of leading tabs and spaces, so it lands on a
/// character boundary whenever it lands inside the line at all.
fn slice_from(line: &str, start: usize) -> &str {
    line.get(start..).unwrap_or("")
}

/// Returns the number of leading tabs and spaces of `str`.
///
/// Both white space characters are ASCII, so the count over bytes is the
/// count over code units.
///
/// Derived from graphql-js 16.12.0, src/language/blockString.ts, MIT.
fn leading_whitespace(str: &str) -> usize {
    str.bytes()
        .take_while(|&byte| is_white_space(i32::from(byte)))
        .count()
}

/// Returns whether `value` can be printed in block form and read back as itself.
///
/// The reference walks code unit indices and reads a code point at each, so
/// the trailing half of a surrogate pair is visited once more than the pair
/// and lands in the default branch a second time, which changes nothing, and
/// the walk over characters here is the same walk.
///
/// Derived from graphql-js 16.12.0, src/language/blockString.ts, MIT.
#[must_use]
pub fn is_printable_as_block_string(value: &str) -> bool {
    if value.is_empty() {
        return true; // empty string is printable
    }

    let mut is_empty_line = true;
    let mut has_indent = false;
    let mut has_common_indent = true;
    let mut seen_non_empty_line = false;

    for code_point in value.chars() {
        match u32::from(code_point) {
            // Has non-printable characters, or has \r or \r\n which will be
            // replaced as \n.
            0x0000 | 0x0001 | 0x0002 | 0x0003 | 0x0004 | 0x0005 | 0x0006 | 0x0007 | 0x0008
            | 0x000b | 0x000c | 0x000e | 0x000f | 0x000d => return false,

            // \n
            10 => {
                if is_empty_line && !seen_non_empty_line {
                    return false; // Has leading new line
                }
                seen_non_empty_line = true;

                is_empty_line = true;
                has_indent = false;
            }
            // \t and <space>
            9 | 32 => {
                has_indent |= is_empty_line;
            }
            _ => {
                has_common_indent &= has_indent;
                is_empty_line = false;
            }
        }
    }

    if is_empty_line {
        return false; // Has trailing empty lines
    }

    if has_common_indent && seen_non_empty_line {
        return false; // Has internal indent
    }

    true
}

/// Print a block string in the indented block form by adding a leading and
/// trailing blank line.
///
/// However, if a block string starts with whitespace and is a single-line,
/// adding a leading blank line would strip that whitespace. `value` is the
/// string value and `minimize` is the `minimize` option of the reference,
/// which drops the blank lines that only improve readability.
///
/// Derived from graphql-js 16.12.0, src/language/blockString.ts, MIT.
#[must_use]
pub fn print_block_string(value: &str, minimize: bool) -> String {
    // `replace` with a global pattern substitutes every match from left to
    // right without overlap, which `str::replace` does as well.
    let escaped_value = value.replace("\"\"\"", "\\\"\"\"");

    // Expand a block string's raw value into independent lines.
    let lines = split_lines(&escaped_value);
    let is_single_line = lines.len() == 1;

    // If common indentation is found we can fix some of those cases by adding leading new line
    let force_leading_new_line = lines.len() > 1
        && lines
            .iter()
            .skip(1)
            .all(|line| line.is_empty() || starts_with_white_space(line));

    // Trailing triple quotes just looks confusing but doesn't force trailing new line
    let has_trailing_triple_quotes = escaped_value.ends_with("\\\"\"\"");

    // Trailing quote (single or double) or slash forces trailing new line
    let has_trailing_quote = value.ends_with('"') && !has_trailing_triple_quotes;
    let has_trailing_slash = value.ends_with('\\');
    let force_trailing_newline = has_trailing_quote || has_trailing_slash;

    // `length` counts UTF-16 code units, so the threshold is compared against
    // that count.
    let print_as_multiple_lines = !minimize
        // add leading and trailing new lines only if it improves readability
        && (!is_single_line
            || value.encode_utf16().count() > 70
            || force_trailing_newline
            || force_leading_new_line
            || has_trailing_triple_quotes);

    let mut result = String::new();

    // Format a multi-line block quote to account for leading space.
    let skip_leading_new_line = is_single_line && starts_with_white_space(value);
    if (print_as_multiple_lines && !skip_leading_new_line) || force_leading_new_line {
        result.push('\n');
    }

    result.push_str(&escaped_value);
    if print_as_multiple_lines || force_trailing_newline {
        result.push('\n');
    }

    format!("\"\"\"{result}\"\"\"")
}

/// Returns `isWhiteSpace(text.charCodeAt(0))`.
///
/// `charCodeAt(0)` of an empty string is NaN, which no predicate admits. Both
/// white space characters are ASCII, so the first code unit is one of them
/// exactly when the first byte is.
fn starts_with_white_space(text: &str) -> bool {
    text.as_bytes()
        .first()
        .is_some_and(|&byte| is_white_space(i32::from(byte)))
}

/// Splits `text` at every line terminator, as `split(/\r\n|[\n\r]/g)` does.
///
/// The alternation tries `\r\n` first, so a carriage return followed by a
/// line feed is one terminator, and either character alone is one as well. A
/// terminator at the end yields a trailing empty line and an empty text
/// yields one empty line. Both terminator characters are ASCII, so every cut
/// lands on a character boundary.
fn split_lines(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut line_start = 0;
    let mut index = 0;

    while index < bytes.len() {
        let length = match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => 2,
            b'\r' | b'\n' => 1,
            _ => {
                index += 1;
                continue;
            }
        };
        lines.push(&text[line_start..index]);
        index += length;
        line_start = index;
    }

    lines.push(&text[line_start..]);
    lines
}
