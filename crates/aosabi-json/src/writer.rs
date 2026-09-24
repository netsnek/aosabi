// SPDX-License-Identifier: MIT
/*
 * aosabi-json writer: a value as JSON.stringify writes it, with no replacer,
 * with no space or with the gap a space argument gives.
 *
 * The writer of the empty gap writes no whitespace and no newline, and the
 * writer of a gap writes the newlines and the indent the two clauses add when
 * the gap is not empty.
 */

//! The writer, which appends to a `Vec<u8>`.
//!
//! [`write()`] appends the bytes of one value, [`to_bytes`] and [`to_string`]
//! wrap it for a caller that wants a fresh buffer, and [`write_object`]
//! appends an object a caller holds as it stands. The functions
//! [`write_with_gap`] and [`to_string_with_gap`] are the same writer with
//! the gap of a space argument, which is how
//! `JSON.stringify(value, null, space)` indents. The functions agree on the
//! one case where `JSON.stringify` returns undefined rather than a string,
//! which is an undefined top level value, and answer `None` for it.
//!
//! The bytes are UTF-8 by construction, because a lone surrogate is written
//! as its escape and every other code point as its UTF-8. A caller that hands
//! the text on as bytes, the body of an answer and the line of the logger
//! among them, takes [`write()`], [`to_bytes`] or [`write_object`], so the
//! body of an answer is never checked again, and the line of the logger is
//! checked once only while a watcher of the console reads it as text.
//! [`to_string`] and [`to_string_with_gap`] check the bytes once as they
//! become a string, with no copy.

use alloc::{
    string::String,
    vec::Vec, //
};

use crate::{
    number,
    string,
    value::{
        JsObject,
        JsValue, //
    }, //
};

/// Appends `value` to `out` the way `JSON.stringify(value)` writes it.
///
/// The number of bytes appended is returned. When `value` is undefined
/// nothing is appended and the result is `None`, which is `JSON.stringify`
/// returning undefined for a top level value that has no JSON form.
///
/// [stringify]: https://tc39.es/ecma262/#sec-json.stringify
pub fn write(out: &mut Vec<u8>, value: &JsValue) -> Option<usize> {
    if value.is_undefined() {
        return None;
    }
    let start = out.len();
    write_property(out, value);
    Some(out.len() - start)
}

/// Returns the bytes of `JSON.stringify(value)` in a fresh buffer.
///
/// The result is `None` when `value` is undefined, as for [`write()`].
#[must_use]
pub fn to_bytes(value: &JsValue) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    write(&mut out, value).map(|_| out)
}

/// Returns `JSON.stringify(value)` as a string.
///
/// The output of the writer is always UTF-8, because a lone surrogate is
/// written as its escape and every other code point is written as its UTF-8,
/// so the check of the bytes passes, the buffer becomes the string with no
/// copy and no byte is ever replaced. The result is `None` when `value` is
/// undefined, as for [`write()`].
// A `from_utf8_unchecked` would skip this pass.
#[must_use]
pub fn to_string(value: &JsValue) -> Option<String> {
    to_bytes(value).map(into_string)
}

/// Appends `value` to `out` the way `JSON.stringify(value, null, space)`
/// writes it with the gap `gap`.
///
/// The number of bytes appended is returned, and an undefined `value` appends
/// nothing and answers `None`, as for [`write()`]. An empty `gap` gives the
/// bytes of [`write()`]. A non empty `gap` writes `{}` and `[]` for an empty
/// object and an empty array, and otherwise the opening bracket, a newline,
/// the indent and the gap before every member, `",\n"` between two members, a
/// newline and the indent before the closing bracket, and `": "` between a
/// key and its value. A member whose value is undefined is left out of an
/// object and written as `null` in an array, and the keys keep the order
/// [`write()`] gives them.
///
/// The gap is taken as it is given. The function `JSON.stringify` derives it
/// from its space argument, at most ten spaces for a number and the first ten
/// code units of a string, and the caller applies that rule.
///
/// [object]: https://tc39.es/ecma262/#sec-serializejsonobject
/// [array]: https://tc39.es/ecma262/#sec-serializejsonarray
/// [stringify]: https://tc39.es/ecma262/#sec-json.stringify
pub fn write_with_gap(out: &mut Vec<u8>, value: &JsValue, gap: &str) -> Option<usize> {
    if gap.is_empty() {
        return write(out, value);
    }
    if value.is_undefined() {
        return None;
    }
    let start = out.len();
    let mut indent = Vec::new();
    write_property_with_gap(out, value, gap.as_bytes(), &mut indent);
    Some(out.len() - start)
}

/// Returns `JSON.stringify(value, null, space)` as a string, written with the
/// gap `gap`.
///
/// The text is what [`write_with_gap`] appends, and the result is `None`
/// when `value` is undefined. The output is always UTF-8 for the reason
/// [`to_string`] gives, and `gap` is a `str`, so the check of the bytes
/// passes and the buffer becomes the string with no copy.
// A `from_utf8_unchecked` would skip this pass.
#[must_use]
pub fn to_string_with_gap(value: &JsValue, gap: &str) -> Option<String> {
    let mut out = Vec::new();
    write_with_gap(&mut out, value, gap)?;
    Some(into_string(out))
}

