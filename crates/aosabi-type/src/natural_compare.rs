// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the natural sort order of two strings, which orders a run of
 * digits by its value.
 *
 * Derived from graphql-js 16.12.0, src/jsutils/naturalCompare.ts, MIT.
 */

//! The natural comparison of the type layer.
//!
//! [`natural_compare()`] is the tie breaker of the suggestion list and the
//! comparator of the lexicographic schema sort. The reference reads a
//! string through `charCodeAt`, which returns UTF-16 code units and `NaN`
//! past the end, so the port walks the code units and reads past the end as
//! `None`.

use alloc::vec::Vec;
use core::cmp::Ordering;

/// The code of `0`, the first decimal digit.
const DIGIT_0: u16 = 48;

/// The code of `9`, the last decimal digit.
const DIGIT_9: u16 = 57;

/// Returns whether a reference string comes before, or after, or is the
/// same as the given string in natural sort order.
///
/// `a_str` and `b_str` are compared code unit by code unit, and where both
/// hold a digit the two runs of digits are compared by value. The reference
/// returns a negative, zero or positive number and only its sign reaches a
/// sort, so the answer is an [`Ordering`]. A run accumulates in a double,
/// as `aNum * 10 + aChar - DIGIT_0` does, and the loop over a run stops
/// after a leading zero, because the accumulator is still zero there.
///
/// Derived from graphql-js 16.12.0, src/jsutils/naturalCompare.ts, MIT.
#[must_use]
pub fn natural_compare(a_str: &str, b_str: &str) -> Ordering {
    let a_str: Vec<u16> = a_str.encode_utf16().collect();
    let b_str: Vec<u16> = b_str.encode_utf16().collect();
    let mut a_index = 0;
    let mut b_index = 0;

    while a_index < a_str.len() && b_index < b_str.len() {
        let mut a_char = char_code_at(&a_str, a_index);
        let mut b_char = char_code_at(&b_str, b_index);

        if is_digit(a_char) && is_digit(b_char) {
            let mut a_num = 0.0;
            loop {
                a_index += 1;
                a_num = a_num * 10.0 + digit_value(a_char);
                a_char = char_code_at(&a_str, a_index);
                if !(is_digit(a_char) && a_num > 0.0) {
                    break;
                }
            }

            let mut b_num = 0.0;
            loop {
                b_index += 1;
                b_num = b_num * 10.0 + digit_value(b_char);
                b_char = char_code_at(&b_str, b_index);
                if !(is_digit(b_char) && b_num > 0.0) {
                    break;
                }
            }

            if a_num < b_num {
                return Ordering::Less;
            }

            if a_num > b_num {
                return Ordering::Greater;
            }
        } else {
            if a_char < b_char {
                return Ordering::Less;
            }
            if a_char > b_char {
                return Ordering::Greater;
            }
            a_index += 1;
            b_index += 1;
        }
    }

    a_str.len().cmp(&b_str.len())
}

/// Returns `str.charCodeAt(index)`, which is `None` where the reference
/// reads `NaN` past the end.
fn char_code_at(units: &[u16], index: usize) -> Option<u16> {
    units.get(index).copied()
}

/// Returns `aChar - DIGIT_0` in the double arithmetic of the reference,
/// which is `NaN` for a read past the end.
fn digit_value(code: Option<u16>) -> f64 {
    code.map_or(f64::NAN, |code| f64::from(code) - f64::from(DIGIT_0))
}

/// Returns whether a code unit is a decimal digit.
///
/// The `isNaN` test of the reference is the `None` of a read past the end.
///
/// Derived from graphql-js 16.12.0, src/jsutils/naturalCompare.ts, MIT.
fn is_digit(code: Option<u16>) -> bool {
    code.is_some_and(|code| (DIGIT_0..=DIGIT_9).contains(&code))
}
