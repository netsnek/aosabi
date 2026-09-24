// SPDX-License-Identifier: MIT
/*
 * aosabi-json string: a string as ECMA-262 QuoteJSONString writes it.
 *
 * The input is a JsString, whose bytes are WTF-8, so a lone surrogate is
 * reachable and takes the escape the clause gives it, and a surrogate pair has
 * already become one code point, or a Rust string, whose bytes are UTF-8 and
 * hold no lone surrogate. The scan for the bytes to escape is the word test of
 * word.rs, which no analogue of the reference has, since its scan runs in the
 * C++ of V8.
 */

//! The string half of the byte contract.
//!
//! `QuoteJSONString` walks the code points of a string. Seven code points take a
//! short escape, every other code point below U+0020 and every lone surrogate
//! takes `\u` plus four lowercase hexadecimal digits, and everything else is
//! written as it is, including U+2028, U+2029 and U+007F.
//!
//! The bytes are read a [`Word`] at a time while whole words remain, and a
//! string of at least two steps of `STEP` bytes, two words on a host of 64 bits,
//! is read a step at a time up to its first byte to escape. The word test of
//! [`word::wtf8_escapes`] marks the bytes that may start an escape of WTF-8, a
//! quotation mark, a backslash, a control byte and 0xED, the lead byte of a lone
//! surrogate, and the one of [`word::string_stops`] marks the first three, all
//! that a UTF-8 string escapes. A word with no marked lane passes as it stands,
//! the lowest lane a test marks is the first such byte of the word, and the
//! bytes after the last whole word are tested one at a time. So a run that needs
//! no escape is copied in one piece, and the byte that ends it is escaped where
//! it stands.
//!
//! Where the rest of a string from a byte to escape on holds at least
//! `DENSE_FROM` bytes, it goes through `escape_dense`, which writes the bytes of
//! a run of words that each hold a byte to escape into a buffer on the stack,
//! appends the buffer once and reads on a step at a time up to the next byte to
//! escape, so a string of many escapes calls no copy per escape. A shorter rest
//! fills no buffer and is read on a word at a time after each escape.
//!
//! [quote]: https://tc39.es/ecma262/#sec-quotejsonstring

use alloc::vec::Vec;

use crate::{
    value::{
        JsString,
        surrogate_unit, //
    },
    word::{
        self,
        Word, //
    }, //
};

/// The sixteen digits of `UnicodeEscape`, which are lowercase.
///
/// The clause formats the code unit as a four digit lowercase hexadecimal number
/// padded with zeros on the left.
///
/// [unicode-escape]: https://tc39.es/ecma262/#sec-unicodeescape
const HEX: &[u8; 16] = b"0123456789abcdef";

/// The lead byte of a lone surrogate in WTF-8.
const SURROGATE_LEAD: u8 = 0xED;

/// The class bit of a byte that a UTF-8 string escapes, a quotation mark, a backslash or a control byte.
const UTF8: u8 = 1;

/// The class bit of a byte that may start an escape of a WTF-8 string, a byte of [`UTF8`] or the lead byte of a lone surrogate.
const WTF8: u8 = 2;

/// The class bits of every byte, one read of the table of 256 bytes per byte the scan tests alone.
static CLASSES: [u8; 256] = classes();

/// The words of one step of the scan, two on a host of 64 bits.
///
/// Two words a step test 16 bytes for one branch, which the compiler turns into
/// a vector of two lanes on aarch64, at 0.24 cycles per byte against 0.86 of one
/// word a step on the model apple-m1 of `llvm-mca` and 0.54 of the time per byte
/// natively over the string of `text-plain`, while the model cortex-a53 reads
/// 3.38 cycles against 3.29. A host of 32 bits, the Worker module and the cores
/// of the cards of 32 bits among them, keeps one word a step.
#[cfg(target_pointer_width = "64")]
const STEP_WORDS: usize = 2;

/// The words of one step of the scan, one where a host has no 64 bits.
#[cfg(not(target_pointer_width = "64"))]
const STEP_WORDS: usize = 1;

/// The bytes of one step of the scan.
const STEP: usize = STEP_WORDS * word::BYTES;

/// The words that each hold a byte to escape whose escapes [`escape_dense`] gathers before it appends them.
const DENSE_WORDS: usize = 4;

