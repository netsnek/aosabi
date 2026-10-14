// SPDX-License-Identifier: MIT
/*
 * aoviewer-core json: the JSON pull parser and the string writer of the core.
 *
 * The core has no dependency, so it reads introspection results and ir.json and
 * writes its JSON files with code of its own.
 */

//! The JSON reader and writer of the core.

mod reader;
mod writer;

pub use reader::{
    JsonError,
    Parser,
    Token, //
};
pub use writer::write_str;
