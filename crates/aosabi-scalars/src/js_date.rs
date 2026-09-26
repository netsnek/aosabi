// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the JavaScript Date the date time scalar reads and writes.
 *
 * Translation of the ECMA-262 clauses the reference reaches through
 * `new Date(value)`, `Date.parse`, `Date.prototype.getTime` and
 * `Date.prototype.toISOString`, read at https://tc39.es/ecma262/ under the
 * anchor each item names, and of the reading V8 gives the one string shape
 * the scalar hands to the constructor.
 */

//! The `Date` instance behind the date time scalar.
//!
//! A `Date` holds one time value, a count of milliseconds since the epoch or
//! NaN for an invalid date, and the count is an integer within 8.64e15 of
//! zero on either side. The scalar creates a date from a number, from a
//! string and reads it back through `toISOString`, and those are the three
//! operations here. The text of `toISOString` is written by one writer, which
//! hands it out as a string or appends its bytes where a caller writes JSON.

use alloc::{string::String, vec::Vec};

use crate::error::RangeError;

/// The message of the `RangeError` that `toISOString` throws.
pub const INVALID_TIME_VALUE: &str = "Invalid time value";

/// `msPerDay` of <https://tc39.es/ecma262/#sec-day-number-and-time-within-day>.
const MS_PER_DAY: i64 = 86_400_000;

/// `msPerHour` of <https://tc39.es/ecma262/#sec-hours-minutes-second-and-milliseconds>.
const MS_PER_HOUR: i64 = 3_600_000;

/// `msPerMinute` of the same clause.
const MS_PER_MINUTE: i64 = 60_000;

/// `msPerSecond` of the same clause.
const MS_PER_SECOND: i64 = 1_000;

/// The bound of `TimeClip`, in milliseconds either side of the epoch.
const TIME_RANGE: f64 = 8.64e15;

/// A `Date` instance.
///
/// The one field is the `[[DateValue]]` slot: a time value, or NaN for an
/// invalid date.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JsDate {
    date_value: f64,
}

impl JsDate {
    /// Creates the date `new Date(value)` makes from a Number.
    ///
    /// The constructor with one argument that is not a `Date` and not a
    /// string converts it with `ToNumber` and stores `TimeClip` of the
    /// result.
    #[must_use]
    pub fn from_time_value(value: f64) -> Self {
        Self {
            date_value: time_clip(value),
        }
    }

    /// Creates the date `new Date(value)` makes from a string.
    ///
    /// The constructor parses a string argument "in exactly the same manner
    /// as for the `parse` method" and stores `TimeClip` of the result. The
    /// reading is [`date_parse()`].
    #[must_use]
    pub fn parse(text: &str) -> Self {
        Self {
            date_value: date_parse(text),
        }
    }

    /// Returns `getTime()`, the time value.
    ///
    /// The value is NaN for an invalid date.
    #[must_use]
    pub const fn get_time(self) -> f64 {
        self.date_value
    }

    /// Returns `toISOString()`.
    ///
    /// The result is the Date Time String Format with every field present and
    /// the `Z` suffix, and a year outside 0000 to 9999 is written in the
    /// expanded form of a sign and six digits. The fields come from the time
    /// value through `Day`, `TimeWithinDay` and the civil calendar clauses,
    /// all in UTC.
    ///
    /// # Errors
    ///
    /// Returns the [`RangeError`] of the clause when the time value is not
    /// finite, with the message [`INVALID_TIME_VALUE`].
    pub fn to_iso_string(self) -> Result<String, RangeError> {
        let text = self
            .iso_text()
            .ok_or_else(|| RangeError::new(INVALID_TIME_VALUE))?;
        Ok(text
            .as_bytes()
            .iter()
            .map(|byte| char::from(*byte))
            .collect())
    }

    /// Appends the text of `toISOString()` to `out`, the bytes of
    /// [`JsDate::to_iso_string`] with no string made, and answers whether it
    /// did.
    ///
    /// The text is ASCII, digits, the signs, `T`, `:`, `.` and `Z`, so it is
    /// its own UTF-8 and its own escaped JSON string. An invalid date, whose
    /// `toISOString` throws, appends nothing and answers `false`, with no
    /// error made.
    #[must_use]
    pub fn write_iso_string(self, out: &mut Vec<u8>) -> bool {
        let Some(text) = self.iso_text() else {
            return false;
        };
        out.extend_from_slice(text.as_bytes());
        true
    }

