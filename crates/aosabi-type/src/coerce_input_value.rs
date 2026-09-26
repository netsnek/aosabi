// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the coercion of a value of the value model against an input
 * type, with the path and the error prefix of the reference.
 *
 * Derived from graphql-js 16.12.0, src/utilities/coerceInputValue.ts, MIT.
 * Derived from graphql-js 16.12.0, src/jsutils/Path.ts, MIT.
 */

//! A value coerced against an input type.
//!
//! The reference reports every problem through a callback that receives the
//! path, the offending value and a `GraphQLError`, and the default callback
//! prefixes the message with the inspected value and the printed path and
//! throws. A callback of the port returns a `Result`, so a callback that
//! throws returns an error and a callback that collects returns `Ok(())`, and
//! the coercion goes on exactly where the reference goes on.
//!
//! A value the caller owns is coerced where each of its parts stands. A list
//! keeps its items in their vector, an input object whose request wrote its
//! fields in the order of their definitions keeps its keys and its entries,
//! and a leaf whose `parseValue` answers its input stays as it is, so no
//! string, no key and no vector of the value is copied or moved. A value
//! another holder shares, the variables of a request its context and its
//! hooks read among them, is read where it stands and coerced into a fresh
//! value, with a copy of each leaf its `parseValue` answers. The reference
//! makes a fresh array for a list and a fresh object for an input object and
//! keeps every leaf its `parseValue` answers, so either way the coerced value
//! is the one the reference makes, and the value the caller shares stays as
//! the reference leaves it.
//!
//! A problem names the value at its own place, which still stands there when
//! it is reported, and three problems name a whole input object: a required
//! field the object leaves out, a key no field defines, and a one of object
//! of other than one key. The coercion looks for those three before it
//! coerces a field of an object, and an object that meets one is coerced as
//! it stands, read where it stands into a fresh object, so every message
//! inspects the value the reference inspects.
//!
//! The messages of this module are the ones `getVariableValues` reports for a
//! variable of the wrong shape, which the executor prints on the wire, and
//! every one of them is copied from the reference.

use alloc::{
    borrow::Cow,
    boxed::Box,
    format,
    string::String,
    vec::Vec, //
};
use core::mem;

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    JsStringBuilder,
    OriginalError,
    PathSegment, //
};

use crate::{
    definition::{
        DefinitionError,
        InputField,
        InputObjectType,
        ObjMap,
        Parsed,
        Type,
        is_non_null_type, //
    },
    did_you_mean::did_you_mean,
    inspect::inspect,
    print_path_array::print_path_array,
    suggestion_list::suggestion_list, //
};

/// One key of a [`Path`], a list index or a field name.
///
/// A name borrows the name of its field definition, so a step of a path costs
/// no allocation, and [`path_to_array`] makes the segments of a path only for
/// a problem the coercion reports.
///
/// Derived from graphql-js 16.12.0, src/jsutils/Path.ts, MIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKey<'a> {
    /// A list index.
    Index(usize),
    /// A field name.
    Name(&'a str),
}

/// A path into a value, as a linked list from the leaf to the root.
///
/// `key` is a field name or an index, `prev` the path of the enclosing value,
/// and `typename` the name of the input object type a field belongs to, which
/// the reference records and this module never reads.
///
/// Derived from graphql-js 16.12.0, src/jsutils/Path.ts, MIT.
#[derive(Debug, Clone, Copy)]
pub struct Path<'a> {
    /// The path of the enclosing value, or `None` at the root.
    pub prev: Option<&'a Path<'a>>,
    /// The key of this step.
    pub key: PathKey<'a>,
    /// The type name the reference records with a field step.
    pub typename: Option<&'a str>,
}

/// Given a Path and a key, return a new Path containing the new key.
///
/// Derived from graphql-js 16.12.0, src/jsutils/Path.ts, MIT.
#[must_use]
pub const fn add_path<'a>(
    prev: Option<&'a Path<'a>>,
    key: PathKey<'a>,
    typename: Option<&'a str>,
) -> Path<'a> {
    Path {
        prev,
        key,
        typename,
    }
}

