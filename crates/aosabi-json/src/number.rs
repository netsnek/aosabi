// SPDX-License-Identifier: MIT
/*
 * aosabi-json number: a double as ECMA-262 Number::toString writes it.
 *
 * The digits of step 5 come from the port of Ryu in number/shortest.rs and
 * number/table.rs, and an integer of at least 1 and below 2^53 in magnitude,
 * read from its bits, takes its own digits without Ryu. The notation of steps
 * 6 to 10 is a translation of ryu-js 1.0.3 src/pretty/mod.rs:55-121,
 * https://docs.rs/crate/ryu-js/1.0.3/source/src/pretty/mod.rs, the Rust
 * translation of Ryu by Ulf Adams, which carries the notice of Ryu under
 * Apache-2.0 OR BSL-1.0, reproduced in NOTICE section 4.
 */

//! The number half of the byte contract.
//!
//! The rule is ECMA-262 `Number::toString` with radix 10 and not any Rust
//! default. The shortest digits that round trip are chosen, fixed notation is
//! used when the decimal exponent lies in the window from -5 to 21 and
//! scientific notation with an explicit sign otherwise, an integer valued
//! double carries no fractional part, and both zeros print as `0`.
//!
//! The digits are Ryu's, in a port written in this crate with no `unsafe`,
//! with the small table of Ryu computed when the crate compiles. The port
//! under `number/` needs `core` alone, and this module adds `alloc` for the
//! `Vec` and the `String` it writes into, so it builds for every target of the
//! crate, `riscv32imc-unknown-none-elf` among them.
//!
//! An integer of at least 1 and below 2^53 in magnitude takes the integer
//! path: its bits tell it apart with no compare of doubles, which a core with
//! no floating point unit, the ESP32-C3 or the player, would call a function
//! of its library for, and its digits are written in blocks of eight with no
//! division of 64 bits. Every other double, both zeros among them, takes the
//! path of Ryu, which writes a zero as `0` by its bits before it reads the
//! sign.

mod digits;
mod shortest;
mod table;

use alloc::{
    string::String,
    vec::Vec, //
};

use self::shortest::shortest;

/// The four bytes a number that is not finite becomes.
///
/// `SerializeJSONProperty` returns the string `"null"` for a Number value that
/// is not finite, which covers NaN and both infinities.
///
/// [serialize]: https://tc39.es/ecma262/#sec-serializejsonproperty
const NULL: &[u8] = b"null";

/// The most bytes a finite double takes.
///
/// That is a sign, `0.`, five zeros and seventeen digits, as in
/// `-0.0000012345678901234567`, while a sign, seventeen digits, a point and
/// `e-308` take 24 and an integer takes at most 22.
const MAX_LEN: usize = 25;

/// The stored bits of the significand of a double.
const MANTISSA_BITS: u32 = 52;

/// The sign bit of a double.
const SIGN: u64 = 1 << 63;

/// The stored exponent of a double that is not finite, an infinity or a NaN.
const NOT_FINITE: u32 = 0x7FF;

/// The stored exponent of the doubles from 1 to 2, the bias.
const EXPONENT_OF_ONE: u32 = 1023;

/// The stored exponent of the doubles from 2^52 to 2^53, whose last bit is
/// worth 1.
const EXPONENT_OF_UNIT: u32 = EXPONENT_OF_ONE + MANTISSA_BITS;

/// The largest decimal exponent `n` that fixed notation writes, 21.
const FIXED_LIMIT: i32 = 21;

