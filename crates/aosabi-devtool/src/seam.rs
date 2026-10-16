// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool seam: the input, output and exit code rules the language,
 * type and validation primitives share.
 *
 * No analogue in the reference.
 */

//! The shared seam of the language, type and validation primitives.
//!
//! A primitive reads its inputs through [`read_input`], answers an [`Answer`]
//! or a [`Failure`], and ends through [`finish`]. The bytes of an answer go to
//! stdout with no trailing newline. The exit codes are 0 for a payload, 1 for
//! an error array written in place of a payload, and the codes of `sysexits.h`
//! for a call that never reached the crate under test: 64 for a call that
//! names no input or a wrong one, 65 for an input that cannot be used, 66 for
//! a file that cannot be read and 74 for a write to stdout that fails. Such a
//! failure writes one line to stderr and no byte to stdout.

use std::{
    fmt,
    fs,
    io::{
        self,
        Read,
        Write, //
    },
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::GraphQLError;
use aosabi_type::DefinitionError;

use crate::{
    batch::{
        self,
        Operations, //
    },
    cli::Matches,
    json::reader,
    stub, //
};

/// The milestone the primitives of this seam land at.
pub const LANDS_AT: &str = "M2";

/// The exit code of an error array written in place of a payload.
const EXIT_REFUSED: u8 = 1;
/// The exit code of a call that names no input or a wrong one, `EX_USAGE`.
const EX_USAGE: u8 = 64;
/// The exit code of an input that cannot be used, `EX_DATAERR`.
const EX_DATAERR: u8 = 65;
/// The exit code of a file that cannot be read, `EX_NOINPUT`.
const EX_NOINPUT: u8 = 66;
/// The exit code of a write to stdout that fails, `EX_IOERR`.
const EX_IOERR: u8 = 74;

/// The path that names stdin in place of a file.
const STDIN: &str = "-";

/// What a primitive writes to stdout.
#[derive(Debug)]
pub enum Answer {
    /// The bytes the primitive exists to write, which exit 0.
    Payload(Vec<u8>),
    /// An error array written in place of the payload, which exits 1.
    Refused(Vec<u8>),
}

impl Answer {
    /// Returns the answer that carries `errors` as its payload.
    ///
    /// This is the answer of a primitive whose question is the error array
    /// itself, so an array with entries still exits 0.
    pub fn errors(errors: Vec<JsValue>) -> Self {
        Self::Payload(error_array(errors))
    }

    /// Returns the answer of a primitive that was refused with `errors`.
    pub fn refused(errors: Vec<JsValue>) -> Self {
        Self::Refused(error_array(errors))
    }

    /// Returns the answer that carries `text` as its payload.
    pub fn text(text: String) -> Self {
        Self::Payload(text.into_bytes())
    }
}

/// Why a call wrote no bytes.
#[derive(Debug)]
pub enum Failure {
    /// The call named no input or a wrong one, with the reason.
    Usage(String),
    /// An input cannot be used, with the reason.
    Data(String),
    /// A file could not be read, with the reason.
    NoInput(PathBuf, io::Error),
    /// Stdout refused the bytes.
    Io(io::Error),
}

impl Failure {
    /// Returns the exit code the failure earns.
    const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => EX_USAGE,
            Self::Data(_) => EX_DATAERR,
            Self::NoInput(..) => EX_NOINPUT,
            Self::Io(_) => EX_IOERR,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(reason) | Self::Data(reason) => write!(f, "{reason}"),
            Self::NoInput(path, error) => write!(f, "cannot read {}: {error}", path.display()),
            Self::Io(error) => write!(f, "cannot write stdout: {error}"),
        }
    }
}

/// Prints the interface line of the implemented primitive `name`.
pub fn smoke(name: &str) -> ExitCode {
    smoke_at(name, LANDS_AT)
}

/// Prints the interface line of the implemented primitive `name`, which lands
/// at the milestone `lands_at`.
///
/// This is [`smoke`] for a primitive of a later milestone than [`LANDS_AT`],
/// so the line names the milestone whose notebooks judge it.
pub fn smoke_at(name: &str, lands_at: &str) -> ExitCode {
    println!("{}", stub::interface_line(name, true, lands_at));
    ExitCode::SUCCESS
}

