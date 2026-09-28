// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the parser of a multipart body.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/request-parser/post-multipart.js, MIT.
 */

//! The parser of a multipart body.
//!
//! The body is read with `request.formData()`, the part `operations` is
//! parsed as the parameters, and every entry of the part `map` sets the part
//! it names at each key path it lists, a path through `__proto__`,
//! `constructor` or `prototype` skipped. A missing part sets `null`, which
//! the coercion of a non null variable refuses.
//!
//! The walk of `map` is the reference's `for in` and `for of` over JSON
//! values, so a map that is an array or a string walks its indices, and a
//! path through a value that is no object throws the `TypeError` V8 throws
//! for `in` and for an assignment in strict mode. A path through a member an
//! object inherits, `toString` among them, reads as absent here. A file sits
//! at its path as an empty object and in the list of files of the parameters
//! (see [`crate::plugin::GraphQLParams`]).

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    ErrorObject,
    Value,
    errors::create_graphql_error, //
};
use aosabi_hooks::LocalBoxFuture;
use aosabi_http::Request;
use aosabi_json::{
    JsObject,
    JsValue,
    Opaque,
    number_to_string,
    value::{
        OpaqueHandle,
        array_index, //
    }, //
};
use aosabi_lang::{
    GraphQLErrorOptions,
    js_string::to_string_lossy, //
};

use crate::{
    file::FileBody,
    form_data::{
        FormDataValue,
        request_form_data, //
    },
    plugin::{
        RequestParserResult,
        UploadedFile, //
    },
    request_parser::{
        json_parse::json_parse,
        utils::is_content_type_match, //
    }, //
};

/// Returns whether `request` is a POST of a form with parts, `isPOSTMultipartRequest(request)`.
#[must_use]
pub fn is_post_multipart_request(request: &Request) -> bool {
    request.method == "POST" && is_content_type_match(request, "multipart/form-data")
}

/// Returns the `GraphQLError` of `message` with no extension, `createGraphQLError(message)`.
fn refusal(message: &str) -> Value {
    Value::from(ErrorObject::GraphQL(create_graphql_error(
        message,
        GraphQLErrorOptions::default(),
    )))
}

/// Parses the parameters of a multipart body, `parsePOSTMultipartRequest(request)`.
pub fn parse_post_multipart_request(
    request: Rc<Request>,
) -> LocalBoxFuture<'static, Result<RequestParserResult, Value>> {
    Box::pin(async move {
        let request_body = request_form_data(&request).await?;
        let operations_str = match request_body.get("operations") {
            None => return Err(refusal("Missing multipart form field \"operations\"")),
            Some(FormDataValue::Text(text)) if text.is_empty() => {
                return Err(refusal("Missing multipart form field \"operations\""));
            }
            Some(FormDataValue::File(_)) => {
                return Err(refusal(
                    "Multipart form field \"operations\" must be a string",
                ));
            }
            Some(FormDataValue::Text(text)) => text,
        };
        let Ok(mut operations) = json_parse(operations_str) else {
            return Err(refusal(
                "Multipart form field \"operations\" must be a valid JSON string",
            ));
        };
        let mut files = Vec::new();
        if let Some(map_str) = request_body.get("map") {
            let FormDataValue::Text(map_str) = map_str else {
                return Err(refusal("Multipart form field \"map\" must be a string"));
            };
            let Ok(map) = json_parse(map_str) else {
                return Err(refusal(
                    "Multipart form field \"map\" must be a valid JSON string",
                ));
            };
            for (file_index, keys) in for_in(&map) {
                let file = request_body.get(&file_index);
                for key in for_of(&keys)? {
                    let value = match file {
                        None => MapValue::Null,
                        Some(FormDataValue::Text(text)) => MapValue::Text(text.clone()),
                        Some(FormDataValue::File(file)) => MapValue::File(Arc::clone(file)),
                    };
                    set_object_key_path(&mut operations, &key, &value, &mut files)?;
                }
            }
        }
        Ok(RequestParserResult::from_value(operations, files))
    })
}

/// The value a map entry sets, the entry `formData.get(fileIndex)` answers.
enum MapValue {
    /// No entry, `null`.
    Null,
    /// A string entry.
    Text(String),
    /// A file entry.
    File(Arc<FileBody>),
}

