// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the naming rules of the specification, applied to every name a
 * type, a field, an argument, a directive or an enum value carries.
 *
 * Derived from graphql-js 16.12.0, src/type/assertName.ts, MIT.
 */

//! The name assertions of the type layer.
//!
//! Every constructor of [`crate::definition`] and of [`crate::directives`]
//! passes the name it is given through [`assert_name()`], and an enum value
//! passes through [`assert_enum_value_name()`] first. The reference reads a
//! name as a JavaScript string, so the loop below walks UTF-16 code units,
//! which is what `charCodeAt` returns, and hands each unit to the character
//! classes of the language layer.
//!
//! The two `devAssert` calls that open the reference function, a name that
//! is not null and a name that is a string, have no reading here because
//! the argument type carries both. The thrown `GraphQLError` travels as the
//! [`DefinitionError`] of the type layer, which every constructor returns.

use alloc::{
    format,
    string::String, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    char_utils::{
        is_name_continue,
        is_name_start, //
    }, //
};

use crate::definition::DefinitionError;

/// Upholds the spec rules about naming.
///
/// Returns `name` as an owned string when it is a valid GraphQL name. The
/// reference checks every unit after the first before it checks the first, so
/// a name that fails both reports the continue rule, and this port keeps that
/// order.
///
/// Derived from graphql-js 16.12.0, src/type/assertName.ts, MIT.
///
/// # Errors
///
/// Returns a [`GraphQLError`] whose message is the reference's when the name
/// is empty, when a unit after the first is outside `[_a-zA-Z0-9]`, or when
/// the first unit is outside `[_a-zA-Z]`.
pub fn assert_name(name: &str) -> Result<String, DefinitionError> {
    let mut units = name.encode_utf16();
    let Some(first) = units.next() else {
        return Err(DefinitionError::from(GraphQLError::new(
            "Expected name to be a non-empty string.",
            GraphQLErrorOptions::default(),
        )));
    };

    for unit in units {
        if !is_name_continue(i32::from(unit)) {
            return Err(DefinitionError::from(GraphQLError::new(
                format!("Names must only contain [_a-zA-Z0-9] but \"{name}\" does not."),
                GraphQLErrorOptions::default(),
            )));
        }
    }

    if !is_name_start(i32::from(first)) {
        return Err(DefinitionError::from(GraphQLError::new(
            format!("Names must start with [_a-zA-Z] but \"{name}\" does not."),
            GraphQLErrorOptions::default(),
        )));
    }

    Ok(String::from(name))
}

/// Upholds the spec rules about naming enum values.
///
/// An enum value may not be spelled `true`, `false` or `null`, because those
/// spellings are the boolean and null literals of the language, and it has to
/// be a valid name beyond that.
///
/// Derived from graphql-js 16.12.0, src/type/assertName.ts, MIT.
///
/// # Errors
///
/// Returns a [`GraphQLError`] for one of the three reserved spellings, and
/// otherwise whatever [`assert_name()`] returns.
pub fn assert_enum_value_name(name: &str) -> Result<String, DefinitionError> {
    if name == "true" || name == "false" || name == "null" {
        return Err(DefinitionError::from(GraphQLError::new(
            format!("Enum values cannot be named: {name}"),
            GraphQLErrorOptions::default(),
        )));
    }
    assert_name(name)
}
