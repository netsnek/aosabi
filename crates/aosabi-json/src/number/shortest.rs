// SPDX-License-Identifier: MIT
/*
 * aosabi-json number shortest: the shortest decimal digits of a double that round trip.
 *
 * The three out values of mul_shift_all_64 are three calls here, and the
 * scale of a binary exponent is one function that the build checks for every
 * exponent a double has.
 *
 * Derived from ryu-js 1.0.3, ., Apache-2.0 OR BSL-1.0.
 */

//! The shortest decimal digits of a double, after Ryu.
//!
//! For a finite double that is not zero, [`shortest`] returns the decimal
//! with the fewest digits that reads back as the same double, and among those
//! the one closest to the double, the even one on a tie. That is the choice
//! step 5 of ECMA-262 `Number::toString` makes and V8 implements. The
//! algorithm is Ryu, which proves the choice for every double.
//!
//! Every index into the table and every shift depends on the binary exponent
//! alone, so a compile time pass over all 2047 exponents checks that each one
//! stays within the table and within the width of its integer.
//!
//! [tostring]: https://tc39.es/ecma262/#sec-numeric-types-number-tostring
//! [paper]: https://dl.acm.org/doi/10.1145/3192366.3192369

use super::table::{
    INV_COUNT,
    POW5_COUNT,
    inv_pow5,
    pow5,
    pow5bits, //
};

/// The stored bits of the significand of a double.
const MANTISSA_BITS: u32 = 52;

/// The bias of the exponent of a double.
const BIAS: i32 = 1023;

/// The bits Ryu scales every multiplier of the table to.
const MULTIPLIER_BITS: i32 = 125;

/// The multiplicative inverse of 5 modulo 2^64.
const INV_5: u64 = 0xCCCC_CCCC_CCCC_CCCD;

// A product with 5 of 1 modulo 2^64 is what makes `INV_5` the inverse.
const _: () = assert!(INV_5.wrapping_mul(5) == 1);

/// A decimal `digits * 10^exponent`.
pub(super) struct Decimal {
    /// The digits, at most seventeen of them.
    pub(super) digits: u64,
    /// The power of ten of the last digit.
    pub(super) exponent: i32,
}

/// The interval of step 3 of Ryu, scaled to a power of ten.
#[derive(Clone, Copy)]
struct Interval {
    /// The decimal exponent of the three bounds.
    e10: i32,
    /// The scaled value.
    vr: u64,
    /// The scaled upper bound.
    vp: u64,
    /// The scaled lower bound.
    vm: u64,
    /// Whether the digits the scaling dropped from the value are all zero.
    vr_is_trailing_zeros: bool,
    /// Whether the digits the scaling dropped from the lower bound are all zero.
    vm_is_trailing_zeros: bool,
}

/// What a binary exponent asks of the table, its multiplier and its shift.
struct Scale {
    /// The decimal exponent `q` of Ryu.
    q: u32,
    /// Whether the multiplier is an inverse power of five.
    inverse: bool,
    /// The power of five, or of its inverse.
    index: u32,
    /// The right shift of the 192 bit product, at least 64 and below 192.
    shift: i32,
}

/// Returns the shortest decimal of the double with these fields.
///
/// The argument `ieee_mantissa` is the stored significand and `ieee_exponent`
/// the stored exponent, and the double is finite and not zero, which the
/// caller checks.
pub(super) fn shortest(ieee_mantissa: u64, ieee_exponent: u32) -> Decimal {
    let (e2, m2) = if ieee_exponent == 0 {
        (1 - BIAS - MANTISSA_BITS.cast_signed() - 2, ieee_mantissa)
    } else {
        (
            ieee_exponent.cast_signed() - BIAS - MANTISSA_BITS.cast_signed() - 2,
            (1 << MANTISSA_BITS) | ieee_mantissa,
        )
    };
    let accept_bounds = m2 & 1 == 0;
    let mm_shift = u64::from(ieee_mantissa != 0 || ieee_exponent <= 1);
    let interval = interval(m2, e2, mm_shift, accept_bounds);
    let e10 = interval.e10;
    let (digits, removed) = if interval.vr_is_trailing_zeros || interval.vm_is_trailing_zeros {
        trim_general(interval, accept_bounds)
    } else {
        trim_common(interval)
    };
    Decimal {
        digits,
        exponent: e10 + removed,
    }
}

