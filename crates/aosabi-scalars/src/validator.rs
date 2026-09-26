// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the RFC 3339 checks of the date time scalar.
 *
 * The three regular expressions of that file are written out as readers over
 * the bytes of the string, one function per group of the expression, because
 * every class the expressions use is ASCII and a JavaScript expression
 * without the `u` flag reads one code unit at a time, so a byte that is not
 * ASCII matches nothing in either reading.
 *
 * Derived from graphql-scalars 1.25.0, ., MIT.
 */

//! The RFC 3339 validators of the date time scalar.

use aosabi_json::case::upper_ascii_exact;

use crate::js_date::{JsDate, date_parse};

/// Returns whether `year` is a leap year.
///
/// Every year that is exactly divisible by four is a leap year, except for
/// years that are exactly divisible by 100, but these centurial years are
/// leap years if they are exactly divisible by 400.
#[must_use]
pub const fn leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Returns whether a time-string is RFC 3339 compliant.
///
/// The string is upper cased and tested against
/// `^([01][0-9]|2[0-3]):([0-5][0-9]):([0-5][0-9])(\.\d{1,})?(([Z])|([+|-]([01][0-9]|2[0-3]):[0-5][0-9]))$`.
/// The sign class of the expression admits `|` beside `+` and `-`, and the
/// reader keeps that. `toUpperCase` is the Unicode default case conversion,
/// and the reader upper cases with [`upper_ascii_exact()`], which maps every
/// character whose upper case holds ASCII as the reference does and keeps the
/// others, whose upper case holds no ASCII. Every class of the expression is
/// ASCII, so the two upper cases match it alike.
#[must_use]
pub fn validate_time(time: &str) -> bool {
    let time = upper_ascii_exact(time);
    let mut scan = Scan::new(&time);
    scan.hour()
        && scan.literal(b':')
        && scan.sixty()
        && scan.literal(b':')
        && scan.sixty()
        && scan.fraction()
        && scan.zone()
        && scan.end()
}

/// Returns whether a date-string is RFC 3339 compliant.
///
/// The string is tested against
/// `^(\d{4}-(0[1-9]|1[012])-(0[1-9]|[12][0-9]|3[01]))$`, and then the day is
/// checked against the length of its month, with February read through
/// [`leap_year()`]. The three `substr` reads of the reference index code
/// units, and after the expression matched the string is ASCII, so a byte
/// index is the same index.
#[must_use]
pub fn validate_date(datestring: &str) -> bool {
    let mut scan = Scan::new(datestring);
    if !(scan.digits(4)
        && scan.literal(b'-')
        && scan.month()
        && scan.literal(b'-')
        && scan.day()
        && scan.end())
    {
        return false;
    }
    let bytes = datestring.as_bytes();
    let year = number_of(&bytes[0..4]);
    let month = number_of(&bytes[5..7]);
    let day = number_of(&bytes[8..10]);
    match month {
        2 => {
            // The reference checks the leap year and the common year in two
            // arms that fail past 29 and past 28, which is one bound.
            let last = if leap_year(year) { 29 } else { 28 };
            day <= last
        }
        4 | 6 | 9 | 11 => day <= 30,
        _ => true,
    }
}

/// Returns whether a date-time-string is RFC 3339 compliant.
///
/// The string is upper cased, tested against
/// `^(\d{4}-(0[1-9]|1[012])-(0[1-9]|[12][0-9]|3[01])T([01][0-9]|2[0-3]):([0-5][0-9]):([0-5][0-9]|60))(\.\d{1,})?(([Z])|([+|-]([01][0-9]|2[0-3]):[0-5][0-9]))$`,
/// then handed to `Date.parse`, which is [`date_parse()`], and a NaN there
/// fails it. The rest splits the upper cased string at its first `T` and
/// checks the two halves with [`validate_date()`] and [`validate_time()`].
/// The string is upper cased with [`upper_ascii_exact()`], as in
/// [`validate_time()`], and every read after the expression reads a string
/// the expression matched, which is ASCII and equal to the reference's upper
/// case.
#[must_use]
pub fn validate_date_time(date_time_string: &str) -> bool {
    let date_time_string = upper_ascii_exact(date_time_string);
    let mut scan = Scan::new(&date_time_string);
    if !(scan.digits(4)
        && scan.literal(b'-')
        && scan.month()
        && scan.literal(b'-')
        && scan.day()
        && scan.literal(b'T')
        && scan.hour()
        && scan.literal(b':')
        && scan.sixty()
        && scan.literal(b':')
        && scan.second_or_sixty()
        && scan.fraction()
        && scan.zone()
        && scan.end())
    {
        return false;
    }
    let time = date_parse(&date_time_string);
    if time.is_nan() {
        return false;
    }
    let Some(index) = date_time_string.find('T') else {
        return false;
    };
    let date_string = &date_time_string[..index];
    let time_string = &date_time_string[index + 1..];
    validate_date(date_string) && validate_time(time_string)
}

