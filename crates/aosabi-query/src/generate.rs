// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-query: the two client sources types.ts and index.ts of a schema.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/query/build/index.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The template of index.ts is the literal client_index_template of
 * tools/provenance/provenance.toml, which the tool writes into index_ts.tmpl
 * beside this file and nothing else does.
 */

//! The client sources.
//!
//! [`generate_client_files()`] builds the schema of an SDL text with
//! `buildSchema` of graphql-js, which is [`aosabi_validate::build_schema()`]
//! with its SDL rules, and writes the two sources a build of the client
//! starts from. `types.ts` is the root type of [`crate::root_type`], and
//! `index.ts` is the template `index_ts.tmpl` with the descriptor of
//! [`crate::describe_schema()`] and the endpoint in its two holes, each written
//! as `JSON.stringify` writes it.

use aosabi_json::JsValue;
use aosabi_lang::ParseOptions;
use aosabi_type::{
    BuildSchemaOptions,
    Schema, //
};

use crate::{
    build_client::ClientError,
    describe_schema::describe_schema,
    root_type::{
        generate_root_type,
        json_text, //
    }, //
};

/// The template of `index.ts`, `const indexTs` of index.ts:49-81 with the
/// escapes of its source evaluated and its two holes left as written.
const TEMPLATE: &str = include_str!("index_ts.tmpl");

/// The hole of the descriptor in [`TEMPLATE`].
const DESCRIPTOR_HOLE: &str = "${JSON.stringify(descriptor)}";

/// The hole of the endpoint in [`TEMPLATE`].
const ENDPOINT_HOLE: &str = "${JSON.stringify(endpoint)}";

/// The endpoint baked into the client when the options name none.
const DEFAULT_ENDPOINT: &str = "/graphql";

/// The three constant parts of [`TEMPLATE`] around its two holes.
///
/// A template that does not carry both holes in that order ends the
/// compilation of this crate, so drift of the extracted file never reaches a
/// build as a client with a hole left in it.
const PARTS: Parts = match split_template(TEMPLATE) {
    Some(parts) => parts,
    None => panic!("the client_index_template literal does not carry its two holes"),
};

/// The constant text of the template, split where the two values go.
struct Parts {
    /// The text in front of the descriptor.
    head: &'static str,
    /// The text between the descriptor and the endpoint.
    middle: &'static str,
    /// The text behind the endpoint.
    tail: &'static str,
}

/// The options of [`generate_client_files()`], `GenerateClientOptions` of
/// index.ts:21-26.
#[derive(Clone, Copy, Debug, Default)]
pub struct GenerateClientOptions<'a> {
    /// GraphQL scalar name to TypeScript type, merged over the five defaults of root-type.ts:19-25.
    pub scalar_types: &'a [(&'a str, &'a str)],
    /// The endpoint baked into the client, `/graphql` when `None`.
    pub endpoint: Option<&'a str>,
}

/// The two generated client sources, `GeneratedClientFiles` of index.ts:28-33.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedClientFiles {
    /// `client/index.ts`, the runtime client and the descriptor.
    pub index_ts: String,
    /// `client/types.ts`, the authoring `Data` root type.
    pub types_ts: String,
}

/// The sources of a client with the schema and the descriptor they come from,
/// which the bundle of `index.js` reads.
pub(crate) struct Generated {
    /// The schema built from the SDL.
    pub(crate) schema: Schema,
    /// The descriptor `index.ts` carries.
    pub(crate) descriptor: JsValue,
    /// The two sources.
    pub(crate) files: GeneratedClientFiles,
}

/// Produces the generated client sources for the schema of `sdl`.
///
/// The build of the client writes these to `client/` under the artifact
/// directory, which is `.aosabi/client/` by default. `options` names the
/// scalar types and the endpoint.
///
/// # Errors
///
/// Returns [`ClientError::Schema`] with the message `buildSchema` throws
/// when the SDL does not build, and with the message the root type throws
/// over a schema it cannot render.
pub fn generate_client_files(
    sdl: &str,
    options: &GenerateClientOptions<'_>,
) -> Result<GeneratedClientFiles, ClientError> {
    generate(sdl, options).map(|generated| generated.files)
}

/// Builds the schema of `sdl` and generates its client sources.
///
/// # Errors
///
/// Returns what [`generate_client_files()`] returns.
pub(crate) fn generate(
    sdl: &str,
    options: &GenerateClientOptions<'_>,
) -> Result<Generated, ClientError> {
    let schema =
        aosabi_validate::build_schema(sdl, BuildSchemaOptions::default(), ParseOptions::default())?;
    let descriptor = describe_schema(&schema)?;
    let types_ts = generate_root_type(&schema, options.scalar_types)?;

    let endpoint = options.endpoint.unwrap_or(DEFAULT_ENDPOINT);
    let index_ts = [
        PARTS.head,
        &json_text(&descriptor),
        PARTS.middle,
        &json_text(&JsValue::from(endpoint)),
        PARTS.tail,
    ]
    .concat();

    Ok(Generated {
        schema,
        descriptor,
        files: GeneratedClientFiles { index_ts, types_ts },
    })
}

/// Splits `template` into the text in front of the descriptor hole, the text
/// between the two holes and the text behind the endpoint hole.
///
/// The answer is `None` when either hole is missing or the endpoint hole
/// stands in front of the descriptor hole.
const fn split_template(template: &'static str) -> Option<Parts> {
    let bytes = template.as_bytes();
    let Some(descriptor_at) = find(bytes, DESCRIPTOR_HOLE.as_bytes(), 0) else {
        return None;
    };
    let after_descriptor = descriptor_at + DESCRIPTOR_HOLE.len();
    let Some(endpoint_at) = find(bytes, ENDPOINT_HOLE.as_bytes(), after_descriptor) else {
        return None;
    };
    let after_endpoint = endpoint_at + ENDPOINT_HOLE.len();

    let Some((head, rest)) = template.split_at_checked(descriptor_at) else {
        return None;
    };
    let Some((_, rest)) = rest.split_at_checked(DESCRIPTOR_HOLE.len()) else {
        return None;
    };
    let Some((middle, rest)) = rest.split_at_checked(endpoint_at - after_descriptor) else {
        return None;
    };
    let Some((_, tail)) = rest.split_at_checked(after_endpoint - endpoint_at) else {
        return None;
    };
    Some(Parts { head, middle, tail })
}

/// Returns the offset of the first `needle` in `haystack` at or behind `from`.
///
/// The search is a loop because no search method of a slice is callable in a
/// constant.
const fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let mut start = from;
    while start + needle.len() <= haystack.len() {
        let mut i = 0;
        while i < needle.len() && haystack[start + i] == needle[i] {
            i += 1;
        }
        if i == needle.len() {
            return Some(start);
        }
        start += 1;
    }
    None
}
