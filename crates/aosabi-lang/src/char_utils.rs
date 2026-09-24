// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the character classes of the GraphQL grammar and the reading
 * of the JavaScript string primitives the lexer walks the body with.
 *
 * Derived from graphql-js 16.12.0, src/language/characterClasses.ts, MIT.
 */

//! The character classes of the language layer.
//!
//! The five predicates of the reference take a `code`, which is the number
//! `charCodeAt` returns, and answer whether it is white space, a digit, a
//! letter, a name start or a name continuation. The reference lexer applies
//! them to the UTF-16 code units of a JavaScript string. The lexer here walks
//! the WTF-8 bytes of the body, so this module also holds the string
//! primitives it relies on, each with the rule of the JavaScript method it
//! reproduces and the reading of that rule over bytes.
//!
//! An index into the body is a byte offset. A character outside ASCII takes
//! two to four bytes there and one or two code units in the reference, so a
//! primitive that answers a code unit answers the first unit of the character
//! at the index, and the lexer steps over the character by its width in bytes,
//! `utf8_width` or `SURROGATE_PAIR_WIDTH`. A lone surrogate is three bytes,
//! the form WTF-8 gives it, and reads as the one code unit it is. A reader
//! that compares a code with ASCII alone reads the byte, because no byte of a
//! character outside ASCII equals an ASCII code.
//!
//! A code is an `i32` here. A JavaScript number in this role is an integer in
//! the range of a code unit or a code point, NaN past the end of the string,
//! or the negative marker the hex digit reader of the lexer answers with, and
//! a signed integer carries all three readings in one type. The range tests
//! of the predicates read exactly as the reference writes them, including
//! the lower bound of zero that only a negative value fails.

use alloc::vec::Vec;

use aosabi_json::JsString;

/// The number `charCodeAt` returns past the end of a string, as the lexer reads it.
///
/// `String.prototype.charCodeAt` returns NaN for a position at or past the
/// length, and every comparison of NaN with a number is false. Every predicate
/// the lexer applies to a code tests a bounded range or an equality against a
/// code unit, and its hex digit reader answers `-1` for anything outside its
/// three ranges, so a negative value reproduces NaN in each of them.
pub(crate) const NOT_A_NUMBER: i32 = -1;

/// The number of bytes a surrogate pair of the reference takes in the body.
///
/// WTF-8 writes the code point a pair encodes as its four byte UTF-8 and never
/// as two surrogate sequences, so the two code units the reference steps over
/// are these four bytes.
pub(crate) const SURROGATE_PAIR_WIDTH: usize = 4;

/// Returns the code unit of `body` at the byte `index` as a JavaScript number.
///
/// This is `body.charCodeAt(index)` at the code unit that starts the
/// character at `index`: the byte itself for ASCII, the code point for a
/// character below U+10000 and for a lone surrogate, the leading surrogate
/// for a character above U+FFFF, and [`NOT_A_NUMBER`] past the end. ASCII
/// takes the first branch, and the decoding of any other character stands
/// out of line.
// An `unsafe` vector scan would read faster.
pub(crate) fn char_code_at(body: &[u8], index: usize) -> i32 {
    match body.get(index) {
        Some(&byte) if byte.is_ascii() => i32::from(byte),
        Some(_) => first_code_unit(body, index),
        None => NOT_A_NUMBER,
    }
}

/// Returns the code unit of `body` at the byte `index` for a comparison against ASCII.
///
/// This is `body.charCodeAt(index)` for a reader that compares the code with
/// ASCII codes alone, through an equality, a digit, a letter or a hex digit
/// test. A byte below 0x80 is the code unit itself, and a byte from 0x80 on
/// belongs to a character outside ASCII, whose code unit equals no ASCII
/// code either, so every such comparison answers as it does on the code
/// unit. Past the end it answers [`NOT_A_NUMBER`]. The reader of a character
/// the lexer steps over, or names in an error, is [`char_code_at`].
pub(crate) fn ascii_code_at(body: &[u8], index: usize) -> i32 {
    body.get(index)
        .map_or(NOT_A_NUMBER, |&byte| i32::from(byte))
}

