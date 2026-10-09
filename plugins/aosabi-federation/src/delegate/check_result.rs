// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the check of a delegated result and the merge of its
 * data and its errors.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.cjs, MIT.
 */

//! The check of a delegated result, `check_result_and_handle_errors`.
//!
//! A delegation reads the value of its root field out of the result the
//! executor answered and places every error of that result into it.
//! [`merge_data_and_errors`] does the placing: a lone error at a null value
//! is relocated to the path of the field that delegates, several become one
//! `AggregateError` of relocated errors, and the errors of a value that is no
//! null are distributed into it by the segment of their path at each level,
//! so an error lands where its path ends and an error no segment places is
//! unpathed. [`check_result_and_handle_errors`] then resolves the value
//! through [`resolve_external_value`].
//!
//! The reference assigns a placed value back into its parent, `data[segment]
//! = newData`. An object takes the write in place, and a list, which the
//! value model holds behind an `Rc` with no cell, is copied at its first
//! write while its parent still holds it and written in place after that,
//! which reads the same since no other value of a fresh result holds that
//! list. A write into a string throws the `TypeError` of a strict mode
//! assignment, as the module of the reference is strict.
//!
//! A segment reads an own member alone: a property of an object, an element
//! or the length of a list, a character or the length of a string, and
//! nothing of any other value. The reference reads `data[segment]` through
//! the prototype chain as well, so a segment that names an inherited member,
//! `constructor` of an object for one, reads that member there and recurses
//! into it, where the port reads nothing and leaves the error unpathed. A
//! string `errors` member is read as the reference reads a string where it
//! expects an array, and the error of one code unit at a null value carries
//! the message `""`, where the reference's carries `undefined`. An object
//! `errors` member reads as no error. The reference reads `{length: 0}` as no
//! error and relocates the one element of `{length: 1, 0: ...}` at a null
//! value, and it throws `errors is not iterable` or
//! `errors.map is not a function` for every other shape.
//!
//! The option `onLocatedError` of a delegation, which `mergeDataAndErrors`
//! hands each error to, is never set by the federation, so the function
//! takes no such parameter.

use std::rc::Rc;

use aosabi_execute::{
    execute::ResolveInfo,
    path::path_to_array,
    value::{
        ErrorObject,
        Value, //
    }, //
};
use aosabi_json::{
    JsString,
    map::IndexMap,
    value::array_index, //
};
use aosabi_lang::{
    PathSegment,
    js_string::{
        join,
        to_string_lossy, //
    }, //
};
use aosabi_type::Type;

use crate::{
    delegate::resolve_external::resolve_external_value,
    transforms::DelegationContext,
    utils::{
        error_message,
        error_path,
        error_path_segment,
        get_response_key_from_info,
        relocated_error, //
    }, //
};

/// What [`merge_data_and_errors`] answers, the data with its errors placed and the errors it could not place.
pub struct MergedData {
    /// The data, an error when the merge relocated one in its place.
    pub data: Value,
    /// The errors no segment of a path placed, in the order they were met.
    pub unpathed_errors: Vec<Value>,
}

/// Returns the value of the delegated field of `result`, with the errors of the result placed and resolved.
///
/// An absent result reads as `{data: null, errors: []}`, the response key is
/// the field name of the delegation context and the return type is its return
/// type, both always set, so [`get_response_key`] and [`get_return_type`],
/// the defaults of the reference, are not reached, and the path is the
/// response path of the field that delegates.
///
/// # Errors
///
/// Returns the `TypeError` `errors.map is not a function` for an `errors`
/// member that is a string of more than one code unit at a null value, and
/// what [`merge_data_and_errors`] and [`resolve_external_value`] throw.
pub fn check_result_and_handle_errors(
    result: &Value,
    delegation_context: &DelegationContext,
) -> Result<Value, Value> {
    let response_key = delegation_context.field_name.clone();
    let data = member(
        &member(result, &PathSegment::from("data")),
        &PathSegment::Key(response_key),
    );
    let errors = errors_of(&member(result, &PathSegment::from("errors")), &data)?;
    let path = delegation_context
        .info
        .as_ref()
        .map(|info| path_to_array(Some(&info.path)));
    let merged = merge_data_and_errors(data, &errors, path.as_deref(), 1)?;
    resolve_external_value(
        merged.data,
        Rc::from(merged.unpathed_errors),
        &delegation_context.subschema,
        delegation_context.context.as_ref(),
        delegation_context.info.as_ref(),
        Some(&delegation_context.return_type),
        delegation_context.skip_type_merging,
    )
}

