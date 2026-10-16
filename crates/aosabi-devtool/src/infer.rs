// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool infer: infers the intermediate representation from a
 * descriptor dump.
 *
 * No analogue in the reference. The judging notebooks are
 * tests/notebooks/10_schema_tier1.ipynb and
 * tests/notebooks/11_schema_builder_corpus.ipynb, the second of which is the
 * inference corpus.
 */

//! The `infer` primitive of the seam.
//!
//! `--descriptors PATH` names the `descriptors.json` a build of an
//! application writes, where the path `-` is stdin. The dump carries every
//! descriptor the roots reach, so the inference runs from the file and the
//! seam links no application. `--emit` names the payload.
//!
//! | `--emit` | Payload |
//! | --- | --- |
//! | `ir`, the default | the JSON of the inferred intermediate representation, with no trailing newline |
//! | `sdl` | the text `to_sdl` writes for it, which ends in the one newline the printer appends |
//! | `type-defs` | the text the build writes as `schema.graphql` |
//! | `resolvers-js` | the text the build writes as `resolvers.js`, which ends in one newline |
//!
//! The payload is UTF-8 and the exit code is 0. Each warning of the inference
//! goes to stderr as one `warning: ` line and changes no exit code. An
//! inference that throws writes a JSON array that holds the one thrown error
//! as `message` and exits 1. A file that is not JSON or is not a descriptor
//! dump exits 65 and writes nothing.
//!
//! The primitive has no batch form, because a dump is a file of its own and
//! not a line of a request stream.

use std::{
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode, //
};

use aosabi_infer::{
    Built,
    DescriptorSource,
    Graph,
    InferError,
    SchemaBuilder, //
};

use crate::{
    cli::{
        self,
        Choice,
        Grammar,
        Matches,
        Opt,
        Refusal, //
    },
    print_sdl,
    seam::{
        self,
        Answer,
        Failure, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "infer";

/// The command line of the `infer` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        Opt::path(
            "descriptors",
            "PATH",
            "Read the descriptor dump from this file, or from stdin when it is `-`",
        ),
        Opt::choice(
            "emit",
            "ARTIFACT",
            "The artifact of the inference to write",
            &Emit::CHOICES,
        )
        .default("ir"),
    ],
    positional: None,
    conflicts: &[],
};

/// What the `infer` primitive accepts.
#[derive(Debug)]
pub struct InferArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of the descriptor dump, where `-` is stdin.
    descriptors: Option<PathBuf>,
    /// The artifact the call writes.
    emit: Emit,
}

impl InferArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    ///
    /// The parser takes no other artifact than the ones of [`Emit::CHOICES`]
    /// and reads `ir` when the command line names none.
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when the name of the artifact is not UTF-8.
    pub fn read(matches: &Matches) -> Result<Self, Refusal> {
        let emit = matches.text("emit")?;
        Ok(Self {
            smoke: matches.flag("smoke"),
            descriptors: matches.path("descriptors"),
            emit: emit.as_deref().and_then(Emit::named).unwrap_or(Emit::Ir),
        })
    }
}

/// The artifacts of one inference, of which a call writes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Emit {
    /// The JSON of the intermediate representation, the text of ir.json
    /// without its newline.
    Ir,
    /// The SDL of the intermediate representation.
    Sdl,
    /// The text of schema.graphql, which is the SDL and the @inContext
    /// directive behind it.
    TypeDefs,
    /// The text of resolvers.js, which is the type resolver of every union
    /// and interface as the reference exports it.
    ResolversJs,
}

impl Emit {
    /// The name of each artifact on the command line and the line the help of
    /// `--emit` describes it with, in the order of the variants.
    const CHOICES: [Choice; 4] = [
        Choice {
            name: "ir",
            help: "The JSON of the intermediate representation, the text of ir.json without its newline",
        },
        Choice {
            name: "sdl",
            help: "The SDL of the intermediate representation",
        },
        Choice {
            name: "type-defs",
            help: "The text of schema.graphql, which is the SDL and the @inContext directive behind it",
        },
        Choice {
            name: "resolvers-js",
            help: "The text of resolvers.js, which is the type resolver of every union and interface as the reference exports it",
        },
    ];

    /// Returns the artifact the command line calls `name`.
    fn named(name: &str) -> Option<Self> {
        match name {
            "ir" => Some(Self::Ir),
            "sdl" => Some(Self::Sdl),
            "type-defs" => Some(Self::TypeDefs),
            "resolvers-js" => Some(Self::ResolversJs),
            _ => None,
        }
    }
}

/// Runs the `infer` primitive.
pub fn run(args: &InferArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, print_sdl::LANDS_AT);
    }
    let answer = seam::required(args.descriptors.as_deref(), "--descriptors")
        .and_then(read_graph)
        .map(|graph| answer(infer(graph), args.emit));
    seam::finish(NAME, answer)
}

/// Reads the descriptor graph the file `path` holds as JSON.
///
/// # Errors
///
/// Returns [`Failure::NoInput`] when the file cannot be read and
/// [`Failure::Data`] when it holds no JSON or no descriptor dump.
fn read_graph(path: &Path) -> Result<Graph, Failure> {
    let value = seam::read_json(path)?;
    Graph::from_js_value(&value).map_err(|error| {
        let path = path.display();
        Failure::Data(format!("{path}: not a descriptor dump: {error}"))
    })
}

/// Runs the inference over `graph` the way a build script runs it.
///
/// # Errors
///
/// Returns what the inference throws.
fn infer(graph: Graph) -> Result<Built, InferError> {
    let source = DescriptorSource::new(graph);
    SchemaBuilder::new(&source).build()
}

/// Returns the answer of the call, and writes the warnings to stderr.
fn answer(built: Result<Built, InferError>, emit: Emit) -> Answer {
    let emitted = built.and_then(|built| {
        warn(&built.warnings);
        artifact(built, emit)
    });
    match emitted {
        Ok(text) => Answer::text(text),
        Err(error) => Answer::refused(vec![seam::message_json(error.to_string())]),
    }
}

/// Writes each warning to stderr behind the prefix `warning: `.
fn warn(warnings: &[String]) {
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}

/// Returns the artifact of `built` that `emit` names.
///
/// # Errors
///
/// Returns the refusal of the printer for [`Emit::Sdl`].
fn artifact(built: Built, emit: Emit) -> Result<String, InferError> {
    match emit {
        Emit::Ir => Ok(aosabi_ir::to_json(&built.ir)),
        Emit::Sdl => Ok(aosabi_ir::to_sdl(&built.ir)?),
        Emit::TypeDefs => Ok(built.type_defs),
        Emit::ResolversJs => Ok(aosabi_build::resolvers_js(&built)),
    }
}