/// The bytes at least that the rest of a string holds from a byte to escape on where it goes through [`escape_dense`].
///
/// A rest of fewer bytes is escaped one byte to escape at a time, so a short
/// string fills no buffer. The buffer for every string takes 1.09 times the time
/// of this threshold in the Worker module at opt-level z and 1.01 natively over
/// the twelve texts of the corpus, and no buffer takes 2.91 and 1.32 times its
/// time over the string of `text-quote`.
const DENSE_FROM: usize = 64;

/// The most bytes [`escape_dense`] writes for the words it gathers, six per byte.
const DENSE_BUFFER: usize = 6 * word::BYTES * DENSE_WORDS;

/// Returns the table of the class bits of every byte.
const fn classes() -> [u8; 256] {
    let mut table = [0; 256];
    let mut byte = 0;
    while byte < 0x20 {
        table[byte] = UTF8 | WTF8;
        byte += 1;
    }
    table[b'"' as usize] = UTF8 | WTF8;
    table[b'\\' as usize] = UTF8 | WTF8;
    table[SURROGATE_LEAD as usize] = WTF8;
    table
}

/// Appends `text` as one JSON string literal, quotation marks included.
///
/// The bytes go into `out`. A code unit that is a lone surrogate is written as
/// its `\u` escape, which is the well formed `JSON.stringify` rule, and a
/// surrogate pair is written as the UTF-8 of the code point it encodes,
/// because [`JsString`] has already merged the pair.
pub fn escape(out: &mut Vec<u8>, text: &JsString) {
    escape_wtf8(out, text.as_wtf8());
}

/// Appends `text` as one JSON string literal, quotation marks included.
///
/// This is [`escape`] for a Rust string, which can carry no lone surrogate,
/// so only the control character and short escape rules apply to it, and its
/// scan tests no byte for the lead of a surrogate.
pub fn escape_str(out: &mut Vec<u8>, text: &str) {
    escape_utf8(out, text);
}

/// Appends the quoted form of a WTF-8 byte string.
///
/// `bytes` is WTF-8, which is UTF-8 plus the three byte encoding of a lone
/// surrogate code unit. The scan of [`first_escape`] finds the first byte that
/// may start an escape, and the bytes of one word from it on are tested one at
/// a time, so a run of escapes stays in one loop. A run of bytes that needs no
/// escape is copied in one piece. A lead byte 0xED that starts a character
/// from U+D000 to U+D7FF is no lone surrogate and stays in its run. A rest of
/// at least [`DENSE_FROM`] bytes from a byte that may start an escape goes
/// through [`escape_dense`].
// An `unsafe` vector scan escapes a long plain string in 0.75 of the time natively and a list of short strings in 1.09.
fn escape_wtf8(out: &mut Vec<u8>, bytes: &[u8]) {
    out.push(b'"');
    let mut copied = 0;
    let mut at = first_escape::<WTF8>(bytes);
    while at < bytes.len() {
        if bytes.len() - at >= DENSE_FROM {
            out.extend_from_slice(bytes.get(copied..at).unwrap_or_default());
            escape_dense::<WTF8>(out, bytes, at);
            out.push(b'"');
            return;
        }
        let stop = bytes.len().min(at + word::BYTES);
        while let Some(&byte) = bytes.get(at).filter(|_| at < stop) {
            if CLASSES[usize::from(byte)] & WTF8 == 0 {
                at += 1;
            } else if byte != SURROGATE_LEAD {
                out.extend_from_slice(bytes.get(copied..at).unwrap_or_default());
                escape_ascii(out, byte);
                at += 1;
                copied = at;
            } else if let Some(unit) = surrogate_unit(bytes.get(at..).unwrap_or_default()) {
                out.extend_from_slice(bytes.get(copied..at).unwrap_or_default());
                unicode_escape(out, unit);
                at += 3;
                copied = at;
            } else {
                at += 1;
            }
        }
        at = next_escape::<{ word::BYTES }, WTF8>(bytes, at);
    }
    out.extend_from_slice(bytes.get(copied..).unwrap_or_default());
    out.push(b'"');
}