/// Appends a double the way ECMA-262 `Number::toString` does.
///
/// The bytes go into `out`, and `value` may be any double. A finite value is
/// written in the shortest form that round trips, under the notation rules of
/// `Number::toString`. Negative zero prints as `0`, because the clause returns
/// `"0"` for both zeros before it looks at the sign. A value that is not
/// finite is written as `null`, which is the JSON rule rather than the
/// `Number::toString` rule, because this crate writes JSON and nothing else.
///
/// An integer of at least 1 and below 2^53 in magnitude is the digits of that
/// integer, which is its shortest round trip form. The rounding interval of
/// such a double is at most 1 wide, so any other decimal in it is no integer
/// and has a digit below the units place, which takes at least as many digits
/// as the integer has and lies farther from the double. It is told apart and
/// read by its bits alone and takes the integer path of the module. Every
/// other value takes the path of Ryu.
///
/// [tostring]: https://tc39.es/ecma262/#sec-numeric-types-number-tostring
// An `unsafe` `set_len` over the digits in the spare room of `out` would skip the bytes the truncate takes back.
pub fn write_f64(out: &mut Vec<u8>, value: f64) {
    let bits = value.to_bits();
    let Some(integer) = small_integer(bits & !SIGN) else {
        write_other(out, bits);
        return;
    };
    if bits & SIGN != 0 {
        out.push(b'-');
    }
    let digits = digits::integer(integer);
    // The head goes in whole and the bytes after its digits leave again, one
    // store of a fixed length where a copy of the digits alone calls `memcpy`.
    let end = out.len() + digits.head_len;
    out.extend_from_slice(&digits.head.to_le_bytes());
    out.truncate(end);
    if let Some(tail) = digits.tail {
        out.extend_from_slice(&tail.to_le_bytes());
    }
}

/// Appends a double that is no integer of at least 1 and below 2^53 in magnitude, as [`write_f64`] does.
///
/// A value that is not finite, by its stored exponent, is `null`, and every
/// finite one takes [`format_finite`].
// Out of line the path of Ryu leaves `write_f64` to the integer path, one
// function a listing can hold to its budget of no library call on a core of 32
// bits, at one call per double that takes Ryu, beside the more than a thousand
// instructions such a number costs in an answer.
#[inline(never)]
fn write_other(out: &mut Vec<u8>, bits: u64) {
    if biased_exponent(bits) == NOT_FINITE {
        out.extend_from_slice(NULL);
        return;
    }
    let mut text = [0; MAX_LEN];
    let len = format_finite(&mut text, bits);
    out.extend_from_slice(&text[..len]);
}

/// Returns `String(value)` for a number, ECMA-262 `Number::toString`.
///
/// This is what a template literal writes for a number. NaN is `NaN`, the two
/// infinities are `Infinity` and `-Infinity`, which the clause names before
/// the digit cases. A finite value takes the digits and the notation of
/// [`write_f64`], which is that clause for a finite value and prints both
/// zeros as `0`.
///
/// [tostring]: https://tc39.es/ecma262/#sec-numeric-types-number-tostring
#[must_use]
pub fn number_to_string(value: f64) -> String {
    if value.is_nan() {
        return String::from("NaN");
    }
    if value.is_infinite() {
        return String::from(if value > 0.0 { "Infinity" } else { "-Infinity" });
    }
    let mut out = Vec::new();
    write_f64(&mut out, value);
    String::from_utf8_lossy(&out).into_owned()
}

/// Writes the finite double of the bits `bits` into `text` and returns the count of its bytes.
///
/// Both zeros are `0`, told by the bits of the magnitude before the sign is
/// read, so `-0` never writes its sign. Every other value takes the digits of
/// Ryu in the notation of the clause.
fn format_finite(text: &mut [u8; MAX_LEN], bits: u64) -> usize {
    let magnitude = bits & !SIGN;
    if magnitude == 0 {
        text[0] = b'0';
        return 1;
    }
    // The sign byte is written in any case, and the digits of a value that is
    // not negative overwrite it, so the sign costs no branch.
    let sign = usize::from(bits != magnitude);
    text[0] = b'-';
    let ieee_mantissa = magnitude & ((1 << MANTISSA_BITS) - 1);
    let decimal = shortest(ieee_mantissa, biased_exponent(magnitude));
    sign + write_notation(&mut text[sign..], decimal.digits, decimal.exponent)
}

