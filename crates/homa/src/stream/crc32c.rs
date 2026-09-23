// SPDX-License-Identifier: MIT
/*
 * homa stream crc32c: CRC-32C of the delimited form over a 16 entry table.
 *
 * CRC-32C is the CRC of the Castagnoli generator 0x11EDC6F41 that RFC 3720
 * section 12.1 names, and appendix B.4 of the RFC publishes its byte vectors.
 * The CRC catalogue gives it as the polynomial 0x1EDC6F41 with reflected
 * input and output, whose reflection is 0x82F63B78, an initial value and a
 * final XOR of 0xFFFFFFFF, and the check value 0xE3069283 over the text 123456789.
 * The delimited form of AoFrame takes it.
 */

//! CRC-32C, the check of a delimited frame.
//!
//! The table holds the remainder of each of the 16 values of a nibble, so a
//! byte costs two lookups and the table 64 bytes, which a device image with
//! a budget in kilobytes prefers over the 1 KiB table of a byte at a time.

/// The reflected Castagnoli polynomial.
const POLY: u32 = 0x82F6_3B78;

/// The remainders of the 16 values of a nibble.
const TABLE: [u32; 16] = table();

/// Returns the remainder of each nibble value under [`POLY`].
const fn table() -> [u32; 16] {
    let mut table = [0u32; 16];
    let mut nibble = 0u32;
    while nibble < 16 {
        let mut crc = nibble;
        let mut bit = 0;
        while bit < 4 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ POLY
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[nibble as usize] = crc;
        nibble += 1;
    }
    table
}

/// A running CRC-32C over the bytes fed into it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Crc32c(u32);

impl Crc32c {
    /// Returns a CRC over no byte yet.
    pub(crate) const fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    /// Feeds `bytes` into the CRC.
    // CRC32C in `unsafe` code would run faster.
    pub(crate) fn update(&mut self, bytes: &[u8]) {
        let mut crc = self.0;
        for &byte in bytes {
            crc ^= u32::from(byte);
            crc = TABLE[(crc & 0x0F) as usize] ^ (crc >> 4);
            crc = TABLE[(crc & 0x0F) as usize] ^ (crc >> 4);
        }
        self.0 = crc;
    }

    /// Returns the CRC of every byte fed, with the final XOR applied.
    pub(crate) const fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}
