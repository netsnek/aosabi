// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core scalars: the seven scalars bound over every schema.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/pylon-handler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * One file per scalar. Each file names its own line range.
 */

//! The bound scalars.
//!
//! The SDL of a service declares its scalars by name alone, and
//! `handler` binds the behaviour: `ID` decodes a global id on input, `GID`
//! keeps it, `Date`, `JSON`, `JSONObject` and `Void` take the hooks of
//! graphql-scalars, and `Number` is its own. A bound scalar replaces the
//! hooks and also the description, the `specifiedByURL` and the extensions of
//! the scalar the SDL declares, so introspection answers the description of
//! the binding.
//!
//! | Module | Scalar | Source |
//! | --- | --- | --- |
//! | [`id`] | `ID` | source lines 55-61 and 251-261 |
//! | [`gid`] | `GID`, bound only when the SDL declares it | `:267-277` |
//! | [`date`] | `Date` | `:279-285`, hooks of `aosabi_scalars::date_time_iso` |
//! | [`json`] | `JSON` | `:286`, `aosabi_scalars::json` |
//! | [`json_object`] | `JSONObject` | `:287`, `aosabi_scalars::json_object` |
//! | [`void`] | `Void` | `:288`, `aosabi_scalars::void` |
//! | [`number`] | `Number` | `:289-320` |
//!
//! Every scalar is bound twice. The binding of the type layer works over the
//! JSON value model, which is what the coercion of a variable and of a
//! literal needs. A `Date` instance is no JSON value, so each scalar also has
//! a leaf serializer over the value of the executor, which the executor asks
//! first. The leaf serializer of a hook of `aosabi-scalars` is the one
//! [`scalar_hook`] answers, so the executor knows the hooks of `Date` and
//! `JSON`, and the one of `JSONObject` is the serializer
//! [`aosabi_execute::execute::object_as_itself`] holds, so the executor
//! completes a date, a value of the JSON model and an object of the three
//! with no call.

pub mod date;
pub mod gid;
pub mod id;
pub mod json;
pub mod json_object;
pub mod number;
pub mod void;