/// Returns the string the bytes of the writer spell.
///
/// The bytes are UTF-8 by construction, so `String::from_utf8` keeps the
/// buffer as the string. A byte that is no UTF-8 would be replaced as
/// `String::from_utf8_lossy` replaces it, which no output of the writer
/// reaches.
fn into_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

/// Appends one property value.
///
/// This is `SerializeJSONProperty` with no `toJSON` and no replacer, which is
/// what a value model with no functions can reach. An undefined value is the
/// clause's undefined result and appends nothing, and each caller decides
/// what that means where it sits. An opaque host object has no own enumerable
/// property, so it is the empty object, `{}`, which is what `JSON.stringify`
/// writes of a web `File`.
fn write_property(out: &mut Vec<u8>, value: &JsValue) {
    match value {
        JsValue::Undefined => {}
        JsValue::Null => out.extend_from_slice(b"null"),
        JsValue::Bool(true) => out.extend_from_slice(b"true"),
        JsValue::Bool(false) => out.extend_from_slice(b"false"),
        JsValue::Number(number) => number::write_f64(out, *number),
        JsValue::String(text) => string::escape(out, text),
        JsValue::Array(items) => write_array(out, items),
        JsValue::Object(object) => write_object(out, object),
        JsValue::Opaque(_) => out.extend_from_slice(b"{}"),
    }
}

/// Appends an object in `OrdinaryOwnPropertyKeys` order.
///
/// A property whose value is undefined is left out, and an object with no
/// remaining property is `{}`. The bytes are those [`write()`] appends for
/// the object as a value, so a caller that holds `object` writes it where it
/// stands, with no value made around it.
///
/// [object]: https://tc39.es/ecma262/#sec-serializejsonobject
pub fn write_object(out: &mut Vec<u8>, object: &JsObject) {
    out.push(b'{');
    let mut first = true;
    for (key, value) in object.own_keys() {
        if value.is_undefined() {
            continue;
        }
        if !first {
            out.push(b',');
        }
        first = false;
        string::escape(out, key);
        out.push(b':');
        write_property(out, value);
    }
    out.push(b'}');
}

/// Appends an array in element order.
///
/// An element that is undefined is written as `null`, because an array keeps
/// its length.
///
/// [array]: https://tc39.es/ecma262/#sec-serializejsonarray
fn write_array(out: &mut Vec<u8>, items: &[JsValue]) {
    out.push(b'[');
    for (position, item) in items.iter().enumerate() {
        if position > 0 {
            out.push(b',');
        }
        if item.is_undefined() {
            out.extend_from_slice(b"null");
        } else {
            write_property(out, item);
        }
    }
    out.push(b']');
}

/// Appends one property value with a non empty gap.
///
/// This is `SerializeJSONProperty` as [`write_property`] reads it, with the
/// object and the array written by their clauses for a non empty gap.
/// `indent` is the indent of the clauses, the gap repeated once per enclosing
/// object or array, and it is the same on return as on entry.
fn write_property_with_gap(out: &mut Vec<u8>, value: &JsValue, gap: &[u8], indent: &mut Vec<u8>) {
    match value {
        JsValue::Array(items) => write_array_with_gap(out, items, gap, indent),
        JsValue::Object(object) => write_object_with_gap(out, object, gap, indent),
        _ => write_property(out, value),
    }
}

/// Appends an object in `OrdinaryOwnPropertyKeys` order with a non empty gap.
///
/// The members are separated by a comma, a newline and the indent, a key is
/// followed by a colon and one space, and an object with no member that is
/// not undefined is `{}`.
///
/// [object]: https://tc39.es/ecma262/#sec-serializejsonobject
fn write_object_with_gap(out: &mut Vec<u8>, object: &JsObject, gap: &[u8], indent: &mut Vec<u8>) {
    let stepback = indent.len();
    indent.extend_from_slice(gap);
    out.push(b'{');
    let mut first = true;
    for (key, value) in object.own_keys() {
        if value.is_undefined() {
            continue;
        }
        if !first {
            out.push(b',');
        }
        first = false;
        out.push(b'\n');
        out.extend_from_slice(indent);
        string::escape(out, key);
        out.extend_from_slice(b": ");
        write_property_with_gap(out, value, gap, indent);
    }
    indent.truncate(stepback);
    if !first {
        out.push(b'\n');
        out.extend_from_slice(indent);
    }
    out.push(b'}');
}

/// Appends an array in element order with a non empty gap.
///
/// The elements are separated by a comma, a newline and the indent, an
/// element that is undefined is written as `null`, and an empty array is
/// `[]`.
///
/// [array]: https://tc39.es/ecma262/#sec-serializejsonarray
fn write_array_with_gap(out: &mut Vec<u8>, items: &[JsValue], gap: &[u8], indent: &mut Vec<u8>) {
    if items.is_empty() {
        out.extend_from_slice(b"[]");
        return;
    }
    let stepback = indent.len();
    indent.extend_from_slice(gap);
    out.push(b'[');
    for (position, item) in items.iter().enumerate() {
        if position > 0 {
            out.push(b',');
        }
        out.push(b'\n');
        out.extend_from_slice(indent);
        if item.is_undefined() {
            out.extend_from_slice(b"null");
        } else {
            write_property_with_gap(out, item, gap, indent);
        }
    }
    indent.truncate(stepback);
    out.push(b'\n');
    out.extend_from_slice(indent);
    out.push(b']');
}
