// SPDX-License-Identifier: MIT
/*
 * homa siphash: SipHash-1-3, the keyed hash of the tables.
 *
 * SipHash is Aumasson and Bernstein's keyed hash, and SipHash-c-d is its
 * variant of c compression rounds and d finalization rounds. The default
 * hasher of the Rust standard library computes SipHash-1-3. The one hasher of
 * `core::hash` is `SipHasher`, which computes SipHash-2-4 and is deprecated
 * since 1.13.0, so this crate carries its own copy.
 */

//! `SipHash-1-3`, the keyed hash of the tables.
//!
//! The key is 16 bytes, read as two little endian words, and the message is
//! read in little endian words of 8 bytes, the last one padded with zeros and
//! the length of the message in its top byte, as the reference implementation
//! reads them. So a hash here equals the one the standard library's
//! `SipHasher13` gives the same key and the same bytes. The notebook
//! `01_json_writer` holds this hasher against a `SipHash-1-3` written in
//! Python, which it holds against the `siphash13` of `CPython`, over every key
//! of its replay under the key of sixteen zero bytes and under a key it draws,
//! and over sequences of several writes into one hasher, in which a write tops
//! up the tail of the write before it, compresses whole words and leaves a new
//! tail.
//!
//! The module is public, so a crate that keys a table of its own hashes with
//! this one [`Sip13`] and carries no second `SipHash`. Every function of
//! [`Sip13`] carries `#[inline]`, because a table hashes a short key on every
//! lookup. [`hash_words`] hashes a whole message in one call, the same
//! `SipHash-1-3`, which the notebook holds against the Python one over every
//! length of 0 to 64 bytes and every key of its replay, under the fixed key
//! and under a key it draws.

use core::hash::Hasher;

/// The first word of the initial state, the text `somepseu`.
const INIT0: u64 = 0x736f_6d65_7073_6575;
/// The second word of the initial state, the text `dorandom`.
const INIT1: u64 = 0x646f_7261_6e64_6f6d;
/// The third word of the initial state, the text `lygenera`.
const INIT2: u64 = 0x6c79_6765_6e65_7261;
/// The fourth word of the initial state, the text `tedbytes`.
const INIT3: u64 = 0x7465_6462_7974_6573;

/// A running `SipHash-1-3` over the bytes written into it.
// A keyed `unsafe` AES hash would be faster.
#[derive(Clone, Debug)]
pub struct Sip13 {
    /// The four words of the state.
    v: [u64; 4],
    /// The bytes of the last word that is not whole yet.
    tail: u64,
    /// How many bytes `tail` holds, 0 to 7.
    ntail: usize,
    /// How many bytes were written, of which the low byte enters the hash.
    length: u64,
}

impl Sip13 {
    /// Returns a hasher keyed with `key`.
    #[inline]
    #[must_use]
    pub fn new(key: &[u8; 16]) -> Self {
        let (low, high) = key.split_at(8);
        Self::with_keys(whole(low), whole(high))
    }

    /// Returns a hasher keyed with the two words of a key, `k0` from its first
    /// eight bytes and `k1` from its last eight, each read little endian.
    #[inline]
    #[must_use]
    pub const fn with_keys(k0: u64, k1: u64) -> Self {
        Self {
            v: [k0 ^ INIT0, k1 ^ INIT1, k0 ^ INIT2, k1 ^ INIT3],
            tail: 0,
            ntail: 0,
            length: 0,
        }
    }

    /// Mixes one whole word of the message into the state.
    #[inline]
    fn compress(&mut self, m: u64) {
        self.v[3] ^= m;
        round(&mut self.v);
        self.v[0] ^= m;
    }
}

impl Hasher for Sip13 {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        let mut rest = bytes;
        if self.ntail != 0 {
            let needed = 8 - self.ntail;
            let (head, after) = rest.split_at(needed.min(rest.len()));
            self.tail |= part(head) << (8 * self.ntail);
            if head.len() < needed {
                self.ntail += head.len();
                return;
            }
            let m = self.tail;
            self.compress(m);
            rest = after;
        }
        let mut words = rest.chunks_exact(8);
        for chunk in &mut words {
            self.compress(whole(chunk));
        }
        let remainder = words.remainder();
        self.tail = part(remainder);
        self.ntail = remainder.len();
    }

    #[inline]
    fn finish(&self) -> u64 {
        let mut state = self.clone();
        let last = state.tail | (state.length << 56);
        state.compress(last);
        state.v[2] ^= 0xff;
        round(&mut state.v);
        round(&mut state.v);
        round(&mut state.v);
        state.v[0] ^ state.v[1] ^ state.v[2] ^ state.v[3]
    }
}