/// Places `errors` into `data` by the segments of their paths from `index` on.
///
/// A null or absent `data` takes a lone error relocated to `path` joined with
/// the rest of the error's path, and several errors as one `AggregateError`
/// of relocated errors whose message joins their messages with a comma and a
/// line feed. Any other `data` hands each group of errors that shares the
/// segment at `index` to the member of that segment, in the order the
/// segments first appear, and an error without such a segment, or whose
/// member is absent, is unpathed.
///
/// The groups are an insertion ordered map from the segment to its errors,
/// the `Map` of the reference, so an error finds its group in one lookup,
/// and a list is copied once, at its first write while its parent still
/// holds it, and written in place after that, as the reference writes
/// `data[pathSegment]`. The merge is linear in the errors, the members they
/// name and the length of each list it writes into, since its first write
/// into a list the parent still holds copies that list once.
///
/// # Errors
///
/// Returns the `TypeError` of an assignment into a string, which a path
/// through a string member meets.
pub fn merge_data_and_errors(
    data: Value,
    errors: &[Value],
    path: Option<&[PathSegment]>,
    index: usize,
) -> Result<MergedData, Value> {
    if data.is_nullish() {
        let data = match errors {
            [] => Value::Null,
            [error] => relocated_error(error, new_path(path, error)),
            errors => aggregate_relocated(errors, path),
        };
        return Ok(MergedData {
            data,
            unpathed_errors: Vec::new(),
        });
    }
    if errors.is_empty() {
        return Ok(MergedData {
            data,
            unpathed_errors: Vec::new(),
        });
    }
    let mut unpathed_errors = Vec::new();
    let mut error_map: IndexMap<PathSegment, Vec<Value>> = IndexMap::default();
    for error in errors {
        match error_path_segment(error, index) {
            Some(path_segment) => error_map
                .entry(path_segment)
                .or_default()
                .push(error.clone()),
            None => unpathed_errors.push(error.clone()),
        }
    }
    let mut data = data;
    for (path_segment, path_segment_errors) in error_map {
        let member_value = member(&data, &path_segment);
        if matches!(member_value, Value::Undefined) {
            unpathed_errors.extend(path_segment_errors);
            continue;
        }
        let merged = merge_data_and_errors(member_value, &path_segment_errors, path, index + 1)?;
        data = assign(data, &path_segment, merged.data)?;
        unpathed_errors.extend(merged.unpathed_errors);
    }
    Ok(MergedData {
        data,
        unpathed_errors,
    })
}

/// Returns the response key of the field that delegates.
///
/// # Errors
///
/// Returns the plain error `Data cannot be extracted from result without an
/// explicit key or source schema.` without a resolve information.
pub fn get_response_key(info: Option<&ResolveInfo>) -> Result<String, Value> {
    info.map(|info| String::from(get_response_key_from_info(info)))
        .ok_or_else(|| {
            Value::from(ErrorObject::error(
                "Data cannot be extracted from result without an explicit key or source schema.",
            ))
        })
}

/// Returns the return type of the field that delegates.
///
/// # Errors
///
/// Returns the plain error `Return type cannot be inferred without a source
/// schema.` without a resolve information.
pub fn get_return_type(info: Option<&ResolveInfo>) -> Result<Type, Value> {
    info.map(|info| info.return_type.clone()).ok_or_else(|| {
        Value::from(ErrorObject::error(
            "Return type cannot be inferred without a source schema.",
        ))
    })
}

/// Returns the member a path segment names, `value[segment]`, and `undefined` where there is none.
///
/// An object answers its own property, a list its element or its length, a
/// string the character at an index or its length in code units, and every
/// other value nothing.
#[must_use]
pub fn member(value: &Value, segment: &PathSegment) -> Value {
    match (value, segment) {
        (Value::Object(object), PathSegment::Key(key)) => Rc::clone(object).get(key),
        (Value::Object(object), PathSegment::Index(index)) => {
            Rc::clone(object).get(&index.to_string())
        }
        (Value::List(items), segment) => match list_index(segment) {
            Some(index) => items.get(index).cloned().unwrap_or(Value::Undefined),
            None if *segment == PathSegment::from("length") => length_number(items.len()),
            None => Value::Undefined,
        },
        (Value::String(string), segment) => string_member(string, segment),
        _ => Value::Undefined,
    }
}

