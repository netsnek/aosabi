// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the DateTime configuration the DateTimeISO scalar spreads.
 *
 * Aosabi binds DateTimeISO and not DateTime, and DateTimeISO spreads this
 * configuration and replaces only the name, the description and serialize,
 * so parseValue and parseLiteral of the bound scalar are the functions here.
 *
 * Derived from graphql-scalars 1.25.0, ., MIT.
 */

//! The `DateTime` scalar configuration.

use alloc::{format, string::String};

use aosabi_json::{JsObject, JsValue};
use aosabi_lang::{GraphQLError, ast::ValueNode};

use crate::{
    ScalarValue,
    error::{create_graphql_error, nodes_option},
    formatter::parse_date_time,
    js_date::JsDate,
    js_string::{json_stringify_to_string, string_text},
    validator::{validate_date_time, validate_js_date},
};

/// The `name` of the configuration, DateTime.js:15.
pub const NAME: &str = "DateTime";

/// The `description` of the configuration, DateTime.js:16-19.
///
/// The reference builds it from four string pieces joined by `+`, and the
/// value here is their concatenation.
pub const DESCRIPTION: &str = "A date-time string at UTC, such as 2007-12-03T10:15:30Z, compliant with the `date-time` format outlined in section 5.6 of the RFC 3339 profile of the ISO 8601 standard for representation of dates and times using the Gregorian calendar.";

/// Returns `serialize(value)` of the configuration, a `Date`.
///
/// A `Date` instance passes when it is valid, a string passes when it is RFC
/// 3339 compliant and becomes the date it names, a number becomes the date
/// of that time value, and anything else fails. The reference wraps the
/// number branch in a `try` whose `catch` throws
/// `DateTime cannot represent an invalid Unix timestamp` followed by the
/// value, and that arm is unreachable, because the `Date` constructor throws
/// nothing for a Number argument, so the translation has no arm for it.
///
/// # Errors
///
/// `DateTime cannot represent an invalid Date instance` for an invalid date,
/// `DateTime cannot represent an invalid date-time-string <value>.` for a
/// string that fails validation, and `DateTime cannot be serialized from a
/// non string, non numeric or non Date type <JSON.stringify(value)>` for
/// every other value. The last message is three string pieces joined by `+`
/// in the reference.
pub fn serialize(value: &ScalarValue) -> Result<JsDate, GraphQLError> {
    match value {
        ScalarValue::Date(date) => valid_date(*date),
        ScalarValue::Value(JsValue::String(text)) => date_of_string(&string_text(text)),
        ScalarValue::Value(JsValue::Number(number)) => Ok(JsDate::from_time_value(*number)),
        ScalarValue::Value(other) => Err(create_graphql_error(
            format!(
                "DateTime cannot be serialized from a non string, non numeric or non Date type {}",
                json_stringify_to_string(other)
            ),
            None,
        )),
    }
}

/// Returns `parseValue(value)` of the configuration, a `Date`.
///
/// A `Date` instance passes when it is valid, a string passes when it is RFC
/// 3339 compliant, and anything else fails.
///
/// # Errors
///
/// `DateTime cannot represent an invalid Date instance` for an invalid date,
/// `DateTime cannot represent an invalid date-time-string <value>.` for a
/// string that fails validation, and `DateTime cannot represent non string
/// or Date type <JSON.stringify(value)>` for every other value.
pub fn parse_value(value: &ScalarValue) -> Result<JsDate, GraphQLError> {
    match value {
        ScalarValue::Date(date) => valid_date(*date),
        ScalarValue::Value(JsValue::String(text)) => date_of_string(&string_text(text)),
        ScalarValue::Value(other) => Err(create_graphql_error(
            format!(
                "DateTime cannot represent non string or Date type {}",
                json_stringify_to_string(other)
            ),
            None,
        )),
    }
}

/// Returns `parseLiteral(ast)` of the configuration, a `Date`.
///
/// Only a string literal is accepted, and both errors carry the literal
/// under `nodes`.
///
/// # Errors
///
/// `DateTime cannot represent non string or Date type <value>` for a literal
/// that is not a string, where `<value>` is [`value_in_ast_and_value()`],
/// and `DateTime cannot represent an invalid date-time-string <value>.` for
/// a string literal that fails validation, where `String(value)` of the
/// reference is the string itself.
pub fn parse_literal(ast: &ValueNode) -> Result<JsDate, GraphQLError> {
    let ValueNode::StringValue(node) = ast else {
        return Err(create_graphql_error(
            format!(
                "DateTime cannot represent non string or Date type {}",
                value_in_ast_and_value(ast)
            ),
            Some(nodes_option(ast)),
        ));
    };
    let value = &node.value;
    if validate_date_time(value) {
        return Ok(parse_date_time(value));
    }
    Err(create_graphql_error(
        format!("DateTime cannot represent an invalid date-time-string {value}."),
        Some(nodes_option(ast)),
    ))
}

/// Returns the `extensions` of the configuration, DateTime.js:74-80.
#[must_use]
pub fn extensions() -> JsObject {
    let mut json_schema = JsObject::new();
    json_schema.insert("type", "string");
    json_schema.insert("format", "date-time");
    let mut extensions = JsObject::new();
    extensions.insert("codegenScalarType", "Date | string");
    extensions.insert("jsonSchema", json_schema);
    extensions
}

/// Renders `'value' in ast && ast.value` of DateTime.js:64 as the template
/// literal does.
///
/// `&&` yields its left operand when that is falsy and its right operand
/// otherwise, and the template renders the result through `ToString`. A
/// variable, a null, a list and an object literal carry no `value` property,
/// so the left operand is `false` and the message reads `false`. An int,
/// float and enum literal carry a string value, which is the message. A
/// boolean literal carries a boolean value, which reads `true` or `false`. A
/// string literal never reaches this function.
#[must_use]
pub fn value_in_ast_and_value(ast: &ValueNode) -> String {
    match ast {
        ValueNode::IntValue(node) => node.value.clone(),
        ValueNode::FloatValue(node) => node.value.clone(),
        ValueNode::StringValue(node) => node.value.clone(),
        ValueNode::EnumValue(node) => node.value.clone(),
        ValueNode::BooleanValue(node) => String::from(if node.value { "true" } else { "false" }),
        ValueNode::Variable(_)
        | ValueNode::NullValue(_)
        | ValueNode::ListValue(_)
        | ValueNode::ObjectValue(_) => String::from("false"),
    }
}

/// Returns a valid `Date` instance unchanged, DateTime.js:21-26 and :48-53.
fn valid_date(date: JsDate) -> Result<JsDate, GraphQLError> {
    if validate_js_date(date) {
        return Ok(date);
    }
    Err(create_graphql_error(
        "DateTime cannot represent an invalid Date instance",
        None,
    ))
}

/// Returns the `Date` a compliant string names, DateTime.js:27-32 and :54-59.
fn date_of_string(value: &str) -> Result<JsDate, GraphQLError> {
    if validate_date_time(value) {
        return Ok(parse_date_time(value));
    }
    Err(create_graphql_error(
        format!("DateTime cannot represent an invalid date-time-string {value}."),
        None,
    ))
}
