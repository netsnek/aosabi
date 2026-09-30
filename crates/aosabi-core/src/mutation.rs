// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the mutation wrapper and its user errors.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/mutation.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! `mutation`, `UserError` and `to_user_errors`.
//!
//! `mutation(fn)` wraps a mutation resolver so an expected failure becomes
//! data: the payload is the result's keys and `userErrors: []` on success,
//! `{userErrors}` for a thrown value with an `issues` array or a
//! `ServiceError`, and any other throw is rethrown. An entry of `graphql!`
//! writes `mutation(...)` around its value, and the macro hands the entry to
//! the glue wrapper of `glue::mutation`, so [`mutation`] itself is the marker
//! the macro reads and is never called at run time. The payload type is the
//! reference's mapped type, every field of the returned object nullable and
//! `userErrors` appended, which the inference makes.
//!
//! A `ServiceError` of the Rust side is thrown as the `GraphQLError` it is,
//! whose extensions open with `code` and `statusCode` in that order, which is
//! how its constructor assigns them. That shape is the reading of
//! `err instanceof ServiceError` here, so a `GraphQLError` an application
//! throws with the same two keys first is read as one.

use std::rc::Rc;

use aosabi_execute::value::{
    ErrorObject,
    PlainObject,
    Value, //
};
use aosabi_json::JsValue;
use aosabi_lang::js_string::to_string_lossy;

pub use crate::descriptor::UserError;
use crate::glue::{
    IntoValue,
    Thrown, //
};

/// The marker of a mutation entry, which `mutation(f)` answers.
#[derive(Clone, Copy, Debug)]
pub struct Mutation<F>(pub F);

/// Wraps a mutation resolver, `mutation(fn)`.
///
/// The macro `graphql!` reads `mutation(...)` around an entry and runs the
/// wrapper of `glue::mutation` around it, so the function is the marker a
/// reader meets and nothing calls it at run time.
pub const fn mutation<F>(f: F) -> Mutation<F> {
    Mutation(f)
}

impl IntoValue for UserError {
    fn into_value(self) -> Value {
        let field: Vec<Value> = self.field.into_iter().map(Value::from).collect();
        Value::Object(PlainObject::from_entries(vec![
            (String::from("field"), Value::from(field)),
            (String::from("message"), Value::from(self.message)),
            (String::from("code"), Value::from(self.code)),
        ]))
    }
}

/// Returns the property `key` of a thrown value, `err?.[key]`.
fn property(value: &Value, key: &str) -> Value {
    match value {
        Value::Object(object) => Rc::clone(object).get(key),
        Value::Error(error) => error.prop(key).map_or(Value::Undefined, Value::from_js),
        _ => Value::Undefined,
    }
}

/// Returns the text of a string value, `None` for any other value.
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(to_string_lossy(text).into_owned()),
        _ => None,
    }
}

/// Returns the issues of a structural validation error, `validationIssues(err)`.
///
/// An `issues` array whose every element is truthy and carries a string
/// `message`.
fn validation_issues(err: &Value) -> Option<Vec<Value>> {
    let Value::List(issues) = property(err, "issues") else {
        return None;
    };
    let all = issues
        .iter()
        .all(|issue| !issue.is_nullish() && matches!(property(issue, "message"), Value::String(_)));
    all.then(|| issues.iter().cloned().collect())
}

/// Returns whether `err` is a `ServiceError`, a `GraphQLError` whose
/// extensions open with `code` and `statusCode`.
fn is_service_error(err: &Value) -> bool {
    let Value::Error(error) = err else {
        return false;
    };
    let ErrorObject::GraphQL(error) = error.as_ref() else {
        return false;
    };
    let mut keys = error.extensions.iter().map(|(key, _)| key.as_wtf8());
    keys.next() == Some(b"code".as_slice()) && keys.next() == Some(b"statusCode".as_slice())
}

/// Maps a thrown value to user errors, `toUserErrors(err)`, `None` for an
/// unexpected one that is rethrown.
#[must_use]
pub fn to_user_errors(thrown: &Thrown) -> Option<Vec<UserError>> {
    let err = &thrown.0;
    if let Some(issues) = validation_issues(err) {
        return Some(
            issues
                .iter()
                .map(|issue| UserError {
                    field: text(&property(issue, "path"))
                        .filter(|path| !path.is_empty())
                        .map(|path| path.split('.').map(String::from).collect())
                        .unwrap_or_default(),
                    message: text(&property(issue, "message"))
                        .unwrap_or_else(|| String::from("Invalid value")),
                    code: text(&property(issue, "code")).unwrap_or_else(|| String::from("invalid")),
                })
                .collect(),
        );
    }
    if is_service_error(err) {
        let Value::Error(error) = err else {
            return None;
        };
        let ErrorObject::GraphQL(error) = error.as_ref() else {
            return None;
        };
        let field = match error.extensions.get("details") {
            Some(JsValue::Object(details)) => match details.get("field") {
                Some(JsValue::Array(field)) => field
                    .iter()
                    .map(aosabi_scalars::js_string::to_string)
                    .collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        let code = match error.extensions.get("code") {
            None | Some(JsValue::Undefined | JsValue::Null) => String::from("ERROR"),
            Some(code) => aosabi_scalars::js_string::to_string(code),
        };
        return Some(vec![UserError {
            field,
            message: to_string_lossy(&error.message).into_owned(),
            code,
        }]);
    }
    None
}

/// Returns the payload of a successful call, `{...result, userErrors: []}`.
///
/// A result that is no object spreads no key.
#[must_use]
pub fn success_payload(result: &Value) -> Value {
    let mut entries: Vec<(String, Value)> = Vec::new();
    if let Value::Object(object) = result {
        for key in object.own_keys() {
            let value = Rc::clone(object).get(&key);
            entries.push((key, value));
        }
    }
    // A key the result holds already keeps its place and takes the new value.
    let empty = Value::from(Vec::<Value>::new());
    match entries.iter_mut().find(|(key, _)| key == "userErrors") {
        Some(entry) => entry.1 = empty,
        None => entries.push((String::from("userErrors"), empty)),
    }
    Value::Object(PlainObject::from_entries(entries))
}

/// Returns the payload of an expected failure, `{userErrors}`.
#[must_use]
pub fn failure_payload(user_errors: Vec<UserError>) -> Value {
    let list: Vec<Value> = user_errors.into_iter().map(IntoValue::into_value).collect();
    Value::Object(PlainObject::from_entries(vec![(
        String::from("userErrors"),
        Value::from(list),
    )]))
}
