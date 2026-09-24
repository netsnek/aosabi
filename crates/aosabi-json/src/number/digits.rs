// SPDX-License-Identifier: MIT
/*
 * aosabi-json number digits: the decimal digits of an integer, two at a time.
 *
 * The raw pointer writes of the original are writes into a slice here, and the
 * tables of the hundred digit pairs and of the powers of ten are computed when
 * the crate compiles. The digits of an integral double below 2^53, integer,
 * head, split, eight and four, have no analogue in ryu-js: they split off a
 * block of eight digits by one multiply high and one correction where the digit
 * writer of Ryu divides by 10^8.
 *
 * Derived from ryu-js 1.0.3, ., Apache-2.0 OR BSL-1.0.
 */

//! The decimal digits of an integer.
//!
//! A number is written from its last digit to its first, two digits per read of
//! the table of the hundred digit pairs, 200 bytes, which is the digit writer
//! of Ryu. The digits of Ryu go eight at a time by a division of 64 bits, which
//! a core of 32 bits such as the ESP32-C3 or the player calls `__udivdi3` and
//! `__umoddi3` for, as the trims of Ryu do. A block of eight digits and one of
//! four take the word of their digits that [`eight`] and [`four`] join for the
//! integer path, in one store each, where a copy of each pair into a slice
//! stays a call of the standard library per pair in the Worker module. The
//! exponent of the scientific form takes the short writer of ryu-js 1.0.3
//! `src/pretty/exponent.rs:1-31`.
//!
//! An integral double of at least 1 and below 2^53 takes [`integer`]. A value
//! of more than eight digits loses its last eight to [`split`], one multiply
//! high of 32 by 32 bits and one correction, so the integer path divides no 64
//! bits, and every other step is 32 bit arithmetic, whose division by a
//! constant the compiler turns into a multiply, or at the opt-level `z` into
//! the division of 32 bits the core has. The digits come as a head of at most
//! eight digits and a tail of eight, each a word joined from the pairs at
//! places no value moves, so no index of the integer path needs a check, and
//! the build for aarch64 takes none.

/// The digits of the hundred numbers from 00 to 99.
static PAIRS: [[u8; 2]; 100] = pairs();

/// Returns the table of the hundred digit pairs, `00` to `99`.
const fn pairs() -> [[u8; 2]; 100] {
    let mut table = [[0; 2]; 100];
    let mut i: u8 = 0;
    while i < 100 {
        table[i as usize] = [b'0' + i / 10, b'0' + i % 10];
        i += 1;
    }
    table
}

/// The powers of ten a `u64` holds, 10^0 to 10^19.
static POW10: [u64; 20] = powers_of_ten();

/// Returns the table of the powers of ten from 10^0 to 10^19.
const fn powers_of_ten() -> [u64; 20] {
    let mut table = [1; 20];
    let mut i = 1;
    while i < 20 {
        table[i] = table[i - 1] * 10;
        i += 1;
    }
    table
}

/// The digits of one block, eight.
const BLOCK: usize = 8;

/// The value of a block of digits, 10^8, the remainder of [`split`].
const LIMB: u32 = 100_000_000;

/// The multiplier of the estimate of [`split`], `2^58 / 10^8` rounded down.
const LIMB_RECIPROCAL: u64 = (1 << 58) / 100_000_000;

// The multiplier holds in 32 bits, so the estimate is one multiply of 32 by
// 32 bits.
const _: () = assert!(LIMB_RECIPROCAL < 1 << 32);

/// Returns the number of decimal digits of `value`, which is 1 for zero.
///
/// The bit length times `1233 / 4096`, just below `log10(2)`, is the count
/// of digits or one less, and one comparison with a power of ten decides.
pub(super) fn count(value: u64) -> u32 {
    let guess = ((64 - (value | 1).leading_zeros()) * 1233) >> 12;
    (guess + u32::from(value >= POW10[guess as usize])).max(1)
}

