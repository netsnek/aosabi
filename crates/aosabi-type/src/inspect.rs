// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the value printer of a message, which names a value the way
 * graphql-js names it inside an error.
 *
 * Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
 */

//! The value printer of the type layer.
//!
//! Every message that quotes a value, a coercion failure, an unexpected
//! configuration or a value that has no AST, runs it through [`inspect()`],
//! so the quoting, the item limit and the depth limit are bytes of the
//! message. The reference prints any JavaScript value. The port prints a
//! [`JsValue`], which is the value model the type layer coerces, and a
//! GraphQL type prints through its `Display`, because a type has a `toJSON`
//! that returns its `toString` and the reference prints the string a
//! `toJSON` returns as it is.

use alloc::{
    format,
    string::String,
    vec::Vec, //
};

use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::definition::number_to_string;

/// The count of items printed of an array before the rest is summarized.
const MAX_ARRAY_LENGTH: usize = 10;

/// The count of enclosing objects above which an object or an array prints
/// as a tag.
const MAX_RECURSIVE_DEPTH: usize = 2;

/// Used to print values in error messages.
///
/// `value` prints as the reference prints the JavaScript value it stands
/// for: a string as `JSON.stringify(value)`, the other primitives as
/// `String(value)`, an array as its first ten items and an object as its
/// properties, each to a depth of two.
///
/// Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
#[must_use]
pub fn inspect(value: &JsValue) -> String {
    format_value(value, 0)
}

/// Formats one value with `seen_values` enclosing objects above it.
///
/// `seen_values` is the length of the `seenValues` list of the reference,
/// which grows by one per enclosing object or array. A value of the model
/// holds no reference and so no cycle, which is why the list never holds a
/// value twice and `[Circular]` has no reading. A function and a `toJSON`
/// have no reading in the model either. The writer answers `None` for an
/// undefined value alone, and a string is never one.
///
/// Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
fn format_value(value: &JsValue, seen_values: usize) -> String {
    match value {
        JsValue::String(_) => aosabi_json::to_string(value).unwrap_or_default(),
        JsValue::Undefined => String::from("undefined"),
        JsValue::Null => String::from("null"),
        JsValue::Bool(boolean) => String::from(if *boolean { "true" } else { "false" }),
        JsValue::Number(number) => number_to_string(*number),
        JsValue::Array(array) => format_array(array, seen_values + 1),
        JsValue::Object(object) => format_object(object, seen_values + 1),
        // A host object carried through unread has no own enumerable key,
        // which `formatObject` prints as the empty object.
        JsValue::Opaque(_) => String::from("{}"),
    }
}

/// Formats an object as `{ key: value, ... }`.
///
/// The entries are the own keys in the order `Object.entries` lists them,
/// and the tag of a plain object is `Object`. A key prints as its text, and
/// a lone surrogate in a key has no text, so the replacement character
/// stands in for it.
///
/// Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
fn format_object(object: &JsObject, seen_values: usize) -> String {
    if object.is_empty() {
        return String::from("{}");
    }

    if seen_values > MAX_RECURSIVE_DEPTH {
        return String::from("[Object]");
    }

    let properties: Vec<String> = object
        .own_keys()
        .map(|(key, value)| {
            format!(
                "{}: {}",
                String::from_utf8_lossy(key.as_wtf8()),
                format_value(value, seen_values)
            )
        })
        .collect();
    format!("{{ {} }}", properties.join(", "))
}

/// Formats an array as `[item, ...]`, ten items at most.
///
/// Derived from graphql-js 16.12.0, src/jsutils/inspect.ts, MIT.
fn format_array(array: &[JsValue], seen_values: usize) -> String {
    if array.is_empty() {
        return String::from("[]");
    }

    if seen_values > MAX_RECURSIVE_DEPTH {
        return String::from("[Array]");
    }

    let len = MAX_ARRAY_LENGTH.min(array.len());
    let remaining = array.len() - len;
    let mut items: Vec<String> = Vec::new();

    for item in &array[..len] {
        items.push(format_value(item, seen_values));
    }

    if remaining == 1 {
        items.push(String::from("... 1 more item"));
    } else if remaining > 1 {
        items.push(format!("... {remaining} more items"));
    }

    format!("[{}]", items.join(", "))
}
