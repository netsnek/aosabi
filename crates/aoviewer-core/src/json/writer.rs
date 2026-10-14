// SPDX-License-Identifier: MIT
/*
 * aoviewer-core json writer: the writer of a JSON string in the escapes of
 * JSON.stringify.
 *
 * Every JSON file of a generated tree writes its strings the way node writes
 * them, which the json primitive of the seam judges.
 */

//! The JSON string writer.

use alloc::string::String;

/// The digits of a `\u` escape, lower case as `UnicodeEscape` writes them.
const HEX: &[u8; 16] = b"0123456789abcdef";

/// Writes `text` as a JSON string the way `JSON.stringify` writes it.
///
/// The quote, the backslash and the control characters U+0000 to U+001F are
/// escaped, the five of them with a short form as `\b`, `\t`, `\n`, `\f` and
/// `\r`, the rest as `\u00` and two lower case digits, and every other character
/// is written as itself, the solidus and U+2028 included. A Rust string holds no
/// lone surrogate, so the escape of one never arises.
pub fn write_str(text: &str, out: &mut String) {
    out.push('"');
    let mut start = 0;
    // The loop runs once per byte of `text`, which a reader capped at
    // `ReadLimits::max_input_bytes` or a writer produced.
    for (i, &b) in text.as_bytes().iter().enumerate() {
        let short = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            0x08 => "\\b",
            b'\t' => "\\t",
            b'\n' => "\\n",
            0x0c => "\\f",
            b'\r' => "\\r",
            0x00..=0x1f => "",
            _ => continue,
        };
        // Every escaped byte is ASCII, so `i` is a character boundary.
        out.push_str(&text[start..i]);
        if short.is_empty() {
            out.push_str("\\u00");
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0x0f)]));
        } else {
            out.push_str(short);
        }
        start = i + 1;
    }
    out.push_str(&text[start..]);
    out.push('"');
}