/// Returns the value and its two bounds times `2^e2`, scaled to a power of ten.
///
/// This is step 2 and step 3 of Ryu. The bounds are `4 m2 + 2` and
/// `4 m2 - 1 - mm_shift`, the lower one closer when the significand is the
/// smallest of its binade. The two flags record whether the scaling dropped
/// only zeros, which step 4 needs to round an exact tie to even.
fn interval(m2: u64, e2: i32, mm_shift: u64, accept_bounds: bool) -> Interval {
    let scale = scale(e2);
    let multiplier = if scale.inverse {
        inv_pow5(scale.index)
    } else {
        pow5(scale.index)
    };
    let shift = scale.shift.cast_unsigned();
    let mv = 4 * m2;
    let mm = mv - 1 - mm_shift;
    let mut interval = Interval {
        e10: if scale.inverse {
            scale.q.cast_signed()
        } else {
            scale.q.cast_signed() + e2
        },
        vr: mul_shift(mv, multiplier, shift),
        vp: mul_shift(mv + 2, multiplier, shift),
        vm: mul_shift(mm, multiplier, shift),
        vr_is_trailing_zeros: false,
        vm_is_trailing_zeros: false,
    };
    let q = scale.q;
    if scale.inverse {
        if q <= 21 {
            mark_multiples_of_5(&mut interval, mv, mm, q, accept_bounds);
        }
    } else if q <= 1 {
        // The value `mv` has two trailing zero bits, `mm` has one when
        // `mm_shift` is 1, and `mp = mv + 2` always has one.
        interval.vr_is_trailing_zeros = true;
        if accept_bounds {
            interval.vm_is_trailing_zeros = mm_shift == 1;
        } else {
            interval.vp -= 1;
        }
    } else if q < 63 {
        interval.vr_is_trailing_zeros = mv.trailing_zeros() >= q;
    }
    interval
}

/// Records which bound of a division by `10^q` drops only zeros.
///
/// Only one of `mv`, `mm` and `mp = mv + 2` can be a multiple of 5, if any. A
/// bound that is not accepted and divides exactly lies outside, so the upper
/// one moves in by one.
fn mark_multiples_of_5(interval: &mut Interval, mv: u64, mm: u64, q: u32, accept_bounds: bool) {
    if mv.is_multiple_of(5) {
        interval.vr_is_trailing_zeros = multiple_of_power_of_5(mv, q);
    } else if accept_bounds {
        interval.vm_is_trailing_zeros = multiple_of_power_of_5(mm, q);
    } else {
        interval.vp -= u64::from(multiple_of_power_of_5(mv + 2, q));
    }
}

/// Returns the shortest digits in the interval and the count of digits removed.
///
/// This is step 4 of Ryu for the rare interval whose dropped digits may all
/// be zero, where a lower bound that is exactly representable counts as
/// inside and an exact tie rounds to the even digit.
fn trim_general(interval: Interval, accept_bounds: bool) -> (u64, i32) {
    let Interval {
        mut vr,
        mut vp,
        mut vm,
        mut vr_is_trailing_zeros,
        mut vm_is_trailing_zeros,
        ..
    } = interval;
    let mut removed = 0;
    let mut last_removed_digit = 0;
    while vp / 10 > vm / 10 {
        vm_is_trailing_zeros &= vm % 10 == 0;
        vr_is_trailing_zeros &= last_removed_digit == 0;
        last_removed_digit = vr % 10;
        vr /= 10;
        vp /= 10;
        vm /= 10;
        removed += 1;
    }
    if vm_is_trailing_zeros {
        while vm % 10 == 0 {
            vr_is_trailing_zeros &= last_removed_digit == 0;
            last_removed_digit = vr % 10;
            vr /= 10;
            vm /= 10;
            removed += 1;
        }
    }
    if vr_is_trailing_zeros && last_removed_digit == 5 && vr % 2 == 0 {
        // The exact value ends in 50...0, so it rounds to the even digit.
        last_removed_digit = 4;
    }
    let outside = vr == vm && (!accept_bounds || !vm_is_trailing_zeros);
    (vr + u64::from(outside || last_removed_digit >= 5), removed)
}

/// Returns the shortest digits in the interval and the count of digits removed.
///
/// This is step 4 of Ryu for the common interval, about 99.3 percent of all
/// doubles, where no tie can occur, so the dropped digits decide the rounding
/// alone.
fn trim_common(interval: Interval) -> (u64, i32) {
    let Interval {
        mut vr,
        mut vp,
        mut vm,
        ..
    } = interval;
    let mut removed = 0;
    let mut round_up = false;
    if vp / 100 > vm / 100 {
        round_up = vr % 100 >= 50;
        vr /= 100;
        vp /= 100;
        vm /= 100;
        removed += 2;
    }
    while vp / 10 > vm / 10 {
        round_up = vr % 10 >= 5;
        vr /= 10;
        vp /= 10;
        vm /= 10;
        removed += 1;
    }
    (vr + u64::from(vr == vm || round_up), removed)
}

