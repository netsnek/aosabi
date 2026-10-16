// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool validate-schema: validates the schema of an SDL text the way
 * the Aosabi build validates it, and folds the errors into the message the
 * build throws.
 *
 * The constant text of the message is the extracted literal
 * crates/aosabi-build/src/schema_invalid_message.rs, which tools/provenance
 * writes and this file only reads.
 */

//! The `validate-schema` primitive of the seam.
//!
//! `--schema PATH` names an SDL file, where the path `-` is stdin. The errors
//! are those of the fold of the Aosabi build, which is
//! `validateSchema(buildSchema(typeDefs))` inside a `try` whose `catch` turns a
//! throw into a one element array. `buildSchema` runs with no options there, so
//! the SDL rules run inside the build and an SDL level problem is the one thrown
//! entry rather than a result of `validateSchema`.
//!
//! - With no further flag the payload is the error array in the `toJSON` shape
//!   of the reference, `[]` for a valid schema, and the exit code is 0.
//! - `--assume-valid-sdl` builds with the `assumeValidSDL` option, which skips
//!   the SDL rules, so the array holds what `validateSchema` reports over a
//!   schema the SDL rules never saw.
//! - `--thrown-message` writes the text the build throws in place of the
//!   array, as UTF-8 with no trailing newline, and writes nothing for a valid
//!   schema, because the build makes the text only when it throws. The exit code is 0.
//!
//! `--requests PATH` is the batch form, which answers the four operations of
//! the type layer listed in [`crate::print_schema`].

use std::{
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    JsStringBuilder,
    ParseOptions,
    Source,
    js_string::{
        join,
        to_string_lossy, //
    }, //
};
use aosabi_type::{
    BuildSchemaOptions,
    DefinitionError, //
};

use crate::{
    batch,
    cli::{
        self,
        Grammar,
        Matches,
        Opt, //
    },
    print_schema,
    seam::{
        self,
        Answer,
        Failure, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "validate-schema";

/// The constant segments of the message, in the order the source carries them.
const SEGMENTS: &str = include_str!("../../aosabi-build/src/schema_invalid_message.rs");

/// The text in front of the message of one error.
///
/// The extractor pins quoted strings only, so the template is carried here.
const BULLET: &str = "  \u{2022} ";

/// The command line of the `validate-schema` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        Opt::path(
            "schema",
            "PATH",
            "Read the SDL of the schema from this file, or from stdin when it is `-`",
        ),
        Opt::flag(
            "assume-valid-sdl",
            "Build with the assumeValidSDL option, which skips the SDL rules",
        ),
        Opt::flag(
            "thrown-message",
            "Write the message the build throws in place of the error array",
        ),
        cli::REQUESTS,
    ],
    positional: None,
    conflicts: &[(
        "requests",
        &["schema", "assume-valid-sdl", "thrown-message"],
    )],
};

/// What the `validate-schema` primitive accepts.
#[derive(Debug)]
pub struct ValidateSchemaArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of the SDL, where `-` is stdin.
    schema: Option<PathBuf>,
    /// The assumeValidSDL option.
    assume_valid_sdl: bool,
    /// Whether the call writes the message the build throws.
    thrown_message: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl ValidateSchemaArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            schema: matches.path("schema"),
            assume_valid_sdl: matches.flag("assume-valid-sdl"),
            thrown_message: matches.flag("thrown-message"),
            requests: matches.path("requests"),
        }
    }
}

/// One entry of the folded array, which is an error or a thrown value.
///
/// The message is what the bullet prints, and the value is what the array
/// serializes.
#[derive(Debug)]
struct Folded {
    /// The `message` property the bullet reads.
    message: JsString,
    /// The entry the way the oracle serializes it.
    json: JsValue,
}

impl From<&DefinitionError> for Folded {
    fn from(error: &DefinitionError) -> Self {
        Self {
            message: error.message().clone(),
            json: seam::definition_error_json(error),
        }
    }
}

/// Runs the `validate-schema` primitive.
pub fn run(args: &ValidateSchemaArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke(NAME);
    }
    if let Some(requests) = &args.requests {
        return seam::finish(NAME, batch::run(print_schema::OPERATIONS, requests));
    }
    seam::finish(NAME, answer(args))
}

