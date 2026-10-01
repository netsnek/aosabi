// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: graphql!, the roots of an application as an object literal.
 *
 * The macro is the twin of the object literal export const graphql = {...}
 * of a TypeScript service, which the TypeScript checker reads as the roots
 * of the schema.
 */

//! `graphql!`.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `parse` | the grammar and the refusal at the token that breaks it |
//! | `expand` | the hidden module, the descriptors, the run time and the constant |
//! | `path_rule` | the rule that tells a type path from a module path, and the paths of the hidden items |

mod expand;
mod parse;
pub(crate) mod path_rule;

pub(crate) use parse::{
    decorate,
    line_of, //
};
use proc_macro2::TokenStream;

/// Expands one invocation of `graphql!`.
///
/// A refusal is the one output, because the constant of a refused literal
/// has no type to carry.
pub(crate) fn entry(input: &TokenStream) -> TokenStream {
    parse::parse(input.clone())
        .and_then(|parsed| expand::expand(&parsed))
        .unwrap_or_else(|error| error.to_compile_error())
}
