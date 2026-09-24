// SPDX-License-Identifier: MIT
/*
 * aosabi-json value: the JavaScript shaped value model and its key order.
 */

//! The value model of the writer.
//!
//! A [`JsValue`] is what `JSON.stringify` can be handed: undefined, null, a
//! boolean, a number that is always an `f64`, a string, an array or an object. A
//! [`JsString`] carries the UTF-16 shape of a JavaScript string in WTF-8, so a
//! lone surrogate survives the trip to the writer. A [`JsObject`] carries
//! insertion order in the [`IndexMap`] of this crate, which is the only accepted
//! carrier, and hands the writer its keys in `OrdinaryOwnPropertyKeys` order
//! through [`JsObject::own_keys`]. An [`Opaque`] is a host object a value
//! carries through and never reads, which `JSON.stringify` writes as the empty
//! object it sees.

use alloc::{
    string::String,
    vec::Vec, //
};
#[cfg(feature = "std")]
use core::cell::Cell;
use core::{
    borrow::Borrow,
    fmt,
    hash::{
        BuildHasher,
        Hash,
        Hasher, //
    }, //
};

use homa::siphash::{
    Sip13,
    hash_words, //
};

use crate::{
    map::{
        IndexMap,
        IntoIter,
        Iter, //
    },
    word, //
};

/// The largest array index, which is 2^32 - 2.
///
/// An array index is an integer index in the range from 0 up to but not
/// including 2^32 - 1.
///
/// [object-type]: https://tc39.es/ecma262/#sec-object-type
pub const MAX_ARRAY_INDEX: u32 = u32::MAX - 1;

/// The key of every [`KeyState`] until the platform calls `seed`, and of
/// every [`KeyState`] of a build without the standard library, sixteen zero
/// bytes.
///
/// It is the key `SipHash` takes where a caller gives none: the deprecated
/// `core::hash::SipHasher::new` sets its two keys to 0, and the standard
/// library's `DefaultHasher::new` is `SipHasher13` under the keys 0 and 0. Under
/// it [`KeyHasher`] hashes as the `siphash13` of `CPython` under
/// `PYTHONHASHSEED=0`.
const FIXED_KEY: [u8; 16] = [0; 16];

/// The two words of [`FIXED_KEY`].
const FIXED_WORDS: [u64; 2] = key_words(&FIXED_KEY);

/// Returns the two words of `key`, each read little endian from eight of its
/// sixteen bytes, the form [`Sip13::with_keys`] takes a key in.
const fn key_words(key: &[u8; 16]) -> [u64; 2] {
    [
        u64::from_le_bytes([
            key[0], key[1], key[2], key[3], key[4], key[5], key[6], key[7],
        ]),
        u64::from_le_bytes([
            key[8], key[9], key[10], key[11], key[12], key[13], key[14], key[15],
        ]),
    ]
}

#[cfg(feature = "std")]
std::thread_local! {
    /// The key [`seed`] stored for the maps this thread makes, none before
    /// it runs.
    ///
    /// A key that one call stores and every later map reads needs a place that
    /// call writes. The standard library offers one per thread, where its
    /// `RandomState` keeps its keys as well, and this crate reads and writes it
    /// through a `Cell` with no atomic type.
    static SEEDED_KEY: Cell<Option<[u64; 2]>> = const { Cell::new(None) };
}

/// The hasher of [`KeyState`], `SipHash-1-3` of the crate `homa`.
///
/// `SipHash-1-3` is the function of the `RandomState` of the standard library,
/// whose hasher is `SipHasher13` under the two keys of the builder, and that is
/// the hasher `indexmap` carries by default where it has the standard library.
/// The implementation is the one `SipHash` the workspace holds,
/// [`homa::siphash::Sip13`], which the tables of `homa` key as well, and it
/// hashes as the `siphash13` of `CPython` and as a `SipHash-1-3` written in
/// Python under the same key.
#[derive(Debug, Clone)]
pub struct KeyHasher(Sip13);

impl Hasher for KeyHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0.finish()
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes);
    }
}

/// The builder of [`KeyHasher`], keyed with the key of `seed` or with the
/// fixed key.
///
/// With the standard library a builder takes its key when it is made, the key
/// `seed` stored for its thread or the fixed key, so a map keeps the key it was
/// made with, and a map made before `seed` runs keeps the fixed key. Such a
/// builder carries a copy of its key, 16 bytes in every map, as the
/// `RandomState` of the standard library carries its two keys. Without the
/// standard library the crate has no `seed`, so the builder has no field and
/// hashes with the fixed key, which the doc of `seed` weighs.
///
/// The fixed key is public, so a client can choose keys that agree in the low
/// bits of their hash that the index table of [`crate::map`] masks, by trying
/// about `2^12` keys for each of them at a start of twelve bits. Such keys share
/// the start of their probe and fill one cluster of groups, so a probe among
/// them reads one more group for every eight of them, or every four where a
/// group holds four slots, whatever their tags, and an object of `n` of them
/// costs time quadratic in `n`. The table tells keys apart by their tag, the top
/// seven bits of the hash, so keys chosen to agree in the tag as well add on top
/// of that a comparison of the key for each of them on every probe that passes
/// them. A cap on the count of keys of one object in the JSON reader would bound
/// both costs. A key the client does not know leaves it no way to choose such
/// keys.
#[derive(Clone, Copy)]
pub struct KeyState {
    /// The two words of the key, read little endian from its sixteen bytes.
    #[cfg(feature = "std")]
    key: [u64; 2],
}

