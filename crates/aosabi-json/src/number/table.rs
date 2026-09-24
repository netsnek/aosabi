// SPDX-License-Identifier: MIT
/*
 * aosabi-json number table: the powers of five Ryu multiplies by, as its small table.
 *
 * The numbers of the table are not copied. The crate computes them when it
 * compiles, from the definition of the full table of Ryu, and checks at
 * compile time that the multiplier this file returns equals that definition
 * for every index a double reaches.
 *
 * Derived from ryu-js 1.0.3, ., Apache-2.0 OR BSL-1.0.
 */

//! The powers of five of Ryu, in its small table.
//!
//! Ryu multiplies the significand of a double by a power of five, or by the
//! inverse of one, held as a 128 bit number. It scales every multiplier to 125
//! bits, the constants `DOUBLE_POW5_INV_BITCOUNT` and `DOUBLE_POW5_BITCOUNT`.
//! The power `5^i` is `floor(5^i / 2^(L - 125))`, where `L` is the bit length
//! of `5^i`, and its inverse is `floor(2^(L - 1 + 125) / 5^i) + 1`, which give
//! all 326 powers and 342 inverses of ryu-js 1.0.3 `src/d2s_full_table.rs`.
//! The scale is not the bit length of every entry. The inverse of `5^0` is
//! `2^125 + 1`, which takes 126 bits.
//!
//! The table indexes the powers `5^0` to `5^325` and the inverses `5^-0` to
//! `5^-290`. A double asks for the powers `5^1` to `5^325` and for every
//! inverse, 325 powers and 291 inverses. No double asks for `5^0`. It is the
//! stored power from which the lookup computes `5^1` to `5^25`. The full table
//! of Ryu stores each multiplier in 16 bytes. The small table stores every
//! twenty sixth one. Beside them it stores the powers of five `5^0` to `5^25`,
//! each below 2^64, and two bits per index that correct the product of a
//! stored multiplier and one of those powers, so a multiplier costs two 128
//! bit products.
//!
//! The table is computed by `const fn` from the definition above in exact
//! integer arithmetic, so no number in it is typed by hand. The same
//! evaluation reads every multiplier back through `pow5_from` and
//! `inv_pow5_from`, the functions [`pow5`] and [`inv_pow5`] call at run time,
//! and fails the build when one differs from the definition, when a
//! correction needs more than two bits, or when an intermediate sum overflows.
//! The table lives in read only data, 784 bytes, and nothing of it is built at
//! run time.

use super::low64;

/// The bits Ryu scales every multiplier to, which the inverse of `5^0`
/// exceeds by one.
const BITS: u32 = 125;

/// The count of powers of five the small table of Ryu stores, `5^0` to `5^25`,
/// each below 2^64.
const BASE: u32 = 26;

/// The powers of five the table indexes, `5^0` to `5^325`.
///
/// A double asks for `5^1` to `5^325`, and `5^0` is the stored power from
/// which the lookup computes `5^1` to `5^25`.
pub(super) const POW5_COUNT: u32 = 326;

/// The inverses of powers of five the table indexes, `5^-0` to `5^-290`.
///
/// A double asks for every one of them.
pub(super) const INV_COUNT: u32 = 291;

/// The stored powers, one for every twenty sixth index.
const POW5_SPLITS: usize = POW5_COUNT.div_ceil(BASE) as usize;

/// The stored inverses, one for every twenty sixth index up to the first at or
/// above the last inverse, because an inverse is reached from the stored one
/// above it.
const INV_SPLITS: u32 = (INV_COUNT - 1).div_ceil(BASE) + 1;

/// The words of the two bit corrections of the powers, sixteen per word.
const POW5_WORDS: usize = POW5_COUNT.div_ceil(16) as usize;

/// The words of the two bit corrections of the inverses, sixteen per word.
const INV_WORDS: usize = INV_COUNT.div_ceil(16) as usize;

/// The small table of Ryu.
struct Table {
    /// The powers of five the small table of Ryu stores, `5^0` to `5^25`.
    base: [u64; BASE as usize],
    /// The power `5^(26 k)` at index `k`, as its low and its high word.
    pow5: [(u64, u64); POW5_SPLITS],
    /// The correction of the power `5^i` in bits `2 (i % 16)` of word `i / 16`.
    pow5_offsets: [u32; POW5_WORDS],
    /// The inverse `5^-(26 k)` at index `k`, as its low and its high word.
    inv: [(u64, u64); INV_SPLITS as usize],
    /// The correction of the inverse `5^-i` in bits `2 (i % 16)` of word `i / 16`.
    inv_offsets: [u32; INV_WORDS],
}

/// The table, computed and checked when the crate compiles.
static TABLE: Table = BUILT;

/// The table as the compile time evaluation builds it.
const BUILT: Table = build();

// The size the module documentation states, the base powers, the stored
// powers and inverses and their corrections.
const _: () = assert!(size_of::<Table>() == 784);