/// Returns the error array or the message of the one call the flags name.
fn answer(args: &ValidateSchemaArgs) -> Result<Answer, Failure> {
    let path = seam::required(args.schema.as_deref(), "--schema")?;
    let sdl = seam::read_input(path)?;
    let options = BuildSchemaOptions {
        assume_valid: false,
        assume_valid_sdl: args.assume_valid_sdl,
    };
    let errors = fold_schema_errors(sdl, options);
    if !args.thrown_message {
        let entries = errors.into_iter().map(|entry| entry.json).collect();
        return Ok(Answer::errors(entries));
    }
    if errors.is_empty() {
        return Ok(Answer::Payload(Vec::new()));
    }
    // Stdout carries text, so a lone surrogate leaves as U+FFFD here. The
    // batch form carries the message as JSON and keeps the code unit.
    schema_invalid_message(&errors)
        .map(|message| Answer::text(to_string_lossy(&message).into_owned()))
        .map_err(Failure::Data)
}

/// Returns the errors of `type_defs` the way the Aosabi build collects them.
///
/// The `try` covers both calls, so a throw of the build and a throw of the
/// validation both become the one entry of the array. Every throw on this path
/// is an `Error`, so the `{message: String(err)}` arm of the reference has no
/// reading here.
fn fold_schema_errors(type_defs: impl Into<Source>, options: BuildSchemaOptions) -> Vec<Folded> {
    let built = aosabi_validate::build_schema(type_defs, options, ParseOptions::default());
    let schema = match built {
        Ok(schema) => schema,
        Err(error) => return vec![Folded::from(&error)],
    };
    match aosabi_type::validate_schema(&schema) {
        Ok(errors) => errors
            .iter()
            .map(|error| Folded {
                message: error.message.clone(),
                json: seam::graphql_error_json(error),
            })
            .collect(),
        Err(error) => vec![Folded::from(&error)],
    }
}

/// Returns the text the build throws over `errors`, which holds at least one entry.
///
/// Every constant byte comes from the extracted literal.
///
/// # Errors
///
/// Returns the reason when the extracted literal does not carry the three
/// segments, which the provenance primitive reports as drift.
fn schema_invalid_message(errors: &[Folded]) -> Result<JsString, String> {
    let (opening, separator, advisory) =
        segments().ok_or("the schema_invalid_message literal does not carry its three segments")?;
    let bullets: Vec<JsString> = errors.iter().map(bullet).collect();
    let mut message = JsStringBuilder::new();
    message
        .push_str(opening)
        .push(&join(&bullets, separator))
        .push_str(advisory);
    Ok(message.finish())
}

/// Returns the bullet of one entry, which is the constant prefix and the message.
fn bullet(entry: &Folded) -> JsString {
    let mut bullet = JsStringBuilder::new();
    bullet.push_str(BULLET).push(&entry.message);
    bullet.finish()
}

/// Returns the opening line, the separator and the advisory paragraph.
///
/// The opening line ends with its newline and the separator is the one
/// character after it, which is the order the quoted strings have in the
/// source.
fn segments() -> Option<(&'static str, &'static str, &'static str)> {
    let end_of_opening = SEGMENTS.find('\n')? + 1;
    let (opening, rest) = SEGMENTS.split_at_checked(end_of_opening)?;
    let (separator, advisory) = rest.split_at_checked(1)?;
    Some((opening, separator, advisory))
}

/// Answers the operation `validate_schema`.
///
/// `message` follows `errors` only when `thrown_message` is `true` and at
/// least one error exists, which is `opValidateSchema` of the oracle.
pub fn op_validate_schema(request: &JsObject) -> Result<JsObject, JsString> {
    let sdl = batch::require_source(request, "sdl")?;
    let errors = fold_schema_errors(sdl, BuildSchemaOptions::default());
    let wants_message = matches!(request.get("thrown_message"), Some(JsValue::Bool(true)));
    let message = if wants_message && !errors.is_empty() {
        let message = schema_invalid_message(&errors);
        Some(message.map_err(|reason| format!("validate_schema: {reason}"))?)
    } else {
        None
    };
    let mut payload = JsObject::with_capacity(2);
    let entries: Vec<JsValue> = errors.into_iter().map(|entry| entry.json).collect();
    payload.insert("errors", entries);
    if let Some(message) = message {
        payload.insert("message", message);
    }
    Ok(payload)
}