impl Default for KeyState {
    #[inline]
    fn default() -> Self {
        Self {
            #[cfg(feature = "std")]
            key: SEEDED_KEY.with(Cell::get).unwrap_or(FIXED_WORDS),
        }
    }
}

impl BuildHasher for KeyState {
    type Hasher = KeyHasher;

    #[inline]
    fn build_hasher(&self) -> KeyHasher {
        #[cfg(feature = "std")]
        let [k0, k1] = self.key;
        #[cfg(not(feature = "std"))]
        let [k0, k1] = FIXED_WORDS;
        KeyHasher(Sip13::with_keys(k0, k1))
    }
}

impl KeyState {
    /// Returns the `SipHash-1-3` of `bytes` under the key of this state.
    ///
    /// The key is the one the state took when it was made, the key of `seed` or
    /// the fixed key, and the fixed key in a build without the standard library.
    /// The hash equals the one the hasher of [`BuildHasher::build_hasher`]
    /// finishes with after one write of `bytes`, and it is computed in one call
    /// of [`homa::siphash::hash_words`], which keeps at opt-level z the speed of
    /// opt-level 3. The streaming hasher takes 2.3 to 2.7 times as long at
    /// opt-level z as at opt-level 3. It is not [`BuildHasher::hash_one`] of
    /// `bytes`, since the `Hash` of a slice writes its length before its bytes.
    /// A cache that hashes a whole text once per lookup calls it.
    #[inline]
    #[must_use]
    pub fn hash_bytes(&self, bytes: &[u8]) -> u64 {
        #[cfg(feature = "std")]
        let [k0, k1] = self.key;
        #[cfg(not(feature = "std"))]
        let [k0, k1] = FIXED_WORDS;
        hash_words(k0, k1, bytes)
    }
}

impl fmt::Debug for KeyState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyState").finish_non_exhaustive()
    }
}

/// Keys every [`KeyState`] this thread makes after it with `key`.
///
/// The platform calls it once at start, before it makes the first map, with
/// sixteen bytes of its randomness. The boot of a Linux card calls it with
/// sixteen bytes of the randomness it installs, before its first pass makes the
/// cache of parsed documents, whose hash of a text is a [`KeyState`] on every
/// target. Until a platform calls it, every map of a build whose builder is
/// [`KeyState`] hashes with the fixed key. Where [`KeyHasherBuilder`] is the
/// `RandomState` of the standard library, the key reaches only the maps that
/// name [`KeyState`], since the standard library keys the others.
///
/// The key is kept for the thread that calls it, as the standard library keeps
/// the keys of `RandomState`, so a map another thread makes keeps the fixed key.
/// No served crate starts a thread, so the thread that calls it at start makes
/// every map.
///
/// Only a build with the feature `std` has it. A function that stores a key
/// every later map reads needs a static it writes, and without the standard
/// library safe Rust offers that as an atomic type alone, since every other type
/// of `core` with interior mutability is not `Sync`. This crate ships on the
/// ESP32-C3, and a crate that ships there uses no atomic type, not even a load
/// or a store.
///
/// # Errors
///
/// Returns `key` when a key was set on this thread before, which stays the
/// key.
#[cfg(feature = "std")]
pub fn seed(key: [u8; 16]) -> Result<(), [u8; 16]> {
    SEEDED_KEY.with(|seeded| {
        if seeded.get().is_some() {
            return Err(key);
        }
        seeded.set(Some(key_words(&key)));
        Ok(())
    })
}

/// The hasher builder every map of [`crate::map`] carries unless it names another.
///
/// This build has the standard library and a target whose standard library draws
/// its keys from the operating system, so the builder is its `RandomState`,
/// which `indexmap` carries by default as well. That is `SipHash-1-3` with a key
/// of 128 bits the standard library draws once per thread and moves on by one
/// for every builder, so two maps hash a key differently, and on Linux the key
/// comes from `getrandom`. The three targets whose standard library takes that
/// key from the addresses of two allocations, `wasm32-unknown-unknown` and the
/// operating systems `xous` and `vexos`, take [`KeyState`]. A client that does
/// not know the key cannot choose keys that collide in it.
#[cfg(all(
    feature = "std",
    not(any(
        all(target_family = "wasm", target_os = "unknown"),
        target_os = "xous",
        target_os = "vexos",
    ))
))]
pub type KeyHasherBuilder = std::hash::RandomState;