/// Returns the multiplier of `5^i`, as its low and its high word.
///
/// Every double asks for an `i` below [`POW5_COUNT`], which the compile time
/// check `check_exponents` of `shortest.rs` holds, and the table build runs
/// the lookup for every such `i`, so no index a double asks for fails a
/// bounds check. An index beyond the table returns zero.
pub(super) fn pow5(i: u32) -> (u64, u64) {
    if i < POW5_COUNT {
        pow5_from(&TABLE, i)
    } else {
        (0, 0)
    }
}

/// Returns the multiplier of `5^-i`, as its low and its high word.
///
/// Every double asks for an `i` below [`INV_COUNT`], which the compile time
/// check `check_exponents` of `shortest.rs` holds, and the table build runs
/// the lookup for every such `i`, so no index a double asks for fails a
/// bounds check. An index beyond the table returns zero.
pub(super) fn inv_pow5(i: u32) -> (u64, u64) {
    if i < INV_COUNT {
        inv_pow5_from(&TABLE, i)
    } else {
        (0, 0)
    }
}

/// Returns `ceil(log2(5^e))`, the bit length of `5^e`, for `e` from 1 to 3528.
///
/// The product is exact up to that bound, and the value for `e` of 0 is 1,
/// which is the bit length of `5^0` too. The table build checks the value
/// against the bit length of every power it computes.
pub(super) const fn pow5bits(e: u32) -> u32 {
    ((e * 1_217_359) >> 19) + 1
}

/// Returns the multiplier of `5^i` from `table`, the product of a stored
/// power and a power below 2^64 corrected by two bits.
const fn pow5_from(table: &Table, i: u32) -> (u64, u64) {
    let base = i / BASE;
    let base2 = base * BASE;
    let offset = i - base2;
    let mul = table.pow5[base as usize];
    if offset == 0 {
        return mul;
    }
    let m = table.base[offset as usize] as u128;
    let low = m * mul.0 as u128;
    let high = m * mul.1 as u128;
    let delta = pow5bits(i) - pow5bits(base2);
    let sum = (low >> delta) + (high << (64 - delta)) + correction(&table.pow5_offsets, i);
    split(sum)
}

/// Returns the multiplier of `5^-i` from `table`, the product of the stored
/// inverse above it and a power below 2^64, corrected by two bits.
const fn inv_pow5_from(table: &Table, i: u32) -> (u64, u64) {
    let base = i.div_ceil(BASE);
    let base2 = base * BASE;
    let offset = base2 - i;
    let mul = table.inv[base as usize];
    if offset == 0 {
        return mul;
    }
    let m = table.base[offset as usize] as u128;
    let low = m * (mul.0 - 1) as u128;
    let high = m * mul.1 as u128;
    let delta = pow5bits(base2) - pow5bits(i);
    let sum = (low >> delta) + (high << (64 - delta)) + 1 + correction(&table.inv_offsets, i);
    split(sum)
}

/// Returns the two bit correction of index `i` in `words`.
const fn correction(words: &[u32], i: u32) -> u128 {
    ((words[(i / 16) as usize] >> ((i % 16) * 2)) & 3) as u128
}

/// Returns the low and the high word of `value`.
const fn split(value: u128) -> (u64, u64) {
    (low64(value), low64(value >> 64))
}

/// Returns the number the low and the high word make.
const fn join(words: (u64, u64)) -> u128 {
    words.0 as u128 | (words.1 as u128) << 64
}

/// The limbs of the exact integers the table is computed from.
///
/// The largest is `2^1024`, the numerator of every inverse before it is
/// scaled down, which needs a seventeenth limb for its one bit.
const LIMBS: u32 = 17;

/// The power of two every inverse is taken of before it is scaled down.
const INV_SCALE: u32 = 1024;

/// A nonnegative integer of [`LIMBS`] words of 64 bits, the least significant
/// first.
type Big = [u64; LIMBS as usize];

/// Returns the table of this file, computed and checked.
///
/// This runs when the crate compiles and never at run time, and every
/// assertion in it and in the functions it calls stops the build.
const fn build() -> Table {
    let mut table = Table {
        base: [0; BASE as usize],
        pow5: [(0, 0); POW5_SPLITS],
        pow5_offsets: [0; POW5_WORDS],
        inv: [(0, 0); INV_SPLITS as usize],
        inv_offsets: [0; INV_WORDS],
    };
    let mut power = 1;
    let mut i = 0;
    while i < BASE as usize {
        table.base[i] = power;
        power *= 5;
        i += 1;
    }
    build_powers(&mut table);
    build_inverses(&mut table);
    table
}

/// Fills the powers of `table` and their corrections, and checks every one.
const fn build_powers(table: &mut Table) {
    let mut power = one();
    let mut i = 0;
    while i < POW5_COUNT {
        assert!(
            bit_length(&power) == pow5bits(i),
            "pow5bits is not the bit length of 5^i"
        );
        let exact = bits(&power, pow5bits(i).cast_signed() - BITS.cast_signed());
        if i % BASE == 0 {
            table.pow5[(i / BASE) as usize] = split(exact);
        } else {
            let approximate = join(pow5_from(table, i));
            let offset = offset_of(exact, approximate);
            table.pow5_offsets[(i / 16) as usize] |= offset << ((i % 16) * 2);
        }
        assert!(
            join(pow5_from(table, i)) == exact,
            "a power differs from the full table"
        );
        power = mul_small(power, 5);
        i += 1;
    }
}

