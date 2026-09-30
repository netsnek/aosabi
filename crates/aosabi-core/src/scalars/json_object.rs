// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core scalars json_object: the bound `JSONObject` scalar.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/pylon-handler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * Binds `JSONObjectResolver` of graphql-scalars as it stands.
 */

//! The bound `JSONObject` scalar, the `JSONObjectResolver` of graphql-scalars.

use aosabi_execute::{
    execute::{
        LeafSerializer,
        object_as_itself, //
    },
    value::{
        ErrorObject,
        Value,
        to_js_string, //
    }, //
};
use aosabi_lang::js_string::JsStringBuilder;
use aosabi_scalars::json_object;
use aosabi_type::schema_bindings::ScalarBinding;

use crate::scalars::hooks_binding;

/// The `name` of the scalar.
pub const NAME: &str = json_object::NAME;

/// Returns the binding of the scalar for the type layer.
#[must_use]
pub fn binding() -> ScalarBinding {
    let hooks = (
        json_object::serialize as _,
        json_object::parse_value as _,
        json_object::parse_literal as _,
    );
    hooks_binding(
        json_object::DESCRIPTION,
        Some(json_object::SPECIFIED_BY_URL),
        json_object::extensions(),
        hooks,
    )
}

/// Returns the serializer over the value of the executor.
///
/// It is `ensureObject` of graphql-scalars, the hook [`json_object::serialize`]
/// translates, as the serializer [`object_as_itself`] holds for the refusal of
/// this module, so the executor completes an object and a date of the scalar
/// with no call.
#[must_use]
pub fn leaf_serializer() -> LeafSerializer {
    object_as_itself(refusal)
}

/// Returns the error `ensureObject` throws for `value`, a value that is no object.
///
/// The message prints the value through `ToString`, the `${value}` of the
/// template literal at `utils.js:8-11`, which is made over the value of the
/// executor, so a date inside a list prints its `toString` and a lone surrogate
/// of a string stays in the message, a JavaScript string, as in the message of
/// the reference. The hook builds the same message over the value of the JSON
/// model, which has no date and no lone surrogate in a Rust string, and the
/// binding of the type layer keeps it for an input. The reference throws a
/// `GraphQLError`, which the executor throws again as a plain `Error` with its
/// message, so the refusal is that plain error.
fn refusal(value: &Value) -> Value {
    let mut message = JsStringBuilder::new();
    message
        .push_str("JSONObject cannot represent non-object value: ")
        .push(&to_js_string(value));
    Value::from(ErrorObject::error(message.finish()))
}