/// The hasher builder every map of [`crate::map`] carries unless it names another.
///
/// This build has no source of entropy this crate reaches through the standard
/// library, so the builder is [`KeyState`], the same `SipHash-1-3`, keyed by
/// `seed` where the feature `std` is on and with the fixed key until then and
/// where it is off. That is the case with no standard library, where the feature
/// `std` is off, and on `wasm32-unknown-unknown` and the operating systems
/// `xous` and `vexos`, whose standard library takes the key of `RandomState`
/// from the addresses of two allocations for want of a source.
#[cfg(not(all(
    feature = "std",
    not(any(
        all(target_family = "wasm", target_os = "unknown"),
        target_os = "xous",
        target_os = "vexos",
    ))
)))]
pub type KeyHasherBuilder = KeyState;

/// The error of [`JsString::from_wtf8`], whose bytes are neither UTF-8 nor a
/// lone surrogate at `offset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wtf8Error {
    /// The byte offset of the first byte no decoding accepts.
    pub offset: usize,
}

impl fmt::Display for Wtf8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not WTF-8 at byte offset {}", self.offset)
    }
}

impl core::error::Error for Wtf8Error {}

/// A JavaScript string, which is a sequence of UTF-16 code units.
///
/// The bytes are WTF-8, which is UTF-8 with one extension: a lone surrogate code
/// unit is encoded as the three byte sequence its code point would have, `0xED`
/// followed by a byte from `0xA0` to `0xBF` and a byte from `0x80` to `0xBF`. A
/// surrogate pair never appears as two such sequences, because a pair is one
/// code point and is stored as its four byte UTF-8. That keeps a well formed
/// string byte identical to its `str`, and it keeps every JavaScript string
/// representable, which a `str` cannot do.
///
/// [wtf8]: https://simonsapin.github.io/wtf-8/
#[derive(Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JsString {
    /// The code units in WTF-8.
    wtf8: Vec<u8>,
}

impl JsString {
    /// Creates the empty string.
    #[must_use]
    pub const fn new() -> Self {
        Self { wtf8: Vec::new() }
    }

    /// Builds a string from UTF-16 code units, which is the JavaScript shape.
    ///
    /// A high surrogate followed by a low surrogate becomes the code point
    /// the pair encodes, and a surrogate with no partner is kept as a lone
    /// surrogate. Nothing is rejected, because every sequence of code units
    /// is a JavaScript string. The units go through `push_wtf8_run`, which
    /// narrows a run of ASCII units in one pass.
    #[must_use]
    pub fn from_utf16(units: &[u16]) -> Self {
        let mut wtf8 = Vec::with_capacity(units.len());
        push_wtf8_run(&mut wtf8, units);
        Self { wtf8 }
    }

    /// Builds a string from WTF-8 bytes, merging a pair written as two
    /// surrogate sequences.
    ///
    /// `bytes` is UTF-8 in which a surrogate code point may appear in its
    /// three byte form. That is the form Python writes with the
    /// `surrogatepass` error handler, which encodes each surrogate on its own,
    /// so a high surrogate sequence directly followed by a low surrogate
    /// sequence is read as the pair it is in JavaScript and becomes one code
    /// point.
    ///
    /// # Errors
    ///
    /// Returns [`Wtf8Error`] with the offset of the first byte that is neither
    /// part of a UTF-8 sequence nor part of a surrogate sequence.
    pub fn from_wtf8(bytes: &[u8]) -> Result<Self, Wtf8Error> {
        let mut wtf8 = Vec::with_capacity(bytes.len());
        let mut at = 0;
        while let Err(error) = core::str::from_utf8(&bytes[at..]) {
            let stop = at + error.valid_up_to();
            wtf8.extend_from_slice(&bytes[at..stop]);
            let taken = push_surrogate_sequence(&mut wtf8, &bytes[stop..])
                .ok_or(Wtf8Error { offset: stop })?;
            at = stop + taken;
        }
        wtf8.extend_from_slice(&bytes[at..]);
        Ok(Self { wtf8 })
    }

    /// Returns the WTF-8 bytes.
    #[must_use]
    pub fn as_wtf8(&self) -> &[u8] {
        &self.wtf8
    }

