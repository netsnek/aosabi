// SPDX-License-Identifier: MIT
/*
 * aoviewer-core sha256: the SHA-256 of FIPS 180-4.
 *
 * No analogue in the reference. The function is a copy of the one in
 * crates/aosabi-devtool/src/provenance/sha256.rs, the project's own SHA-256,
 * which is written from FIPS 180-4 sections 4.1.2, 4.2.2, 5.1.1, 5.3.3 and
 * 6.2. The core is a leaf with no dependency and cannot reach the devtool, so
 * it keeps a copy with this comment naming the source. The round constants,
 * the initial hash value, the compression and the message schedule are copied
 * unchanged, and the padding is the one that file's finalize applies to a
 * message taken in one piece.
 */

//! SHA-256.
//!
//! [`sha256`] answers the digest of a byte string and [`sha256_hex`] the same
//! digest as 64 lowercase hexadecimal digits, the form `manifest.json` writes
//! for the schema, the theme and every file of a tree. The manifest hashes for
//! change detection and not for authentication, every byte it hashes is a byte
//! of the tree, and no secret enters the hash, so a hash that is constant in
//! time buys nothing here and the function is the plain algorithm of the
//! standard in safe code. Step 9 of `tests/notebooks/51_viewer_core.ipynb`
//! holds it to `hashlib` of Python.
//!
//! Every loop runs over the blocks of 64 bytes of the message, over the at
//! most two blocks of its padding, over the 64 rounds and the 64 words of the
//! schedule of one block, over the 8 words of the hash value, or over the 32
//! bytes of a digest.

use alloc::string::String;

/// The round constants, the first 32 bits of the fractional parts of the
/// cube roots of the first sixty four primes (FIPS 180-4 section 4.2.2).
const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// The initial hash value, the first 32 bits of the fractional parts of the
/// square roots of the first eight primes (FIPS 180-4 section 5.3.3).
const H0: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// The bytes of one message block.
const BLOCK: usize = 64;

/// The bytes the length of the message takes at the end of the padding.
const LENGTH: usize = 8;

/// The digits of the hexadecimal form, in lower case.
const DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Returns the SHA-256 of `bytes` (FIPS 180-4 section 6.2).
///
/// The message is padded with the bit 1, the zero bits that leave 64 bits of
/// its last block, and its length in bits as a big endian 64 bit integer
/// (FIPS 180-4 section 5.1.1). The standard hashes messages of fewer than
/// 2^64 bits. The bit length of a longer message wraps, as in the source of
/// the copy, and no file of a tree comes near that length.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut state = H0;
    let blocks = bytes.chunks_exact(BLOCK);
    let rest = blocks.remainder();
    for block in blocks {
        compress(&mut state, block);
    }
    let bits = (bytes.len() as u64).wrapping_mul(8);
    let mut tail = [0u8; 2 * BLOCK];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let end = if rest.len() < BLOCK - LENGTH {
        BLOCK
    } else {
        2 * BLOCK
    };
    tail[end - LENGTH..end].copy_from_slice(&bits.to_be_bytes());
    for block in tail[..end].chunks_exact(BLOCK) {
        compress(&mut state, block);
    }
    let mut out = [0u8; 32];
    for (word_bytes, word) in out.chunks_exact_mut(4).zip(state) {
        word_bytes.copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Returns the SHA-256 of `bytes` as 64 lowercase hexadecimal digits, the
/// form `manifest.json` records.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(64);
    for byte in sha256(bytes) {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

/// Runs the compression of one block of 64 bytes over `state`.
// CPU SHA instructions would compress faster.
#[expect(
    clippy::many_single_char_names,
    reason = "the working variables keep their names of FIPS 180-4 section 6.2.2"
)]
fn compress(state: &mut [u32; 8], block: &[u8]) {
    let schedule = schedule(block);
    let mut v = *state;
    for (k, w) in K.iter().zip(schedule) {
        let [a, b, c, d, e, f, g, h] = v;
        let t1 = h
            .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
            .wrapping_add((e & f) ^ (!e & g))
            .wrapping_add(*k)
            .wrapping_add(w);
        let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
            .wrapping_add((a & b) ^ (a & c) ^ (b & c));
        v = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
    }
    for (word, add) in state.iter_mut().zip(v) {
        *word = word.wrapping_add(add);
    }
}

/// Returns the message schedule of one block, its sixteen big endian words
/// and the forty eight words the standard derives from them.
fn schedule(block: &[u8]) -> [u32; 64] {
    let mut w = [0u32; 64];
    for (word, bytes) in w.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    for t in 16..64 {
        let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
        let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
        w[t] = w[t - 16]
            .wrapping_add(s0)
            .wrapping_add(w[t - 7])
            .wrapping_add(s1);
    }
    w
}
