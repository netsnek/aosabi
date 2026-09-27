// SPDX-License-Identifier: MIT
/*
 * aosabi-execute coerce_error: the error a thrown value that is no error becomes.
 *
 * Derived from @graphql-tools/executor 1.5.1, cjs/execution/coerceError.js, MIT.
 */

//! The coercion of a thrown value into an error.
//!
//! The executor of graphql-js wraps a thrown value that is no error as
//! `Unexpected error value: ` followed by the inspected value. The executor on
//! the wire runs [`coerce_error`] first, so that wrapper never sees such a
//! value and the message of the response is the `message` member of a thrown
//! object or the `String` of any other thrown value.

use std::rc::Rc;

use aosabi_json::JsString;

use crate::value::{
    ErrorName,
    ErrorObject,
    ObjectLike,
    Value,
    to_js_string, //
};

/// Returns `error` when it is an error, and the error the executor makes of it otherwise.
///
/// An object with a string `message` becomes `new Error(message)` and keeps a
/// string `name`. The `cause` and the `stack` of the reference have no reading,
/// because no response carries either. Any other value becomes
/// `new Error(String(value))`, and the text is ECMA-262 `ToString`.
#[must_use]
pub fn coerce_error(error: Value) -> Value {
    if error.is_error() {
        return error;
    }
    // `typeof error === 'object' && error != null` holds for an array, a date,
    // a promise and a host value too, and none of them has a `message`.
    if let Value::Object(object) = &error
        && let Some(coerced_error) = coerce_object(object)
    {
        return Value::from(coerced_error);
    }
    Value::from(ErrorObject::error(to_js_string(&error)))
}

/// Returns the error of an object whose `message` is a string, and `None` for any other object.
fn coerce_object(object: &Rc<dyn ObjectLike>) -> Option<ErrorObject> {
    let Value::String(message) = Rc::clone(object).get("message") else {
        return None;
    };
    let name = match Rc::clone(object).get("name") {
        Value::String(name) => error_name(name),
        _ => ErrorName::Error,
    };
    Some(ErrorObject::Plain { name, message })
}

/// Returns the [`ErrorName`] whose text is `name`.
fn error_name(name: JsString) -> ErrorName {
    let known = [
        ErrorName::Error,
        ErrorName::TypeError,
        ErrorName::RangeError,
        ErrorName::AggregateError,
        ErrorName::NonErrorThrown,
    ];
    known
        .into_iter()
        .find(|known| known.to_js_string() == name)
        .unwrap_or(ErrorName::Other(name))
}
