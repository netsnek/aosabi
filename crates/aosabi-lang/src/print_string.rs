// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the printer of a string value literal.
 *
 * Derived from graphql-js 16.12.0, src/language/printString.ts, MIT.
 */

//! The string value printer of the language layer.
//!
//! [`print_string`] writes a string as a GraphQL `StringValue` literal, with
//! every control character, the double quote and the backslash replaced by
//! the escape sequence of the reference table. The printer writes every non
//! block string value through it.
//!
//! The reference matches the regular expression
//! `/[\x00-\x1f\x22\x5c\x7f-\x9f]/g` over UTF-16 code units and replaces each
//! match by the table entry at its code unit value. Every code unit the class
//! names is below U+00A0, and such a code unit is one code point, so a walk
//! over the characters of a Rust string meets the same matches in the same
//! order.

use alloc::string::String;

/// The escape sequence of each code unit below U+00A0, indexed by the code
/// unit value, with an empty entry where the reference expression does not
/// match.
// The rows are the rows of the reference, eight entries each, so a reader
// checks the table against the vendored file line by line.
#[rustfmt::skip]
const ESCAPE_SEQUENCES: [&str; 160] = [
    "\\u0000", "\\u0001", "\\u0002", "\\u0003", "\\u0004", "\\u0005", "\\u0006", "\\u0007",
    "\\b", "\\t", "\\n", "\\u000B", "\\f", "\\r", "\\u000E", "\\u000F",
    "\\u0010", "\\u0011", "\\u0012", "\\u0013", "\\u0014", "\\u0015", "\\u0016", "\\u0017",
    "\\u0018", "\\u0019", "\\u001A", "\\u001B", "\\u001C", "\\u001D", "\\u001E", "\\u001F",
    "", "", "\\\"", "", "", "", "", "",
    "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "",
    "", "", "", "", "\\\\", "", "", "",
    "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "\\u007F",
    "\\u0080", "\\u0081", "\\u0082", "\\u0083", "\\u0084", "\\u0085", "\\u0086", "\\u0087",
    "\\u0088", "\\u0089", "\\u008A", "\\u008B", "\\u008C", "\\u008D", "\\u008E", "\\u008F",
    "\\u0090", "\\u0091", "\\u0092", "\\u0093", "\\u0094", "\\u0095", "\\u0096", "\\u0097",
    "\\u0098", "\\u0099", "\\u009A", "\\u009B", "\\u009C", "\\u009D", "\\u009E", "\\u009F",
];

/// Prints a string as a GraphQL `StringValue` literal.
///
/// Replaces control characters and excluded characters (`"` U+0022 and `\`
/// U+005C) with escape sequences, and wraps the result in double quotes.
/// `str` is the value of the string node.
///
/// Derived from graphql-js 16.12.0, src/language/printString.ts, MIT.
#[must_use]
pub fn print_string(str: &str) -> String {
    let mut out = String::with_capacity(str.len() + 2);
    out.push('"');
    for character in str.chars() {
        match escaped_replacer(character) {
            Some(sequence) => out.push_str(sequence),
            None => out.push(character),
        }
    }
    out.push('"');
    out
}

/// Returns the escape sequence of a character the reference expression
/// matches, and `None` for a character it leaves as it is.
///
/// The character class of the expression is `[\x00-\x1f\x22\x5c\x7f-\x9f]`,
/// and the replacement is the table entry at the code unit value.
fn escaped_replacer(character: char) -> Option<&'static str> {
    if !matches!(character, '\0'..='\x1f' | '"' | '\\' | '\x7f'..='\u{9f}') {
        return None;
    }
    ESCAPE_SEQUENCES
        .get(usize::try_from(u32::from(character)).ok()?)
        .copied()
}
