// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool client: writes the three files of the TypeScript client a
 * build emits for an SDL text.
 *
 * No analogue in the reference. The judging notebook is
 * tests/notebooks/70_client.ipynb, which sends the same request lines to this
 * primitive and to the operation client of the command line sidecar.
 */

//! The `client` primitive of the seam.
//!
//! Its one form is `--requests PATH`, the batch form of [`crate::batch`].
//!
//! | `op` | Request fields | Payload |
//! | --- | --- | --- |
//! | `client` | `sdl` | `index_ts`, `types_ts` and `index_js`, or `error` |
//!
//! The operation carries the name and the field `sdl` of the operation
//! `client` of the command line oracle, so one request line reaches both
//! sides, and the three texts carry the keys of its answer. The steps are
//! those of `buildClient` after it has read the schema: the client files with
//! the two scalar types [`BUILD_CLIENT_SCALARS`] and the endpoint `/graphql`,
//! and the bundle of `index.ts` with its import `./types`. The payload `error`
//! is the message of what either step throws, the message of `buildSchema` for
//! an SDL that does not build. The primitive writes no file, where
//! `build_client` writes the three under the artifact directory.
//!
//! The oracle takes the endpoint of `generateClientFiles` as well, which
//! rolldown carries into `index.js`. The build passes none, and the segments
//! of the bundle hold the default endpoint, so a request that names
//! `endpoint` answers `fatal` rather than a bundle the build never writes.

use std::process::ExitCode;

use aosabi_json::{
    JsObject,
    JsString, //
};
use aosabi_lang::ParseOptions;
use aosabi_query::{
    BUILD_CLIENT_SCALARS,
    ClientError,
    GenerateClientOptions,
    GeneratedClientFiles,
    bundle_index_js,
    describe_schema,
    generate_client_files, //
};
use aosabi_type::BuildSchemaOptions;

use crate::{
    batch::{
        self,
        Operations, //
    },
    seam::{
        self,
        BatchArgs, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "client";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M8";

/// The operations of the batch form.
const OPERATIONS: &Operations = &[("client", op_client)];

/// Runs the `client` primitive, whose one form is the batch form.
pub fn run(args: &BatchArgs) -> ExitCode {
    seam::run_batch(NAME, LANDS_AT, OPERATIONS, args)
}

/// The three files of a client.
struct ClientFiles {
    /// The files `client/index.ts` and `client/types.ts`.
    sources: GeneratedClientFiles,
    /// The file `client/index.js`, the bundle of `index.ts`.
    index_js: String,
}

/// Returns the three files the build of the client writes for the schema
/// of `sdl`.
///
/// The bundle reads the schema and the descriptor the client files come
/// from, so the schema is built a second time from the same text with the
/// same options, which gives the same type map.
///
/// # Errors
///
/// Returns what [`generate_client_files()`] returns, and the message of what
/// the bundle throws as [`ClientError::Schema`].
fn client_files(sdl: &str) -> Result<ClientFiles, ClientError> {
    let options = GenerateClientOptions {
        scalar_types: BUILD_CLIENT_SCALARS,
        endpoint: None,
    };
    let sources = generate_client_files(sdl, &options)?;
    let schema =
        aosabi_validate::build_schema(sdl, BuildSchemaOptions::default(), ParseOptions::default())?;
    let descriptor = describe_schema(&schema)?;
    let index_js = bundle_index_js(&schema, &descriptor)?;
    Ok(ClientFiles { sources, index_js })
}

/// Answers the operation `client`.
fn op_client(request: &JsObject) -> Result<JsObject, JsString> {
    let sdl = batch::require_string(request, "sdl")?;
    if request.contains_key("endpoint") {
        return Err(JsString::from(
            "client: the bundle carries the endpoint the build passes, so the request names no endpoint",
        ));
    }
    let mut payload = JsObject::with_capacity(3);
    match client_files(sdl) {
        Ok(files) => {
            payload.insert("index_ts", files.sources.index_ts);
            payload.insert("types_ts", files.sources.types_ts);
            payload.insert("index_js", files.index_js);
        }
        Err(error) => {
            payload.insert("error", error.to_string());
        }
    }
    Ok(payload)
}
