// SPDX-License-Identifier: MIT
/*
 * aosabi-execute value to_string: the string `String(value)` answers.
 *
 * Translation of the ECMA-262 abstract operation ToString,
 * https://tc39.es/ecma262/#sec-tostring, and of the `toString` methods it
 * reaches through OrdinaryToPrimitive for an array, a date, an error, a
 * function and a plain object.
 */

//! ECMA-262 `ToString` over the executor value.
//!
//! The bound `ID` scalar serializes with `String(value)` whatever
//! the value is, and the `Number` scalar writes the refused value into its
//! message the same way, so the conversion is part of the byte contract for
//! every variant.

use aosabi_json::{
    JsString,
    number_to_string, //
};
use aosabi_lang::js_string::JsStringBuilder;
use aosabi_scalars::JsDate;

use crate::value::{
    HostValue,
    Value, //
};

/// The day names of the `DateString` clause, from Sunday on.
const WEEK_DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The month names of the `DateString` clause.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `msPerDay` of <https://tc39.es/ecma262/#sec-day-number-and-time-within-day>.
const MS_PER_DAY: f64 = 86_400_000.0;

/// Returns `ToString(value)`, which is what `String(value)` answers.
///
/// The table of the clause maps `undefined`, `null`, a boolean, a number and
/// a string directly. Every other value goes through `ToPrimitive` with hint
/// string, which calls its `toString` method. For an array that is `join`
/// with the comma separator, where an item that is `undefined` or `null`
/// contributes the empty string. For an object and a promise it is
/// `Object.prototype.toString`, which answers the tag. A function answers
/// the form the clause fixes for a function without source text. A user
/// object that overrides `toString` has no reading in the model.
#[must_use]
pub fn to_js_string(value: &Value) -> JsString {
    match value {
        Value::Undefined => JsString::from("undefined"),
        Value::Null => JsString::from("null"),
        Value::Bool(true) => JsString::from("true"),
        Value::Bool(false) => JsString::from("false"),
        Value::Number(number) => JsString::from(number_to_string(*number)),
        Value::String(string) => string.clone(),
        Value::Date(date) => JsString::from(date_to_string(*date)),
        Value::List(items) => array_join(items),
        Value::Object(_) => JsString::from("[object Object]"),
        Value::Function(function) => {
            JsString::from(format!("function {}() {{ [native code] }}", function.name))
        }
        Value::Promise(_) => JsString::from("[object Promise]"),
        Value::Error(error) => JsString::from(error.to_js_string()),
        Value::Host(host) => JsString::from(host_to_string(host)),
    }
}

/// Returns `Array.prototype.join` of `items` with the comma separator.
fn array_join(items: &[Value]) -> JsString {
    let mut out = JsStringBuilder::new();
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(",");
        }
        if !item.is_nullish() {
            out.push(&to_js_string(item));
        }
    }
    out.finish()
}

/// Returns `toString()` of a host value.
///
/// A type answers its name with its wrappers and a directive its name after
/// an `@`. A schema carries the tag `GraphQLSchema`, and the other host
/// values are plain objects.
fn host_to_string(host: &HostValue) -> String {
    match host {
        HostValue::Type(named) => named.to_string(),
        HostValue::Directive(directive) => directive.to_string(),
        HostValue::Schema(_) => String::from("[object GraphQLSchema]"),
        _ => String::from("[object Object]"),
    }
}

/// Returns `Date.prototype.toString` for a date in the time zone UTC.
///
/// The clause reads the local time zone. Aosabi has no time zone database
/// and answers for UTC, and the name in parentheses, which the clause leaves
/// to the implementation, is the one V8 writes for UTC.
fn date_to_string(date: JsDate) -> String {
    let Ok(iso) = date.to_iso_string() else {
        return String::from("Invalid Date");
    };
    let Some(fields) = IsoFields::read(&iso) else {
        return String::from("Invalid Date");
    };
    // `WeekDay(t)` is `(Day(t) + 4) modulo 7`, and the floored quotient of a
    // time value lies within 1e8 of zero, which an i64 holds.
    #[expect(clippy::cast_possible_truncation)]
    let day = (date.get_time() / MS_PER_DAY).floor() as i64;
    let week_day = usize::try_from((day + 4).rem_euclid(7)).unwrap_or(0);
    let year_sign = if fields.year < 0 { "-" } else { "" };
    format!(
        "{} {} {} {year_sign}{:04} {} GMT+0000 (Coordinated Universal Time)",
        WEEK_DAYS[week_day],
        MONTHS[fields.month.saturating_sub(1).min(11)],
        fields.date,
        fields.year.unsigned_abs(),
        fields.time
    )
}

/// The calendar fields of a date, read back out of its ISO text.
///
/// `toISOString` already runs the civil calendar clauses, so the fields come
/// from its text instead of a second copy of those clauses.
struct IsoFields<'a> {
    /// `YearFromTime`.
    year: i64,
    /// `MonthFromTime` plus one.
    month: usize,
    /// `DateFromTime` as its two digits.
    date: &'a str,
    /// The hour, the minute and the second, as `HH:mm:ss`.
    time: &'a str,
}

impl<'a> IsoFields<'a> {
    /// Reads the fields of `iso`, the form `YYYY-MM-DDTHH:mm:ss.sssZ` or its
    /// expanded year form with a sign and six digits.
    fn read(iso: &'a str) -> Option<Self> {
        let (calendar, clock) = iso.split_once('T')?;
        let mut parts = calendar.rsplitn(3, '-');
        let date = parts.next()?;
        let month = parts.next()?.parse().ok()?;
        let year = parts.next()?.trim_start_matches('+').parse().ok()?;
        let time = clock.get(..8)?;
        Some(Self {
            year,
            month,
            date,
            time,
        })
    }
}
