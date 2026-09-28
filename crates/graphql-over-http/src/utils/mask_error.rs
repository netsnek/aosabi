// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the default mask of an error and its serialization.
 *
 * Derived from graphql-yoga 5.18.0, cjs/utils/mask-error.js, MIT.
 */

//! `mask_error`.
//!
//! The mask keeps an original `GraphQLError` and makes any other error a
//! `GraphQLError` of the message it is given with the extensions
//! `{ code: "INTERNAL_SERVER_ERROR", unexpected: true }`. A `GraphQLError`
//! lends the new one its nodes, source, positions and path and its `http`
//! extension, and in development the serialized original error rides under
//! `originalError`. The new error carries no original error, so it is an
//! original `GraphQLError` itself and a second mask keeps it.
//!
//! The serialization of an `Error` is `{ message, stack, cause }`, and the
//! frames of a stack have no reading in the model, so the stack is its first
//! line, `name: message`, and `cause`, which no error of the model carries,
//! is left out as `JSON.stringify` leaves out `undefined`.

use aosabi_execute::{
    ErrorObject,
    Value,
    errors::copy_graphql_error, //
};
use aosabi_hooks::plugins::use_masked_errors::is_original_graphql_error;
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    OriginalError, //
};

/// Returns the serialization of `error`, `serializeError(error)`.
#[must_use]
pub fn serialize_error(error: &Value) -> JsValue {
    match error {
        Value::Error(object) => match object.as_ref() {
            ErrorObject::GraphQL(error) => JsValue::from(error.to_json()),
            object => serialize_plain(&object.name(), &object.message()),
        },
        value => value.to_js(),
    }
}

/// Returns the serialization of the original error `original`.
fn serialize_original(original: &OriginalError) -> JsValue {
    match original {
        OriginalError::GraphQL(error) => JsValue::from(error.to_json()),
        OriginalError::Other(error) => {
            let name = error
                .downcast_ref::<aosabi_execute::errors::ThrownError>()
                .map_or_else(
                    || aosabi_json::JsString::from("Error"),
                    |thrown| thrown.name.clone(),
                );
            serialize_plain(&name, &original.message())
        }
    }
}

/// Returns `{ message, stack }` of an `Error` with `name` and `message`.
fn serialize_plain(name: &aosabi_json::JsString, message: &aosabi_json::JsString) -> JsValue {
    let mut object = JsObject::new();
    object.insert("message", JsValue::String(message.clone()));
    let stack = aosabi_execute::ErrorObject::Plain {
        name: aosabi_execute::value::error::ErrorName::Other(name.clone()),
        message: message.clone(),
    }
    .to_js_string();
    object.insert("stack", JsValue::from(stack));
    JsValue::Object(object)
}

/// Returns whether the development mode holds, `process.env.NODE_ENV === 'development'`.
#[must_use]
pub fn is_dev_default() -> bool {
    std::env::var("NODE_ENV").is_ok_and(|env| env == "development")
}

/// Masks `error` with `message`, `maskError(error, message, isDev)`.
///
/// `is_dev` of `None` reads the environment at the call, as the default
/// parameter of the reference does.
#[must_use]
pub fn mask_error(error: Value, message: &str, is_dev: Option<bool>) -> Value {
    if is_original_graphql_error(&error) {
        return error;
    }
    let is_dev = is_dev.unwrap_or_else(is_dev_default);
    let mut extensions = JsObject::new();
    extensions.insert("code", JsValue::from("INTERNAL_SERVER_ERROR"));
    extensions.insert("unexpected", JsValue::Bool(true));
    let mut options = GraphQLErrorOptions::default();
    let graphql = match &error {
        Value::Error(object) => match object.as_ref() {
            ErrorObject::GraphQL(error) => Some(error),
            _ => None,
        },
        _ => None,
    };
    if let Some(error) = graphql {
        let copy = copy_graphql_error(error);
        options.nodes = copy.nodes;
        options.source = copy.source;
        options.positions = copy.positions;
        options.path = copy.path;
        if is_dev && let Some(original) = &error.original_error {
            extensions.insert("originalError", serialize_original(original));
        }
        if let Some(http) = error.extensions.get("http").filter(|http| truthy(http)) {
            extensions.insert("http", http.clone());
        }
    } else if is_dev {
        extensions.insert("originalError", serialize_error(&error));
    }
    options.extensions = Some(extensions);
    Value::from(ErrorObject::GraphQL(GraphQLError::new(message, options)))
}

/// Returns whether `value` is truthy, which is `ToBoolean` of ECMA-262.
#[must_use]
pub fn truthy(value: &JsValue) -> bool {
    match value {
        JsValue::Undefined | JsValue::Null => false,
        JsValue::Bool(value) => *value,
        JsValue::Number(number) => *number != 0.0 && !number.is_nan(),
        JsValue::String(text) => !text.is_empty(),
        JsValue::Array(_) | JsValue::Object(_) | JsValue::Opaque(_) => true,
    }
}
