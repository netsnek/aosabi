// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-build: the validation that runs before a build writes a byte, and
 * the message of a schema that fails it.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/bundler/emit-server-glue.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The constant text of the message is the extracted literal
 * schema_invalid_message.rs beside this file, which tools/provenance writes
 * and this file only reads.
 */

//! The validation that runs before a build writes a byte.
//!
//! A build never emits an invalid schema. It validates the inferred text
//! exactly the way the runtime builds it, and it fails instead of writing a
//! schema that fails at serve time. `buildSchema` runs with no options there,
//! so the SDL rules run inside the build and an SDL level problem is the one
//! thrown entry rather than a result of `validateSchema`.

use core::fmt;

use aosabi_json::JsString;
use aosabi_lang::{
    JsStringBuilder,
    ParseOptions,
    js_string::{
        join,
        to_string_lossy, //
    }, //
};
use aosabi_type::BuildSchemaOptions;

/// The extractor concatenates them, so the text is the opening line with its
/// newline, the one newline the bullets are joined by, and the advisory
/// paragraph with the blank line in front of it.
const LITERAL: &str = include_str!("schema_invalid_message.rs");

/// The text in front of the message of one error.
///
/// The extractor pins quoted strings only, so the template is carried here.
const BULLET: &str = "  \u{2022} ";

/// The three segments of [`LITERAL`].
///
/// A literal that does not split ends the compilation of this crate, so drift
/// of the extracted file never reaches a build script as a wrong message.
const SEGMENTS: Segments = match split_segments(LITERAL) {
    Some(segments) => segments,
    None => panic!("the schema_invalid_message literal does not carry its three segments"),
};

/// The constant text of the message, split where the variable text goes.
struct Segments {
    /// The opening line with its newline, which stands in front of the
    /// bullets.
    opening: &'static str,
    /// The text the bullets are joined by.
    separator: &'static str,
    /// The advisory paragraph with the blank line in front of it, which
    /// stands behind the bullets.
    advisory: &'static str,
}

/// Says that a schema text does not form a valid schema.
///
/// A message is a [`JsString`] because the message of a syntax error quotes
/// the code unit the lexer stopped at, which may be a lone surrogate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaInvalid {
    /// The `message` of every entry of the folded array, in its order.
    pub messages: Vec<JsString>,
    /// The message of the `Error` the reference throws over those entries.
    pub message: JsString,
}

impl fmt::Display for SchemaInvalid {
    /// Writes the message with every lone surrogate replaced by U+FFFD,
    /// because the text of a formatter is UTF-8.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&to_string_lossy(&self.message))
    }
}

impl core::error::Error for SchemaInvalid {}

/// Checks that `type_defs` forms a valid schema.
///
/// The check is the one the serving side makes when it builds the schema, so
/// a text that passes here builds there.
///
/// # Errors
///
/// Returns [`SchemaInvalid`] with the message the reference throws when the
/// folded array holds at least one entry.
pub fn assert_schema_is_valid(type_defs: &str) -> Result<(), SchemaInvalid> {
    let messages = fold_schema_errors(type_defs);
    if messages.is_empty() {
        return Ok(());
    }

    let message = schema_invalid_message(&messages);
    Err(SchemaInvalid { messages, message })
}

/// Returns the messages of the errors of `type_defs` the way the reference
/// collects them.
///
/// The `try` covers both calls, so a throw of the build and a throw of the
/// validation both become the one entry of the array. Every throw on this path
/// is an `Error`, so the `{message: String(err)}` arm of the reference has no
/// reading here.
fn fold_schema_errors(type_defs: &str) -> Vec<JsString> {
    let built = aosabi_validate::build_schema(
        type_defs,
        BuildSchemaOptions::default(),
        ParseOptions::default(),
    );
    let schema = match built {
        Ok(schema) => schema,
        Err(error) => return vec![error.message().clone()],
    };

    match aosabi_type::validate_schema(&schema) {
        Ok(errors) => errors.iter().map(|error| error.message.clone()).collect(),
        Err(error) => vec![error.message().clone()],
    }
}

/// Returns the message of the `Error` the reference throws over `messages`.
///
/// `messages` holds the `message` of every entry, and the reference builds the
/// text only for a list that is not empty. Every constant byte except the
/// bullet prefix comes from the extracted literal.
#[must_use]
pub fn schema_invalid_message(messages: &[JsString]) -> JsString {
    let bullets: Vec<JsString> = messages.iter().map(bullet).collect();
    let mut message = JsStringBuilder::new();
    message
        .push_str(SEGMENTS.opening)
        .push(&join(&bullets, SEGMENTS.separator))
        .push_str(SEGMENTS.advisory);
    message.finish()
}

/// Returns the bullet of one entry, which is the constant prefix and the
/// message.
fn bullet(message: &JsString) -> JsString {
    let mut bullet = JsStringBuilder::new();
    bullet.push_str(BULLET).push(message);
    bullet.finish()
}

/// Splits `literal` into the opening line, the separator and the advisory
/// paragraph.
///
/// The opening line ends with its newline and the separator is the one
/// character after it, which is the order the quoted strings have in the
/// source. The answer is `None` when `literal` holds no newline or nothing
/// behind the first one.
const fn split_segments(literal: &'static str) -> Option<Segments> {
    let Some(end_of_opening) = end_of_first_line(literal.as_bytes()) else {
        return None;
    };
    let Some((opening, rest)) = literal.split_at_checked(end_of_opening) else {
        return None;
    };
    let Some((separator, advisory)) = rest.split_at_checked(1) else {
        return None;
    };

    Some(Segments {
        opening,
        separator,
        advisory,
    })
}

/// Returns the offset behind the first newline of `bytes`.
///
/// The search is a loop because no search method of a slice is callable in a
/// constant.
const fn end_of_first_line(bytes: &[u8]) -> Option<usize> {
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] == b'\n' {
            return Some(offset + 1);
        }
        offset += 1;
    }
    None
}
