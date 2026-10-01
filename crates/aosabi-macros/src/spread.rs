// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: the key `#[aosabi(spread)]` of a field that stands for an
 * object spread.
 *
 * No analogue on the reference side, where the spread is the syntax {..value}
 * of an object literal, whose members the TypeScript checker presents in
 * place of the spread.
 */

//! The fields under the key `#[aosabi(spread)]` of a struct.
//!
//! A struct with a field under the key is the twin of an object literal with
//! a spread, `{...u, fullName, org}`, its fields in the order of the literal
//! and the spread field at the place the spread stands. The descriptor
//! records the field with `spread` set, and the inference presents the
//! members of the field's type in its place by the spread order of the
//! checker. This module reads the fields under the key, and the module
//! `spread` of `aosabi-codegen` writes the object of the struct, which
//! answers what the object of the literal answers.

use aosabi_codegen::spread::Spread;
use syn::Fields;

use crate::attrs;

/// Returns the named fields of `fields` under `#[aosabi(spread)]`, in
/// declaration order.
///
/// # Errors
///
/// Returns the refusals of [`attrs::field_attrs`].
pub(crate) fn fields(fields: &Fields) -> syn::Result<Vec<Spread<'_>>> {
    let mut spreads = Vec::new();
    for field in fields {
        let Some(span) = attrs::field_attrs(&field.attrs)?.spread else {
            continue;
        };
        if let Some(ident) = &field.ident {
            spreads.push(Spread { ident, span });
        }
    }
    Ok(spreads)
}