    /// Returns the string as a `str` when it carries no lone surrogate.
    // An `unsafe` view would skip this check.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.wtf8).ok()
    }

    /// Returns the string as a `String` when it carries no lone surrogate,
    /// and the string itself when it carries one.
    ///
    /// The WTF-8 of a string with no lone surrogate is its UTF-8, so the
    /// `String` takes the bytes as they stand and copies none.
    ///
    /// # Errors
    ///
    /// Returns the string unchanged when it carries a lone surrogate, which
    /// no `String` can hold.
    // An `unsafe` view would skip this check.
    pub fn into_string(self) -> Result<String, Self> {
        String::from_utf8(self.wtf8).map_err(|error| Self {
            wtf8: error.into_bytes(),
        })
    }

    /// Returns the length in WTF-8 bytes, which is not the JavaScript length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.wtf8.len()
    }

    /// Returns whether the string is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.wtf8.is_empty()
    }
}

impl fmt::Debug for JsString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&String::from_utf8_lossy(&self.wtf8), f)
    }
}

impl From<&str> for JsString {
    fn from(text: &str) -> Self {
        Self {
            wtf8: text.as_bytes().to_vec(),
        }
    }
}

impl From<String> for JsString {
    fn from(text: String) -> Self {
        Self {
            wtf8: text.into_bytes(),
        }
    }
}

impl From<char> for JsString {
    fn from(c: char) -> Self {
        let mut wtf8 = Vec::new();
        push_char(&mut wtf8, c);
        Self { wtf8 }
    }
}

impl Borrow<[u8]> for JsString {
    fn borrow(&self) -> &[u8] {
        &self.wtf8
    }
}

impl AsRef<[u8]> for JsString {
    fn as_ref(&self) -> &[u8] {
        &self.wtf8
    }
}

impl PartialEq<str> for JsString {
    fn eq(&self, other: &str) -> bool {
        self.wtf8 == other.as_bytes()
    }
}

impl PartialEq<&str> for JsString {
    fn eq(&self, other: &&str) -> bool {
        self.wtf8 == other.as_bytes()
    }
}

/// Appends the WTF-8 of the code units `units`, a whole JavaScript string.
///
/// A run of ASCII units, which the word test of [`word::ascii_units`] finds
/// [`word::UNITS`] units a step, is narrowed to its bytes by one `extend` of
/// a `TrustedLen` iterator, which reserves once for the run. Every other
/// character is appended from an array of its known length by
/// `extend_from_slice`, so the loop calls no copy per character: two bytes
/// below U+0800, four for a high surrogate followed by a low one, which is the
/// code point of the pair, and three for every other unit, a lone surrogate
/// among them. A pair is joined only inside `units`, so `units` holds a whole
/// string.
fn push_wtf8_run(wtf8: &mut Vec<u8>, units: &[u16]) {
    let mut at = 0;
    while let Some(rest) = units.get(at..).filter(|rest| !rest.is_empty()) {
        let ascii = ascii_run(rest);
        wtf8.extend(rest.iter().take(ascii).map(|&unit| narrow(unit)));
        at += ascii;
        let Some(&unit) = units.get(at) else {
            break;
        };
        at += push_unit(wtf8, unit, units.get(at + 1).copied());
    }
}

/// Returns how many code units at the start of `units` are ASCII.
///
/// The units are read [`word::UNITS`] at a time while whole words remain, and
/// the units of the first word that holds one above ASCII and the units after
/// the last whole word one at a time.
// An `unsafe` vector narrowing would test and narrow 16 units a step. An
// unaligned word read through a raw pointer would read the last units as a word
// too.
fn ascii_run(units: &[u16]) -> usize {
    let (words, tail) = units.as_chunks::<{ word::UNITS }>();
    for (at, quad) in words.iter().enumerate() {
        if !word::ascii_units(word::load_units(quad)) {
            return at * word::UNITS + quad.iter().take_while(|&&unit| unit < 0x80).count();
        }
    }
    words.len() * word::UNITS + tail.iter().take_while(|&&unit| unit < 0x80).count()
}

/// Returns the byte of the ASCII code unit `unit`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "an ASCII unit is below 0x80, so its byte is the unit"
)]
#[inline]
fn narrow(unit: u16) -> u8 {
    unit as u8
}

/// Appends the WTF-8 of the character the code unit `unit` above ASCII starts and returns the units it takes.
///
/// `next` is the unit after `unit`, if any, which completes a pair when `unit`
/// is a high surrogate and `next` a low one. A unit of three bytes and a lone
/// surrogate share the generalized UTF-8 of a code point below U+10000, which is
/// what WTF-8 writes for a lone surrogate. Each arm appends an array of its
/// known length, and the shifts and masks keep the bits each byte carries, so no
/// value is lost.
///
/// [wtf8]: https://simonsapin.github.io/wtf-8/
fn push_unit(wtf8: &mut Vec<u8>, unit: u16, next: Option<u16>) -> usize {
    if unit < 0x800 {
        wtf8.extend_from_slice(&[
            0xC0 | ((unit >> 6) & 0x1F) as u8,
            0x80 | (unit & 0x3F) as u8,
        ]);
        return 1;
    }
    if let Some(low) = next.filter(|low| is_high(unit) && is_low(*low)) {
        let point = 0x10000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
        wtf8.extend_from_slice(&[
            0xF0 | ((point >> 18) & 0x07) as u8,
            0x80 | ((point >> 12) & 0x3F) as u8,
            0x80 | ((point >> 6) & 0x3F) as u8,
            0x80 | (point & 0x3F) as u8,
        ]);
        return 2;
    }
    wtf8.extend_from_slice(&[
        0xE0 | (unit >> 12) as u8,
        0x80 | ((unit >> 6) & 0x3F) as u8,
        0x80 | (unit & 0x3F) as u8,
    ]);
    1
}

