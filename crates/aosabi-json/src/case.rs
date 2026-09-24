// SPDX-License-Identifier: MIT
/*
 * aosabi-json: the lower case and the upper case of JavaScript for a caller
 * that reads ASCII alone.
 *
 * No translation. The mappings it keeps are those of
 * String.prototype.toLowerCase and String.prototype.toUpperCase, ECMA-262
 * sections 22.1.3.28 and 22.1.3.30, whose result holds an ASCII character, as
 * the pinned node 24.13.0 answers them for every Unicode scalar value, which
 * tests/notebooks/02_language.ipynb derives and holds this module to.
 */

//! The case conversions of JavaScript, reduced to the mappings that reach ASCII.
//!
//! A caller that compares mapped text with ASCII, or scans it with a grammar
//! whose classes are ASCII, decides by the ASCII characters of the mapping
//! alone. [`lower_ascii_exact()`] and [`upper_ascii_exact()`] map every ASCII
//! letter, map every other character whose full mapping holds an ASCII
//! character to exactly that mapping, and keep every other character as it is.
//! The reference maps such a kept character to text with no ASCII character,
//! so both results hold the same ASCII characters in the same order with text
//! beyond ASCII at the same places between them, and a comparison with ASCII
//! or an ASCII grammar answers alike. The lower case also keeps the length in
//! UTF-16 code units of every character it keeps, and the reference does the
//! same, so an ASCII code unit sits at the same index of both results.
//!
//! Neither function carries a Unicode table, where `str::to_lowercase` and
//! `str::to_uppercase` link two conversion tables of `core`, of 12,432 and
//! 11,696 bytes on aarch64. A caller whose mapped text reaches an answer or a
//! log line keeps the functions of the standard library.

use alloc::string::String;

/// The characters beyond ASCII whose lower case holds an ASCII character, with
/// that lower case.
///
/// U+0130 LATIN CAPITAL LETTER I WITH DOT ABOVE and U+212A KELVIN SIGN.
const LOWER: [(char, &str); 2] = [('\u{130}', "i\u{307}"), ('\u{212A}', "k")];

/// The characters beyond ASCII whose upper case holds an ASCII character, with
/// that upper case.
///
/// The dotless i, the long s, the sharp s, the letters whose upper case spells
/// an ASCII letter with a modifier or a combining mark, and the Latin
/// ligatures.
const UPPER: [(char, &str); 17] = [
    ('\u{DF}', "SS"),
    ('\u{131}', "I"),
    ('\u{149}', "\u{2BC}N"),
    ('\u{17F}', "S"),
    ('\u{1F0}', "J\u{30C}"),
    ('\u{1E96}', "H\u{331}"),
    ('\u{1E97}', "T\u{308}"),
    ('\u{1E98}', "W\u{30A}"),
    ('\u{1E99}', "Y\u{30A}"),
    ('\u{1E9A}', "A\u{2BE}"),
    ('\u{FB00}', "FF"),
    ('\u{FB01}', "FI"),
    ('\u{FB02}', "FL"),
    ('\u{FB03}', "FFI"),
    ('\u{FB04}', "FFL"),
    ('\u{FB05}', "ST"),
    ('\u{FB06}', "ST"),
];

/// Returns the lower case of `text` as far as it holds ASCII.
///
/// An ASCII letter maps to its lower case, U+0130 to `i` with U+0307 and
/// U+212A to `k`, the mappings of `String.prototype.toLowerCase` that hold an
/// ASCII character, and every other character stays as it is.
#[must_use]
pub fn lower_ascii_exact(text: &str) -> String {
    map(text, &LOWER, char::to_ascii_lowercase)
}

/// Returns the upper case of `text` as far as it holds ASCII.
///
/// An ASCII letter maps to its upper case, and each character of the table
/// above to the upper case `String.prototype.toUpperCase` gives it, which
/// holds an ASCII character, `SS` for U+00DF among them. Every other
/// character stays as it is.
#[must_use]
pub fn upper_ascii_exact(text: &str) -> String {
    map(text, &UPPER, char::to_ascii_uppercase)
}

/// Returns `text` with every character mapped through `table` or `ascii`.
fn map(text: &str, table: &[(char, &str)], ascii: fn(&char) -> char) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(ascii(&c));
            continue;
        }
        match table.iter().find(|(from, _)| *from == c) {
            Some((_, to)) => out.push_str(to),
            None => out.push(c),
        }
    }
    out
}