/// Returns the integer a double of the magnitude `magnitude` is, when it is one of at least 1 and below 2^53.
///
/// The argument `magnitude` is the bits of a double with the sign bit clear.
/// A double with the stored exponent `e` from 1023 to 1075 is its significand
/// with the implicit bit, shifted right by `1075 - e`, and it is an integer
/// when the bits the shift drops are zero. A zero, a subnormal and every value
/// below 1 have a stored exponent below 1023, and every value from 2^53 on, an
/// infinity and a NaN one above 1075, so one compare of `1075 - e` tells the
/// range.
fn small_integer(magnitude: u64) -> Option<u64> {
    let fraction_bits = EXPONENT_OF_UNIT.wrapping_sub(biased_exponent(magnitude));
    if fraction_bits > MANTISSA_BITS {
        return None;
    }
    let significand = (1 << MANTISSA_BITS) | (magnitude & ((1 << MANTISSA_BITS) - 1));
    (significand.trailing_zeros() >= fraction_bits).then_some(significand >> fraction_bits)
}

/// Writes `digits * 10^exponent` into `text` in the notation of
/// `Number::toString` and returns the count of its bytes.
///
/// The argument `digits` is `s` of step 5 of the clause with its `k` digits,
/// and `n` is `k + exponent`, so `s * 10^(n - k)` is the value. Steps 6 to 10
/// choose between the integer, the fixed and the scientific form by `n` and
/// `k`. The digits are written once, where the form puts them, as ryu-js does.
///
/// [tostring]: https://tc39.es/ecma262/#sec-numeric-types-number-tostring
fn write_notation(text: &mut [u8], digits: u64, exponent: i32) -> usize {
    let count = digits::count(digits);
    let n = count.cast_signed() + exponent;
    let k = count as usize;
    if count.cast_signed() <= n && n <= FIXED_LIMIT {
        // Twenty one zeros go first, and the digits overwrite their head,
        // which is a fixed store where a fill of `n - k` would call memset.
        text[..21].copy_from_slice(&[b'0'; 21]);
        digits::write(text, k, digits);
        n.unsigned_abs() as usize
    } else if 0 < n && n <= FIXED_LIMIT {
        let n = n.unsigned_abs() as usize;
        digits::write(text, k + 1, digits);
        text.copy_within(1..=n, 0);
        text[n] = b'.';
        k + 1
    } else if -6 < n && n <= 0 {
        // The five zeros the form can need go first, and the digits overwrite
        // the ones it does not.
        let zeros = n.unsigned_abs() as usize;
        text[..7].copy_from_slice(b"0.00000");
        digits::write(text, 2 + zeros + k, digits);
        2 + zeros + k
    } else {
        write_scientific(text, digits, k, n - 1)
    }
}

/// Writes `digits`, of `k` digits, as `d.ddd` followed by `e`, the sign and
/// `exponent`, and returns the count of its bytes.
///
/// A single digit takes no point, which is step 9 of the clause, and more
/// digits take one after the first, which is step 10.
fn write_scientific(text: &mut [u8], digits: u64, k: usize, exponent: i32) -> usize {
    let at = if k == 1 {
        digits::write(text, 1, digits);
        1
    } else {
        digits::write(text, k + 1, digits);
        text[0] = text[1];
        text[1] = b'.';
        k + 1
    };
    text[at] = b'e';
    text[at + 1] = if exponent < 0 { b'-' } else { b'+' };
    digits::write_short(text, at + 2, exponent.unsigned_abs())
}

/// Returns the eleven bits of the stored exponent of a double.
fn biased_exponent(bits: u64) -> u32 {
    ((bits >> MANTISSA_BITS) & 0x7FF) as u32
}

/// Returns the low 64 bits of `x`.
// Dropping the high half is the point, because a product of Ryu is 128 bits
// wide and its two words are used apart.
#[expect(clippy::cast_possible_truncation)]
const fn low64(x: u128) -> u64 {
    x as u64
}
