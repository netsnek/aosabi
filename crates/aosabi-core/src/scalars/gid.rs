// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core scalars gid: the bound `GID` scalar, which passes a global id
 * through verbatim.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/pylon-handler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The bound `GID` scalar.
//!
//! The refetch field `node(id: GID!)` dispatches on the type a global id
//! embeds, which the stripping `ID` scalar would tear off. So global ids get
//! a scalar of their own that keeps the whole id. It is bound only when the
//! SDL declares it.

use aosabi_execute::execute::LeafSerializer;
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::ast::ValueNode;
use aosabi_type::{
    ScalarLiteralParser,
    ScalarSerializer,
    ScalarValueParser,
    schema_bindings::ScalarBinding, //
};

use crate::scalars::id;

/// The `name` of the scalar, source line 270.
pub const NAME: &str = "GID";

/// The `description` of the scalar, source line 271.
pub const DESCRIPTION: &str =
    "A global id (`gid://namespace/Type/localId`), passed through verbatim.";

/// Returns `parseLiteral(ast)`, which is `'value' in ast ? ast.value : null`.
///
/// Five node kinds carry a `value` member: the integer, the float, the string
/// and the enum literal carry a string there, and the boolean literal carries
/// a boolean. A variable, a `null`, a list and an object literal carry none.
#[must_use]
pub fn parse_literal(ast: &ValueNode, _variables: Option<&JsObject>) -> JsValue {
    match ast {
        ValueNode::IntValue(node) => JsValue::from(node.value.as_str()),
        ValueNode::FloatValue(node) => JsValue::from(node.value.as_str()),
        ValueNode::StringValue(node) => JsValue::from(node.value.as_str()),
        ValueNode::EnumValue(node) => JsValue::from(node.value.as_str()),
        ValueNode::BooleanValue(node) => JsValue::Bool(node.value),
        ValueNode::Variable(_)
        | ValueNode::NullValue(_)
        | ValueNode::ListValue(_)
        | ValueNode::ObjectValue(_) => JsValue::Null,
    }
}

/// Returns the binding of the scalar for the type layer.
///
/// `serialize` is the one of `ID`, and `parseValue` is the identity.
#[must_use]
pub fn binding() -> ScalarBinding {
    ScalarBinding {
        description: Some(Some(DESCRIPTION.to_owned())),
        specified_by_url: Some(None),
        serialize: Some(ScalarSerializer::new(|value| Ok(id::serialize(value)))),
        parse_value: Some(ScalarValueParser::new(|value| Ok(value.clone()))),
        parse_literal: Some(ScalarLiteralParser::new(|ast, variables| {
            Ok(parse_literal(ast, variables))
        })),
        extensions: JsObject::new(),
    }
}

/// Returns the serializer over the value of the executor, which is the one of `ID`.
#[must_use]
pub fn leaf_serializer() -> LeafSerializer {
    id::leaf_serializer()
}