/// Returns whether a number is a valid Unix timestamp.
///
/// A Unix timestamp is a signed 32 bit integer count of seconds, and NaN
/// fails the comparison with itself.
#[must_use]
pub fn validate_unix_timestamp(timestamp: f64) -> bool {
    const MAX_INT: f64 = 2_147_483_647.0;
    const MIN_INT: f64 = -2_147_483_648.0;
    !timestamp.is_nan() && (MIN_INT..=MAX_INT).contains(&timestamp)
}

/// Returns whether a `Date` instance is valid.
///
/// The time value of an invalid date is NaN, which fails the comparison with
/// itself.
#[must_use]
pub fn validate_js_date(date: JsDate) -> bool {
    !date.get_time().is_nan()
}

/// Returns `Number(text)` for a run of ASCII digits.
fn number_of(digits: &[u8]) -> i64 {
    digits
        .iter()
        .fold(0, |acc, byte| acc * 10 + i64::from(byte - b'0'))
}

/// A reader over the bytes of the string an expression is tested against.
///
/// Each method is one group of the three expressions. The alternatives of
/// every group start with distinct bytes, and a greedy digit run is always
/// followed by a byte that is not a digit, so the reader never has to back
/// up and its answer is the answer of the expression.
struct Scan<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Scan<'a> {
    /// Starts at the first byte of `text`, the `^` anchor.
    const fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }

    /// Consumes the next byte when `accept` holds for it.
    fn take(&mut self, accept: impl Fn(u8) -> bool) -> bool {
        if self.bytes.get(self.pos).is_some_and(|byte| accept(*byte)) {
            self.pos += 1;
            return true;
        }
        false
    }

    /// One literal byte.
    fn literal(&mut self, byte: u8) -> bool {
        self.take(|next| next == byte)
    }

    /// One `\d`, which is an ASCII digit without the `u` flag.
    fn digit(&mut self) -> bool {
        self.take(|next| next.is_ascii_digit())
    }

    /// `\d{count}`.
    fn digits(&mut self, count: usize) -> bool {
        (0..count).all(|_| self.digit())
    }

    /// `$` without the `m` flag, which is the end of the input only.
    fn end(&self) -> bool {
        self.pos == self.bytes.len()
    }

    /// `([01][0-9]|2[0-3])`.
    fn hour(&mut self) -> bool {
        if self.take(|next| next == b'0' || next == b'1') {
            return self.digit();
        }
        self.literal(b'2') && self.take(|next| (b'0'..=b'3').contains(&next))
    }

    /// `[0-5][0-9]`.
    fn sixty(&mut self) -> bool {
        self.take(|next| (b'0'..=b'5').contains(&next)) && self.digit()
    }

    /// `([0-5][0-9]|60)`.
    fn second_or_sixty(&mut self) -> bool {
        if self.literal(b'6') {
            return self.literal(b'0');
        }
        self.sixty()
    }

    /// `(0[1-9]|1[012])`.
    fn month(&mut self) -> bool {
        if self.literal(b'0') {
            return self.take(|next| (b'1'..=b'9').contains(&next));
        }
        self.literal(b'1') && self.take(|next| (b'0'..=b'2').contains(&next))
    }

    /// `(0[1-9]|[12][0-9]|3[01])`.
    fn day(&mut self) -> bool {
        if self.literal(b'0') {
            return self.take(|next| (b'1'..=b'9').contains(&next));
        }
        if self.take(|next| next == b'1' || next == b'2') {
            return self.digit();
        }
        self.literal(b'3') && self.take(|next| next == b'0' || next == b'1')
    }

    /// `(\.\d{1,})?`.
    ///
    /// A point that no digit follows leaves the group empty, and the zone
    /// group cannot start at a point, so that case fails here directly.
    fn fraction(&mut self) -> bool {
        if !self.literal(b'.') {
            return true;
        }
        if !self.digit() {
            return false;
        }
        while self.digit() {}
        true
    }

    /// `(([Z])|([+|-]([01][0-9]|2[0-3]):[0-5][0-9]))`.
    fn zone(&mut self) -> bool {
        if self.literal(b'Z') {
            return true;
        }
        self.take(|next| next == b'+' || next == b'|' || next == b'-')
            && self.hour()
            && self.literal(b':')
            && self.sixty()
    }
}
