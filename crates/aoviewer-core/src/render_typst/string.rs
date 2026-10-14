// SPDX-License-Identifier: MIT
/*
 * aoviewer-core render_typst string: the Typst string literal of a text.
 *
 * A string literal in code mode escapes the backslash, the quote, the line
 * breaks and the tab and runs nothing.
 */

//! The Typst string literal.
//!
//! Every string a generated Typst file takes from a schema, a name, a
//! description, a reason or an anchor, enters it through [`typst_str`], so no
//! schema text reaches the compiler as markup or as code. The escape set is
//! small enough to audit: the backslash, the double quote, the line feed, the
//! carriage return and the tab take their escapes, and every other control
//! character and the two Unicode separators of lines and paragraphs take
//! `\u{...}`, so a literal holds no character that ends it or starts a line.
//! The rule is the one the writer of the Markdown subset applies to the text
//! of a description, byte for byte.
//!
//! Every loop runs over the characters of one string, whose length the
//! limits of the reader cap, or over the six hexadecimal digits of one code
//! point.
//!
//! [Typst string]: https://typst.app/docs/reference/foundations/str/

use alloc::string::String;

/// The shifts of the hexadecimal digits of a code point, the highest first,
/// which cover the 21 bits of the Unicode range.
const HEX_SHIFTS: [u32; 6] = [20, 16, 12, 8, 4, 0];

/// The hexadecimal digits in lower case.
const HEX: &[u8; 16] = b"0123456789abcdef";

/// Writes `text` as a Typst string literal in code mode.
///
/// The literal is `text` between double quotes, with the backslash, the
/// quote, the line feed, the carriage return and the tab escaped and every
/// other control character and U+2028 and U+2029 written as `\u{...}` in
/// lower case hexadecimal without leading zeros. The compiler reads the same
/// characters back and evaluates nothing inside a literal.
pub fn typst_str(text: &str, out: &mut String) {
    out.push('"');
    push_body(text, out);
    out.push('"');
}

/// Writes `text` as the inside of a Typst string literal, the escapes of
/// [`typst_str`] without the quotes around them.
fn push_body(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() || c == '\u{2028}' || c == '\u{2029}' => push_code_point(c, out),
            c => out.push(c),
        }
    }
}

/// Writes the escape `\u{...}` of `c`.
fn push_code_point(c: char, out: &mut String) {
    let code = u32::from(c);
    out.push_str("\\u{");
    for shift in HEX_SHIFTS {
        if code >> shift != 0 || shift == 0 {
            out.push(char::from(HEX[((code >> shift) & 0xf) as usize]));
        }
    }
    out.push('}');
}
