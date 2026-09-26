// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the untyped reading of a value literal into a value of the
 * value model.
 *
 * Derived from graphql-js 16.12.0, src/utilities/valueFromASTUntyped.ts, MIT.
 */

//! A value literal read without a type.
//!
//! The reference reads a literal into a JavaScript value by its kind alone,
//! and every scalar whose config gives no `parseLiteral` reads its literal
//! this way before `parseValue` runs. The value model is the [`JsValue`] of
//! the JSON writer, and the `typeof` mapping is the one the definition
//! module documents.

use alloc::vec::Vec;

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::ast::ValueNode;

use crate::definition::parse_number_token;

/// Produces a JavaScript value given a GraphQL Value AST.
///
/// Unlike `valueFromAST()`, no type is provided. The resulting JavaScript
/// value reflects the provided GraphQL value AST.
///
/// Derived from graphql-js 16.12.0, src/utilities/valueFromASTUntyped.ts, MIT.
///
/// `value_node` is the literal and `variables` the map a variable reads
/// from. A variable that `variables` does not hold, or a missing map, reads
/// as undefined, which is what the optional chain `variables?.[name]` of the
/// reference evaluates to. An `Int` literal goes through
/// `parseInt(value, 10)` and a `Float` literal through `parseFloat(value)`,
/// both of which [`parse_number_token`] reproduces over the token grammar.
/// An object literal keeps the field order of the document, and a repeated
/// field name keeps its first position with the last value, which is what
/// assignment to an existing property does.
#[must_use]
pub fn value_from_ast_untyped(value_node: &ValueNode, variables: Option<&JsObject>) -> JsValue {
    match value_node {
        ValueNode::NullValue(_) => JsValue::Null,
        ValueNode::IntValue(node) => JsValue::Number(parse_number_token(&node.value)),
        ValueNode::FloatValue(node) => JsValue::Number(parse_number_token(&node.value)),
        ValueNode::StringValue(node) => JsValue::from(node.value.as_str()),
        ValueNode::EnumValue(node) => JsValue::from(node.value.as_str()),
        ValueNode::BooleanValue(node) => JsValue::Bool(node.value),
        ValueNode::ListValue(node) => JsValue::Array(
            node.values
                .iter()
                .map(|node| value_from_ast_untyped(node, variables))
                .collect::<Vec<JsValue>>(),
        ),
        ValueNode::ObjectValue(node) => {
            // keyValMap builds a null prototype object one field at a time.
            let mut object = JsObject::new();
            for field in &node.fields {
                object.insert(
                    field.name.value(),
                    value_from_ast_untyped(&field.value, variables),
                );
            }
            JsValue::Object(object)
        }
        ValueNode::Variable(node) => variables
            .and_then(|variables| variables.get(node.name.value()))
            .cloned()
            .unwrap_or(JsValue::Undefined),
    }
}