/// Returns the `SipHash-1-3` of `bytes` under the key words `k0` and `k1` in one call.
///
/// It equals what [`Sip13::with_keys`] of `k0` and `k1` answers after one
/// write of `bytes`. The four words of the state stay in locals and every
/// round is inlined into the loop over the words. A cache that hashes a whole
/// text once per lookup calls it, and a table that hashes a key in several
/// writes keeps [`Sip13`].
///
/// The words are split off the front of the message with `split_first_chunk`,
/// stable since Rust 1.77, since `as_chunks` is stable from Rust 1.88 only and
/// this crate keeps Rust 1.85.
// A keyed `unsafe` AES hash would be faster.
#[must_use]
pub fn hash_words(k0: u64, k1: u64, bytes: &[u8]) -> u64 {
    let mut v0 = k0 ^ INIT0;
    let mut v1 = k1 ^ INIT1;
    let mut v2 = k0 ^ INIT2;
    let mut v3 = k1 ^ INIT3;
    let mut rest = bytes;
    while let Some((word, after)) = rest.split_first_chunk::<8>() {
        let m = u64::from_le_bytes(*word);
        v3 ^= m;
        round_words(&mut v0, &mut v1, &mut v2, &mut v3);
        v0 ^= m;
        rest = after;
    }
    let last = part(rest) | ((bytes.len() as u64) << 56);
    v3 ^= last;
    round_words(&mut v0, &mut v1, &mut v2, &mut v3);
    v0 ^= last;
    v2 ^= 0xff;
    round_words(&mut v0, &mut v1, &mut v2, &mut v3);
    round_words(&mut v0, &mut v1, &mut v2, &mut v3);
    round_words(&mut v0, &mut v1, &mut v2, &mut v3);
    v0 ^ v1 ^ v2 ^ v3
}

/// Returns the keyed `SipHash-1-3` of `bytes`.
pub(crate) fn hash(key: &[u8; 16], bytes: &[u8]) -> u64 {
    let mut hasher = Sip13::new(key);
    hasher.write(bytes);
    hasher.finish()
}

/// Returns the little endian word of the first 8 bytes of `bytes`, which
/// holds 8 bytes wherever it is called, or 0 where it holds fewer.
#[inline]
fn whole(bytes: &[u8]) -> u64 {
    bytes
        .first_chunk::<8>()
        .map_or(0, |word| u64::from_le_bytes(*word))
}

/// Returns the little endian word of up to 7 bytes, the missing high bytes
/// zero.
///
/// It reads four, two and one byte in turn, as the standard library reads the
/// tail of a message for its `SipHasher13`, where a loop over the bytes would
/// read them one at a time.
#[inline]
fn part(bytes: &[u8]) -> u64 {
    let mut out = 0;
    let mut at = 0;
    if let Some(four) = bytes.first_chunk::<4>() {
        out = u64::from(u32::from_le_bytes(*four));
        at = 4;
    }
    if let Some(two) = bytes.get(at..).and_then(<[u8]>::first_chunk::<2>) {
        out |= u64::from(u16::from_le_bytes(*two)) << (8 * at);
        at += 2;
    }
    if let Some(&one) = bytes.get(at) {
        out |= u64::from(one) << (8 * at);
    }
    out
}

/// Runs one `SipRound` over the state.
#[inline]
fn round(v: &mut [u64; 4]) {
    v[0] = v[0].wrapping_add(v[1]);
    v[1] = v[1].rotate_left(13);
    v[1] ^= v[0];
    v[0] = v[0].rotate_left(32);
    v[2] = v[2].wrapping_add(v[3]);
    v[3] = v[3].rotate_left(16);
    v[3] ^= v[2];
    v[0] = v[0].wrapping_add(v[3]);
    v[3] = v[3].rotate_left(21);
    v[3] ^= v[0];
    v[2] = v[2].wrapping_add(v[1]);
    v[1] = v[1].rotate_left(17);
    v[1] ^= v[2];
    v[2] = v[2].rotate_left(32);
}

/// Runs one `SipRound` over the four words of a state held in locals.
// Always inlined, since at opt-level z a round marked `#[inline]` stays a
// call, and `hash_words` then takes as long as the streaming hasher.
#[expect(clippy::inline_always)]
#[inline(always)]
fn round_words(v0: &mut u64, v1: &mut u64, v2: &mut u64, v3: &mut u64) {
    *v0 = v0.wrapping_add(*v1);
    *v1 = v1.rotate_left(13);
    *v1 ^= *v0;
    *v0 = v0.rotate_left(32);
    *v2 = v2.wrapping_add(*v3);
    *v3 = v3.rotate_left(16);
    *v3 ^= *v2;
    *v0 = v0.wrapping_add(*v3);
    *v3 = v3.rotate_left(21);
    *v3 ^= *v0;
    *v2 = v2.wrapping_add(*v1);
    *v1 = v1.rotate_left(17);
    *v1 ^= *v2;
    *v2 = v2.rotate_left(32);
}
