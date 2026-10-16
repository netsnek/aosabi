// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool build-schema: builds a schema from SDL and reports what
 * buildSchema of graphql-js throws.
 *
 * No analogue in the reference. The crates under test are aosabi-type and the
 * SDL rules of aosabi-validate, and the judging notebook is
 * tests/notebooks/02_language.ipynb.
 */

//! The `build-schema` primitive of the seam.
//!
//! `--schema PATH` names an SDL file, where the path `-` is stdin. The schema
//! is built the way `buildSchema` builds it with no options, so the SDL rules
//! run inside the build. A schema that builds writes nothing and exits 0. A
//! schema that does not build writes a JSON array that holds the one thrown
//! error and exits 1. A syntax error is a `GraphQLError` and carries its
//! locations. The error of the SDL rules is a plain `Error` that joins every
//! message with a blank line, so it carries `message` alone.
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
    ParseOptions,
    Source, //
};
use aosabi_type::{
    BuildSchemaOptions,
    DefinitionError,
    Schema, //
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
        Answer, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "build-schema";

/// The command line of the `build-schema` primitive.
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

/// What the `build-schema` primitive accepts.
#[derive(Debug)]
pub struct BuildSchemaArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of the SDL, where `-` is stdin.
    schema: Option<PathBuf>,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl BuildSchemaArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            schema: matches.path("schema"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `build-schema` primitive.
pub fn run(args: &BuildSchemaArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke(NAME);
    }
    if let Some(requests) = &args.requests {
        return seam::finish(NAME, batch::run(print_schema::OPERATIONS, requests));
    }
    let answer = seam::required(args.schema.as_deref(), "--schema")
        .and_then(seam::read_input)
        .map(|sdl| match build(sdl) {
            Ok(_) => Answer::Payload(Vec::new()),
            Err(error) => Answer::refused(vec![seam::definition_error_json(&error)]),
        });
    seam::finish(NAME, answer)
}

/// Builds the schema of `sdl` the way `buildSchema(sdl)` builds it.
///
/// No option is set, so the SDL rules and the default parser both run, which
/// is the call of the Aosabi build.
///
/// # Errors
///
/// Returns what `buildSchema` throws: the syntax error of the parser, the
/// plain error of the SDL rules or the error of a type that cannot be built.
pub fn build(sdl: impl Into<Source>) -> Result<Schema, DefinitionError> {
    aosabi_validate::build_schema(sdl, BuildSchemaOptions::default(), ParseOptions::default())
}

/// Answers the operation `build_schema`.
///
/// `errors` is empty when the schema builds and carries the one thrown error
/// otherwise, which is `opBuildSchema` of the oracle.
pub fn op_build_schema(request: &JsObject) -> Result<JsObject, JsString> {
    let sdl = batch::require_source(request, "sdl")?;
    let errors: Vec<JsValue> = match build(sdl) {
        Ok(_) => Vec::new(),
        Err(error) => vec![seam::definition_error_json(&error)],
    };
    let mut payload = JsObject::with_capacity(1);
    payload.insert("errors", errors);
    Ok(payload)
}