use aosabi_execute::execute::{
    LeafSerializer,
    scalar_hook, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::ast::ValueNode;
use aosabi_scalars::{
    ScalarError,
    ScalarResult,
    ScalarValue, //
};
use aosabi_type::{
    DefinitionError,
    ObjMap,
    ScalarLiteralParser,
    ScalarSerializer,
    ScalarValueParser,
    schema_bindings::ScalarBinding, //
};

/// The text `typeDefs` is searched for to decide whether `GID` is bound,
/// source line 267.
const SCALAR_GID: &str = "scalar GID";

/// A `serialize` or a `parseValue` hook of `aosabi-scalars`.
type ValueHook = fn(&ScalarValue) -> ScalarResult;

/// A `parseLiteral` hook of `aosabi-scalars`.
type LiteralHook = fn(&ValueNode, Option<&JsObject>) -> ScalarResult;

/// Returns the scalars `handler` binds over the SDL text `type_defs`.
///
/// The key order is the order of the object literal at
/// source lines 251-320: `ID`, `GID`, `Date`, `JSON`, `JSONObject`,
/// `Void`, `Number`. `GID` is bound only when `type_defs` contains the text
/// `scalar GID`.
#[must_use]
pub fn bound_scalars(type_defs: &str) -> ObjMap<ScalarBinding> {
    let mut scalars = ObjMap::default();
    scalars.insert(id::NAME.to_owned(), id::binding());
    if type_defs.contains(SCALAR_GID) {
        scalars.insert(gid::NAME.to_owned(), gid::binding());
    }
    scalars.insert(date::NAME.to_owned(), date::binding());
    scalars.insert(json::NAME.to_owned(), json::binding());
    scalars.insert(json_object::NAME.to_owned(), json_object::binding());
    scalars.insert(void::NAME.to_owned(), void::binding());
    scalars.insert(number::NAME.to_owned(), number::binding());
    scalars
}

/// Returns the leaf serializers of the bound scalars, in the key order of
/// [`bound_scalars`].
///
/// The executor asks this table by scalar name before it calls the
/// `serialize` of the type layer, because only this side sees a `Date`
/// instance as what it is.
#[must_use]
pub fn leaf_serializers(type_defs: &str) -> ObjMap<LeafSerializer> {
    let mut serializers = ObjMap::default();
    serializers.insert(id::NAME.to_owned(), id::leaf_serializer());
    if type_defs.contains(SCALAR_GID) {
        serializers.insert(gid::NAME.to_owned(), gid::leaf_serializer());
    }
    serializers.insert(date::NAME.to_owned(), date::leaf_serializer());
    serializers.insert(json::NAME.to_owned(), json::leaf_serializer());
    serializers.insert(json_object::NAME.to_owned(), json_object::leaf_serializer());
    serializers.insert(void::NAME.to_owned(), void::leaf_serializer());
    serializers.insert(number::NAME.to_owned(), number::leaf_serializer());
    serializers
}

/// Returns what a scalar hook answered as a JSON value.
///
/// A date is carried on as its `toISOString` text, which is also what
/// `JSON.stringify` writes for it, and an invalid date is `null` there.
fn scalar_output(value: ScalarValue) -> JsValue {
    match value {
        ScalarValue::Value(value) => value,
        ScalarValue::Date(date) => date.to_iso_string().map_or(JsValue::Null, JsValue::from),
    }
}

/// Returns the error of a scalar hook as the error of the type layer.
///
/// The error keeps its kind: a `GraphQLError` stays one, and the
/// `RangeError` of `toISOString` is a plain error with the same message.
fn definition_error(error: ScalarError) -> DefinitionError {
    match error {
        ScalarError::GraphQL(error) => DefinitionError::from(error),
        ScalarError::Range(error) => DefinitionError::plain(error.message()),
    }
}

/// Returns the `TypeError` with `message` as the error of the type layer.
fn type_error_definition(message: impl Into<JsString>) -> DefinitionError {
    DefinitionError::plain(message)
}

/// Returns the binding of a scalar of graphql-scalars over its three hooks.
///
/// `description` and `specified_by_url` always replace what the SDL built,
/// also when they are absent, because the reference copies every enumerable
/// property of the bound scalar over the config, an `undefined` one included.
fn hooks_binding(
    description: &str,
    specified_by_url: Option<&str>,
    extensions: JsObject,
    hooks: (ValueHook, ValueHook, LiteralHook),
) -> ScalarBinding {
    let (serialize, parse_value, parse_literal) = hooks;
    ScalarBinding {
        description: Some(Some(description.to_owned())),
        specified_by_url: Some(specified_by_url.map(str::to_owned)),
        serialize: Some(ScalarSerializer::new(move |value| {
            call_value_hook(serialize, value)
        })),
        parse_value: Some(ScalarValueParser::new(move |value| {
            call_value_hook(parse_value, value)
        })),
        parse_literal: Some(ScalarLiteralParser::new(move |ast, variables| {
            parse_literal(ast, variables)
                .map(scalar_output)
                .map_err(definition_error)
        })),
        extensions,
    }
}

/// Calls a value hook of `aosabi-scalars` over a JSON value.
fn call_value_hook(hook: ValueHook, value: &JsValue) -> Result<JsValue, DefinitionError> {
    hook(&ScalarValue::Value(value.clone()))
        .map(scalar_output)
        .map_err(definition_error)
}

/// Returns the leaf serializer over a `serialize` hook of `aosabi-scalars`,
/// the one the executor answers for it (see [`scalar_hook`]).
fn hook_leaf_serializer(serialize: ValueHook) -> LeafSerializer {
    scalar_hook(serialize)
}