    /// Returns the text of `toISOString()`, from the time value through
    /// `Day`, `TimeWithinDay` and the civil calendar clauses, all in UTC,
    /// and none for a time value that is not finite.
    fn iso_text(self) -> Option<IsoText> {
        if !self.date_value.is_finite() {
            return None;
        }
        // TimeClip made the value an integer within 8.64e15 of zero, which an
        // i64 holds exactly.
        #[allow(clippy::cast_possible_truncation)]
        let time = self.date_value as i64;
        let day = time.div_euclid(MS_PER_DAY);
        let time_within_day = time.rem_euclid(MS_PER_DAY);
        let (year, month, date) = civil_from_days(day);
        let mut text = IsoText::default();
        text.year(year);
        text.push(b'-');
        text.digits(month.unsigned_abs(), 2);
        text.push(b'-');
        text.digits(date.unsigned_abs(), 2);
        text.push(b'T');
        text.digits((time_within_day / MS_PER_HOUR).unsigned_abs(), 2);
        text.push(b':');
        text.digits((time_within_day / MS_PER_MINUTE % 60).unsigned_abs(), 2);
        text.push(b':');
        text.digits((time_within_day / MS_PER_SECOND % 60).unsigned_abs(), 2);
        text.push(b'.');
        text.digits((time_within_day % MS_PER_SECOND).unsigned_abs(), 3);
        text.push(b'Z');
        Some(text)
    }
}

/// The most bytes the text of `toISOString()` takes, a sign and six digits of
/// an expanded year and the twenty bytes after the year.
const ISO_TEXT_MAX: usize = 27;

/// The text of `toISOString()` of a valid date, in a buffer of its own.
struct IsoText {
    /// The bytes written so far.
    bytes: [u8; ISO_TEXT_MAX],
    /// The count of the bytes written.
    len: usize,
}

impl Default for IsoText {
    fn default() -> Self {
        Self {
            bytes: [0; ISO_TEXT_MAX],
            len: 0,
        }
    }
}

impl IsoText {
    /// Returns the bytes written.
    fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..self.len).unwrap_or_default()
    }

    /// Appends the byte `byte`.
    ///
    /// The longest text fills the buffer, so no byte is ever dropped.
    fn push(&mut self, byte: u8) {
        if let Some(slot) = self.bytes.get_mut(self.len) {
            *slot = byte;
            self.len += 1;
        }
    }

    /// Appends the last `width` decimal digits of `value`, padded with zeros.
    fn digits(&mut self, value: u64, width: u32) {
        let mut divisor = 10_u64.pow(width.saturating_sub(1));
        while divisor > 0 {
            self.push(b'0' + u8::try_from(value / divisor % 10).unwrap_or_default());
            divisor /= 10;
        }
    }

    /// Appends the year the way `toISOString` writes it.
    ///
    /// Four digits inside 0000 to 9999, and otherwise a sign and six digits.
    fn year(&mut self, year: i64) {
        if (0..=9999).contains(&year) {
            self.digits(year.unsigned_abs(), 4);
            return;
        }
        self.push(if year < 0 { b'-' } else { b'+' });
        self.digits(year.unsigned_abs(), 6);
    }
}

/// Returns `TimeClip(time)`.
///
/// A value that is not finite or lies more than 8.64e15 from zero is NaN, and
/// every other value is truncated toward zero, which turns a negative zero
/// into a positive one.
#[must_use]
pub fn time_clip(time: f64) -> f64 {
    if !time.is_finite() || time.abs() > TIME_RANGE {
        return f64::NAN;
    }
    // The value lies within 8.64e15 of zero, so an i64 holds it, and the
    // conversion truncates toward zero the way ToIntegerOrInfinity does.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let clipped = (time as i64) as f64;
    if clipped == 0.0 { 0.0 } else { clipped }
}

/// Returns `Date.parse(text)` for the strings the date time scalar reads.
///
/// The scalar hands the constructor a string only after the RFC 3339 test of
/// the validator accepted it, so the shape is `YYYY-MM-DDTHH:mm:ss`, an
/// optional fraction and either `Z` or a signed `HH:mm` offset. That shape is
/// the Date Time String Format of ECMA-262 with the three readings V8 adds to
/// it. The separator and the `Z` may be lower case, because the reader
/// matches letters without regard to case. A fraction may carry any count of
/// digits, and its first three digits are the milliseconds, right padded with
/// zeros. A day past the end of its month rolls over into the next month
/// rather than failing. A field outside its range, a second of `60`, a sign
/// that is neither `+` nor `-` and any other shape are NaN here, where V8
/// would try its legacy reader, which fails on those shapes too.
#[must_use]
pub fn date_parse(text: &str) -> f64 {
    let Some(time) = parse_iso(text.as_bytes()) else {
        return f64::NAN;
    };
    // A four digit year keeps the value below 2^53, so the conversion is
    // exact and TimeClip decides the range.
    #[allow(clippy::cast_precision_loss)]
    time_clip(time as f64)
}