/// Returns `value / 10^8` and `value % 10^8` for a `value` below 2^57.
///
/// The bits of `value` above its lowest 26, at most 31 of them, times
/// `2^58 / 10^8` rounded down have in their high 32 bits an estimate of the
/// quotient, one multiply high of 32 by 32 bits. The estimate is never above
/// the quotient and falls short of it by less than one: the multiplier,
/// 2,882,303,761 for 2,882,303,761.52, loses less than `2^31 * 0.52 / 2^32`,
/// below 0.26, and the 26 dropped bits less than `2^26 / 10^8`, below 0.68. So
/// the quotient is the estimate or one more, and the remainder of the estimate
/// is below `2 * 10^8`, which 32 bits hold, so its low 32 bits are the low 32
/// bits of `value` less those of the estimate times 10^8. One compare corrects
/// both.
fn split(value: u64) -> (u32, u32) {
    let estimate = high32(u64::from(low32(value >> 26)) * LIMB_RECIPROCAL);
    let rest = low32(value).wrapping_sub(estimate.wrapping_mul(LIMB));
    if rest < LIMB {
        (estimate, rest)
    } else {
        (estimate + 1, rest - LIMB)
    }
}

/// Writes the decimal digits of `value` into `text`, the last one at `end - 1`.
///
/// The digits take the [`count`] bytes before `end`, with no leading zero.
/// Eight digits go at a time while eight or more are left, so sixteen or
/// seventeen digits take two blocks and at most one digit. The average shortest
/// output of Ryu is 16.38 digits.
// Unchecked `unsafe` writes would be faster.
pub(super) fn write(text: &mut [u8], mut end: usize, mut value: u64) {
    while value >= 10_000_000 {
        let limb = low32(value % 100_000_000);
        value /= 100_000_000;
        // The range is eight bytes long, so the conversion always succeeds, and
        // the block of fixed length takes the word of its eight digits in one
        // store, the word of the integer path.
        if let Ok(block) = <&mut [u8; 8]>::try_from(&mut text[end - 8..end]) {
            *block = eight(limb).to_le_bytes();
        }
        end -= 8;
        if value == 0 {
            return;
        }
    }
    write_u32(text, end, low32(value));
}

/// The digits of an integer of at least 1 and below 2^53, as the blocks the writer appends.
///
/// A block is the bytes of a word read little endian, its first digit in the
/// lowest byte.
pub(super) struct Integer {
    /// The first digits, at most eight, from the lowest byte on, and zeros above them.
    pub(super) head: u64,
    /// The count of the digits of `head`.
    pub(super) head_len: usize,
    /// The last eight digits, where the integer has more than eight.
    pub(super) tail: Option<u64>,
}

/// Returns the decimal digits of `value`, at least 1 and below 2^53.
///
/// A value below 10^8 is its head alone. Every other one is split by
/// [`split`] into its last eight digits, the tail, and the digits above
/// them, below 10^8 too, its head. So the integer path divides no 64 bits,
/// and every write and every read of a pair lands at a place no value moves,
/// so no index needs a check.
// Out of line the integer path is one function a listing can hold to its budget
// of no call and no library call, at one call per integral number. Unchecked
// reads of the pairs would drop the bound each limb takes.
#[inline(never)]
pub(super) fn integer(value: u64) -> Integer {
    let (lead, tail) = if value < u64::from(LIMB) {
        (low32(value), None)
    } else {
        let (high, low) = split(value);
        (high, Some(eight(low)))
    };
    let (head, head_len) = head(lead);
    Integer {
        head,
        head_len,
        tail,
    }
}

