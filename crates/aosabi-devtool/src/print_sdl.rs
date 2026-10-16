// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool print-sdl: prints an intermediate representation as SDL the
 * way the build prints it.
 *
 * No analogue in the reference.
 */

//! The `print-sdl` primitive of the seam.
//!
//! `--ir PATH` names a file that holds the JSON of an intermediate
//! representation, where the path `-` is stdin. The payload is the text
//! `to_sdl` writes for it, as UTF-8, with exit code 0. The text ends in the
//! one newline the printer appends, and the seam adds none of its own. With
//! `--type-defs` the payload is the text a build writes as `schema.graphql`,
//! which is the same SDL, then an empty line, the `@inContext` directive and
//! one newline. A name the collation refuses writes a JSON array that holds
//! the one refusal as `message` and exits 1. A file that is not JSON or is
//! not an intermediate representation exits 65 and writes nothing.
//!
//! `--requests PATH` is the batch form. It answers the four operations of the
//! protocol that reach aosabi-ir, and the batch form of `collate` answers the
//! same four.
//!
//! | `op` | Request fields | Payload |
//! | --- | --- | --- |
//! | `to_sdl` | `ir` | `sdl`, or `errors` |
//! | `type_defs` | `ir` | `type_defs`, or `errors` |
//! | `ir_json` | `ir` | `ir` |
//! | `collate` | `a`, `b` | `order`, or `errors` |
//!
//! The field `ir` is the JSON text of an intermediate representation, carried
//! as a string, so the bytes of the text reach the reader unchanged. An `ir`
//! that is not JSON or is not an intermediate representation answers `fatal`.
//! The `ir` of `ir_json` is the text the JSON writer of aosabi-ir gives for
//! the value it read.

use std::{
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode, //
};

use aosabi_ir::{
    CollateError,
    IN_CONTEXT_SDL,
    SchemaIr, //
};
use aosabi_json::{
    JsObject,
    JsString, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    cli::{
        self,
        Grammar,
        Matches,
        Opt, //
    },
    collate,
    json::reader,
    seam::{
        self,
        Answer,
        Failure, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "print-sdl";

/// The milestone the primitives of the schema layer land at.
pub const LANDS_AT: &str = "M3a";

/// The operations of the protocol that reach aosabi-ir.
pub const OPERATIONS: &Operations = &[
    ("to_sdl", op_to_sdl),
    ("type_defs", op_type_defs),
    ("ir_json", op_ir_json),
    ("collate", collate::op_collate),
];

/// The command line of the `print-sdl` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        Opt::path(
            "ir",
            "PATH",
            "Read the JSON of the intermediate representation from this file, or from stdin when it is `-`",
        ),
        Opt::flag(
            "type-defs",
            "Write the text of schema.graphql, which is the SDL and the @inContext directive behind it",
        ),
        cli::REQUESTS,
    ],
    positional: None,
    conflicts: &[("requests", &["ir", "type-defs"])],
};

/// What the `print-sdl` primitive accepts.
#[derive(Debug)]
pub struct PrintSdlArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of the intermediate representation, where `-` is stdin.
    ir: Option<PathBuf>,
    /// Whether the call writes the text of schema.graphql.
    type_defs: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl PrintSdlArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            ir: matches.path("ir"),
            type_defs: matches.flag("type-defs"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `print-sdl` primitive.
pub fn run(args: &PrintSdlArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    if let Some(requests) = &args.requests {
        return seam::finish(NAME, batch::run(OPERATIONS, requests));
    }
    let answer = seam::required(args.ir.as_deref(), "--ir")
        .and_then(read_ir)
        .map(|ir| print(&ir, args.type_defs));
    seam::finish(NAME, answer)
}

/// Reads the intermediate representation the file `path` holds as JSON.
///
/// # Errors
///
/// Returns [`Failure::NoInput`] when the file cannot be read and
/// [`Failure::Data`] when it holds no JSON or no intermediate representation.
fn read_ir(path: &Path) -> Result<SchemaIr, Failure> {
    let value = seam::read_json(path)?;
    aosabi_ir::from_js_value(&value)
        .map_err(|error| Failure::Data(format!("{}: not an IR: {error}", path.display())))
}

/// Returns the answer of the single call, the SDL or the text of the build.
fn print(ir: &SchemaIr, with_directive: bool) -> Answer {
    let printed = if with_directive {
        type_defs(ir)
    } else {
        aosabi_ir::to_sdl(ir)
    };
    match printed {
        Ok(text) => Answer::text(text),
        Err(error) => Answer::refused(vec![collate::error_json(&error)]),
    }
}

/// Returns the text a build writes as `schema.graphql` for `ir`.
///
/// The text is the SDL, an empty line, the directive and one newline, which
/// is the template literal of the reference. The builder of aosabi-infer
/// joins the same parts over the IR it inferred, and this function joins them
/// over an IR that was read from a file, so the printer is judged with no
/// inference in front of it.
///
/// # Errors
///
/// Returns what [`aosabi_ir::to_sdl`] refuses.
pub fn type_defs(ir: &SchemaIr) -> Result<String, CollateError> {
    Ok([&aosabi_ir::to_sdl(ir)?, "\n\n", IN_CONTEXT_SDL, "\n"].concat())
}

/// Returns the intermediate representation that travels as `ir` in `request`.
///
/// `op` is the name of the operation that asks, which opens the `fatal` the
/// way every `fatal` of the protocol opens.
///
/// # Errors
///
/// Returns the `fatal` of a field that is missing, is no string, is not JSON
/// or is not an intermediate representation.
fn ir_of(request: &JsObject, op: &str) -> Result<SchemaIr, String> {
    let text = batch::require_string(request, "ir")?;
    let value = reader::parse(text.as_bytes())
        .map_err(|error| format!("{op}: the field ir is not JSON: {error}"))?;
    aosabi_ir::from_js_value(&value)
        .map_err(|error| format!("{op}: the field ir is not an IR: {error}"))
}

/// Returns the payload that carries `printed` under `key`, or its refusal
/// under `errors`.
fn printed_payload(key: &str, printed: Result<String, CollateError>) -> JsObject {
    let mut payload = JsObject::with_capacity(1);
    match printed {
        Ok(text) => payload.insert(key, text),
        Err(error) => payload.insert("errors", vec![collate::error_json(&error)]),
    };
    payload
}

/// Answers the operation `to_sdl`.
fn op_to_sdl(request: &JsObject) -> Result<JsObject, JsString> {
    let ir = ir_of(request, "to_sdl")?;
    Ok(printed_payload("sdl", aosabi_ir::to_sdl(&ir)))
}

/// Answers the operation `type_defs`.
fn op_type_defs(request: &JsObject) -> Result<JsObject, JsString> {
    let ir = ir_of(request, "type_defs")?;
    Ok(printed_payload("type_defs", type_defs(&ir)))
}

/// Answers the operation `ir_json`.
fn op_ir_json(request: &JsObject) -> Result<JsObject, JsString> {
    let ir = ir_of(request, "ir_json")?;
    let mut payload = JsObject::with_capacity(1);
    payload.insert("ir", aosabi_ir::to_json(&ir));
    Ok(payload)
}
