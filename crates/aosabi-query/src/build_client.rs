// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-query: the client step of the build, which writes the three client
 * files.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/build-client.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The client step of the build.
//!
//! [`build_client()`] reads the schema the build wrote under the artifact
//! directory and writes the typed client beside it, `client/types.ts`,
//! `client/index.ts` and the bundled `client/index.js`, which is
//! `.aosabi/client/` by default. The two sources go through a write that
//! leaves an unchanged file alone, and the bundle is written on every run that
//! generates, as rolldown writes its output file on every run.

use core::fmt;
use std::{
    fs,
    io,
    path::{
        Path,
        PathBuf, //
    }, //
};

use aosabi_type::DefinitionError;

use crate::{
    generate::{
        GenerateClientOptions,
        generate, //
    },
    index_js::bundle_index_js, //
};

/// The scalar types the build of the client passes, build-client.ts:44-49.
pub const BUILD_CLIENT_SCALARS: &[(&str, &str)] = &[
    ("Number", "number"),
    ("JSONObject", "Record<string, unknown>"),
];

/// The schema the build writes under the artifact directory.
const SCHEMA_FILE: &str = "schema.graphql";

/// The directory of the client under the artifact directory.
const CLIENT_DIR: &str = "client";

/// The authoring root type of the client.
const TYPES_TS: &str = "types.ts";

/// The source of the runtime client.
const INDEX_TS: &str = "index.ts";

/// The bundled runtime client, whose presence marks a current client.
const INDEX_JS: &str = "index.js";

/// What [`build_client()`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuiltClient {
    /// The files written, which is empty when the client was current.
    pub written: Vec<PathBuf>,
}

/// Says why the client could not be generated or written.
#[derive(Debug)]
pub enum ClientError {
    /// The schema the build writes does not exist.
    ///
    /// Its message names `cargo aosabi build` and `cargo aosabi dev`.
    SchemaNotFound,
    /// The SDL did not build, with the graphql-js message.
    Schema(String),
    /// A read or a write of the file at the path failed.
    Io(PathBuf, io::Error),
}

impl fmt::Display for ClientError {
    /// Writes the message of the error.
    ///
    /// The text of [`ClientError::SchemaNotFound`] names the two commands of
    /// `cargo aosabi` that write the schema.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaNotFound => f.write_str(
                "Schema not found. Please run `cargo aosabi build` or `cargo aosabi dev` first.",
            ),
            Self::Schema(message) => f.write_str(message),
            Self::Io(path, error) => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl From<DefinitionError> for ClientError {
    /// Returns [`ClientError::Schema`] with the message of `error`.
    fn from(error: DefinitionError) -> Self {
        Self::Schema(error.to_string())
    }
}

impl core::error::Error for ClientError {
    /// Returns the error of the file system behind [`ClientError::Io`].
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io(_, error) => Some(error),
            Self::SchemaNotFound | Self::Schema(_) => None,
        }
    }
}

/// Writes `client/types.ts`, `client/index.ts` and `client/index.js` under `artifact_dir`, unless
/// `schema_changed` is false and `client/index.js` exists.
///
/// The schema is `schema.graphql` under `artifact_dir`, which the build
/// writes. The client is regenerated when the schema changed or when the
/// client does not exist, with [`BUILD_CLIENT_SCALARS`] as its scalar types
/// and the default endpoint.
///
/// # Errors
///
/// Returns [`ClientError::SchemaNotFound`] when the schema does not exist,
/// [`ClientError::Schema`] when it does not build, and [`ClientError::Io`]
/// when its read or a write of the client fails.
pub fn build_client(artifact_dir: &Path, schema_changed: bool) -> Result<BuiltClient, ClientError> {
    let schema_path = artifact_dir.join(SCHEMA_FILE);
    let client_dir = artifact_dir.join(CLIENT_DIR);

    if !schema_path.exists() {
        return Err(ClientError::SchemaNotFound);
    }
    if !schema_changed && client_dir.join(INDEX_JS).exists() {
        return Ok(BuiltClient::default());
    }

    let sdl = fs::read(&schema_path).map_err(|error| ClientError::Io(schema_path, error))?;
    // The reference reads the file as `utf-8`, whose decoder replaces an ill
    // formed sequence with U+FFFD.
    let sdl = String::from_utf8_lossy(&sdl);
    let options = GenerateClientOptions {
        scalar_types: BUILD_CLIENT_SCALARS,
        endpoint: None,
    };
    let generated = generate(&sdl, &options)?;

    fs::create_dir_all(&client_dir).map_err(|error| ClientError::Io(client_dir.clone(), error))?;
    let mut written = Vec::new();
    for (name, text) in [
        (TYPES_TS, &generated.files.types_ts),
        (INDEX_TS, &generated.files.index_ts),
    ] {
        let path = client_dir.join(name);
        match update_file_if_changed(&path, text) {
            Ok(true) => written.push(path),
            Ok(false) => {}
            Err(error) => return Err(ClientError::Io(path, error)),
        }
    }

    let index_js = bundle_index_js(&generated.schema, &generated.descriptor)?;
    let path = client_dir.join(INDEX_JS);
    match fs::write(&path, index_js) {
        Ok(()) => written.push(path),
        Err(error) => return Err(ClientError::Io(path, error)),
    }
    Ok(BuiltClient { written })
}

/// Writes `new_content` to `path` unless the file already holds it, and
/// answers whether it wrote.
fn update_file_if_changed(path: &Path, new_content: &str) -> io::Result<bool> {
    match fs::read(path) {
        Ok(current_content) if current_content == new_content.as_bytes() => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::write(path, new_content)?;
    Ok(true)
}
