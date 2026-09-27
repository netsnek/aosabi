// SPDX-License-Identifier: MIT
/*
 * aosabi-execute value inspect: the two value printers of a message.
 *
 * Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
 * Derived from cross-inspect 1.0.1, cjs/index.js, MIT.
 */

//! The value printers of the executor.
//!
//! Two printers write a value into a message. [`inspect()`] is the printer of
//! graphql-js, which the type layer and `toError` use. [`cross_inspect`] is
//! the printer of the `cross-inspect` package, which the executor and the
//! utilities use. The second is a copy of the first with three differences:
//! it recurses to depth 3 where the first recurses to depth 2, it never
//! truncates an array where the first prints ten items, and it prints an
//! error through its `toString` where the first prints it as an object. One
//! set of functions holds both, with the differences as the fields of a
//! private `Printer`.
//!
//! Two reads of the reference have no reading here. A user object with a
//! `toJSON` member is not asked for it, the same limit as the `then` member
//! of a thenable, and a host value other than a type or a directive prints as
//! its tag at every depth, because user code never holds one where a message
//! prints it.

use std::rc::Rc;

use aosabi_json::{
    JsValue,
    number_to_string, //
};

use crate::value::{
    ErrorObject,
    HostValue,
    ObjectLike,
    Value,
    error::error_prototype_to_string, //
};

/// The differences between the two printers.
struct Printer {
    /// `MAX_RECURSIVE_DEPTH`, the count of enclosing values above which an
    /// object or an array prints as a tag.
    max_recursive_depth: usize,
    /// `MAX_ARRAY_LENGTH`, which only graphql-js has.
    max_array_length: Option<usize>,
    /// Whether an instance of `Error` prints through `formatError`, which
    /// only cross-inspect does.
    errors_through_to_string: bool,
}

/// The printer of graphql-js.
const GRAPHQL_JS: Printer = Printer {
    max_recursive_depth: 2,
    max_array_length: Some(10),
    errors_through_to_string: false,
};

/// The printer of cross-inspect.
const CROSS_INSPECT: Printer = Printer {
    max_recursive_depth: 3,
    max_array_length: None,
    errors_through_to_string: true,
};

/// The name `formatError` of cross-inspect writes into every error it prints.
///
/// The condition of that function is the assignment
/// `value.name = 'GraphQLError'` and not a comparison, so it always holds,
/// and the `toString` that follows reads the assigned name.
const ASSIGNED_ERROR_NAME: &str = "GraphQLError";

/// Used to print values in error messages.
///
/// A string prints as `JSON.stringify(value)`, the other primitives as
/// `String(value)`, an array as its first ten items and an object as its
/// properties, each to a depth of two.
///
/// Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
#[must_use]
pub fn inspect(value: &Value) -> String {
    GRAPHQL_JS.format_value(value, &[])
}

/// Used to print values in error messages.
///
/// An array prints whole, an object to a depth of three, and an error through
/// its `toString`.
#[must_use]
pub fn cross_inspect(value: &Value) -> String {
    CROSS_INSPECT.format_value(value, &[])
}

impl Printer {
    /// Formats one value below the values of `seen_values`.
    ///
    /// `typeof` answers `object` for null, a date, an array, an object, a
    /// promise, an error and a host value.
    ///
    /// Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
    fn format_value(&self, value: &Value, seen_values: &[*const ()]) -> String {
        match value {
            Value::String(_) => aosabi_json::to_string(&value.to_js()).unwrap_or_default(),
            Value::Function(function) if function.name.is_empty() => String::from("[function]"),
            Value::Function(function) => format!("[function {}]", function.name),
            Value::Undefined => String::from("undefined"),
            Value::Bool(boolean) => String::from(if *boolean { "true" } else { "false" }),
            Value::Number(number) => number_to_string(*number),
            _ => self.format_object_value(value, seen_values),
        }
    }

    /// Formats a value whose `typeof` is `object`.
    ///
    /// Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
    fn format_object_value(&self, value: &Value, previously_seen_values: &[*const ()]) -> String {
        if let (true, Value::Error(error)) = (self.errors_through_to_string, value) {
            return self.format_error_value(error, previously_seen_values);
        }
        let Some(address) = address_of(value) else {
            return String::from("null");
        };

        if previously_seen_values.contains(&address) {
            return String::from("[Circular]");
        }

        let mut seen_values = previously_seen_values.to_vec();
        seen_values.push(address);

        match value {
            Value::Date(_) | Value::Error(_) | Value::Host(_) => {
                self.format_jsonable(value, &seen_values)
            }
            Value::List(array) => self.format_array(array, &seen_values),
            Value::Object(object) => self.format_object(object, &seen_values),
            _ => String::from("{}"),
        }
    }

