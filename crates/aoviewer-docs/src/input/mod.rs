// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs input: the four inputs of the generator and the one loader of
 * all of them.
 *
 * A build directory, an ir.json, an SDL file and an introspection result load
 * into one model, and the seam of the notebooks reads through the same
 * loader.
 */

//! The inputs.
//!
//! [`load`] reads every input into one [`Model`]. A build directory goes
//! through the SDL path with `ir.json` beside it, an SDL file through the SDL
//! path alone, and `ir.json` and an introspection result through the two
//! readers of the core, past a byte order mark. Every file is read under the
//! [`ReadLimits`] the caller passes, which the command and the seam give as
//! [`ReadLimits::DEFAULT`]. A regular file longer than the readers take is
//! refused by its length, with the refusal its reader gives such a file, and
//! no byte of it past a byte order mark is read, so a document of 64 MiB
//! costs no memory of its size. Any other file, such as a pipe, is read at
//! most one byte past what the readers take. [`detect()`] names the input a
//! path holds, and [`load_path`] reads that input for the seam, with one
//! read of a file for both.

mod build;
mod detect;
mod sdl;

use std::{
    convert::Infallible,
    fs::{
        File,
        Metadata, //
    },
    io::{
        self,
        Read as _,
        Seek as _, //
    },
    path::{
        Path,
        PathBuf, //
    },
};

use aoviewer_core::{
    json::JsonError,
    limits::ReadLimits,
    model::Model,
    read::{
        ReadError,
        from_introspection,
        from_ir_json, //
    },
};

pub use detect::detect;

use crate::error::{
    DocsError,
    JsonRefusal, //
};

/// The served schema a build writes into its directory.
const SCHEMA_FILE: &str = "schema.graphql";

/// The intermediate representation a build writes beside it.
const IR_FILE: &str = "ir.json";

/// The byte order mark of UTF-8.
const BOM: &[u8] = b"\xef\xbb\xbf";

/// One input of the generator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// The default of a build: the served `schema.graphql` of the directory,
    /// and `ir.json` beside it for the argument descriptions and the
    /// agreement check.
    Build {
        /// The build directory.
        dir: PathBuf,
    },
    /// `ir.json` alone, the design's mapping, every description as the
    /// intermediate representation carries it.
    Ir(PathBuf),
    /// Any SDL file, through the SDL path.
    Schema(PathBuf),
    /// Any introspection result.
    Introspection(PathBuf),
}

impl Input {
    /// Returns the word of the input kind: `build`, `ir`, `schema` or
    /// `introspection`.
    ///
    /// The last three are the flags of `aosabi docs` that name the input, and
    /// `build` is its default.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Build { .. } => "build",
            Self::Ir(_) => "ir",
            Self::Schema(_) => "schema",
            Self::Introspection(_) => "introspection",
        }
    }
}

/// What the loader reads of one file.
#[derive(Debug)]
enum Read {
    /// The bytes of the file, at most one more than the readers take past a
    /// byte order mark.
    Bytes(Vec<u8>),
    /// No bytes, since the file is a regular file longer than the readers
    /// take. The value is the byte offset its document starts at, 3 past a
    /// byte order mark and 0 otherwise.
    TooLong(u64),
}

/// A loaded input: the model and the kind of input it came from.
#[derive(Clone, Debug)]
pub struct Loaded {
    /// The model of the schema.
    pub model: Model,
    /// The word of the input kind, as [`Input::kind`] returns it.
    pub kind: &'static str,
}

/// Reads `input` into a model under `limits`.
///
/// # Errors
///
/// Returns [`DocsError::Unreadable`] for a file that cannot be read,
/// [`DocsError::Introspection`], [`DocsError::Sdl`] or [`DocsError::Ir`] for
/// a file its reader refuses, [`DocsError::Disagree`] for a build directory
/// whose two files disagree, and [`DocsError::Model`] for a schema the model
/// builder refuses, whichever input describes it.
pub fn load(input: &Input, limits: &ReadLimits) -> Result<Loaded, DocsError> {
    let model = match input {
        Input::Build { dir } => build::load(dir, limits)?,
        Input::Ir(path) => read_ir(path, limits)?,
        Input::Schema(path) => read_schema(path, &read_input(path, false, limits)?, limits)?,
        Input::Introspection(path) => {
            read_introspection(path, &read_input(path, false, limits)?, limits)?
        }
    };
    Ok(Loaded {
        model,
        kind: input.kind(),
    })
}

/// Reads the input `path` holds into a model under `limits`, its kind named
/// as [`detect()`] names it.
///
/// A directory and a file called `ir.json` are named without a read of
/// them. Any other file is named from the bytes the loader reads of it, which
/// are the bytes [`detect()`] scans, so the seam reads each file once and
/// names it as [`detect()`] would. A file the loader refuses by its length is
/// named by the scan of [`detect()`] itself, which reads it up to its first
/// byte that is no whitespace of JSON, and is refused as its kind refuses it.
///
/// # Errors
///
/// Returns what [`detect()`] and [`load`] return for the path.
pub fn load_path(path: &Path, limits: &ReadLimits) -> Result<Loaded, DocsError> {
    if let Some(input) = detect::named(path)? {
        return load(&input, limits);
    }
    let read = read_input(path, false, limits)?;
    let first = match &read {
        Read::Bytes(bytes) => {
            let Ok(first) = detect::first_byte(bytes.iter().map(|&b| Ok::<u8, Infallible>(b)));
            first
        }
        Read::TooLong(_) => detect::scan(path, limits)?,
    };
    let input = detect::by_first_byte(path, first);
    let model = match &input {
        Input::Introspection(_) => read_introspection(path, &read, limits)?,
        _ => read_schema(path, &read, limits)?,
    };
    Ok(Loaded {
        model,
        kind: input.kind(),
    })
}