/// Returns the keys `for (const key in value)` visits with the value at each.
fn for_in(value: &JsValue) -> Vec<(String, JsValue)> {
    match value {
        JsValue::Object(object) => object
            .own_keys()
            .map(|(key, value)| (to_string_lossy(key).into_owned(), value.clone()))
            .collect(),
        JsValue::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| (index.to_string(), item.clone()))
            .collect(),
        JsValue::String(text) => {
            let units: Vec<u16> = to_string_lossy(text).encode_utf16().collect();
            units
                .iter()
                .enumerate()
                .map(|(index, unit)| {
                    (
                        index.to_string(),
                        JsValue::from(String::from_utf16_lossy(&[*unit])),
                    )
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Returns the values `for (const key of keys)` visits.
fn for_of(keys: &JsValue) -> Result<Vec<JsValue>, Value> {
    match keys {
        JsValue::Array(items) => Ok(items.clone()),
        JsValue::String(text) => Ok(to_string_lossy(text)
            .chars()
            .map(|c| JsValue::from(c.to_string()))
            .collect()),
        _ => Err(Value::from(ErrorObject::type_error("keys is not iterable"))),
    }
}

/// Returns `String(value)` of a primitive, as a message of V8 names it.
fn primitive_text(value: &JsValue) -> String {
    match value {
        JsValue::Undefined => String::from("undefined"),
        JsValue::Null => String::from("null"),
        JsValue::Bool(value) => value.to_string(),
        JsValue::Number(number) => number_to_string(*number),
        JsValue::String(text) => to_string_lossy(text).into_owned(),
        JsValue::Array(_) | JsValue::Object(_) | JsValue::Opaque(_) => {
            String::from("[object Object]")
        }
    }
}

/// Returns the kind `typeof` names of a primitive.
fn primitive_kind(value: &JsValue) -> &'static str {
    match value {
        JsValue::Bool(_) => "boolean",
        JsValue::Number(_) => "number",
        _ => "string",
    }
}

/// Returns whether `key in current` holds, or the `TypeError` of `in` over a primitive.
fn has_key(current: &JsValue, key: &str) -> Result<bool, Value> {
    match current {
        JsValue::Object(object) => Ok(object.contains_key(key)),
        // A file set at an earlier key path has no own property the path
        // reads.
        JsValue::Opaque(_) => Ok(false),
        JsValue::Array(items) => Ok(key == "length"
            || array_index(key.as_bytes()).is_some_and(|index| (index as usize) < items.len())),
        primitive => Err(Value::from(ErrorObject::type_error(format!(
            "Cannot use 'in' operator to search for '{key}' in {}",
            primitive_text(primitive)
        )))),
    }
}

/// Assigns `value` to `current[key]` in strict mode.
///
/// # Errors
///
/// Returns the `TypeError` of an assignment to a primitive, `null` or
/// `undefined`, and the `RangeError` of a length an array refuses.
fn assign(current: &mut JsValue, key: &str, value: JsValue) -> Result<(), Value> {
    match current {
        JsValue::Object(object) => {
            object.insert(key, value);
            Ok(())
        }
        // A property set on a file is no part of any JSON a parameter
        // becomes, so the assignment has no reading.
        JsValue::Opaque(_) => Ok(()),
        JsValue::Array(items) => {
            if let Some(index) = array_index(key.as_bytes()) {
                let index = index as usize;
                if index >= items.len() {
                    items.resize(index + 1, JsValue::Undefined);
                }
                items[index] = value;
            } else if key == "length" {
                let length = match value {
                    JsValue::Null => Some(0),
                    JsValue::String(text) => to_string_lossy(&text).trim().parse::<u32>().ok(),
                    _ => None,
                };
                let Some(length) = length else {
                    return Err(Value::from(ErrorObject::range_error(
                        "Invalid array length",
                    )));
                };
                items.resize(length as usize, JsValue::Undefined);
            }
            // Any other key is a property of the array that JSON never writes.
            Ok(())
        }
        JsValue::Null | JsValue::Undefined => Err(Value::from(ErrorObject::type_error(format!(
            "Cannot set properties of {} (setting '{key}')",
            primitive_text(current)
        )))),
        primitive => Err(Value::from(ErrorObject::type_error(format!(
            "Cannot create property '{key}' on {} '{}'",
            primitive_kind(primitive),
            primitive_text(primitive)
        )))),
    }
}

/// Sets `value` at the dotted `key_path` of `object`, `setObjectKeyPath(object, keyPath, value)`.
fn set_object_key_path(
    object: &mut JsValue,
    key_path: &JsValue,
    value: &MapValue,
    files: &mut Vec<UploadedFile>,
) -> Result<(), Value> {
    let key_path = match key_path {
        JsValue::String(key_path) => to_string_lossy(key_path).into_owned(),
        JsValue::Null => {
            return Err(Value::from(ErrorObject::type_error(
                "Cannot read properties of null (reading 'split')",
            )));
        }
        JsValue::Undefined => {
            return Err(Value::from(ErrorObject::type_error(
                "Cannot read properties of undefined (reading 'split')",
            )));
        }
        _ => {
            return Err(Value::from(ErrorObject::type_error(
                "keyPath.split is not a function",
            )));
        }
    };
    let keys: Vec<&str> = key_path.split('.').collect();
    let mut current = object;
    for (i, key) in keys.iter().enumerate() {
        if matches!(*key, "__proto__" | "constructor" | "prototype") {
            return Ok(());
        }
        let is_last_key = i == keys.len() - 1;
        if is_last_key {
            let value = match value {
                MapValue::Null => JsValue::Null,
                MapValue::Text(text) => JsValue::from(text.clone()),
                MapValue::File(file) => {
                    files.push(UploadedFile {
                        path: key_path.clone(),
                        file: Arc::clone(file),
                    });
                    let handle: OpaqueHandle = Arc::clone(file) as OpaqueHandle;
                    JsValue::Opaque(Opaque(handle))
                }
            };
            return assign(current, key, value);
        }
        if !has_key(current, key)? {
            assign(current, key, JsValue::Object(JsObject::new()))?;
        }
        current = match current {
            JsValue::Object(object) => match object.get_mut(key) {
                Some(next) => next,
                None => return Ok(()),
            },
            JsValue::Array(items) => match array_index(key.as_bytes())
                .map(|index| index as usize)
                .filter(|index| *index < items.len())
            {
                Some(index) => &mut items[index],
                // The length of an array is a number, which the next step
                // cannot search, and any other key is a property JSON never
                // writes.
                None if *key == "length" => {
                    let next = keys.get(i + 1).copied().unwrap_or_default();
                    let mut length = JsValue::from(u32::try_from(items.len()).unwrap_or(u32::MAX));
                    if matches!(next, "__proto__" | "constructor" | "prototype") {
                        return Ok(());
                    }
                    if i + 2 == keys.len() {
                        return assign(&mut length, next, JsValue::Null);
                    }
                    return has_key(&length, next).map(|_| ());
                }
                None => return Ok(()),
            },
            _ => return Ok(()),
        };
    }
    Ok(())
}
