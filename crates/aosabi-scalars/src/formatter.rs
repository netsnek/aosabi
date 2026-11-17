// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the reading of a date-time-string into a Date.
 *
 * Derived from graphql-scalars 1.25.0, MIT.
 *
 * The function parseDateTime of graphql-scalars is new Date(dateTime), here
 * over the reader of JsDate, and the one function of the formatter a bound
 * scalar reaches, while the others serve the Date, Time and LocalTime
 * scalars. The doc comment of parse_date_time is the comment of that
 * function as it reads.
 *
 * Derived from graphql-scalars 1.25.0, esm/scalars/iso-date/formatter.js, MIT.
 */

//! The formatter of the date time scalar.

use crate::js_date::JsDate;

/// Parses an RFC 3339 compliant date-time-string into a `Date`.
///
/// It is `new Date(dateTime)` on the string as given, not on the upper
/// cased copy the validator tested, so a lower case separator or zone
/// letter reaches the constructor and [`JsDate::parse()`] reads it without
/// regard to case.
#[must_use]
pub fn parse_date_time(date_time: &str) -> JsDate {
    JsDate::parse(date_time)
}