/// Appends the quoted form of a Rust string.
///
/// The bytes of `text` are UTF-8, which hold no lone surrogate, so the scan
/// stops at a quotation mark, a backslash and a control byte alone, each of
/// which takes the escape of [`escape_ascii`], and every run between them is
/// copied in one piece. The bytes of one word from the first such byte on are
/// tested one at a time, as [`escape_wtf8`] tests them, and a rest of at
/// least [`DENSE_FROM`] bytes goes through [`escape_dense`] as well.
// An `unsafe` vector scan escapes a long plain string in 0.75 of the time natively and a list of short strings in 1.09.
fn escape_utf8(out: &mut Vec<u8>, text: &str) {
    let bytes = text.as_bytes();
    out.push(b'"');
    let mut copied = 0;
    let mut at = first_escape::<UTF8>(bytes);
    while at < bytes.len() {
        if bytes.len() - at >= DENSE_FROM {
            out.extend_from_slice(bytes.get(copied..at).unwrap_or_default());
            escape_dense::<UTF8>(out, bytes, at);
            out.push(b'"');
            return;
        }
        let stop = bytes.len().min(at + word::BYTES);
        while let Some(&byte) = bytes.get(at).filter(|_| at < stop) {
            if CLASSES[usize::from(byte)] & UTF8 != 0 {
                out.extend_from_slice(bytes.get(copied..at).unwrap_or_default());
                escape_ascii(out, byte);
                copied = at + 1;
            }
            at += 1;
        }
        at = next_escape::<{ word::BYTES }, UTF8>(bytes, at);
    }
    out.extend_from_slice(bytes.get(copied..).unwrap_or_default());
    out.push(b'"');
}

/// Appends the bytes of `bytes` from `at` on, where `at` may start an escape of the class `CLASS`, and every byte before `at` is written.
///
/// The bytes of the word from `at` on and of each next word that holds a
/// byte to escape, up to [`DENSE_WORDS`] words, are written into a buffer on
/// the stack, a byte that needs no escape as it stands and a byte to escape
/// as its escape, and the buffer is appended once. The scan of
/// [`next_escape`] then finds the next byte to escape, and the run before it
/// is copied in one piece. `CLASS` is [`WTF8`] for the bytes of a
/// [`JsString`], where a lead byte 0xED may start a lone surrogate, and
/// [`UTF8`] for those of a Rust string, where it starts a character as it
/// stands.
// Out of line the buffer takes room on the stack of a string that reaches it
// alone. An `unsafe` vector escape takes 0.98 of the time natively and 1.02 in
// the Worker over a string of many escapes.
#[inline(never)]
fn escape_dense<const CLASS: u8>(out: &mut Vec<u8>, bytes: &[u8], mut at: usize) {
    let mut buffer = [0; DENSE_BUFFER];
    loop {
        let mut len = 0;
        for _ in 0..DENSE_WORDS {
            let stop = bytes.len().min(at + word::BYTES);
            while at < stop {
                let (read, written) = escape_into::<CLASS>(&mut buffer, len, bytes, at);
                at += read;
                len += written;
            }
            let next = bytes
                .get(at..)
                .and_then(<[u8]>::first_chunk::<{ word::BYTES }>);
            if next.is_some_and(|next| marks::<CLASS>(word::load(next)) == 0) {
                break;
            }
            if at >= bytes.len() {
                break;
            }
        }
        out.extend_from_slice(buffer.get(..len).unwrap_or_default());
        let found = next_escape::<STEP, CLASS>(bytes, at);
        out.extend_from_slice(bytes.get(at..found).unwrap_or_default());
        if found >= bytes.len() {
            break;
        }
        at = found;
    }
}

/// Writes at `len` of `buffer` the byte of `bytes` at `at` as it stands or as its escape of the class `CLASS`, and returns the bytes it reads and the bytes it writes.
///
/// A lone surrogate reads its three bytes and writes its six. A write past
/// the end of `buffer` is dropped, which no caller reaches, since
/// [`escape_dense`] writes at most six bytes for each of the bytes of
/// [`DENSE_WORDS`] words into a buffer of [`DENSE_BUFFER`] bytes.
// Always inlined into the loop of `escape_dense`, which calls it for every byte it writes.
#[expect(
    clippy::inline_always,
    reason = "a hint inlines nothing into a caller built at opt-level z"
)]
#[inline(always)]
fn escape_into<const CLASS: u8>(
    buffer: &mut [u8],
    len: usize,
    bytes: &[u8],
    at: usize,
) -> (usize, usize) {
    let Some(&byte) = bytes.get(at) else {
        return (1, 0);
    };
    if CLASSES[usize::from(byte)] & CLASS == 0 {
        if let Some(slot) = buffer.get_mut(len) {
            *slot = byte;
        }
        return (1, 1);
    }
    if byte != SURROGATE_LEAD {
        return (1, ascii_escape_into(buffer, len, byte));
    }
    if let Some(unit) = surrogate_unit(bytes.get(at..).unwrap_or_default()) {
        if let Some(slot) = buffer.get_mut(len..len + 6) {
            slot.copy_from_slice(&unicode_digits(unit));
        }
        return (3, 6);
    }
    if let Some(slot) = buffer.get_mut(len) {
        *slot = byte;
    }
    (1, 1)
}