/// Fills the inverses of `table` and their corrections, and checks every one.
///
/// An inverse is reached from the stored inverse at or above it, so a first
/// pass stores every twenty sixth one and a second pass computes the
/// corrections. Both keep `floor(2^1024 / 5^i)` and divide it by five per
/// index, because `floor(floor(a / b) / c)` is `floor(a / (b c))` for
/// positive integers, and `floor(2^j / 5^i)` is that number shifted right by
/// `1024 - j`.
const fn build_inverses(table: &mut Table) {
    let mut scaled = power_of_two(INV_SCALE);
    let mut i = 0;
    while i < INV_SPLITS * BASE {
        if i % BASE == 0 {
            table.inv[(i / BASE) as usize] = split(inverse(&scaled, i));
        }
        scaled = div_small(scaled, 5);
        i += 1;
    }
    let mut scaled = power_of_two(INV_SCALE);
    let mut i = 0;
    while i < INV_COUNT {
        let exact = inverse(&scaled, i);
        if i % BASE != 0 {
            let approximate = join(inv_pow5_from(table, i));
            let offset = offset_of(exact, approximate);
            table.inv_offsets[(i / 16) as usize] |= offset << ((i % 16) * 2);
        }
        assert!(
            join(inv_pow5_from(table, i)) == exact,
            "an inverse differs from the full table"
        );
        scaled = div_small(scaled, 5);
        i += 1;
    }
}

/// Returns the inverse `floor(2^(L - 1 + 125) / 5^i) + 1` from `scaled`,
/// which is `floor(2^1024 / 5^i)`.
const fn inverse(scaled: &Big, i: u32) -> u128 {
    let exponent = pow5bits(i) - 1 + BITS;
    assert!(exponent <= INV_SCALE, "an inverse needs a larger scale");
    bits(scaled, (INV_SCALE - exponent).cast_signed()) + 1
}

/// Returns the correction that turns `approximate` into `exact`, which fits
/// the two bits the table has for it.
const fn offset_of(exact: u128, approximate: u128) -> u32 {
    assert!(
        approximate <= exact,
        "the product lies above the full table"
    );
    match exact - approximate {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        _ => panic!("a correction needs more than two bits"),
    }
}

/// Returns 1.
const fn one() -> Big {
    let mut value = [0; LIMBS as usize];
    value[0] = 1;
    value
}

/// Returns `2^exponent`.
const fn power_of_two(exponent: u32) -> Big {
    let mut value = [0; LIMBS as usize];
    value[(exponent / 64) as usize] = 1 << (exponent % 64);
    value
}

/// Returns `value * factor`, and stops the build when it overflows.
const fn mul_small(mut value: Big, factor: u64) -> Big {
    let mut carry = 0;
    let mut i = 0;
    while i < LIMBS as usize {
        let product = value[i] as u128 * factor as u128 + carry;
        value[i] = low64(product);
        carry = product >> 64;
        i += 1;
    }
    assert!(carry == 0, "an exact power overflows its limbs");
    value
}

/// Returns `floor(value / divisor)`.
const fn div_small(mut value: Big, divisor: u64) -> Big {
    let mut remainder = 0;
    let mut i = LIMBS as usize;
    while i > 0 {
        i -= 1;
        let current = remainder << 64 | value[i] as u128;
        value[i] = low64(current / divisor as u128);
        remainder = current % divisor as u128;
    }
    value
}

/// Returns the number of bits of `value` up to its highest set bit.
const fn bit_length(value: &Big) -> u32 {
    let mut i = LIMBS;
    while i > 0 {
        i -= 1;
        let word = value[i as usize];
        if word != 0 {
            return i * 64 + 64 - word.leading_zeros();
        }
    }
    0
}

/// Returns `floor(value / 2^shift)`, or `value * 2^-shift` for a negative
/// `shift`, and stops the build when the result does not fit 128 bits.
const fn bits(value: &Big, shift: i32) -> u128 {
    let length = bit_length(value).cast_signed();
    assert!(length - shift <= 128, "a multiplier does not fit 128 bits");
    if shift <= 0 {
        return join((value[0], value[1])) << shift.unsigned_abs();
    }
    let shift = shift.unsigned_abs();
    let word = (shift / 64) as usize;
    let bit = shift % 64;
    let low = join((limb(value, word), limb(value, word + 1)));
    let high = limb(value, word + 2) as u128;
    if bit == 0 {
        low
    } else {
        low >> bit | high << (128 - bit)
    }
}

/// Returns limb `i` of `value`, which is 0 above the last limb.
const fn limb(value: &Big, i: usize) -> u64 {
    if i < LIMBS as usize { value[i] } else { 0 }
}
