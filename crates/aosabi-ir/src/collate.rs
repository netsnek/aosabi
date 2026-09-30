// SPDX-License-Identifier: MIT
/*
 * aosabi-ir: the collation of the `implements` sort, the ICU root collation
 * restricted to the alphabet of a type name.
 *
 * No file of the reference is translated here.
 */

//! The collation of the printer.
//!
//! The one sort that touches the bytes of a printed schema compares two
//! interface names with `String.prototype.localeCompare` and no further
//! argument. That call resolves to the root collation of ICU under an English
//! or a C process locale, at tertiary strength and with punctuation not
//! ignored, and it is not the order of the code units. Aosabi ships that
//! collation for the alphabet `[_0-9A-Za-z]` and for nothing else, because
//! the names the inference produces live there, and [`collate()`] refuses a
//! name that leaves the alphabet rather than guess a weight.
//!
//! # The rule
//!
//! Inside the alphabet the root collation reduces to two levels.
//!
//! 1. The primary level. `_` sorts below the digits `0` to `9`, the digits
//!    sort below the letters, and a letter has one primary weight for both
//!    of its cases. The two primary sequences are compared position by
//!    position, and a proper prefix sorts first.
//! 2. The tertiary level, which decides only when the primary sequences are
//!    equal. The two names then have the same length and differ in case
//!    alone, and the first position that differs decides, the lower case
//!    letter before the upper case one.
//!
//! The secondary level compares accents, and no unit of the alphabet carries
//! one. A run of digits is compared digit by digit and never by its value,
//! because the numeric option of a collator is off unless a caller asks for
//! it.
//!
//! The judging notebook is `tests/notebooks/15_sdl_printer.ipynb`, which
//! drives `aosabi-devtool collate` against node over the name pairs of
//! `tests/capture/15_corpus/`.
//!
//! [collator]: https://tc39.es/ecma402/#sec-intl.collator

use alloc::string::String;
use core::{
    cmp::Ordering,
    fmt, //
};

/// The refusal of a name that leaves the collation alphabet.
///
/// The error carries the reason and the message is built in one place, its
/// `Display`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollateError {
    /// The whole name that was refused.
    pub name: String,
    /// The offset of the first unit outside the alphabet, counted in UTF-16
    /// code units, which is how the reference counts a string position.
    pub offset: usize,
}

impl fmt::Display for CollateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the type name \"{}\" leaves the collation alphabet [_0-9A-Za-z] at UTF-16 offset {}",
            self.name, self.offset
        )
    }
}

impl core::error::Error for CollateError {}

/// Compares two type names the way `a.localeCompare(b)` orders them under
/// the root collation.
///
/// The reference returns a negative number, zero or a positive number and
/// only the sign reaches the sort, so the answer is an [`Ordering`]. `a` is
/// checked before `b`, so when both names leave the alphabet the error names
/// `a`.
///
/// # Errors
///
/// Returns a [`CollateError`] when `a` or `b` holds a UTF-16 code unit
/// outside `[_0-9A-Za-z]`.
pub fn collate(a: &str, b: &str) -> Result<Ordering, CollateError> {
    check_name(a)?;
    check_name(b)?;
    Ok(compare(a, b))
}

/// Checks that every UTF-16 code unit of `name` lies inside the alphabet.
///
/// # Errors
///
/// Returns a [`CollateError`] that carries `name` and the offset of the
/// first code unit outside `[_0-9A-Za-z]`.
pub(crate) fn check_name(name: &str) -> Result<(), CollateError> {
    match name
        .encode_utf16()
        .position(|unit| primary_weight(unit).is_none())
    {
        Some(offset) => Err(CollateError {
            name: String::from(name),
            offset,
        }),
        None => Ok(()),
    }
}

/// Compares two names that [`check_name`] accepts.
///
/// The comparison is a total order over every pair of strings, so a sort
/// that runs on it never meets an inconsistent answer. A unit outside the
/// alphabet has no primary weight and sorts below every unit that has one,
/// which no caller relies on, because each caller checks its names first.
pub(crate) fn compare(a: &str, b: &str) -> Ordering {
    let primary = a
        .encode_utf16()
        .map(primary_weight)
        .cmp(b.encode_utf16().map(primary_weight));
    if primary != Ordering::Equal {
        return primary;
    }
    a.encode_utf16()
        .map(tertiary_weight)
        .cmp(b.encode_utf16().map(tertiary_weight))
}

/// Returns the primary weight of a UTF-16 code unit, or `None` for a unit
/// outside `[_0-9A-Za-z]`.
///
/// The weights are the crate's own numbers and only their order carries
/// meaning: `_` is 0, the digits are 1 to 10, and a letter is 11 plus its
/// position in the Latin alphabet, the same for both cases.
fn primary_weight(unit: u16) -> Option<u8> {
    let byte = u8::try_from(unit).ok()?;
    match byte {
        b'_' => Some(0),
        b'0'..=b'9' => Some(1 + (byte - b'0')),
        b'a'..=b'z' => Some(11 + (byte - b'a')),
        b'A'..=b'Z' => Some(11 + (byte - b'A')),
        _ => None,
    }
}

/// Returns the tertiary weight of a UTF-16 code unit, which sorts a lower
/// case letter before its upper case form.
///
/// Two units that share a primary weight and differ are the two cases of one
/// letter, so the flag decides between them. The unit itself follows the
/// flag only to keep the comparison total over units outside the alphabet.
fn tertiary_weight(unit: u16) -> (bool, u16) {
    let upper = (u16::from(b'A')..=u16::from(b'Z')).contains(&unit);
    (upper, unit)
}