/// Given a Path, return an Array of the path keys.
///
/// Derived from graphql-js 16.12.0, src/jsutils/Path.ts, MIT.
#[must_use]
pub fn path_to_array(path: Option<&Path<'_>>) -> Vec<PathSegment> {
    let mut flattened = Vec::new();
    let mut curr = path;
    while let Some(node) = curr {
        flattened.push(match node.key {
            PathKey::Index(index) => PathSegment::Index(index),
            PathKey::Name(name) => PathSegment::from(name),
        });
        curr = node.prev;
    }
    flattened.reverse();
    flattened
}

/// Coerces a JavaScript value given a GraphQL Input Type.
///
/// `input_value` is the value to coerce, which the coercion takes and coerces
/// part by part where each part stands when it is owned, and reads into a
/// fresh value when it is borrowed, `ty` the input type, and `on_error`
/// receives the path, the invalid value and the error of every problem, where
/// [`default_on_error`] is the callback the reference installs when none is
/// given. The result is [`JsValue::Undefined`] wherever the reference returns
/// without a value after an error the callback swallowed.
///
/// Derived from graphql-js 16.12.0, src/utilities/coerceInputValue.ts, MIT.
///
/// # Errors
///
/// Returns what `on_error` returns, what the field thunk of an input object
/// type throws, and the reference's invariant, a plain `Error` whose
/// message is `Unexpected input type: ` followed by the type, when `ty` is
/// no input type.
pub fn coerce_input_value<F>(
    input_value: Cow<'_, JsValue>,
    ty: &Type,
    mut on_error: F,
) -> Result<JsValue, DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    match input_value {
        Cow::Owned(mut value) => {
            coerce_input_value_impl(&mut value, ty, &mut on_error, None)?;
            Ok(value)
        }
        Cow::Borrowed(value) => coerce_fresh(value, ty, &mut on_error, None),
    }
}

/// Prefixes the message of `error` with the invalid value and the path and
/// returns it as the error.
///
/// `path` is the flattened path and `invalid_value` the value the message
/// inspects.
///
/// Derived from graphql-js 16.12.0, src/utilities/coerceInputValue.ts, MIT.
///
/// # Errors
///
/// Always returns `error` with its message prefixed, because the reference
/// throws it.
pub fn default_on_error(
    path: &[PathSegment],
    invalid_value: &JsValue,
    mut error: GraphQLError,
) -> Result<(), DefinitionError> {
    let mut error_prefix = format!("Invalid value {}", inspect(invalid_value));
    if !path.is_empty() {
        error_prefix = format!("{error_prefix} at \"value{}\"", print_path_array(path));
    }
    let mut message = JsStringBuilder::new();
    message
        .push_str(&error_prefix)
        .push_str(": ")
        .push(&error.message);
    error.message = message.finish();
    Err(DefinitionError::from(error))
}

