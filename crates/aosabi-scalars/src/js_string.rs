// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the string a template literal renders a value as.
 *
 * Translation of the ECMA-262 abstract operation ToString and of the
 * conversions it reaches for an array and a plain object, read at
 * https://tc39.es/ecma262/#sec-tostring. The hooks interpolate a value into
 * a message with `${value}`, which is ToString of the value, and with
 * `${JSON.stringify(value)}`, which is ToString of the stringify result.
 */

//! ECMA-262 `ToString` over the JSON value model.

use alloc::string::String;

use aosabi_json::{JsString, JsValue};

/// Returns `ToString(value)` for a value of the model.
///
/// The table of <https://tc39.es/ecma262/#sec-tostring> maps `undefined`,
/// `null`, a boolean, a number and a string directly. An object goes through
/// `ToPrimitive` with hint string, which calls its `toString` method. For an
/// array that method is `Array.prototype.toString`, which is `join` with the
/// comma separator, and for a plain object it is `Object.prototype.toString`,
/// which yields `[object Object]`.
#[must_use]
pub fn to_string(value: &JsValue) -> String {
    match value {
        JsValue::Undefined => String::from("undefined"),
        JsValue::Null => String::from("null"),
        JsValue::Bool(true) => String::from("true"),
        JsValue::Bool(false) => String::from("false"),
        JsValue::Number(number) => number_to_string(*number),
        JsValue::String(text) => string_text(text),
        JsValue::Array(elements) => array_join(elements),
        // A host object carried through unread has no `toString` of its own
        // the model reads, so it takes the one of a plain object.
        JsValue::Object(_) | JsValue::Opaque(_) => String::from("[object Object]"),
    }
}

pub use aosabi_json::number::number_to_string;

/// Returns `Array.prototype.join` of `elements` with the comma separator.
///
/// An element that is `undefined` or `null` contributes the empty string, and
/// every other element contributes its `ToString`. A nested array joins the
/// same way, because its `ToString` is this function.
#[must_use]
pub fn array_join(elements: &[JsValue]) -> String {
    let mut out = String::new();
    for (index, element) in elements.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        if !matches!(element, JsValue::Undefined | JsValue::Null) {
            out.push_str(&to_string(element));
        }
    }
    out
}

/// Returns the text of a JavaScript string for a message.
///
/// A [`JsString`] carries the UTF-16 shape of the JavaScript string as WTF-8,
/// so a lone surrogate has no UTF-8 form. It becomes U+FFFD here, which is a
/// limit of the UTF-8 `message` of the GraphQL error and not of the scalar,
/// and every other string comes back unchanged.
#[must_use]
pub fn string_text(text: &JsString) -> String {
    String::from_utf8_lossy(text.as_wtf8()).into_owned()
}

/// Returns `${JSON.stringify(value)}`.
///
/// `JSON.stringify` returns `undefined` for a value that has no JSON form,
/// which in this model is `undefined` itself, and the template literal
/// renders that result through `ToString`, so the message reads `undefined`.
/// Every other value is the JSON text the writer of aosabi-json produces.
#[must_use]
pub fn json_stringify_to_string(value: &JsValue) -> String {
    aosabi_json::to_string(value).unwrap_or_else(|| String::from("undefined"))
}
