// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the DateTimeISO scalar, which Aosabi binds as Date.
 *
 * The configuration spreads the DateTime configuration and replaces the
 * name, the description and serialize, so parseValue and parseLiteral are
 * the DateTime functions. Aosabi takes the three hooks and gives the scalar
 * its own name and description.
 *
 * Derived from graphql-scalars 1.25.0, ., MIT.
 */

//! The `DateTimeISO` scalar.

use aosabi_json::{JsObject, JsValue};
use aosabi_lang::ast::ValueNode;

use crate::{ScalarResult, ScalarValue, date_time};

/// The `name` of the configuration, DateTimeISO.js:5.
pub const NAME: &str = "DateTimeISO";

/// The `description` of the configuration, DateTimeISO.js:6-10.
///
/// The reference builds it from five string pieces joined by `+`, and the
/// value here is their concatenation, including the missing space between
/// `calendar.` and `This`.
pub const DESCRIPTION: &str = "A date-time string at UTC, such as 2007-12-03T10:15:30Z, compliant with the `date-time` format outlined in section 5.6 of the RFC 3339 profile of the ISO 8601 standard for representation of dates and times using the Gregorian calendar.This scalar is serialized to a string in ISO 8601 format and parsed from a string in ISO 8601 format.";

/// Returns `serialize(value)`, an ISO 8601 string.
///
/// The `DateTime` serialize, then `toISOString` of the date it returns.
///
/// # Errors
///
/// The errors of [`date_time::serialize()`], and the `RangeError` of
/// `toISOString` when the date is invalid, which happens for a number
/// outside the time range, because a number becomes a date without a
/// validity check.
pub fn serialize(value: &ScalarValue) -> ScalarResult {
    let date = date_time::serialize(value)?;
    Ok(ScalarValue::Value(JsValue::from(date.to_iso_string()?)))
}

/// Returns `parseValue(value)`, a `Date`.
///
/// The spread of DateTimeISO.js:4 makes this [`date_time::parse_value()`].
///
/// # Errors
///
/// The errors of [`date_time::parse_value()`].
pub fn parse_value(value: &ScalarValue) -> ScalarResult {
    Ok(ScalarValue::Date(date_time::parse_value(value)?))
}

/// Returns `parseLiteral(ast)`, a `Date`.
///
/// The spread of DateTimeISO.js:4 makes this [`date_time::parse_literal()`],
/// which reads no variable, so `variables` is unused the way the JavaScript
/// function leaves its second argument unused.
///
/// # Errors
///
/// The errors of [`date_time::parse_literal()`].
pub fn parse_literal(ast: &ValueNode, _variables: Option<&JsObject>) -> ScalarResult {
    Ok(ScalarValue::Date(date_time::parse_literal(ast)?))
}

/// Returns the `extensions` of the configuration.
///
/// The spread of DateTimeISO.js:4 carries the `DateTime` extensions
/// unchanged.
#[must_use]
pub fn extensions() -> JsObject {
    date_time::extensions()
}
