// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core scalars number: the bound `Number` scalar.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/pylon-handler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The two message prefixes are the extracted literal
 * ../number_scalar_messages.rs, which tools/provenance writes.
 */

//! The bound `Number` scalar.
//!
//! Every TypeScript `number` is this scalar, an integer and a float alike.
//! The scalar takes a JavaScript number as it is and refuses every other
//! value with a `TypeError`, on the way in and on the way out. On the way in
//! it answers that a number is its own internal value, as `parseValue`
//! returns `value`, so the coercion keeps the number where it stands.

use aosabi_execute::{
    execute::{
        LeafSerializer,
        number_as_itself, //
    },
    value::{
        ErrorObject,
        Value,
        to_js_string, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    ast::ValueNode,
    js_string::JsStringBuilder, //
};
use aosabi_scalars::js_string::to_string;
use aosabi_type::{
    DefinitionError,
    Parsed,
    ScalarLiteralParser,
    ScalarSerializer,
    ScalarValueParser,
    definition::parse_number_token,
    schema_bindings::ScalarBinding, //
};

use crate::scalars::type_error_definition;

/// The `name` of the scalar, source line 290.
pub const NAME: &str = "Number";

/// The `description` of the scalar, source line 291.
pub const DESCRIPTION: &str = "Custom scalar that handles both integers and floats";

/// The two message prefixes of the scalar, one after the other.
const MESSAGES: &str = include_str!("../number_scalar_messages.rs");

/// The text both prefixes start with, which is where the literal is split.
const PREFIX_START: &str = "Value is";

/// The text a template literal prints for an object, which is what
/// `Object.prototype.toString` answers for it.
const OBJECT_TEXT: &str = "[object Object]";

/// Returns the prefix of `parseValue` and `serialize` and the prefix of
/// `parseLiteral`.
///
/// They are `Value is not a number: ` at source lines 296 and 316
/// and `Value is not a valid number or float: ` at `:307`. The extracted
/// literal holds both, and the second starts at the second `Value is`.
fn messages() -> (&'static str, &'static str) {
    let second = MESSAGES
        .match_indices(PREFIX_START)
        .nth(1)
        .map_or(MESSAGES.len(), |(at, _)| at);
    MESSAGES.split_at(second)
}

/// Returns [`Parsed::Input`] when `value` is a number, the body
/// `parseValue` and `serialize` share.
///
/// # Errors
///
/// Returns the `TypeError` `Value is not a number: ` followed by the
/// `ToString` of `value`, which is what the template literal prints.
pub fn ensure_number(value: &JsValue) -> Result<Parsed, DefinitionError> {
    if let JsValue::Number(_) = value {
        return Ok(Parsed::Input); // Valid number
    }
    let (not_a_number, _) = messages();
    Err(type_error_definition(format!(
        "{not_a_number}{}",
        to_string(value)
    )))
}

/// Returns `serialize(value)`, a copy of a number and the `TypeError` of
/// [`ensure_number`] for any other value.
///
/// # Errors
///
/// Returns the `TypeError` of [`ensure_number`].
fn serialize(value: &JsValue) -> Result<JsValue, DefinitionError> {
    Ok(ensure_number(value)?.into_value(value))
}

/// Returns `parseLiteral(ast)`, the double an integer or a float literal names.
///
/// # Errors
///
/// Returns the `TypeError` `Value is not a valid number or float: ` followed
/// by the `value` of a literal that has one, or by `[object Object]` for a
/// variable, a `null`, a list and an object literal.
pub fn parse_literal(
    ast: &ValueNode,
    _variables: Option<&JsObject>,
) -> Result<JsValue, DefinitionError> {
    let text = match ast {
        ValueNode::IntValue(node) => return Ok(JsValue::Number(parse_number_token(&node.value))),
        ValueNode::FloatValue(node) => return Ok(JsValue::Number(parse_number_token(&node.value))),
        ValueNode::StringValue(node) => node.value.as_str(),
        ValueNode::EnumValue(node) => node.value.as_str(),
        ValueNode::BooleanValue(node) if node.value => "true",
        ValueNode::BooleanValue(_) => "false",
        ValueNode::Variable(_)
        | ValueNode::NullValue(_)
        | ValueNode::ListValue(_)
        | ValueNode::ObjectValue(_) => OBJECT_TEXT,
    };
    let (_, not_a_literal) = messages();
    Err(type_error_definition(format!("{not_a_literal}{text}")))
}

/// Returns the binding of the scalar for the type layer.
#[must_use]
pub fn binding() -> ScalarBinding {
    ScalarBinding {
        description: Some(Some(DESCRIPTION.to_owned())),
        specified_by_url: Some(None),
        serialize: Some(ScalarSerializer::new(serialize)),
        parse_value: Some(ScalarValueParser::keeping(ensure_number)),
        parse_literal: Some(ScalarLiteralParser::new(parse_literal)),
        extensions: JsObject::new(),
    }
}

/// Returns the serializer over the value of the executor.
///
/// It is the serializer [`number_as_itself`] holds for the refusal of this
/// scalar, one serializer the process holds, so the executor knows it among
/// the serializers of a request and completes a number of the scalar as the
/// number itself with no call of it, which is what `serialize` returns for
/// it.
#[must_use]
pub fn leaf_serializer() -> LeafSerializer {
    number_as_itself(refusal)
}

/// Returns the `TypeError` the serializer throws for `value`, which is no number.
///
/// The message prints the value through `ToString`, and a `Date` instance, an
/// error and a function print differently from any JSON value, so the text is
/// made over the value of the executor. The message is a JavaScript string,
/// so a lone surrogate of the printed value stays in it, as in the message of
/// the template literal of the reference.
fn refusal(value: &Value) -> Value {
    let (not_a_number, _) = messages();
    let mut message = JsStringBuilder::new();
    message.push_str(not_a_number).push(&to_js_string(value));
    Value::from(ErrorObject::type_error(message.finish()))
}