/// Returns the first code unit of the character outside ASCII at the byte `index`.
///
/// The names, the numbers, the punctuators and the white space of a document
/// are ASCII, and only a comment, a string, a byte order mark or an error
/// meets a character outside it, so the decoding is marked cold and stays
/// out of the loops that walk the rest.
#[cold]
fn first_code_unit(body: &[u8], index: usize) -> i32 {
    match code_point_at(body, index) {
        Some(code) if code > 0xffff => 0xd800 + ((code - 0x10000) >> 10),
        Some(code) => code,
        None => NOT_A_NUMBER,
    }
}

/// Returns the code point of `body` at the byte `index`, or `None` past the end.
///
/// This is `body.codePointAt(index)` at the code unit that starts the
/// character at `index`: a surrogate pair reads as the code point it encodes,
/// which is the four byte character of the body, and any other code unit reads
/// as itself, a lone surrogate included. A byte inside a character reads as
/// its own value, which no caller asks for, since every index the lexer
/// decodes at stands on a character boundary.
pub(crate) fn code_point_at(body: &[u8], index: usize) -> Option<i32> {
    let &lead = body.get(index)?;
    let (width, bits) = match lead {
        0xc0..=0xdf => (2, lead & 0x1f),
        0xe0..=0xef => (3, lead & 0x0f),
        0xf0..=0xff => (SURROGATE_PAIR_WIDTH, lead & 0x07),
        // ASCII, and a byte inside a character.
        _ => return Some(i32::from(lead)),
    };
    let tail = body.get(index + 1..index + width).unwrap_or_default();
    Some(tail.iter().fold(i32::from(bits), |code, &byte| {
        (code << 6) | i32::from(byte & 0x3f)
    }))
}

/// Returns the count of UTF-16 code units the WTF-8 byte `byte` starts.
///
/// A byte of ASCII is one code unit, and the lead of a character of two or
/// three bytes starts one, a lone surrogate among them. The lead of a
/// character of four bytes starts the two units of its surrogate pair, and a
/// continuation byte starts none.
pub(crate) fn code_units_started_by(byte: u8) -> usize {
    usize::from(byte & 0xc0 != 0x80) + usize::from(byte >= 0xf0)
}

/// Returns how many bytes the WTF-8 stretch `bytes` holds beyond its UTF-16 code units.
///
/// This is the count the lexer subtracts from a byte offset to reach the
/// code unit index the reference reports. Every continuation byte adds one,
/// and every lead of four bytes takes one back, because its character is
/// two code units, so a character of two or three bytes counts one or two
/// and a lone surrogate, three bytes and one code unit, counts two. A
/// stretch of ASCII counts none after one pass of [`slice::is_ascii`]. A
/// stretch cut inside a character at either end counts the continuation
/// bytes it holds less the leads of four bytes it holds. Only a stretch that
/// ends right after the lead of a character of four bytes can hold more
/// leads than continuation bytes, such as the lone byte `0xF0`, and the
/// count saturates at zero for it.
pub(crate) fn bytes_beyond_units(bytes: &[u8]) -> usize {
    if bytes.is_ascii() {
        return 0;
    }
    let mut continuation = 0;
    let mut four = 0;
    for &byte in bytes {
        continuation += usize::from(byte & 0xc0 == 0x80);
        four += usize::from(byte >= 0xf0);
    }
    continuation.saturating_sub(four)
}

/// Returns the number of bytes of the character whose code point is `code`.
///
/// This is the length of its UTF-8, and of its WTF-8 for a lone surrogate,
/// which is three bytes like every other code point from U+0800 to U+FFFF.
pub(crate) const fn utf8_width(code: i32) -> usize {
    if code < 0x80 {
        1
    } else if code < 0x800 {
        2
    } else if code < 0x10000 {
        3
    } else {
        SURROGATE_PAIR_WIDTH
    }
}

