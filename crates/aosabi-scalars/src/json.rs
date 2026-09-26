// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the JSON scalar.
 *
 * Aosabi binds the scalar object itself under the name JSON.
 *
 * Derived from graphql-scalars 1.25.0, ., MIT.
 */

//! The `JSON` scalar.

use aosabi_json::{JsObject, JsValue};
use aosabi_lang::ast::ValueNode;

use crate::{
    ScalarResult, ScalarValue,
    json_utils::{identity, parse_literal as parse_literal_value},
};

/// The `name` of the configuration, JSON.js:6.
pub const NAME: &str = "JSON";

/// The `description` of the configuration, JSON.js:7.
pub const DESCRIPTION: &str = "The `JSON` scalar type represents JSON values as specified by [ECMA-404](http://www.ecma-international.org/publications/files/ECMA-ST/ECMA-404.pdf).";

/// The `specifiedByURL` of the configuration, JSON.js:4, which the
/// reference also sets under the older key `specifiedByUrl`.
pub const SPECIFIED_BY_URL: &str =
    "http://www.ecma-international.org/publications/files/ECMA-ST/ECMA-404.pdf";

/// Returns `serialize(value)`, which is [`identity()`], JSON.js:8.
///
/// # Errors
///
/// None. The hook never fails and shares the signature of the four scalars.
pub fn serialize(value: &ScalarValue) -> ScalarResult {
    Ok(identity(value.clone()))
}

/// Returns `parseValue(value)`, which is [`identity()`], JSON.js:9.
///
/// # Errors
///
/// None. The hook never fails and shares the signature of the four scalars.
pub fn parse_value(value: &ScalarValue) -> ScalarResult {
    Ok(identity(value.clone()))
}

/// Returns `parseLiteral(ast, variables)`, JSON.js:10.
///
/// # Errors
///
/// The errors of [`json_utils::parse_literal()`](crate::json_utils::parse_literal),
/// which are none in practice.
pub fn parse_literal(ast: &ValueNode, variables: Option<&JsObject>) -> ScalarResult {
    Ok(ScalarValue::Value(parse_literal_value(ast, variables)?))
}

/// Returns the `extensions` of the configuration, JSON.js:13-15.
#[must_use]
pub fn extensions() -> JsObject {
    let mut extensions = JsObject::new();
    extensions.insert("codegenScalarType", JsValue::from("any"));
    extensions
}