/// Appends the UTF-8 of one character.
fn push_char(wtf8: &mut Vec<u8>, c: char) {
    let mut buffer = [0; 4];
    wtf8.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
}

/// Appends the surrogate sequence at the front of `bytes`, merged with the
/// one after it when the two form a pair, and returns how many bytes it took.
///
/// A lone surrogate is copied as its three bytes, and a high surrogate
/// sequence directly followed by a low surrogate sequence becomes the UTF-8
/// of the code point the pair encodes. The result is `None` when `bytes` does
/// not start with a surrogate sequence.
fn push_surrogate_sequence(wtf8: &mut Vec<u8>, bytes: &[u8]) -> Option<usize> {
    let high = surrogate_unit(bytes)?;
    let low = surrogate_unit(&bytes[3..]).filter(|low| is_high(high) && is_low(*low));
    let Some(low) = low else {
        wtf8.extend_from_slice(&bytes[..3]);
        return Some(3);
    };
    for c in char::decode_utf16([high, low]).flatten() {
        push_char(wtf8, c);
    }
    Some(6)
}

/// Decodes the surrogate sequence at the front of `bytes`, if one is there.
///
/// The sequence is `0xED`, a byte from `0xA0` to `0xBF`, and a byte from
/// `0x80` to `0xBF`, which is the three byte form of a code point from U+D800
/// to U+DFFF. The result is that code unit.
// Always inlined, since at opt-level z a hint leaves it a call in the byte loop
// of the escape of WTF-8 and in the loop of `JsString::from_wtf8`.
#[expect(
    clippy::inline_always,
    reason = "a hint inlines nothing into a caller built at opt-level z"
)]
#[inline(always)]
pub(crate) fn surrogate_unit(bytes: &[u8]) -> Option<u16> {
    match bytes {
        [0xED, second @ 0xA0..=0xBF, third @ 0x80..=0xBF, ..] => {
            Some(0xD000 | (u16::from(*second & 0x3F) << 6) | u16::from(*third & 0x3F))
        }
        _ => None,
    }
}

/// Returns whether `unit` is a leading surrogate.
// Always inlined, since at opt-level z a hint leaves it a call in the loops of
// `JsString::from_utf16` and `JsString::from_wtf8`, and a pattern, since the
// call of `RangeInclusive::contains` stays a call there as well.
#[expect(
    clippy::inline_always,
    reason = "a hint inlines nothing into a caller built at opt-level z"
)]
#[inline(always)]
fn is_high(unit: u16) -> bool {
    matches!(unit, 0xD800..=0xDBFF)
}

/// Returns whether `unit` is a trailing surrogate.
// Always inlined, since at opt-level z a hint leaves it a call in the loops of
// `JsString::from_utf16` and `JsString::from_wtf8`, and a pattern, since the
// call of `RangeInclusive::contains` stays a call there as well.
#[expect(
    clippy::inline_always,
    reason = "a hint inlines nothing into a caller built at opt-level z"
)]
#[inline(always)]
fn is_low(unit: u16) -> bool {
    matches!(unit, 0xDC00..=0xDFFF)
}

/// Returns the array index a property key names, if it names one.
///
/// A key is an array index when it is the canonical decimal form of an integer
/// from 0 to [`MAX_ARRAY_INDEX`], which means no sign, no leading zero except
/// for `0` itself, and nothing but digits. Such keys are written first, in
/// ascending numeric order, and node confirms the boundary: `4294967294` moves
/// to the front and `4294967295` stays in insertion order.
///
/// [canonical]: https://tc39.es/ecma262/#sec-canonicalnumericindexstring
/// [object-type]: https://tc39.es/ecma262/#sec-object-type
#[must_use]
pub fn array_index(key: &[u8]) -> Option<u32> {
    match key {
        [] | [b'0', _, ..] => None,
        [b'0'] => Some(0),
        _ if key.len() > 10 => None,
        _ => {
            let mut value: u64 = 0;
            for &byte in key {
                if !byte.is_ascii_digit() {
                    return None;
                }
                value = value * 10 + u64::from(byte - b'0');
            }
            u32::try_from(value)
                .ok()
                .filter(|index| *index <= MAX_ARRAY_INDEX)
        }
    }
}