/// Reads the ISO shape into a time value, or nothing when it does not fit.
fn parse_iso(bytes: &[u8]) -> Option<i64> {
    let mut reader = Reader { bytes, pos: 0 };
    let year = reader.digits(4)?;
    reader.expect(b'-')?;
    let month = reader.digits(2)?;
    reader.expect(b'-')?;
    let date = reader.digits(2)?;
    reader.expect_letter(b'T')?;
    let hour = reader.digits(2)?;
    reader.expect(b':')?;
    let minute = reader.digits(2)?;
    reader.expect(b':')?;
    let second = reader.digits(2)?;
    let millisecond = reader.milliseconds()?;
    let offset = reader.offset()?;
    if !reader.done() {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&date) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let day = make_day(year, month - 1, date);
    let time = make_time(hour, minute, second, millisecond);
    Some(day * MS_PER_DAY + time - offset * MS_PER_MINUTE)
}

/// A cursor over the bytes of the string being read.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    /// Reads exactly `count` ASCII digits as a number.
    fn digits(&mut self, count: usize) -> Option<i64> {
        let end = self.pos.checked_add(count)?;
        let slice = self.bytes.get(self.pos..end)?;
        if !slice.iter().all(u8::is_ascii_digit) {
            return None;
        }
        self.pos = end;
        Some(
            slice
                .iter()
                .fold(0, |acc, byte| acc * 10 + i64::from(byte - b'0')),
        )
    }

    /// Consumes `byte` when it is next.
    fn expect(&mut self, byte: u8) -> Option<()> {
        if self.bytes.get(self.pos) == Some(&byte) {
            self.pos += 1;
            return Some(());
        }
        None
    }

    /// Consumes the letter `upper` in either case when it is next.
    fn expect_letter(&mut self, upper: u8) -> Option<()> {
        let next = *self.bytes.get(self.pos)?;
        if next.to_ascii_uppercase() != upper {
            return None;
        }
        self.pos += 1;
        Some(())
    }

    /// Reads the optional fraction and returns the milliseconds it names.
    ///
    /// A missing fraction is zero milliseconds, and a point with no digit
    /// after it is a failure.
    fn milliseconds(&mut self) -> Option<i64> {
        if self.expect(b'.').is_none() {
            return Some(0);
        }
        let start = self.pos;
        while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        let mut value = 0;
        for index in 0..3 {
            let digit = self
                .bytes
                .get(start + index)
                .map_or(0, |byte| i64::from(byte - b'0'));
            value = value * 10 + digit;
        }
        Some(value)
    }

    /// Reads the zone designator and returns its offset in minutes.
    fn offset(&mut self) -> Option<i64> {
        if self.expect_letter(b'Z').is_some() {
            return Some(0);
        }
        let sign = match self.bytes.get(self.pos) {
            Some(b'+') => 1,
            Some(b'-') => -1,
            _ => return None,
        };
        self.pos += 1;
        let hours = self.digits(2)?;
        self.expect(b':')?;
        let minutes = self.digits(2)?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        Some(sign * (hours * 60 + minutes))
    }

    /// Returns whether every byte is consumed.
    fn done(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

/// Returns `MakeDay(year, month, date)` for a month already within 0 to 11.
///
/// The clause finds the day number of the first of the month and adds
/// `date - 1`, so a date past the end of the month rolls forward.
fn make_day(year: i64, month: i64, date: i64) -> i64 {
    days_from_civil(year, month + 1) + date - 1
}

/// Returns `MakeTime(hour, min, sec, ms)`.
const fn make_time(hour: i64, minute: i64, second: i64, millisecond: i64) -> i64 {
    hour * MS_PER_HOUR + minute * MS_PER_MINUTE + second * MS_PER_SECOND + millisecond
}

/// Returns the day number of the first of `month` in `year`.
///
/// The proleptic Gregorian calendar of ECMA-262 is computed in closed form by
/// counting from the first of March in eras of 400 years, which is the
/// algorithm of <https://howardhinnant.github.io/date_algorithms.html#days_from_civil>
/// and is exact for every integer year. `month` is 1 to 12.
fn days_from_civil(year: i64, month: i64) -> i64 {
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = if shifted_year >= 0 {
        shifted_year
    } else {
        shifted_year - 399
    } / 400;
    let year_of_era = shifted_year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Returns the year, month and date of a day number.
///
/// The inverse of [`days_from_civil()`], the algorithm of
/// <https://howardhinnant.github.io/date_algorithms.html#civil_from_days>,
/// which yields `YearFromTime`, `MonthFromTime` plus one and `DateFromTime`
/// of the clause <https://tc39.es/ecma262/#sec-year-number> for every day
/// number in the time range.
fn civil_from_days(day: i64) -> (i64, i64, i64) {
    let shifted = day + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let date = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, date)
}