/// Reads the SDL file `path`, of which the loader read `read`, under
/// `limits`.
fn read_schema(path: &Path, read: &Read, limits: &ReadLimits) -> Result<Model, DocsError> {
    Ok(sdl::read(path, read, limits)?.builder.finish()?)
}

/// Reads the introspection result `path`, of which the loader read `read`,
/// under `limits`.
fn read_introspection(path: &Path, read: &Read, limits: &ReadLimits) -> Result<Model, DocsError> {
    read_json(path, read, limits, from_introspection, |path, error| {
        DocsError::Introspection { path, error }
    })
}

/// Reads the `ir.json` at `path` under `limits`.
fn read_ir(path: &Path, limits: &ReadLimits) -> Result<Model, DocsError> {
    let read = read_input(path, true, limits)?;
    read_json(path, &read, limits, from_ir_json, |path, error| {
        DocsError::Ir { path, error }
    })
}

/// Reads the JSON document of the file `path`, of which the loader read
/// `read`, with `reader` of the core, past a byte order mark.
///
/// The JSON readers of the core refuse the mark, which [RFC 8259] section
/// 8.1 lets a parser ignore, so the loader passes over it as [`detect()`]
/// does, and the byte offset of a refusal still counts from the start of the
/// file. What the reader refuses in the bytes is `file`, and what the model
/// builder refuses is [`DocsError::Model`], so one defect of a schema has one
/// code whatever input describes it.
///
/// A file refused by its length is refused as the readers refuse a document
/// longer than [`ReadLimits::max_input_bytes`], with [`JsonError::TooLarge`]
/// at the offset where the document starts.
///
/// [RFC 8259]: https://www.rfc-editor.org/rfc/rfc8259
fn read_json(
    path: &Path,
    read: &Read,
    limits: &ReadLimits,
    reader: fn(&[u8], &ReadLimits) -> Result<Model, ReadError>,
    file: fn(PathBuf, JsonRefusal) -> DocsError,
) -> Result<Model, DocsError> {
    let bytes = match read {
        Read::Bytes(bytes) => bytes.as_slice(),
        Read::TooLong(offset) => {
            let error = JsonRefusal::Bytes {
                offset: *offset,
                error: JsonError::TooLarge,
            };
            return Err(file(path.to_path_buf(), error));
        }
    };
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let skipped = (bytes.len() - body.len()) as u64;
    reader(body, limits).map_err(|error| match error {
        ReadError::Model(e) => DocsError::Model(e),
        ReadError::Json { offset, error } => file(
            path.to_path_buf(),
            JsonRefusal::Bytes {
                offset: offset + skipped,
                error,
            },
        ),
        ReadError::Shape { path: at, expected } => file(
            path.to_path_buf(),
            JsonRefusal::Shape { path: at, expected },
        ),
    })
}

/// Reads the file `path`, at most one byte more than `limits` admits past a
/// byte order mark, `from_build` telling whether `cargo aosabi build` writes
/// it.
///
/// The readers refuse a document longer than
/// [`ReadLimits::max_input_bytes`], which a mark before it does not count
/// toward, so the one byte more is enough for them to see it, and a larger
/// file is never read whole. A regular file whose length alone says so is
/// [`Read::TooLong`] with no byte read past the mark.
fn read_input(path: &Path, from_build: bool, limits: &ReadLimits) -> Result<Read, DocsError> {
    let unreadable = |error| DocsError::Unreadable {
        path: path.to_path_buf(),
        error,
        from_build,
    };
    let mut file = File::open(path).map_err(unreadable)?;
    let metadata = file.metadata().ok().filter(Metadata::is_file);
    if let Some(metadata) = &metadata
        && let Some(offset) = too_long(&mut file, metadata.len(), limits).map_err(unreadable)?
    {
        return Ok(Read::TooLong(offset));
    }
    let most = window(limits);
    let size = metadata.map_or(0, |m| m.len()).min(most);
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.take(most)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    Ok(Read::Bytes(bytes))
}

/// Returns the byte offset of the document of the regular file `file`, of
/// `len` bytes, when the readers of `limits` refuse it by its length, 3 past
/// a byte order mark and 0 otherwise, or `None` for a file they read.
///
/// The readers refuse a document of more than
/// [`ReadLimits::max_input_bytes`] past a mark, so a file longer than that
/// and the mark is refused whatever it holds, and a file up to the mark
/// longer is refused when it does not open with the mark. Only such a file
/// has its first three bytes read, and the file is left at its start.
fn too_long(file: &mut File, len: u64, limits: &ReadLimits) -> io::Result<Option<u64>> {
    let most = u64::from(limits.max_input_bytes);
    if len <= most {
        return Ok(None);
    }
    let mark = BOM.len() as u64;
    let mut head = [0u8; BOM.len()];
    file.read_exact(&mut head)?;
    file.rewind()?;
    let marked = head.as_slice() == BOM;
    if marked && len <= most + mark {
        return Ok(None);
    }
    Ok(Some(if marked { mark } else { 0 }))
}

/// Returns the bytes the loader reads of a file under `limits`, one more
/// than [`ReadLimits::max_input_bytes`] past a byte order mark.
fn window(limits: &ReadLimits) -> u64 {
    u64::from(limits.max_input_bytes) + 1 + BOM.len() as u64
}
