// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the JSONObject scalar.
 *
 * Aosabi binds the scalar object itself under the name JSONObject.
 *
 * Derived from graphql-scalars 1.25.0, ., MIT.
 */

//! The `JSONObject` scalar.

use aosabi_json::{JsObject, JsValue};
use aosabi_lang::ast::ValueNode;

use crate::{
    ScalarResult, ScalarValue,
    json_utils::{ensure_object, parse_object},
};

/// The `name` of the configuration, JSONObject.js:5.
pub const NAME: &str = "JSONObject";

/// The `description` of the configuration, JSONObject.js:6.
pub const DESCRIPTION: &str = "The `JSONObject` scalar type represents JSON objects as specified by [ECMA-404](http://www.ecma-international.org/publications/files/ECMA-ST/ECMA-404.pdf).";

/// The `specifiedByURL` of the configuration, JSONObject.js:3, which the
/// reference also sets under the older key `specifiedByUrl`.
pub const SPECIFIED_BY_URL: &str =
    "http://www.ecma-international.org/publications/files/ECMA-ST/ECMA-404.pdf";

/// Returns `serialize(value)`, which is `ensureObject`, JSONObject.js:7.
///
/// # Errors
///
/// The error of [`ensure_object()`] for a value that is not an object.
pub fn serialize(value: &ScalarValue) -> ScalarResult {
    Ok(ensure_object(value, None)?)
}

/// Returns `parseValue(value)`, which is `ensureObject`, JSONObject.js:8.
///
/// # Errors
///
/// The error of [`ensure_object()`] for a value that is not an object.
pub fn parse_value(value: &ScalarValue) -> ScalarResult {
    Ok(ensure_object(value, None)?)
}

/// Returns `parseLiteral(ast, variables)`, which is `parseObject`,
/// JSONObject.js:9.
///
/// # Errors
///
/// The error of [`parse_object()`] for a literal that is not an object.
pub fn parse_literal(ast: &ValueNode, variables: Option<&JsObject>) -> ScalarResult {
    Ok(ScalarValue::Value(JsValue::Object(parse_object(
        ast, variables,
    )?)))
}

/// Returns the `extensions` of the configuration, JSONObject.js:12-18.
#[must_use]
pub fn extensions() -> JsObject {
    let mut json_schema = JsObject::new();
    json_schema.insert("type", "object");
    json_schema.insert("additionalProperties", true);
    let mut extensions = JsObject::new();
    extensions.insert("codegenScalarType", "Record<string, any>");
    extensions.insert("jsonSchema", json_schema);
    extensions
}