/// The shared handle of an [`Opaque`], `Send` and `Sync` where the target has
/// atomic pointers.
///
/// A target with no atomic pointer has no `Arc`, so the handle there is an `Rc`,
/// and no build of such a target makes an opaque value.
#[cfg(target_has_atomic = "ptr")]
pub type OpaqueHandle = alloc::sync::Arc<dyn core::any::Any + Send + Sync>;

/// The shared handle of an [`Opaque`] on a target with no atomic pointer.
#[cfg(not(target_has_atomic = "ptr"))]
pub type OpaqueHandle = alloc::rc::Rc<dyn core::any::Any>;

/// A host object a value carries through and never reads.
///
/// A `File` of a multipart request is one: the parser of the form sets it at the
/// key path of its variable, the variable reaches the resolver untouched, and
/// the resolver reads the file through the object behind the handle.
/// `JSON.stringify` of a web `File` has no own enumerable property to write, so
/// the writer writes an opaque value as `{}`. Two opaque values are equal when
/// they are the same object, as two objects of JavaScript are.
#[derive(Clone)]
pub struct Opaque(pub OpaqueHandle);

impl Opaque {
    /// Returns the object behind the handle when it is a `T`.
    #[must_use]
    pub fn downcast_ref<T: core::any::Any>(&self) -> Option<&T> {
        self.0.downcast_ref::<T>()
    }
}

impl PartialEq for Opaque {
    fn eq(&self, other: &Self) -> bool {
        #[cfg(target_has_atomic = "ptr")]
        return alloc::sync::Arc::ptr_eq(&self.0, &other.0);
        #[cfg(not(target_has_atomic = "ptr"))]
        return alloc::rc::Rc::ptr_eq(&self.0, &other.0);
    }
}

impl fmt::Debug for Opaque {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Opaque")
    }
}

/// A JavaScript value as `JSON.stringify` can see it.
///
/// `Undefined` is the explicit absent variant: a property whose value is
/// undefined is left out of its object, an array element that is undefined is
/// written as `null`, and an undefined top level value produces no bytes. A
/// number is always an `f64`, which reproduces the precision loss the reference
/// has for integers wider than 2^53.
///
/// The derived equality compares numbers as `f64`, so a NaN is not equal to
/// itself.
///
/// `Debug` writes the JSON of [`crate::write()`], so a number takes the
/// digits of [`crate::number::write_f64`] and no float formatting of `core`
/// is linked for it, and it writes the absent value as `undefined`.
///
/// [stringify]: https://tc39.es/ecma262/#sec-json.stringify
#[derive(Clone, PartialEq, Default)]
pub enum JsValue {
    /// The absent value, which `JSON.stringify` leaves out or turns into null
    /// depending on where it sits.
    #[default]
    Undefined,
    /// The null value.
    Null,
    /// A boolean.
    Bool(bool),
    /// A number, which is always a double.
    Number(f64),
    /// A string.
    String(JsString),
    /// An array, written in element order.
    Array(Vec<JsValue>),
    /// An object, written in `OrdinaryOwnPropertyKeys` order.
    Object(JsObject),
    /// A host object carried through unread, written as `{}`.
    Opaque(Opaque),
}

impl JsValue {
    /// Returns whether the value is the absent variant.
    #[must_use]
    pub fn is_undefined(&self) -> bool {
        matches!(self, Self::Undefined)
    }

    /// Returns whether the value is truthy, `ToBoolean` of ECMA-262.
    ///
    /// The absent value, `null`, `false`, a zero, `NaN` and the empty string are
    /// falsy, and every other value is truthy, an empty array, an empty object
    /// and a host object among them.
    ///
    /// [to-boolean]: https://tc39.es/ecma262/#sec-toboolean
    #[must_use]
    pub fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::String(text) => !text.is_empty(),
            Self::Array(_) | Self::Object(_) | Self::Opaque(_) => true,
        }
    }
}

impl fmt::Debug for JsValue {
    /// Writes the JSON [`crate::write()`] makes of the value, and `undefined`
    /// for the absent value, which has no JSON of its own.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut json = Vec::new();
        if crate::writer::write(&mut json, self).is_none() {
            return f.write_str("undefined");
        }
        write_json(f, &json)
    }
}

impl From<bool> for JsValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f64> for JsValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<i32> for JsValue {
    fn from(value: i32) -> Self {
        Self::Number(f64::from(value))
    }
}

impl From<u32> for JsValue {
    fn from(value: u32) -> Self {
        Self::Number(f64::from(value))
    }
}

impl From<i64> for JsValue {
    // The widening is the contract. Every integer becomes a double before it is
    // written, which reproduces the reference's precision loss above 2^53.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the widening to a double is the byte contract"
    )]
    fn from(value: i64) -> Self {
        Self::Number(value as f64)
    }
}

impl From<u64> for JsValue {
    // The widening is the contract, as for `i64`.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the widening to a double is the byte contract"
    )]
    fn from(value: u64) -> Self {
        Self::Number(value as f64)
    }
}