/// Returns the offset of the first byte of `bytes` that may start an escape of the class `CLASS`, or the length of `bytes`.
///
/// A string of at least two steps of [`STEP`] bytes is scanned a step at a time
/// where a step holds more than a word, and a shorter string a word at a time,
/// so a short string, the most frequent key and value of an answer, runs the
/// loop of one word alone. Two words a step for every string take 1.08 times the
/// instructions of this test and 1.06 times its time natively over the twelve
/// texts of the corpus, 1.24 times its instructions over the short strings of
/// `texts-plain`.
// Always inlined into the two escapes, each of which calls it once per string.
#[expect(clippy::inline_always)]
#[inline(always)]
fn first_escape<const CLASS: u8>(bytes: &[u8]) -> usize {
    if STEP > word::BYTES && bytes.len() >= 2 * STEP {
        next_escape::<STEP, CLASS>(bytes, 0)
    } else {
        next_escape::<{ word::BYTES }, CLASS>(bytes, 0)
    }
}

/// Returns the top bit of every lane of `word` that may start an escape of the class `CLASS`, the word test of [`next_escape`] for that class.
// Always inlined, as the tests of `word.rs` are, so a word loop holds no call.
#[expect(clippy::inline_always)]
#[inline(always)]
const fn marks<const CLASS: u8>(word: Word) -> Word {
    if CLASS == WTF8 {
        word::wtf8_escapes(word)
    } else {
        word::string_stops(word)
    }
}

/// Returns the offset of the first byte of `bytes` at or after `from` that may start an escape of the class `CLASS`, or the length of `bytes`.
///
/// Where a step of `STEP` bytes holds more than a word, the bytes are read a
/// step at a time while whole steps remain, and then a word at a time while
/// whole words remain. The words of a step are tested together and branch once,
/// and in the first word of a step that holds a mark the lowest lane the word
/// test [`marks`] marks is the first such byte, so the bytes of a lane that
/// holds none pass in the word as they stand. The bytes after the last whole
/// word are tested one at a time against the class bit `CLASS` of [`CLASSES`],
/// which holds for the bytes of the same classes. The test is named by the
/// class, so no loop calls it through a reference, which the player's opt-level
/// z keeps a call.
// An `unsafe` vector escape tests 16 bytes a step, at 0.75 of the time natively
// and 0.62 in the Worker over a long plain string and 1.09 and 1.23 over short
// strings. An unaligned word read through a raw pointer, which reads the last
// bytes as a word too, takes 0.80 of the time natively and 0.89 and 1.07 in the
// Worker at opt-level z and 3 over short strings.
#[inline]
fn next_escape<const STEP: usize, const CLASS: u8>(bytes: &[u8], from: usize) -> usize {
    let mut start = from;
    let mut rest = bytes.get(from..).unwrap_or_default();
    if STEP > word::BYTES {
        let (steps, _) = rest.as_chunks::<STEP>();
        for (at, step) in steps.iter().enumerate() {
            let (words, _) = step.as_chunks::<{ word::BYTES }>();
            let mut any = 0;
            for chunk in words {
                any |= marks::<CLASS>(word::load(chunk));
            }
            if any != 0 {
                return next_in_step::<CLASS>(words, from + at * STEP);
            }
        }
        start = from + steps.len() * STEP;
        rest = rest.get(steps.len() * STEP..).unwrap_or_default();
    }
    let (words, tail) = rest.as_chunks::<{ word::BYTES }>();
    for (at, chunk) in words.iter().enumerate() {
        let marked = marks::<CLASS>(word::load(chunk));
        if marked != 0 {
            return start + at * word::BYTES + word::first_lane(marked);
        }
    }
    let start = start + words.len() * word::BYTES;
    tail.iter()
        .position(|&byte| CLASSES[usize::from(byte)] & CLASS != 0)
        .map_or(bytes.len(), |at| start + at)
}

