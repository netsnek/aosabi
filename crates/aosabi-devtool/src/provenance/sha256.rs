// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool provenance sha256: the SHA-256 the provenance check
 * recomputes over every extracted literal.
 *
 * No analogue in the reference. The hash function is SHA-256 of FIPS 180-4,
 * sections 4.1.2, 4.2.2, 5.1.1, 5.3.3 and 6.2, written from the standard.
 * tools/provenance/extract.py records each literal with the SHA-256 of
 * Python's hashlib, so the lock is the value this function has to meet.
 */

//! The SHA-256 of the provenance check.
//!
//! The check compares the bytes on disk with the bytes the lock recorded when
//! they were extracted. Both are public files of this repository and no
//! secret enters the hash, so a hash that is fast or constant in time buys
//! nothing here, and the function is the plain algorithm of the standard in
//! safe code. `tests/notebooks/00_environment.ipynb`, section The hash of the
//! provenance check, holds it to the examples NIST publishes and to hashlib
//! over every length around the block boundaries.

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

/// Returns the SHA-256 of `message` as 64 lowercase hexadecimal digits, the
/// form the lock records.
pub fn hex(message: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(64);
    for byte in digest(message) {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

/// Returns the SHA-256 of `message` (FIPS 180-4 section 6.2).
///
/// The message is padded with the bit 1, the zero bits that leave 64 bits of
/// its block, and its length in bits as a big endian 64 bit integer (FIPS
/// 180-4 section 5.1.1).
pub fn digest(message: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(message);
    hasher.finalize()
}

/// The SHA-256 of a message that arrives in pieces (FIPS 180-4 section 6.2).
///
/// The simulator of `homa-sim` hashes every packet an end puts on its link as
/// the packet leaves, and the fill of a message block by block, so it feeds
/// the hash piece by piece. [`digest`] is the same hash over a message in one
/// piece, which `tests/notebooks/00_environment.ipynb` holds to `hashlib`.
#[derive(Debug, Clone)]
pub struct Sha256 {
    /// The hash value over the whole blocks taken so far.
    state: [u32; 8],
    /// The bytes of the block that is not whole yet.
    block: [u8; BLOCK],
    /// How many bytes at the front of `block` belong to the message.
    filled: usize,
    /// The length of the message so far, in bytes.
    length: u64,
}

impl Sha256 {
    /// Returns the hash of the empty message, ready for its first piece.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: H0,
            block: [0; BLOCK],
            filled: 0,
            length: 0,
        }
    }

    /// Takes the next piece of the message.
    pub fn update(&mut self, piece: impl AsRef<[u8]>) {
        let mut piece = piece.as_ref();
        // The standard hashes messages of fewer than 2^64 bits, and no
        // message of the seam comes near that.
        self.length = self.length.wrapping_add(piece.len() as u64);
        if self.filled > 0 {
            let take = (BLOCK - self.filled).min(piece.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&piece[..take]);
            self.filled += take;
            piece = &piece[take..];
            if self.filled < BLOCK {
                return;
            }
            compress(&mut self.state, &self.block);
            self.filled = 0;
        }
        let blocks = piece.chunks_exact(BLOCK);
        let rest = blocks.remainder();
        for block in blocks {
            compress(&mut self.state, block);
        }
        self.block[..rest.len()].copy_from_slice(rest);
        self.filled = rest.len();
    }

    /// Pads the message and returns its SHA-256.
    ///
    /// The message is padded with the bit 1, the zero bits that leave 64
    /// bits of its block, and its length in bits as a big endian 64 bit
    /// integer (FIPS 180-4 section 5.1.1).
    #[must_use]
    pub fn finalize(self) -> [u8; 32] {
        let mut state = self.state;
        let bits = self.length.wrapping_mul(8);
        let mut tail = [0u8; 2 * BLOCK];
        tail[..self.filled].copy_from_slice(&self.block[..self.filled]);
        tail[self.filled] = 0x80;
        let end = if self.filled < BLOCK - LENGTH {
            BLOCK
        } else {
            2 * BLOCK
        };
        tail[end - LENGTH..end].copy_from_slice(&bits.to_be_bytes());
        for block in tail[..end].chunks_exact(BLOCK) {
            compress(&mut state, block);
        }
        let mut out = [0u8; 32];
        for (bytes, word) in out.chunks_exact_mut(4).zip(state) {
            bytes.copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    /// Returns the SHA-256 of `message` in one piece, as [`digest`] does.
    #[must_use]
    pub fn digest(message: impl AsRef<[u8]>) -> [u8; 32] {
        digest(message.as_ref())
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs the compression of one block of 64 bytes over `state`.
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