/// Coerces the value `value` holds at `path`, and leaves the coerced value
/// in its place.
///
/// A value the reference returns no value for reads as [`JsValue::Undefined`]
/// in its place once its problem is reported.
///
/// Derived from graphql-js 16.12.0, src/utilities/coerceInputValue.ts, MIT.
fn coerce_input_value_impl<F>(
    value: &mut JsValue,
    ty: &Type,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<(), DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    if let Some(non_null) = ty.as_non_null() {
        if !is_nullish(value) {
            return coerce_input_value_impl(value, non_null.of_type(), on_error, path);
        }
        on_error(
            &path_to_array(path),
            value,
            plain_error(format!(
                "Expected non-nullable type \"{ty}\" not to be null."
            )),
        )?;
        *value = JsValue::Undefined;
        return Ok(());
    }

    if is_nullish(value) {
        // Explicitly return the value null.
        *value = JsValue::Null;
        return Ok(());
    }

    if let Some(list) = ty.as_list() {
        let item_type = &list.of_type;
        if let JsValue::Array(items) = value {
            for (index, item) in items.iter_mut().enumerate() {
                let item_path = add_path(path, PathKey::Index(index), None);
                coerce_input_value_impl(item, item_type, on_error, Some(&item_path))?;
            }
            return Ok(());
        }
        // Lists accept a non-list value as a list of one.
        coerce_input_value_impl(value, item_type, on_error, path)?;
        *value = JsValue::Array(alloc::vec![mem::take(value)]);
        return Ok(());
    }

    if let Type::InputObject(input_object) = ty {
        return coerce_input_object(value, input_object, on_error, path);
    }

    // Scalars and Enums determine if a input value is valid via parseValue(),
    // which can throw to indicate failure. If it throws, maintain a reference
    // to the original error.
    match ty {
        Type::Scalar(scalar) => match scalar.parse_input(value) {
            // The input is its own internal value and stays where it stands.
            Ok(Parsed::Input) => Ok(()),
            parse_result => coerce_leaf(value, &scalar.name, parse_result, on_error, path),
        },
        Type::Enum(enum_type) => {
            let parse_result = enum_type.parse_value(value).map(Parsed::Value);
            coerce_leaf(value, &enum_type.name, parse_result, on_error, path)
        }
        // Not reachable, all possible types have been considered.
        _ => Err(DefinitionError::plain(format!(
            "Unexpected input type: {ty}"
        ))),
    }
}

/// Coerces the value `input_value` at `path` into a fresh value and leaves
/// it as it stands.
///
/// A list and an input object are made anew and a leaf its `parseValue`
/// answers as its own value is copied, as the reference makes a fresh array
/// and a fresh object, and every problem names the value where it stands.
///
/// Derived from graphql-js 16.12.0, src/utilities/coerceInputValue.ts, MIT.
fn coerce_fresh<F>(
    input_value: &JsValue,
    ty: &Type,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<JsValue, DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    if let Some(non_null) = ty.as_non_null() {
        if !is_nullish(input_value) {
            return coerce_fresh(input_value, non_null.of_type(), on_error, path);
        }
        on_error(
            &path_to_array(path),
            input_value,
            plain_error(format!(
                "Expected non-nullable type \"{ty}\" not to be null."
            )),
        )?;
        return Ok(JsValue::Undefined);
    }

    if is_nullish(input_value) {
        // Explicitly return the value null.
        return Ok(JsValue::Null);
    }

    if let Some(list) = ty.as_list() {
        let item_type = &list.of_type;
        if let JsValue::Array(items) = input_value {
            let mut coerced = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                let item_path = add_path(path, PathKey::Index(index), None);
                coerced.push(coerce_fresh(item, item_type, on_error, Some(&item_path))?);
            }
            return Ok(JsValue::Array(coerced));
        }
        // Lists accept a non-list value as a list of one.
        let coerced = coerce_fresh(input_value, item_type, on_error, path)?;
        return Ok(JsValue::Array(alloc::vec![coerced]));
    }

    if let Type::InputObject(input_object) = ty {
        return coerce_object_fresh(input_value, input_object, on_error, path);
    }

    // Scalars and Enums determine if a input value is valid via parseValue(),
    // which can throw to indicate failure. If it throws, maintain a reference
    // to the original error.
    match ty {
        Type::Scalar(scalar) => match scalar.parse_input(input_value) {
            // The input is its own internal value, which the fresh value copies.
            Ok(Parsed::Input) => Ok(Parsed::Input.into_value(input_value)),
            parse_result => {
                coerce_leaf_fresh(input_value, &scalar.name, parse_result, on_error, path)
            }
        },
        Type::Enum(enum_type) => {
            let parse_result = enum_type.parse_value(input_value).map(Parsed::Value);
            coerce_leaf_fresh(input_value, &enum_type.name, parse_result, on_error, path)
        }
        // Not reachable, all possible types have been considered.
        _ => Err(DefinitionError::plain(format!(
            "Unexpected input type: {ty}"
        ))),
    }
}

