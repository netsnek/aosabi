// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the text of a response path, as a message names it.
 *
 * Derived from graphql-js 16.12.0, src/jsutils/printPathArray.ts, MIT.
 */

//! The path printer of the type layer.
//!
//! A coercion message names the position of the offending value as a path,
//! `.field[0].name`, and [`print_path_array()`] builds that text from the
//! segments a [`PathSegment`] list carries. The segment type is the two
//! variant type the error layer keeps, because a path segment is a string
//! or an integer on the wire.

use alloc::{
    format,
    string::String, //
};

use aosabi_lang::PathSegment;

/// Builds a string describing the path.
///
/// An index of `path` prints as `[<index>]`, where `key.toString()` of an
/// integer is its decimal digits, and a key prints as `.<key>`.
///
/// Derived from graphql-js 16.12.0, src/jsutils/printPathArray.ts, MIT.
#[must_use]
pub fn print_path_array(path: &[PathSegment]) -> String {
    path.iter()
        .map(|key| match key {
            PathSegment::Index(index) => format!("[{index}]"),
            PathSegment::Key(key) => format!(".{key}"),
        })
        .collect::<String>()
}
