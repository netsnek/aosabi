// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the hooks the JSON and JSONObject scalars share.
 *
 * Derived from graphql-scalars 1.25.0, ., MIT.
 */

//! The shared functions of the two JSON scalars.

use alloc::format;

use aosabi_json::{JsObject, JsValue};
use aosabi_lang::{GraphQLError, ast::ValueNode, print};

use crate::{
    ScalarValue,
    error::{create_graphql_error, nodes_option},
    js_string::to_string,
};

/// Returns `value`.
///
/// The `serialize` and `parseValue` of the `JSON` scalar.
pub fn identity<T>(value: T) -> T {
    value
}

/// Returns `value` when it is an object.
///
/// `typeof value` is `object` for a plain object, an array, `null` and a
/// `Date`, and the check then refuses `null` and an array, so a `Date`
/// passes the way a plain object does. graphql-js calls `serialize` and
/// `parseValue` with one argument, so `ast` is `None` on both hooks and the
/// error carries no node there.
///
/// # Errors
///
/// `JSONObject cannot represent non-object value: <value>` for every other
/// value, where `<value>` is `ToString` of the value.
pub fn ensure_object(
    value: &ScalarValue,
    ast: Option<&ValueNode>,
) -> Result<ScalarValue, GraphQLError> {
    match value {
        ScalarValue::Date(_) | ScalarValue::Value(JsValue::Object(_)) => Ok(value.clone()),
        ScalarValue::Value(other) => Err(create_graphql_error(
            format!(
                "JSONObject cannot represent non-object value: {}",
                to_string(other)
            ),
            ast.map(nodes_option),
        )),
    }
}

/// Returns the object an object literal names.
///
/// The fields land in an object with no prototype in source order, with a
/// repeated name keeping its first position and its last value, which is the
/// assignment rule the object of aosabi-json follows. The reference passes
/// `ast ? { nodes: ast } : undefined`, and the literal is never nullish
/// here, so the error always carries it.
///
/// # Errors
///
/// `JSONObject cannot represent non-object value: <print(ast)>` for a
/// literal that is not an object, with the literal printed by the printer
/// of aosabi-lang, and any error of a nested [`parse_literal()`].
pub fn parse_object(
    ast: &ValueNode,
    variables: Option<&JsObject>,
) -> Result<JsObject, GraphQLError> {
    let ValueNode::ObjectValue(node) = ast else {
        return Err(create_graphql_error(
            format!(
                "JSONObject cannot represent non-object value: {}",
                print(ast)
            ),
            Some(nodes_option(ast)),
        ));
    };
    let mut value = JsObject::new();
    for field in &node.fields {
        value.insert(field.name.value(), parse_literal(&field.value, variables)?);
    }
    Ok(value)
}

/// Returns the value a literal names.
///
/// A string and a boolean literal are their value, an int and a float
/// literal go through `parseFloat`, an object literal through
/// [`parse_object()`], a list maps its values, null is `null`, and a
/// variable reads `variables ? variables[name] : undefined`, which is
/// `undefined` for a missing variable and for a missing map, because an
/// object is truthy even when it is empty. The `switch` of the reference has
/// no case for an enum literal, so the function falls off its end and
/// returns `undefined`.
///
/// # Errors
///
/// The errors of [`parse_object()`] for a nested object literal, which the
/// `switch` reaches only for an object literal, so none in practice.
pub fn parse_literal(
    ast: &ValueNode,
    variables: Option<&JsObject>,
) -> Result<JsValue, GraphQLError> {
    match ast {
        ValueNode::StringValue(node) => Ok(JsValue::from(node.value.clone())),
        ValueNode::BooleanValue(node) => Ok(JsValue::Bool(node.value)),
        ValueNode::IntValue(node) => Ok(JsValue::Number(parse_float(&node.value))),
        ValueNode::FloatValue(node) => Ok(JsValue::Number(parse_float(&node.value))),
        ValueNode::ObjectValue(_) => Ok(JsValue::Object(parse_object(ast, variables)?)),
        ValueNode::ListValue(node) => Ok(JsValue::Array(
            node.values
                .iter()
                .map(|value| parse_literal(value, variables))
                .collect::<Result<_, _>>()?,
        )),
        ValueNode::NullValue(_) => Ok(JsValue::Null),
        ValueNode::Variable(node) => Ok(variables
            .and_then(|variables| variables.get(node.name.value()))
            .cloned()
            .unwrap_or(JsValue::Undefined)),
        ValueNode::EnumValue(_) => Ok(JsValue::Undefined),
    }
}

/// Returns `parseFloat(text)` for the lexeme of an int or float literal.
///
/// `parseFloat` reads the longest prefix that is a `StrDecimalLiteral` and
/// converts it with the rounding of `StringToNumber`. An int or float lexeme
/// of the lexer is a decimal literal in full, which `f64::from_str` reads
/// with the same correctly rounded conversion, so an exponent past the range
/// is infinity on both sides. A lexeme that neither reads is not produced by
/// the lexer, and NaN stands in for it.
fn parse_float(text: &str) -> f64 {
    text.parse().unwrap_or(f64::NAN)
}