/// The most field definitions [`Layout::InPlace`] names, one bit of a `u64`
/// each.
const FIELD_BITS: usize = 64;

/// How the fields of an input object are coerced, which [`layout_of`]
/// reads from its keys.
enum Layout {
    /// The keys are fields in the order of their definitions, no value is
    /// `undefined` and no field the object leaves out has a default, so each
    /// value is coerced in its place. The bits name the fields the object
    /// holds, one per position of a field definition.
    InPlace(u64),
    /// No problem names the whole object, and its values move into a new
    /// object in the order of the field definitions, with the defaults of the
    /// fields it leaves out.
    Moved,
    /// A problem names the whole object, or the value is no object, so it is
    /// coerced as it stands.
    AsItStands,
}

/// Coerces the value `value` holds against an input object type.
///
/// An object whose fields are coerced where they stand goes to
/// [`coerce_in_place`] or to [`coerce_moved`], as [`layout_of`] decides, and
/// a value that is no object or an object a problem names as a whole to
/// [`coerce_as_it_stands`].
fn coerce_input_object<F>(
    value: &mut JsValue,
    ty: &InputObjectType,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<(), DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    let layout = match &*value {
        JsValue::Object(object) => layout_of(object, ty, ty.get_fields()?),
        _ => Layout::AsItStands,
    };
    match (layout, value) {
        (Layout::InPlace(given), JsValue::Object(object)) => {
            coerce_in_place(object, given, ty, on_error, path)
        }
        (Layout::Moved, JsValue::Object(object)) => {
            *object = coerce_moved(mem::take(object), ty, on_error, path)?;
            Ok(())
        }
        (_, value) => {
            *value = coerce_as_it_stands(value, ty, on_error, path)?;
            Ok(())
        }
    }
}

/// Coerces `input_value` against an input object type into a fresh value
/// and leaves it as it stands.
///
/// An object whose keys are its fields in the order of their definitions,
/// where [`Layout::InPlace`] holds, goes to [`coerce_in_order_fresh`], and
/// any other value to [`coerce_as_it_stands`], which reads it where it stands
/// as the reference does.
fn coerce_object_fresh<F>(
    input_value: &JsValue,
    ty: &InputObjectType,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<JsValue, DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    if let JsValue::Object(object) = input_value
        && let Layout::InPlace(given) = layout_of(object, ty, ty.get_fields()?)
    {
        return coerce_in_order_fresh(object, given, ty, on_error, path).map(JsValue::Object);
    }
    coerce_as_it_stands(input_value, ty, on_error, path)
}

/// Returns how the fields of `object` are coerced against `ty`.
///
/// The keys are read once, each looked up among `field_defs` by its name, and
/// the field definitions once after them, so the three problems that name the
/// whole object are found before a field is coerced: a key no field defines,
/// a field of a non null type the object leaves out with no default, and a
/// one of object whose coerced object would hold other than one key. A key
/// whose value is `undefined` reads as left out, as `inputValue[field.name]`
/// reads it.
fn layout_of(object: &JsObject, ty: &InputObjectType, field_defs: &ObjMap<InputField>) -> Layout {
    let bits = field_defs.len() <= FIELD_BITS;
    let mut in_order = bits;
    let mut given: u64 = 0;
    let mut next = 0;
    for (key, value) in object.iter() {
        let Some(position) = key.as_str().and_then(|name| field_defs.get_index_of(name)) else {
            return Layout::AsItStands;
        };
        if value.is_undefined() {
            in_order = false;
            continue;
        }
        in_order &= position >= next;
        next = position + 1;
        given |= bit(position);
    }

    let mut coerced_keys = 0;
    for (position, field) in field_defs.values().enumerate() {
        let held = if bits {
            given & bit(position) != 0
        } else {
            object
                .get(&field.name)
                .is_some_and(|value| !value.is_undefined())
        };
        if held {
            coerced_keys += 1;
        } else if !field.default_value.is_undefined() {
            coerced_keys += 1;
            in_order = false;
        } else if is_non_null_type(&field.r#type) {
            return Layout::AsItStands;
        }
    }
    if ty.is_one_of && coerced_keys != 1 {
        return Layout::AsItStands;
    }
    if in_order {
        Layout::InPlace(given)
    } else {
        Layout::Moved
    }
}

