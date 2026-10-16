// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool print-schema: prints the schema of an SDL text the way
 * printSchema of graphql-js prints it.
 *
 * No analogue in the reference. The crate under test is aosabi-type, and the
 * judging notebook is tests/notebooks/02_language.ipynb, which compares the
 * bytes against a node sidecar running the pinned graphql-js.
 */

//! The `print-schema` primitive of the seam.
//!
//! `--schema PATH` names an SDL file, where the path `-` is stdin. The schema
//! is built the way `buildSchema` builds it with no options, and the payload
//! is the text `printSchema` writes for it, as UTF-8 with no trailing newline,
//! with exit code 0. A schema that does not build, or a printer that throws,
//! writes a JSON array that holds the one thrown error and exits 1.
//!
//! `--requests PATH` is the batch form. It answers the four operations of the
//! protocol that reach the type layer, which are `build_schema`,
//! `validate_schema`, `print_schema` and `natural_compare`, and the batch form
//! of `build-schema`, `validate-schema` and `natural-compare` answers the same
//! four.

use std::{
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsString, //
};
use aosabi_lang::Source;
use aosabi_type::DefinitionError;

use crate::{
    batch::{
        self,
        Operations, //
    },
    build_schema,
    cli::{
        self,
        Grammar,
        Matches,
        Opt, //
    },
    natural_compare,
    seam::{
        self,
        Answer, //
    },
    validate_schema, //
};

/// The name of the primitive.
const NAME: &str = "print-schema";

/// The operations of the protocol that reach aosabi-type.
pub const OPERATIONS: &Operations = &[
    ("build_schema", build_schema::op_build_schema),
    ("validate_schema", validate_schema::op_validate_schema),
    ("print_schema", op_print_schema),
    ("natural_compare", natural_compare::op_natural_compare),
];

/// The command line of the `print-schema` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        Opt::path(
            "schema",
            "PATH",
            "Read the SDL of the schema from this file, or from stdin when it is `-`",
        ),
        cli::REQUESTS,
    ],
    positional: None,
    conflicts: &[("requests", &["schema"])],
};

/// What the `print-schema` primitive accepts.
#[derive(Debug)]
pub struct PrintSchemaArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of the SDL, where `-` is stdin.
    schema: Option<PathBuf>,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl PrintSchemaArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            schema: matches.path("schema"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `print-schema` primitive.
pub fn run(args: &PrintSchemaArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke(NAME);
    }
    if let Some(requests) = &args.requests {
        return seam::finish(NAME, batch::run(OPERATIONS, requests));
    }
    let answer = seam::required(args.schema.as_deref(), "--schema")
        .and_then(seam::read_input)
        .map(|sdl| match print(sdl) {
            Ok(printed) => Answer::text(printed),
            Err(error) => Answer::refused(vec![seam::definition_error_json(&error)]),
        });
    seam::finish(NAME, answer)
}

/// Returns `printSchema(buildSchema(sdl))`.
///
/// # Errors
///
/// Returns what either call throws.
fn print(sdl: impl Into<Source>) -> Result<String, DefinitionError> {
    let schema = build_schema::build(sdl)?;
    aosabi_type::print_schema(&schema)
}

/// Answers the operation `print_schema`.
fn op_print_schema(request: &JsObject) -> Result<JsObject, JsString> {
    let sdl = batch::require_source(request, "sdl")?;
    let mut payload = JsObject::with_capacity(1);
    match print(sdl) {
        Ok(printed) => payload.insert("printed", printed),
        Err(error) => payload.insert("errors", vec![seam::definition_error_json(&error)]),
    };
    Ok(payload)
}
