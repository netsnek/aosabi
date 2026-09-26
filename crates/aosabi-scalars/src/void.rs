// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the Void scalar.
 *
 * Aosabi binds the scalar object itself under the name Void.
 *
 * Derived from graphql-scalars 1.25.0, ., MIT.
 */

//! The `Void` scalar.

use aosabi_json::{JsObject, JsValue};
use aosabi_lang::ast::ValueNode;

use crate::{ScalarResult, ScalarValue};

/// The `name` of the configuration, Void.js:3.
pub const NAME: &str = "Void";

/// The `description` of the configuration, Void.js:4.
pub const DESCRIPTION: &str = "Represents NULL values";

/// Returns `serialize()`, the empty string, Void.js:5-7.
///
/// The hook reads no argument, so `value` is unused the way the JavaScript
/// function leaves its argument unused. The empty string reaches the wire
/// only for a resolver result that is not nullish, because the executor
/// completes a nullish result as `null` before it calls any hook.
///
/// # Errors
///
/// None. The hook never fails and shares the signature of the four scalars.
pub fn serialize(_value: &ScalarValue) -> ScalarResult {
    Ok(ScalarValue::Value(JsValue::from("")))
}

/// Returns `parseValue()`, `null`, Void.js:8-10.
///
/// # Errors
///
/// None. The hook never fails and shares the signature of the four scalars.
pub fn parse_value(_value: &ScalarValue) -> ScalarResult {
    Ok(ScalarValue::Value(JsValue::Null))
}

/// Returns `parseLiteral()`, `null`, Void.js:11-13.
///
/// # Errors
///
/// None. The hook never fails and shares the signature of the four scalars.
pub fn parse_literal(_ast: &ValueNode, _variables: Option<&JsObject>) -> ScalarResult {
    Ok(ScalarValue::Value(JsValue::Null))
}

/// Returns the `extensions` of the configuration, Void.js:14-16.
#[must_use]
pub fn extensions() -> JsObject {
    let mut extensions = JsObject::new();
    extensions.insert("codegenScalarType", "void");
    extensions
}