/// Returns the bit of the field definition at `position`, and no bit from
/// [`FIELD_BITS`] on.
fn bit(position: usize) -> u64 {
    u32::try_from(position)
        .ok()
        .and_then(|position| 1_u64.checked_shl(position))
        .unwrap_or(0)
}

/// Coerces each value of `object` where it stands, where [`Layout::InPlace`]
/// holds with the fields `given`.
///
/// The object is the coerced object, since its keys are the fields it gives
/// in the order of their definitions, the order the reference sets them in.
fn coerce_in_place<F>(
    object: &mut JsObject,
    given: u64,
    ty: &InputObjectType,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<(), DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    let field_defs = ty.get_fields()?;
    let mut values = object.values_mut();
    for (position, field) in field_defs.values().enumerate() {
        if given & bit(position) == 0 {
            continue;
        }
        let Some(value) = values.next() else {
            break;
        };
        let field_path = add_path(path, PathKey::Name(&field.name), Some(ty.name.as_str()));
        coerce_input_value_impl(value, &field.r#type, on_error, Some(&field_path))?;
    }
    drop(values);
    if ty.is_one_of {
        check_one_of_null(object, on_error, path)?;
    }
    Ok(())
}

/// Coerces each value of `object` into a fresh object with the keys of
/// `object`, where [`Layout::InPlace`] holds with the fields `given`, and
/// returns the fresh object.
///
/// The keys are the fields `object` gives in the order of their definitions,
/// the order the reference sets them in, so the fresh object keeps the hash
/// of each key of `object` and hashes none again.
fn coerce_in_order_fresh<F>(
    object: &JsObject,
    given: u64,
    ty: &InputObjectType,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<JsObject, DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    let field_defs = ty.get_fields()?;
    let mut fields = field_defs
        .values()
        .enumerate()
        .filter(|(position, _)| given & bit(*position) != 0)
        .map(|(_, field)| field);
    let coerced_value = object.try_map_values(|value| {
        // The layout gives every key of the object a field in this order.
        let Some(field) = fields.next() else {
            return Ok(JsValue::Undefined);
        };
        let field_path = add_path(path, PathKey::Name(&field.name), Some(ty.name.as_str()));
        coerce_fresh(value, &field.r#type, on_error, Some(&field_path))
    })?;
    if ty.is_one_of {
        check_one_of_null(&coerced_value, on_error, path)?;
    }
    Ok(coerced_value)
}

/// Moves the values of `object` into a new object in the order of the field
/// definitions, with the defaults of the fields it leaves out, where
/// [`Layout::Moved`] holds, and returns the new object.
///
/// A value keeps the key the request wrote for it.
fn coerce_moved<F>(
    object: JsObject,
    ty: &InputObjectType,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<JsObject, DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    let field_defs = ty.get_fields()?;
    let mut given: Vec<Option<(JsString, JsValue)>> = alloc::vec![None; field_defs.len()];
    for (key, value) in object {
        if value.is_undefined() {
            continue;
        }
        let position = key.as_str().and_then(|name| field_defs.get_index_of(name));
        if let Some(slot) = position.and_then(|position| given.get_mut(position)) {
            *slot = Some((key, value));
        }
    }

    let mut coerced_value = JsObject::with_capacity(field_defs.len());
    for (field, slot) in field_defs.values().zip(given) {
        let Some((key, mut value)) = slot else {
            if !field.default_value.is_undefined() {
                coerced_value.insert(field.name.as_str(), field.default_value.clone());
            }
            continue;
        };
        let field_path = add_path(path, PathKey::Name(&field.name), Some(ty.name.as_str()));
        coerce_input_value_impl(&mut value, &field.r#type, on_error, Some(&field_path))?;
        coerced_value.insert(key, value);
    }
    if ty.is_one_of {
        check_one_of_null(&coerced_value, on_error, path)?;
    }
    Ok(coerced_value)
}

/// Coerces `input_value` against an input object type as it stands, a
/// value that is no object, an object [`Layout::AsItStands`] names or one
/// another holder shares whose keys stand in another order, and returns the
/// coerced value.
///
/// The object stays as the request sent it, so a problem that names the whole
/// object inspects it, and each field is read where it stands into a fresh
/// value. `Object.keys` of the input lists array index keys first and the
/// rest in insertion order, which is what `own_keys` walks. A key that is
/// UTF-8 is looked up among the fields as it stands, and the text of a key no
/// field defines is made for its message alone.
fn coerce_as_it_stands<F>(
    input_value: &JsValue,
    ty: &InputObjectType,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<JsValue, DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    let JsValue::Object(object) = input_value else {
        on_error(
            &path_to_array(path),
            input_value,
            plain_error(format!("Expected type \"{}\" to be an object.", ty.name)),
        )?;
        return Ok(JsValue::Undefined);
    };

    let mut coerced_value = JsObject::new();
    let field_defs = ty.get_fields()?;

    for field in field_defs.values() {
        let field_value = object
            .get(&field.name)
            .filter(|value| !value.is_undefined());
        let Some(field_value) = field_value else {
            if !field.default_value.is_undefined() {
                coerced_value.insert(field.name.as_str(), field.default_value.clone());
            } else if is_non_null_type(&field.r#type) {
                let type_str = format!("{}", field.r#type);
                on_error(
                    &path_to_array(path),
                    input_value,
                    plain_error(format!(
                        "Field \"{}\" of required type \"{type_str}\" was not provided.",
                        field.name
                    )),
                )?;
            }
            continue;
        };

        let field_path = add_path(path, PathKey::Name(&field.name), Some(ty.name.as_str()));
        let coerced = coerce_fresh(field_value, &field.r#type, on_error, Some(&field_path))?;
        coerced_value.insert(field.name.as_str(), coerced);
    }

    // Ensure every provided field is defined.
    for (field_name, _) in object.own_keys() {
        if field_name
            .as_str()
            .is_some_and(|name| field_defs.contains_key(name))
        {
            continue;
        }
        let field_name = key_text(field_name);
        let options: Vec<String> = field_defs.keys().cloned().collect();
        let suggestions = suggestion_list(&field_name, &options);
        on_error(
            &path_to_array(path),
            input_value,
            plain_error(format!(
                "Field \"{field_name}\" is not defined by type \"{}\".{}",
                ty.name,
                did_you_mean(None, &suggestions)
            )),
        )?;
    }

    if ty.is_one_of {
        check_one_of(input_value, ty, &coerced_value, on_error, path)?;
    }

    Ok(JsValue::Object(coerced_value))
}

/// Reports a one of object that holds other than one key, or a null key.
///
/// With no key at all, `keys[0]` is undefined and `coercedValue[undefined]`
/// is undefined, which is not null, so only the count is reported.
fn check_one_of<F>(
    input_value: &JsValue,
    ty: &InputObjectType,
    coerced_value: &JsObject,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<(), DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    if coerced_value.len() != 1 {
        on_error(
            &path_to_array(path),
            input_value,
            plain_error(format!(
                "Exactly one key must be specified for OneOf type \"{}\".",
                ty.name
            )),
        )?;
    }
    check_one_of_null(coerced_value, on_error, path)
}

/// Reports the first key of a one of object when its coerced value is null.
///
/// The keys of a coerced object are field names, none of which is an array
/// index, so its first key in insertion order is `keys[0]` of `Object.keys`.
fn check_one_of_null<F>(
    coerced_value: &JsObject,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<(), DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    let Some((key, JsValue::Null)) = coerced_value.iter().next() else {
        return Ok(());
    };
    let key = key_text(key);
    let mut key_path = path_to_array(path);
    key_path.push(PathSegment::Key(key.clone()));
    on_error(
        &key_path,
        &JsValue::Null,
        plain_error(format!("Field \"{key}\" must be non-null.")),
    )
}

/// Reports the outcome of `parseValue` of a leaf type for the input `value`
/// holds, and leaves the internal value in its place.
///
/// `type_name` is the name of the leaf and `parse_result` what its
/// `parseValue` answered or threw, read while the input still stands in
/// `value`, so a problem names it.
fn coerce_leaf<F>(
    value: &mut JsValue,
    type_name: &str,
    parse_result: Result<Parsed, DefinitionError>,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<(), DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    match leaf_outcome(type_name, parse_result) {
        Ok(Parsed::Input) => Ok(()),
        Ok(Parsed::Value(parse_result)) => {
            *value = parse_result;
            Ok(())
        }
        Err(error) => {
            on_error(&path_to_array(path), value, *error)?;
            *value = JsValue::Undefined;
            Ok(())
        }
    }
}

/// Reports the outcome of `parseValue` of a leaf type for `input_value`,
/// and returns a fresh internal value.
///
/// An input that is its own internal value is copied, and a problem names the
/// input where it stands.
fn coerce_leaf_fresh<F>(
    input_value: &JsValue,
    type_name: &str,
    parse_result: Result<Parsed, DefinitionError>,
    on_error: &mut F,
    path: Option<&Path<'_>>,
) -> Result<JsValue, DefinitionError>
where
    F: FnMut(&[PathSegment], &JsValue, GraphQLError) -> Result<(), DefinitionError>,
{
    match leaf_outcome(type_name, parse_result) {
        Ok(parsed) => Ok(parsed.into_value(input_value)),
        Err(error) => {
            on_error(&path_to_array(path), input_value, *error)?;
            Ok(JsValue::Undefined)
        }
    }
}

/// Returns the internal value `parseValue` of a leaf type answered, or the
/// error of the problem it makes.
///
/// A parser that answers `undefined` makes the problem `Expected type`, a
/// `GraphQLError` it throws is the problem itself, and any other error it
/// throws is the original error of a problem whose message names the type
/// before its own. The error stays in the box a thrown `GraphQLError` comes
/// in.
fn leaf_outcome(
    type_name: &str,
    parse_result: Result<Parsed, DefinitionError>,
) -> Result<Parsed, Box<GraphQLError>> {
    match parse_result {
        Ok(Parsed::Value(parse_result)) if parse_result.is_undefined() => Err(Box::new(
            plain_error(format!("Expected type \"{type_name}\".")),
        )),
        Ok(parsed) => Ok(parsed),
        Err(DefinitionError::GraphQL(error)) => Err(error),
        Err(error @ DefinitionError::Plain(_)) => {
            let mut message = JsStringBuilder::new();
            message
                .push_str(&format!("Expected type \"{type_name}\". "))
                .push(error.message());
            let message = message.finish();
            let options = GraphQLErrorOptions {
                original_error: Some(OriginalError::from(error)),
                ..GraphQLErrorOptions::default()
            };
            Err(Box::new(GraphQLError::new(message, options)))
        }
    }
}

/// Returns whether `value` is `null` or `undefined`, which is `== null`.
///
/// The loose equality of a value with `null` holds for exactly those two.
fn is_nullish(value: &JsValue) -> bool {
    matches!(value, JsValue::Null | JsValue::Undefined)
}

/// Returns `new GraphQLError(message)` with no options.
fn plain_error(message: String) -> GraphQLError {
    GraphQLError::new(message, GraphQLErrorOptions::default())
}

/// Returns the text of a property key for a message.
///
/// A key that holds a lone surrogate has no `str` form, and the replacement
/// character stands in for it.
fn key_text(key: &JsString) -> String {
    String::from_utf8_lossy(key.as_wtf8()).into_owned()
}