impl From<&str> for JsValue {
    fn from(text: &str) -> Self {
        Self::String(JsString::from(text))
    }
}

impl From<String> for JsValue {
    fn from(text: String) -> Self {
        Self::String(JsString::from(text))
    }
}

impl From<JsString> for JsValue {
    fn from(text: JsString) -> Self {
        Self::String(text)
    }
}

impl From<Vec<JsValue>> for JsValue {
    fn from(items: Vec<JsValue>) -> Self {
        Self::Array(items)
    }
}

impl From<JsObject> for JsValue {
    fn from(object: JsObject) -> Self {
        Self::Object(object)
    }
}

/// Implements `From` for a number type every value of which is a double.
macro_rules! exact_number_from {
    ($($type:ty),+ $(,)?) => {
        $(
            impl From<$type> for JsValue {
                fn from(value: $type) -> Self {
                    Self::Number(f64::from(value))
                }
            }
        )+
    };
}

/// Implements `From` for a number type wider than the 53 bits of a double.
macro_rules! wide_number_from {
    ($($type:ty),+ $(,)?) => {
        $(
            impl From<$type> for JsValue {
                // The nearest double is the value a JavaScript number holds
                // for an integer of this size, as for `i64`.
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "the widening to a double is the byte contract"
                )]
                fn from(value: $type) -> Self {
                    Self::Number(value as f64)
                }
            }
        )+
    };
}

exact_number_from!(f32, i8, i16, u8, u16);
wide_number_from!(isize, usize, i128, u128);

impl From<char> for JsValue {
    fn from(c: char) -> Self {
        Self::String(JsString::from(c))
    }
}

/// `None` is `null` and `Some(value)` the value, as `IntoJson` and the
/// `Option` of `serde_json` convert it.
impl<T: Into<JsValue>> From<Option<T>> for JsValue {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
    }
}

/// An array of values, `[a, b]` of JavaScript, in element order.
impl<T: Into<JsValue>, const N: usize> From<[T; N]> for JsValue {
    fn from(items: [T; N]) -> Self {
        Self::Array(items.into_iter().map(Into::into).collect())
    }
}

/// An array of the values an iterator yields, in their order.
impl<T: Into<JsValue>> FromIterator<T> for JsValue {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::Array(iter.into_iter().map(Into::into).collect())
    }
}

/// A JavaScript object, which carries its properties in insertion order.
///
/// The carrier is the [`IndexMap`] of this crate, the only accepted one.
/// Inserting a key that exists replaces its value and keeps its position, which
/// is what assigning an existing property does in JavaScript, and what
/// `JSON.parse` does with a repeated key. The writer reads the keys through
/// [`JsObject::own_keys`], which applies `OrdinaryOwnPropertyKeys` to every
/// object, including a `JSONObject` or `JSON` scalar value nested inside `data`,
/// because the reordering is V8's and applies to every object it writes.
///
/// `Debug` writes the JSON of [`crate::write()`], as [`JsValue`] does.
#[derive(Clone, Default, PartialEq)]
pub struct JsObject {
    /// The properties in insertion order.
    entries: IndexMap<JsString, JsValue, KeyHasherBuilder>,
}