/// Returns the offset of the first byte the test of the class `CLASS` marks in the words `words` of a step that starts at `start` and holds one.
///
/// The words come in the order of the bytes, so the first word with a mark
/// holds the first such byte in its lowest marked lane.
// Always inlined into the scan, whose step leaves it at its first mark alone.
#[expect(clippy::inline_always)]
#[inline(always)]
fn next_in_step<const CLASS: u8>(words: &[[u8; word::BYTES]], start: usize) -> usize {
    for (at, chunk) in words.iter().enumerate() {
        let marked = marks::<CLASS>(word::load(chunk));
        if marked != 0 {
            return start + at * word::BYTES + word::first_lane(marked);
        }
    }
    start + words.len() * word::BYTES
}

/// Appends the escape of one code unit below U+0020, or of `"` or `\`.
///
/// The seven rows of the single character escape table take their two byte
/// escape, and every other code unit below U+0020 takes `UnicodeEscape`.
///
/// [quote]: https://tc39.es/ecma262/#sec-quotejsonstring
// Always inlined, since at opt-level z a hint leaves it a call in the byte loop
// of each escape.
#[expect(
    clippy::inline_always,
    reason = "a hint inlines nothing into a caller built at opt-level z"
)]
#[inline(always)]
fn escape_ascii(out: &mut Vec<u8>, byte: u8) {
    match short_escape(byte) {
        Some(short) => out.extend_from_slice(&[b'\\', short]),
        None => out.extend_from_slice(&unicode_digits(u16::from(byte))),
    }
}

/// Writes at `len` of `buffer` the escape of one code unit below U+0020, or of `"` or `\`, as [`escape_ascii`] appends it, and returns its bytes.
// Always inlined into `escape_into`, as `escape_ascii` is into the byte loops.
#[expect(clippy::inline_always)]
#[inline(always)]
fn ascii_escape_into(buffer: &mut [u8], len: usize, byte: u8) -> usize {
    if let Some(short) = short_escape(byte) {
        if let Some(slot) = buffer.get_mut(len..len + 2) {
            slot.copy_from_slice(&[b'\\', short]);
        }
        return 2;
    }
    if let Some(slot) = buffer.get_mut(len..len + 6) {
        slot.copy_from_slice(&unicode_digits(u16::from(byte)));
    }
    6
}

/// Returns the character of the short escape of `byte`, a row of the single character escape table, or `None` for a code unit that takes `UnicodeEscape`.
// Always inlined, as the escapes that read it are.
#[expect(clippy::inline_always)]
#[inline(always)]
const fn short_escape(byte: u8) -> Option<u8> {
    match byte {
        0x08 => Some(b'b'),
        0x09 => Some(b't'),
        0x0A => Some(b'n'),
        0x0C => Some(b'f'),
        0x0D => Some(b'r'),
        b'"' => Some(b'"'),
        b'\\' => Some(b'\\'),
        _ => None,
    }
}

/// Appends `\u` and the four lowercase hexadecimal digits of `unit`.
///
/// This is `UnicodeEscape`.
///
/// [unicode-escape]: https://tc39.es/ecma262/#sec-unicodeescape
// Always inlined, since at opt-level z a hint leaves it a call in the byte loop
// of the escape of WTF-8 and in `escape_ascii`.
#[expect(
    clippy::inline_always,
    reason = "a hint inlines nothing into a caller built at opt-level z"
)]
#[inline(always)]
fn unicode_escape(out: &mut Vec<u8>, unit: u16) {
    out.extend_from_slice(&unicode_digits(unit));
}

/// Returns `\u` and the four lowercase hexadecimal digits of `unit`, the six bytes of `UnicodeEscape`.
// Always inlined, as the escapes that write it are.
#[expect(clippy::inline_always)]
#[inline(always)]
fn unicode_digits(unit: u16) -> [u8; 6] {
    let digit = |shift: u16| HEX[usize::from((unit >> shift) & 0xF)];
    [b'\\', b'u', digit(12), digit(8), digit(4), digit(0)]
}