    /// Formats an instance of `Error` the way cross-inspect does.
    ///
    /// The items of an `AggregateError` print below the values seen so far,
    /// without the error itself among them, as the reference passes
    /// `previouslySeenValues` on.
    fn format_error_value(&self, error: &ErrorObject, seen_values: &[*const ()]) -> String {
        if let ErrorObject::Aggregate { errors, .. } = error {
            return format!(
                "{}\n{}",
                format_error(error),
                self.format_array(errors, seen_values)
            );
        }
        format_error(error)
    }

    /// Formats a value that carries a `toJSON` method, or that carries none
    /// and has no enumerable own property either.
    ///
    /// A string the method returns prints as it is, and any other result
    /// prints as a value. A `Date` answers its ISO text and null when it is
    /// invalid, a `GraphQLError` answers its formatted error, and a type
    /// answers its `toString`. An error without the method has no enumerable
    /// own property and prints as an empty object.
    ///
    /// Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
    fn format_jsonable(&self, value: &Value, seen_values: &[*const ()]) -> String {
        match value {
            Value::Date(date) => date
                .to_iso_string()
                .unwrap_or_else(|_| String::from("null")),
            Value::Error(error) => match error.as_ref() {
                ErrorObject::GraphQL(error) => {
                    let json_value = Value::from_js(&JsValue::from(error.to_json()));
                    self.format_value(&json_value, seen_values)
                }
                // An error with own properties prints them as the object it is.
                ErrorObject::Custom { props, .. } if !props.is_empty() => {
                    let json_value = Value::from_js(&JsValue::Object((**props).clone()));
                    self.format_value(&json_value, seen_values)
                }
                _ => String::from("{}"),
            },
            Value::Host(HostValue::Type(named)) => named.to_string(),
            Value::Host(HostValue::Directive(directive)) => directive.to_string(),
            Value::Host(HostValue::Schema(_)) => String::from("[GraphQLSchema]"),
            _ => String::from("[Object]"),
        }
    }

    /// Formats an object as `{ key: value, ... }`.
    ///
    /// The entries are the ones `Object.entries` lists.
    ///
    /// Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
    fn format_object(&self, object: &Rc<dyn ObjectLike>, seen_values: &[*const ()]) -> String {
        let keys = object.own_keys();
        if keys.is_empty() {
            return String::from("{}");
        }

        if seen_values.len() > self.max_recursive_depth {
            return format!("[{}]", object.constructor_name());
        }

        let properties: Vec<String> = keys
            .iter()
            .map(|key| {
                let value = Rc::clone(object).get(key);
                format!("{key}: {}", self.format_value(&value, seen_values))
            })
            .collect();
        format!("{{ {} }}", properties.join(", "))
    }

    /// Formats an array as `[item, ...]`.
    ///
    /// Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
    fn format_array(&self, array: &[Value], seen_values: &[*const ()]) -> String {
        if array.is_empty() {
            return String::from("[]");
        }

        if seen_values.len() > self.max_recursive_depth {
            return String::from("[Array]");
        }

        let len = self
            .max_array_length
            .map_or(array.len(), |max| max.min(array.len()));
        let remaining = array.len() - len;
        let mut items: Vec<String> = Vec::new();

        for item in &array[..len] {
            items.push(self.format_value(item, seen_values));
        }

        if remaining == 1 {
            items.push(String::from("... 1 more item"));
        } else if remaining > 1 {
            items.push(format!("... {remaining} more items"));
        }

        format!("[{}]", items.join(", "))
    }
}

/// Formats an error as its `toString` under the name `GraphQLError`.
///
/// The second return of that function is unreachable, see
/// [`ASSIGNED_ERROR_NAME`]. The reference also leaves the assigned name on
/// the error, which a later read of `name` would see. An [`ErrorObject`] is
/// not written to, and nothing in the executor reads the name of an error
/// after it printed it.
fn format_error(value: &ErrorObject) -> String {
    if let ErrorObject::GraphQL(error) = value {
        return error.to_string();
    }
    error_prototype_to_string(&ASSIGNED_ERROR_NAME.into(), &value.message())
}

/// Returns the identity of a value whose `typeof` is `object`, or `None` for null.
///
/// The identity is what `previouslySeenValues.includes(value)` compares. A
/// date and a host value are never inside themselves, so the address of the
/// borrowed value stands in for them.
fn address_of(value: &Value) -> Option<*const ()> {
    match value {
        Value::List(array) => Some(Rc::as_ptr(array).cast()),
        Value::Object(object) => Some(Rc::as_ptr(object).cast()),
        Value::Error(error) => Some(Rc::as_ptr(error).cast()),
        Value::Null => None,
        other => Some(core::ptr::from_ref(other).cast()),
    }
}
