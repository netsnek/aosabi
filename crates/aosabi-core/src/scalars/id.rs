// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core scalars id: the bound `ID` scalar, which decodes a global id on
 * input.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/pylon-handler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The bound `ID` scalar.
//!
//! A client may hand back a global id, `gid://namespace/Type/localId`, or a
//! raw local id. This scalar is the one place a global id is decoded: every
//! `ID` typed input runs through [`decode_gid_input`], so nothing below the
//! schema ever sees the global form. The output side is the plain string
//! coercion, which prints any value through `String(value)` and refuses
//! none, unlike the `ID` of graphql-js.

use aosabi_execute::execute::{
    LeafSerializer,
    string_of_value, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::ast::ValueNode;
use aosabi_scalars::js_string::to_string;
use aosabi_type::{
    ScalarLiteralParser,
    ScalarSerializer,
    ScalarValueParser,
    schema_bindings::ScalarBinding, //
};

/// The `name` of the scalar, source line 252.
pub const NAME: &str = "ID";

/// The `description` of the scalar, source lines 253-254.
pub const DESCRIPTION: &str = "A global id (`gid://namespace/Type/localId`) or a raw local id. Decoded to the raw local id on input.";

/// The prefix of a global id, source line 56.
const GID_PREFIX: &[u8] = b"gid://";

/// Strips a global id back to its raw local id.
///
/// A string that starts with `gid://` is cut after its last `/`, and a global
/// id with an empty tail comes back whole, because the empty string is falsy.
/// Any other value passes through untouched. The cut is made on the bytes of
/// the string, which is the cut `slice` makes on code units, because `/` is
/// one byte and one code unit and never a part of a longer sequence.
#[must_use]
pub fn decode_gid_input(value: &JsValue) -> JsValue {
    let JsValue::String(text) = value else {
        return value.clone();
    };
    let bytes = text.as_wtf8();
    if !bytes.starts_with(GID_PREFIX) {
        return value.clone();
    }
    let tail = bytes
        .iter()
        .rposition(|&byte| byte == b'/')
        .map_or(bytes, |slash| bytes.get(slash + 1..).unwrap_or_default());
    match JsString::from_wtf8(tail) {
        Ok(local) if !local.is_empty() => JsValue::String(local),
        _ => value.clone(),
    }
}

/// Returns `serialize(value)`, source line 255.
///
/// A nullish value passes, and anything else is `String(value)`.
#[must_use]
pub fn serialize(value: &JsValue) -> JsValue {
    match value {
        JsValue::Undefined | JsValue::Null => value.clone(),
        other => JsValue::from(to_string(other)),
    }
}

/// Returns `parseLiteral(ast)`, source lines 257-260.
///
/// A string literal and an integer literal go through
/// [`decode_gid_input`], and every other literal is `null`.
#[must_use]
pub fn parse_literal(ast: &ValueNode, _variables: Option<&JsObject>) -> JsValue {
    match ast {
        ValueNode::StringValue(node) => decode_gid_input(&JsValue::from(node.value.as_str())),
        ValueNode::IntValue(node) => decode_gid_input(&JsValue::from(node.value.as_str())),
        _ => JsValue::Null,
    }
}

/// Returns the binding of the scalar for the type layer.
#[must_use]
pub fn binding() -> ScalarBinding {
    ScalarBinding {
        description: Some(Some(DESCRIPTION.to_owned())),
        specified_by_url: Some(None),
        serialize: Some(ScalarSerializer::new(|value| Ok(serialize(value)))),
        parse_value: Some(ScalarValueParser::new(|value| Ok(decode_gid_input(value)))),
        parse_literal: Some(ScalarLiteralParser::new(|ast, variables| {
            Ok(parse_literal(ast, variables))
        })),
        extensions: JsObject::new(),
    }
}

/// Returns the serializer over the value of the executor.
///
/// `String(value)` of a `Date` instance and of an error is the text their
/// own `toString` answers, which only the value of the executor can tell.
/// The executor holds that serializer, [`string_of_value`], and knows it among
/// the serializers of a request, so it completes a string of `ID` as the
/// string itself with no call of it.
#[must_use]
pub fn leaf_serializer() -> LeafSerializer {
    string_of_value()
}
