// SPDX-License-Identifier: MIT
/*
 * aosabi-json word: the word test that finds the bytes a scan of text stops
 * at, a word at a time.
 *
 * No analogue in the reference, whose scans run in the C++ of V8.
 */

//! The word test of a scan.
//!
//! A scan of text that stops at a few classes of bytes reads a [`Word`] at a
//! time and tests all its lanes at once, and only a word that holds such a
//! byte goes on to the byte it stops at. The test of a class is exact for a
//! word: it is nonzero exactly when some lane of the word is in the class, by
//! the borrow argument of the zero byte test. A borrow runs toward the higher
//! lanes alone, so of a word read little endian the lowest lane the test marks
//! is the first byte of the class, and a lane above it may be marked although
//! its byte is outside the class.
//!
//! The request parser of `graphql-over-http` ends a run of a string at a
//! quotation mark, a backslash or a control byte, [`string_stops`], and a run
//! of UTF-16 code units narrows to bytes as long as no unit is above ASCII,
//! [`ascii_units`]. The writer of a string literal in `string.rs` escapes the
//! same three classes of a UTF-8 string, [`string_stops`], and of a WTF-8
//! string the lead byte of a lone surrogate beside them, [`wtf8_escapes`]. A
//! word is 64 bits where the target computes in 64 bits, a host of 64 bits and
//! `wasm32`, whose instructions of 64 bits V8 compiles to registers of 64 bits
//! on an aarch64 host, and 32 bits on a core of 32 bits, the ESP32-C3 and the
//! player.
//!
//! Every function of the module is inlined always, so a word loop holds no
//! call. At the opt-level `z` of the player a hint alone leaves
//! [`string_stops`] a call in the word loop of the escape.

/// The integer a scan reads at a time, 64 bits.
#[cfg(any(target_pointer_width = "64", target_family = "wasm"))]
pub type Word = u64;

/// The integer a scan reads at a time, 32 bits on a core of 32 bits.
#[cfg(not(any(target_pointer_width = "64", target_family = "wasm")))]
pub type Word = u32;

/// The bytes of a [`Word`].
pub const BYTES: usize = core::mem::size_of::<Word>();

/// The UTF-16 code units of a [`Word`].
pub const UNITS: usize = BYTES / 2;

/// The byte 0x01 in every lane.
const ONES: Word = Word::MAX / 0xFF;

/// The top bit of every lane.
const HIGHS: Word = ONES << 7;

/// The first byte that is no control byte, 0x20, in every lane.
const SPACES: Word = ONES * 0x20;

/// The quotation mark in every lane.
const QUOTES: Word = ONES * 0x22;

/// The backslash in every lane.
const BACKSLASHES: Word = ONES * 0x5C;

/// The lead byte of a surrogate code point in WTF-8, 0xED, in every lane.
const SURROGATE_LEADS: Word = ONES * 0xED;

/// The bits of a code unit above ASCII, 0xFF80, in every lane of a code unit.
const NON_ASCII_UNITS: Word = Word::MAX / 0xFFFF * 0xFF80;

/// Returns the word of the bytes `bytes`, the first byte in the lowest lane.
// Always inlined, as the module says, so a word loop holds no call.
#[expect(clippy::inline_always)]
#[inline(always)]
#[must_use]
pub const fn load(bytes: &[u8; BYTES]) -> Word {
    Word::from_le_bytes(*bytes)
}

/// Returns the word of the code units `units`, the first unit in the lowest lane.
// Always inlined, as the module says, so a word loop holds no call.
#[expect(clippy::inline_always)]
#[inline(always)]
#[must_use]
pub fn load_units(units: &[u16; UNITS]) -> Word {
    units
        .iter()
        .rev()
        .fold(0, |word, &unit| (word << 16) | Word::from(unit))
}

/// Returns the lane of the lowest bit `marked` holds, a byte offset into the word.
///
/// `marked` is the nonzero result of a test of this module over a word read
/// little endian, so its lowest bit is the top bit of the first lane the test
/// marks.
// Always inlined, as the module says, so a word loop holds no call.
#[expect(clippy::inline_always)]
#[inline(always)]
#[must_use]
pub const fn first_lane(marked: Word) -> usize {
    (marked.trailing_zeros() / 8) as usize
}

/// Returns the top bit of every lane of `word` that is zero, and possibly of lanes above the lowest of them.
///
/// The test subtracts one from every lane and keeps the top bit of a lane that
/// was below 0x80 and borrowed. A lane below the lowest zero lane never
/// borrows, so the lowest bit the test sets marks the lowest zero lane
/// exactly.
// Always inlined, as the module says, so a word loop holds no call.
#[expect(clippy::inline_always)]
#[inline(always)]
#[must_use]
pub const fn zero_lanes(word: Word) -> Word {
    word.wrapping_sub(ONES) & !word & HIGHS
}

/// Returns the top bit of every lane of `word` below 0x20, a control byte, and possibly of lanes above the lowest of them.
///
/// The test is the zero byte test with 0x20 subtracted from every lane in
/// place of 1, which marks a lane below 0x20 and no lane of 0x20 or above
/// that no borrow reaches.
// Always inlined, as the module says, so a word loop holds no call.
#[expect(clippy::inline_always)]
#[inline(always)]
#[must_use]
pub const fn control_lanes(word: Word) -> Word {
    word.wrapping_sub(SPACES) & !word & HIGHS
}

/// Returns the top bit of every lane of `word` that ends a run of a JSON string, and possibly of lanes above the lowest of them.
///
/// A run of a string ends at a quotation mark, which closes it, at a
/// backslash, which escapes, and at a control byte, which no string holds
/// unescaped. The result is nonzero exactly when some lane of `word` is one of
/// them.
///
/// The same three classes are the bytes `QuoteJSONString` escapes in a UTF-8
/// string, the quotation mark, the backslash and every code unit below U+0020,
/// since every other code point of a string that holds no lone surrogate is
/// written as it stands.
///
/// [parse]: https://tc39.es/ecma262/#sec-json.parse
/// [quote]: https://tc39.es/ecma262/#sec-quotejsonstring
// Always inlined, as the module says, so a word loop holds no call.
#[expect(clippy::inline_always)]
#[inline(always)]
#[must_use]
pub const fn string_stops(word: Word) -> Word {
    control_lanes(word) | zero_lanes(word ^ QUOTES) | zero_lanes(word ^ BACKSLASHES)
}

/// Returns the top bit of every lane of `word` that may start an escape of a WTF-8 string, and possibly of lanes above the lowest of them.
///
/// `QuoteJSONString` escapes the three classes of [`string_stops`] and every
/// lone surrogate. A lone surrogate is three bytes in WTF-8 that start with
/// 0xED, which starts the characters from U+D000 to U+D7FF as well, so a
/// marked lead byte is a candidate whose next two bytes decide. The result is
/// nonzero exactly when some lane of `word` is a quotation mark, a backslash,
/// a control byte or 0xED.
///
/// [quote]: https://tc39.es/ecma262/#sec-quotejsonstring
// Always inlined, as the module says, so a word loop holds no call.
#[expect(clippy::inline_always)]
#[inline(always)]
#[must_use]
pub const fn wtf8_escapes(word: Word) -> Word {
    string_stops(word) | zero_lanes(word ^ SURROGATE_LEADS)
}

/// Returns whether every code unit of the word `word` of [`load_units`] is ASCII.
// Always inlined, as the module says, so a word loop holds no call.
#[expect(clippy::inline_always)]
#[inline(always)]
#[must_use]
pub const fn ascii_units(word: Word) -> bool {
    word & NON_ASCII_UNITS == 0
}