/// Returns the multiplier and the shift the binary exponent `e2` needs.
///
/// For `e2` at or above zero the value is divided by `10^q` through the
/// inverse of `5^q`, with `q` of `max(0, floor(log10(2^e2)) - 1)`. Below zero
/// it is multiplied by `5^(-e2 - q)`, with `q` of
/// `max(0, floor(log10(5^-e2)) - 1)`.
const fn scale(e2: i32) -> Scale {
    if e2 >= 0 {
        let q = log10_pow2(e2.cast_unsigned()) - (e2 > 3) as u32;
        let k = MULTIPLIER_BITS + pow5bits(q).cast_signed() - 1;
        Scale {
            q,
            inverse: true,
            index: q,
            shift: -e2 + q.cast_signed() + k,
        }
    } else {
        let q = log10_pow5(e2.unsigned_abs()) - (e2 < -1) as u32;
        let index = e2.unsigned_abs() - q;
        let k = pow5bits(index).cast_signed() - MULTIPLIER_BITS;
        Scale {
            q,
            inverse: false,
            index,
            shift: q.cast_signed() - k,
        }
    }
}

/// Returns `floor(log10(2^e))` for `e` up to 1650.
///
/// The product is exact up to that bound, and [`check_exponents`] holds every
/// `e` a double reaches below it.
const fn log10_pow2(e: u32) -> u32 {
    (e * 78_913) >> 18
}

/// Returns `floor(log10(5^e))` for `e` up to 2620.
///
/// The product is exact up to that bound, and [`check_exponents`] holds every
/// `e` a double reaches below it.
const fn log10_pow5(e: u32) -> u32 {
    (e * 732_923) >> 20
}

/// Returns bits 64 to 127 of `m` times the multiplier, shifted right by `shift - 64`.
fn mul_shift(m: u64, multiplier: (u64, u64), shift: u32) -> u64 {
    let low = u128::from(m) * u128::from(multiplier.0);
    let high = u128::from(m) * u128::from(multiplier.1);
    super::low64(((low >> 64) + high) >> (shift - 64))
}

/// Returns whether `value` is a multiple of `5^p`.
///
/// A multiple of 5 times the inverse of 5 modulo 2^64 is the quotient, which
/// is at most `u64::MAX / 5`, and any other value lands above it.
fn multiple_of_power_of_5(mut value: u64, p: u32) -> bool {
    let mut i = 0;
    while i < p {
        value = value.wrapping_mul(INV_5);
        if value > u64::MAX / 5 {
            return false;
        }
        i += 1;
    }
    true
}

/// Checks that every exponent of a double stays within the table and the
/// integer widths [`shortest`] relies on, and records the entries it asks for.
///
/// This runs when the crate compiles and never at run time. The subnormal
/// exponent 0 and the 2046 normal exponents are all the exponents a finite
/// double has. The two logarithms stay within their exact range, every shift
/// of [`mul_shift`] lies from 64 to 191, and every index lies below the count
/// of its table. The indices the exponents visit are the powers `5^1` to
/// `5^325` and the inverses `5^-0` to `5^-290`, each of them and no other, so
/// a double asks for 325 powers and 291 inverses.
const fn check_exponents() {
    let mut powers = [false; POW5_COUNT as usize];
    let mut inverses = [false; INV_COUNT as usize];
    let mut ieee_exponent = 0;
    while ieee_exponent < 2047 {
        let biased = if ieee_exponent == 0 { 1 } else { ieee_exponent };
        let e2 = biased - BIAS - MANTISSA_BITS.cast_signed() - 2;
        assert!(
            e2 <= 1650 && e2 >= -2620,
            "a logarithm leaves its exact range"
        );
        let scale = scale(e2);
        assert!(
            scale.shift >= 64 && scale.shift < 192,
            "a shift leaves the product"
        );
        if scale.inverse {
            assert!(scale.index < INV_COUNT, "an inverse lies beyond the table");
            inverses[scale.index as usize] = true;
        } else {
            assert!(scale.index < POW5_COUNT, "a power lies beyond the table");
            powers[scale.index as usize] = true;
        }
        ieee_exponent += 1;
    }
    assert!(!powers[0], "a double asks for the power 5^0");
    let mut i = 1;
    while i < POW5_COUNT as usize {
        assert!(powers[i], "the table holds a power no double asks for");
        i += 1;
    }
    let mut i = 0;
    while i < INV_COUNT as usize {
        assert!(inverses[i], "the table holds an inverse no double asks for");
        i += 1;
    }
}

const _: () = check_exponents();