/// Returns the digits of `value`, at least 1 and below 10^8, from the lowest byte of a block on, and their count.
///
/// The digits of the four or eight places the value takes come with their
/// leading zeros in the lowest bytes, so a shift right by the bytes of those
/// zeros moves the first digit to the lowest byte. The count is three compares
/// beside the four or eight places, with no division.
fn head(value: u32) -> (u64, usize) {
    let value = value.min(LIMB - 1);
    if value < 10_000 {
        let count =
            1 + usize::from(value >= 10) + usize::from(value >= 100) + usize::from(value >= 1000);
        return (u64::from(four(value) >> (8 * (4 - count))), count);
    }
    let count = 5
        + usize::from(value >= 100_000)
        + usize::from(value >= 1_000_000)
        + usize::from(value >= 10_000_000);
    (eight(value) >> (8 * (BLOCK - count)), count)
}

/// Writes `value`, which is below 1000, with no leading zero from `at` on,
/// and returns the index after its last digit.
///
/// This is the exponent of the scientific form, which is at most 324.
pub(super) fn write_short(text: &mut [u8], at: usize, value: u32) -> usize {
    if value >= 100 {
        text[at] = PAIRS[(value / 100) as usize][1];
        text[at + 1..at + 3].copy_from_slice(&PAIRS[(value % 100) as usize]);
        at + 3
    } else if value >= 10 {
        text[at..at + 2].copy_from_slice(&PAIRS[value as usize]);
        at + 2
    } else {
        text[at] = PAIRS[value as usize][1];
        at + 1
    }
}

/// Writes the digits of `value`, which is below 10^7, with no leading zero,
/// the last one at `end - 1`.
fn write_u32(text: &mut [u8], mut end: usize, mut value: u32) {
    while value >= 10_000 {
        if let Ok(block) = <&mut [u8; 4]>::try_from(&mut text[end - 4..end]) {
            *block = four(value % 10_000).to_le_bytes();
        }
        value /= 10_000;
        end -= 4;
    }
    if value >= 100 {
        text[end - 2..end].copy_from_slice(&PAIRS[(value % 100) as usize]);
        value /= 100;
        end -= 2;
    }
    if value >= 10 {
        text[end - 2..end].copy_from_slice(&PAIRS[value as usize]);
    } else {
        text[end - 1] = PAIRS[value as usize][1];
    }
}

/// Returns the eight digits of `limb`, which is below 10^8, leading zeros
/// included, as the bytes of a word read little endian.
// Always inlined, since at opt-level z a hint leaves it a call of the integer
// path and of the block of eight digits of `write`.
#[expect(
    clippy::inline_always,
    reason = "a hint inlines nothing into a caller built at opt-level z"
)]
#[inline(always)]
fn eight(limb: u32) -> u64 {
    // The bound changes no limb below 10^8 and lets the compiler see, where
    // it inlines `four`, that every pair this block reads is one of the
    // hundred, so no read of the table takes a check there.
    let limb = limb.min(LIMB - 1);
    u64::from(four(limb / 10_000)) | (u64::from(four(limb % 10_000)) << 32)
}

/// Returns the four digits of `value`, which is below 10^4, leading zeros
/// included, as the bytes of a word read little endian.
///
/// Each pair is read as one word of 16 bits, so a block of eight joins its four
/// pairs in two steps where a join of its bytes takes one step per byte, and a
/// caller stores the word in one piece.
// Always inlined, since at opt-level z a hint leaves it a call of the integer
// path, of `eight` and of the block of four digits of `write_u32`.
#[expect(
    clippy::inline_always,
    reason = "a hint inlines nothing into a caller built at opt-level z"
)]
#[inline(always)]
fn four(value: u32) -> u32 {
    let high = u16::from_le_bytes(PAIRS[(value / 100) as usize]);
    let low = u16::from_le_bytes(PAIRS[(value % 100) as usize]);
    u32::from(high) | (u32::from(low) << 16)
}

/// Returns the low 32 bits of `value`.
// Dropping the high half is the point: every caller passes a value whose
// high half is zero or whose low half alone it wants.
#[expect(clippy::cast_possible_truncation)]
fn low32(value: u64) -> u32 {
    value as u32
}

/// Returns the high 32 bits of `value`.
fn high32(value: u64) -> u32 {
    low32(value >> 32)
}