/// Returns the code point a surrogate pair encodes.
///
/// This is `UTF16SurrogatePairToCodePoint`:
/// `(lead - 0xD800) * 0x400 + (trail - 0xDC00) + 0x10000`. The two code units
/// `String.fromCodePoint(lead, trail)` writes into a string read as this one
/// code point, so a Rust string holds the pair as this character.
pub(crate) const fn utf16_surrogate_pair_to_code_point(lead: i32, trail: i32) -> i32 {
    (lead - 0xd800) * 0x400 + (trail - 0xdc00) + 0x10000
}

/// Returns the character of a code point the caller has checked.
///
/// This is `String.fromCodePoint(code)` for one code point that
/// [`is_unicode_scalar_value`] admits. Every caller in the lexer tests the
/// code point first, so the replacement character is a total function's
/// fallback and never a value the lexer emits.
pub(crate) fn from_code_point(code: i32) -> char {
    u32::try_from(code)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// Returns `count` code units of `body` from the byte `start` as a JavaScript string.
///
/// This is `body.slice(start, start + count)` in code units: the end is
/// clamped to the length of the body. A slice counts code units, so it may end
/// between the two halves of a surrogate pair and carry the leading one alone,
/// which the reference then prints inside an error message. The [`JsString`]
/// keeps that code unit, and the JSON writer puts it on the wire as the
/// reference does.
///
/// A code unit takes at most three bytes, so the window lies inside the
/// first `3 * count` bytes from `start`, taken on to the end of the
/// character they cut. Those bytes are converted and the units past `count`
/// dropped, which leaves a leading surrogate alone where the window cuts an
/// astral character.
pub(crate) fn slice_code_units(body: &[u8], start: usize, count: usize) -> JsString {
    let rest = body.get(start..).unwrap_or_default();
    let mut end = rest.len().min(3 * count);
    while rest.get(end).is_some_and(|&byte| byte & 0xc0 == 0x80) {
        end += 1;
    }
    let mut units = Vec::with_capacity(end);
    push_utf16(&mut units, rest.get(..end).unwrap_or_default());
    units.truncate(count);
    JsString::from_utf16(&units)
}

/// Appends the UTF-16 code units of the WTF-8 bytes `wtf8` to `units`.
///
/// This is the sequence a JavaScript string is. The UTF-8 stretches of the
/// WTF-8 bytes encode the way `str::encode_utf16` encodes them, a code point
/// above U+FFFF as its surrogate pair, and a lone surrogate is the one code
/// unit its three byte sequence carries, `0xED` then six bits in each of the
/// two bytes after it. `wtf8` starts and ends on a character boundary of a
/// `JsString`, which makes it well formed WTF-8, so what UTF-8 refuses in it
/// is the three bytes of a lone surrogate and nothing else.
// A `from_utf8_unchecked` would skip the recheck.
pub(crate) fn push_utf16(units: &mut Vec<u16>, wtf8: &[u8]) {
    let mut rest = wtf8;
    loop {
        let error = match core::str::from_utf8(rest) {
            Ok(text) => {
                units.extend(text.encode_utf16());
                return;
            }
            Err(error) => error,
        };
        let (text, tail) = rest.split_at(error.valid_up_to());
        // The stretch before the error is UTF-8 by the contract of
        // `valid_up_to`, so the default is never taken.
        let text = core::str::from_utf8(text).unwrap_or_default();
        units.extend(text.encode_utf16());
        let [_, second, third, after @ ..] = tail else {
            return;
        };
        units.push(0xd000 | (u16::from(second & 0x3f) << 6) | u16::from(third & 0x3f));
        rest = after;
    }
}

/// Returns the text of the WTF-8 `body` between the bytes `start` and `end`.
///
/// This is `body.slice(start, end)` for the value of a token, where both ends
/// stand on a character boundary, in a body that carries a lone surrogate,
/// which [󰀁](crate::source::Source::slice) reads through here. Every reader
/// that builds a token value steps over a surrogate pair as a whole and stops
/// at a surrogate without its partner, so the slice holds no lone surrogate
/// and is UTF-8, and the empty default of a failed decoding is never taken.
/// Safe Rust makes a `&str` of such bytes only through the check of
/// `core::str::from_utf8`, which a body of text does not need. A slice that
/// may cut a pair is [`slice_code_units`].
// A `from_utf8_unchecked` would skip this check.
pub(crate) fn slice(body: &[u8], start: usize, end: usize) -> &str {
    let end = end.min(body.len());
    let start = start.min(end);
    core::str::from_utf8(&body[start..end]).unwrap_or_default()
}

/// Returns whether `code` is a leading surrogate.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
#[must_use]
pub const fn is_leading_surrogate(code: i32) -> bool {
    code >= 0xd800 && code <= 0xdbff
}

/// Returns whether `code` is a trailing surrogate.
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
#[must_use]
pub const fn is_trailing_surrogate(code: i32) -> bool {
    code >= 0xdc00 && code <= 0xdfff
}

/// Returns whether `code` is a Unicode scalar value.
///
/// A Unicode scalar value is any Unicode code point except surrogate code
/// points. In other words, the inclusive ranges of values 0x0000 to 0xD7FF and
/// 0xE000 to 0x10FFFF.
///
/// ```text
/// SourceCharacter ::
///   - "Any Unicode scalar value"
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/lexer.ts, MIT.
#[must_use]
pub const fn is_unicode_scalar_value(code: i32) -> bool {
    (code >= 0x0000 && code <= 0xd7ff) || (code >= 0xe000 && code <= 0x0010_ffff)
}

/// Returns whether `code` is horizontal tab or space.
///
/// ```text
/// WhiteSpace ::
///   - "Horizontal Tab (U+0009)"
///   - "Space (U+0020)"
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/characterClasses.ts, MIT.
#[must_use]
pub const fn is_white_space(code: i32) -> bool {
    code == 0x0009 || code == 0x0020
}

/// Returns whether `code` is a decimal digit.
///
/// ```text
/// Digit :: one of
///   - `0` `1` `2` `3` `4` `5` `6` `7` `8` `9`
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/characterClasses.ts, MIT.
#[must_use]
pub const fn is_digit(code: i32) -> bool {
    code >= 0x0030 && code <= 0x0039
}

/// Returns whether `code` is an ASCII letter.
///
/// ```text
/// Letter :: one of
///   - `A` `B` `C` `D` `E` `F` `G` `H` `I` `J` `K` `L` `M`
///   - `N` `O` `P` `Q` `R` `S` `T` `U` `V` `W` `X` `Y` `Z`
///   - `a` `b` `c` `d` `e` `f` `g` `h` `i` `j` `k` `l` `m`
///   - `n` `o` `p` `q` `r` `s` `t` `u` `v` `w` `x` `y` `z`
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/characterClasses.ts, MIT.
#[must_use]
pub const fn is_letter(code: i32) -> bool {
    (code >= 0x0061 && code <= 0x007a) || (code >= 0x0041 && code <= 0x005a)
}

/// Returns whether `code` can start a name.
///
/// ```text
/// NameStart ::
///   - Letter
///   - `_`
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/characterClasses.ts, MIT.
#[must_use]
pub const fn is_name_start(code: i32) -> bool {
    is_letter(code) || code == 0x005f
}

/// Returns whether `code` can continue a name.
///
/// ```text
/// NameContinue ::
///   - Letter
///   - Digit
///   - `_`
/// ```
///
/// Derived from graphql-js 16.12.0, src/language/characterClasses.ts, MIT.
#[must_use]
pub const fn is_name_continue(code: i32) -> bool {
    is_letter(code) || is_digit(code) || code == 0x005f
}