/// Returns the errors of a result's `errors` member, `result.errors == null ? [] : result.errors`.
///
/// A list answers its items, and any other value but a string has no length
/// and reads as no error. A string is read as `mergeDataAndErrors` reads it
/// at `data`. At a null value an empty string is no error, a string of one
/// code unit is that unit as the one error, and a longer string throws, since
/// a string has no `map`. At any other value `for...of` yields its code
/// points.
///
/// # Errors
///
/// Returns the `TypeError` `errors.map is not a function` for a string of
/// more than one code unit at a null value.
fn errors_of(errors: &Value, data: &Value) -> Result<Vec<Value>, Value> {
    match errors {
        Value::List(items) => Ok(items.as_ref().clone()),
        Value::String(string) if data.is_nullish() => match code_units(string).as_slice() {
            [] => Ok(Vec::new()),
            [unit] => Ok(vec![Value::String(JsString::from_utf16(&[*unit]))]),
            _ => Err(Value::from(ErrorObject::type_error(
                "errors.map is not a function",
            ))),
        },
        Value::String(string) => Ok(to_string_lossy(string)
            .chars()
            .map(|code_point| {
                Value::String(JsString::from_utf16(
                    code_point.encode_utf16(&mut [0_u16; 2]),
                ))
            })
            .collect()),
        _ => Ok(Vec::new()),
    }
}

/// Returns `path` joined with the rest of the error's own path, as the relocation writes it.
fn new_path(path: Option<&[PathSegment]>, error: &Value) -> Option<Vec<PathSegment>> {
    let own = error_path(error);
    match path {
        None => own,
        Some(path) => match own {
            None => Some(path.to_vec()),
            Some(own) => Some(
                path.iter()
                    .cloned()
                    .chain(own.into_iter().skip(1))
                    .collect(),
            ),
        },
    }
}

/// Returns the `AggregateError` of several errors at a null value, each relocated.
fn aggregate_relocated(errors: &[Value], path: Option<&[PathSegment]>) -> Value {
    let relocated = errors
        .iter()
        .map(|error| relocated_error(error, new_path(path, error)))
        .collect();
    let messages: Vec<JsString> = errors.iter().map(error_message).collect();
    Value::from(ErrorObject::Aggregate {
        message: join(&messages, ",\n"),
        errors: relocated,
    })
}

/// Writes `value` to the member `segment` of `data` and returns the data, `data[segment] = value`.
///
/// A list that another value still holds is copied before the write, and a
/// list only `data` holds is written in place.
fn assign(data: Value, segment: &PathSegment, value: Value) -> Result<Value, Value> {
    match data {
        Value::Object(object) => {
            object.set(&segment_key(segment), value);
            Ok(Value::Object(object))
        }
        Value::List(mut items) => {
            if let Some(index) = list_index(segment)
                && index < items.len()
            {
                Rc::make_mut(&mut items)[index] = value;
            }
            Ok(Value::List(items))
        }
        Value::String(string) => Err(Value::from(ErrorObject::type_error(format!(
            "Cannot assign to read only property '{}' of string '{}'",
            segment_key(segment),
            String::from_utf8_lossy(string.as_wtf8())
        )))),
        data => Ok(data),
    }
}

/// Returns the property key of a segment, an index written in decimal.
fn segment_key(segment: &PathSegment) -> String {
    match segment {
        PathSegment::Key(key) => key.clone(),
        PathSegment::Index(index) => index.to_string(),
    }
}

/// Returns the array index a segment names, a canonical numeric key included.
fn list_index(segment: &PathSegment) -> Option<usize> {
    match segment {
        PathSegment::Index(index) => Some(*index),
        PathSegment::Key(key) => array_index(key.as_bytes()).map(|index| index as usize),
    }
}

/// Returns a length as the number a JavaScript `length` is.
fn length_number(length: usize) -> Value {
    // A length the value model holds is far below 2^53.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a length of a value of a request is far below 2^53"
    )]
    let length = length as f64;
    Value::Number(length)
}

/// Returns the member of a string a segment names, a character, the length or nothing.
fn string_member(string: &JsString, segment: &PathSegment) -> Value {
    let units = code_units(string);
    match list_index(segment) {
        Some(index) => units.get(index).map_or(Value::Undefined, |unit| {
            Value::String(JsString::from_utf16(&[*unit]))
        }),
        None if *segment == PathSegment::from("length") => length_number(units.len()),
        None => Value::Undefined,
    }
}

/// Returns the UTF-16 code units of a string.
///
/// A lone surrogate reads as U+FFFD, the one place the reading differs from
/// the code units of the reference, and no string a remote writes into a path
/// or an `errors` member carries one.
fn code_units(string: &JsString) -> Vec<u16> {
    to_string_lossy(string).encode_utf16().collect()
}