/// What a primitive whose one form is the batch form accepts, the matches of
/// [`crate::cli::BATCH_GRAMMAR`].
#[derive(Debug)]
pub struct BatchArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl BatchArgs {
    /// Reads the arguments `matches` holds for [`crate::cli::BATCH_GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the primitive `name` of the milestone `lands_at` over `operations` in the batch form.
///
/// Under `--smoke` it prints the interface line, and otherwise it answers
/// every request line of the file `--requests` names through [`batch::run`]
/// and ends through [`finish`], with the usage failure of a call that names
/// no file.
pub fn run_batch(
    name: &str,
    lands_at: &str,
    operations: &Operations,
    args: &BatchArgs,
) -> ExitCode {
    if args.smoke {
        return smoke_at(name, lands_at);
    }
    let outcome = required(args.requests.as_deref(), "--requests")
        .and_then(|requests| batch::run(operations, requests));
    finish(name, outcome)
}

/// Ends the call of the primitive `name` with the exit code `outcome` earns.
///
/// An answer goes to stdout as bytes with no trailing newline. A failure goes
/// to stderr as one line that names the primitive, and stdout stays empty.
pub fn finish(name: &str, outcome: Result<Answer, Failure>) -> ExitCode {
    let written = outcome.and_then(|answer| match answer {
        Answer::Payload(bytes) => emit(&bytes).map(|()| ExitCode::SUCCESS),
        Answer::Refused(bytes) => emit(&bytes).map(|()| ExitCode::from(EXIT_REFUSED)),
    });
    written.unwrap_or_else(|failure| {
        eprintln!("aosabi-devtool {name}: {failure}");
        ExitCode::from(failure.exit_code())
    })
}

/// Writes the bytes to stdout with no trailing newline.
fn emit(bytes: &[u8]) -> Result<(), Failure> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(bytes)
        .and_then(|()| stdout.flush())
        .map_err(Failure::Io)
}

/// Returns the input the flag `flag` names, or the usage failure of its absence.
///
/// # Errors
///
/// Returns [`Failure::Usage`] when `path` is `None`.
pub fn required<'a>(path: Option<&'a Path>, flag: &str) -> Result<&'a Path, Failure> {
    path.ok_or_else(|| Failure::Usage(format!("the call names no {flag}")))
}

/// Reads one input as UTF-8 text, from stdin when `path` is `-`.
///
/// # Errors
///
/// Returns [`Failure::NoInput`] when the file or stdin cannot be read and
/// [`Failure::Data`] when the bytes are not UTF-8.
pub fn read_input(path: &Path) -> Result<String, Failure> {
    let bytes = read_bytes(path)?;
    String::from_utf8(bytes)
        .map_err(|error| Failure::Data(format!("{}: not UTF-8: {error}", path.display())))
}

/// Reads one input as a JSON document, from stdin when `path` is `-`.
///
/// # Errors
///
/// Returns [`Failure::NoInput`] when the file or stdin cannot be read and
/// [`Failure::Data`] when the bytes are not one JSON document.
pub fn read_json(path: &Path) -> Result<JsValue, Failure> {
    let bytes = read_bytes(path)?;
    reader::parse(&bytes)
        .map_err(|error| Failure::Data(format!("{}: not JSON: {error}", path.display())))
}

/// Reads one input as bytes, from stdin when `path` is `-`.
///
/// # Errors
///
/// Returns [`Failure::NoInput`] when the file or stdin cannot be read.
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, Failure> {
    let read = if path.as_os_str() == STDIN {
        let mut bytes = Vec::new();
        io::stdin().lock().read_to_end(&mut bytes).map(|_| bytes)
    } else {
        fs::read(path)
    };
    read.map_err(|error| Failure::NoInput(path.to_path_buf(), error))
}

/// Returns the usage failure of a call that reads stdin more than once.
///
/// # Errors
///
/// Returns [`Failure::Usage`] when both paths are `-`, because stdin is one
/// stream and the second reader would see it empty.
pub fn refuse_shared_stdin(first: &Path, second: &Path) -> Result<(), Failure> {
    if first.as_os_str() == STDIN && second.as_os_str() == STDIN {
        return Err(Failure::Usage(String::from(
            "only one input of a call can be read from stdin",
        )));
    }
    Ok(())
}

/// Returns `error` the way its own `toJSON` writes it.
///
/// The keys are `message`, `locations`, `path` and `extensions` in that order,
/// each left out when absent.
pub fn graphql_error_json(error: &GraphQLError) -> JsValue {
    JsValue::from(error.to_json())
}

/// Returns a thrown value of the type layer the way the oracle serializes it.
///
/// A `GraphQLError` renders through its own `toJSON`. A plain `Error` has no
/// `toJSON`, so it renders as `message` alone, which is the second arm of
/// `serializeError` in tests/sidecar/graphql-js.mjs.
pub fn definition_error_json(error: &DefinitionError) -> JsValue {
    match error {
        DefinitionError::GraphQL(error) => graphql_error_json(error),
        DefinitionError::Plain(message) => {
            let mut object = JsObject::with_capacity(1);
            object.insert("message", message.clone());
            JsValue::Object(object)
        }
    }
}

/// Returns the entry of an error array that carries `message` and nothing else.
///
/// This is the shape of a refusal that is no `GraphQLError`, which is the
/// plain arm of [`definition_error_json`]. The refusals of the schema layer
/// are of that kind, because the reference throws no error where the port
/// refuses, or throws a plain `Error`.
pub fn message_json(message: impl Into<JsValue>) -> JsValue {
    let mut object = JsObject::with_capacity(1);
    object.insert("message", message);
    JsValue::Object(object)
}

/// Returns the bytes of `errors` as one JSON array.
fn error_array(errors: Vec<JsValue>) -> Vec<u8> {
    // An array always has a JSON form, so the writer's `None` arm, which is
    // the undefined top level value, is unreachable here.
    aosabi_json::to_bytes(&JsValue::Array(errors)).unwrap_or_default()
}