impl JsObject {
    /// Creates an empty object.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: IndexMap::default(),
        }
    }

    /// Creates an empty object with room for `capacity` properties.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: IndexMap::with_capacity_and_hasher(capacity, KeyHasherBuilder::default()),
        }
    }

    /// Returns the number of properties.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the object has no property.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Sets a property, keeping the position of a key that already exists.
    ///
    /// `key` and `value` may be anything that converts, and the previous
    /// value of an existing key is returned.
    pub fn insert(
        &mut self,
        key: impl Into<JsString>,
        value: impl Into<JsValue>,
    ) -> Option<JsValue> {
        self.entries.insert(key.into(), value.into())
    }

    /// Returns the value of the property named `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&JsValue> {
        self.entries.get(key.as_bytes())
    }

    /// Returns the value of the property named `key` for writing.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut JsValue> {
        self.entries.get_mut(key.as_bytes())
    }

    /// Returns whether a property named `key` exists.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key.as_bytes())
    }

    /// Removes the property named `key` and keeps the order of the rest.
    ///
    /// This is the `delete` operator, after which the remaining properties
    /// keep their relative creation order, so the removal shifts rather than
    /// swaps.
    pub fn remove(&mut self, key: &str) -> Option<JsValue> {
        self.entries.shift_remove(key.as_bytes())
    }

    /// Iterates the properties in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&JsString, &JsValue)> {
        self.entries.iter()
    }

    /// Iterates the values of the properties in insertion order, each one
    /// mutable.
    ///
    /// A value written through it keeps the key and the position of its
    /// property.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut JsValue> {
        self.entries.values_mut()
    }

    /// Returns an object of the same keys in the same order, whose values
    /// `make` answers from the values of this object in their order.
    ///
    /// The new object keeps the hash of each key, so no key is hashed again.
    ///
    /// # Errors
    ///
    /// Returns the first error `make` answers, which stops the walk.
    pub fn try_map_values<E>(
        &self,
        make: impl FnMut(&JsValue) -> Result<JsValue, E>,
    ) -> Result<Self, E> {
        Ok(Self {
            entries: self.entries.try_map_values(make)?,
        })
    }

    /// Iterates the properties in `OrdinaryOwnPropertyKeys` order.
    ///
    /// Keys that are array indices come first in ascending numeric order, then
    /// the remaining keys in insertion order. An object with no array index key
    /// is walked in place, so the common `data` map costs no allocation.
    ///
    /// [own-keys]: https://tc39.es/ecma262/#sec-ordinaryownpropertykeys
    #[must_use]
    pub fn own_keys(&self) -> OwnKeys<'_> {
        let mut indexed: Vec<(u32, usize)> = Vec::new();
        for (position, key) in self.entries.keys().enumerate() {
            if let Some(index) = array_index(key.as_wtf8()) {
                indexed.push((index, position));
            }
        }
        let order = if indexed.is_empty() {
            None
        } else {
            indexed.sort_unstable();
            let mut order: Vec<usize> = Vec::with_capacity(self.entries.len());
            order.extend(indexed.iter().map(|(_, position)| *position));
            order.extend(
                self.entries
                    .keys()
                    .enumerate()
                    .filter(|(_, key)| array_index(key.as_wtf8()).is_none())
                    .map(|(position, _)| position),
            );
            Some(order)
        };
        OwnKeys {
            object: self,
            order,
            next: 0,
        }
    }
}

impl fmt::Debug for JsObject {
    /// Writes the JSON [`crate::write()`] makes of the object, its keys in
    /// `OrdinaryOwnPropertyKeys` order and a property that is undefined left
    /// out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut json = Vec::new();
        crate::writer::write_object(&mut json, self);
        write_json(f, &json)
    }
}

/// Writes the bytes `json` of the writer into `f`.
///
/// The writer writes a lone surrogate as its escape and every other code
/// point as its UTF-8, so the check of UTF-8 holds for every value it writes.
// A `from_utf8_unchecked` would skip this check.
fn write_json(f: &mut fmt::Formatter<'_>, json: &[u8]) -> fmt::Result {
    f.write_str(core::str::from_utf8(json).map_err(|_| fmt::Error)?)
}

impl<'a> IntoIterator for &'a JsObject {
    type Item = (&'a JsString, &'a JsValue);
    type IntoIter = Iter<'a, JsString, JsValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl IntoIterator for JsObject {
    type Item = (JsString, JsValue);
    type IntoIter = IntoIter<JsString, JsValue>;

    /// Consumes the object and iterates its properties in insertion order,
    /// so a key and a value move out of it with no copy.
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<K: Into<JsString>, V: Into<JsValue>> FromIterator<(K, V)> for JsObject {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut object = Self::new();
        object.extend(iter);
        object
    }
}

/// Sets each property of the iterator in its order, as [`JsObject::insert`]
/// sets one, so a key that exists keeps its position and takes the value.
impl<K: Into<JsString>, V: Into<JsValue>> Extend<(K, V)> for JsObject {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

/// The object of a literal of pairs, `Map::from([("id", id), ..])`, the map
/// literal of the standard library, `HashMap::from([(k, v), ..])`.
///
/// The pairs are set in their order, so a key written twice keeps its first
/// place and takes its last value, as a key written twice in an object literal
/// of JavaScript does.
impl<K: Into<JsString>, V: Into<JsValue>, const N: usize> From<[(K, V); N]> for JsObject {
    fn from(pairs: [(K, V); N]) -> Self {
        let mut object = Self::with_capacity(N);
        object.extend(pairs);
        object
    }
}

/// The properties of a [`JsObject`] in the order the writer emits them.
///
/// Returned by [`JsObject::own_keys`].
#[derive(Debug)]
pub struct OwnKeys<'a> {
    /// The object being walked.
    object: &'a JsObject,
    /// The positions to visit, or `None` to walk the entries in place.
    order: Option<Vec<usize>>,
    /// How many properties have been handed out.
    next: usize,
}

impl<'a> Iterator for OwnKeys<'a> {
    type Item = (&'a JsString, &'a JsValue);

    fn next(&mut self) -> Option<Self::Item> {
        let position = match &self.order {
            Some(order) => *order.get(self.next)?,
            None => self.next,
        };
        let entry = self.object.entries.get_index(position)?;
        self.next += 1;
        Some(entry)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.object.entries.len().saturating_sub(self.next);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for OwnKeys<'_> {}
