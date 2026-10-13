// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-query: the TypeScript client a build emits.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/query/build/ and
 * packages/pylon/src/cli/builder/build-client.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * This file holds the module list and the re-exports. The files index_ts.tmpl
 * and index_js.segments beside these modules are no modules. They are the
 * literals client_index_template and client_index_js_segments, written by
 * tools/provenance and by nothing else.
 */

//! The TypeScript client a build emits.
//!
//! The client is build output for the browsers that import it and not part of
//! Aosabi, which ships no client runtime, so the files import the query runtime
//! the app installs and read the locale from the global its pages set.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`describe_schema`](mod@describe_schema) | `query/build/describe-schema.ts:1-72` | [`describe_schema()`] |
//! | [`root_type`] | `query/build/root-type.ts:1-243` | [`generate_root_type()`], [`DEFAULT_SCALARS`] |
//! | [`generate`] | `query/build/index.ts:21-84` | [`GenerateClientOptions`], [`GeneratedClientFiles`], [`generate_client_files()`] |
//! | [`index_js`] | the output of rolldown 1.2.4 | [`bundle_index_js()`] |
//! | [`build_client`](mod@build_client) | `cli/builder/build-client.ts:1-68` | [`build_client()`], [`BuiltClient`], [`ClientError`], [`BUILD_CLIENT_SCALARS`] |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3.

pub mod build_client;
pub mod describe_schema;
pub mod generate;
pub mod index_js;
pub mod root_type;

pub use crate::{
    build_client::{
        BUILD_CLIENT_SCALARS,
        BuiltClient,
        ClientError,
        build_client, //
    },
    describe_schema::describe_schema,
    generate::{
        GenerateClientOptions,
        GeneratedClientFiles,
        generate_client_files, //
    },
    index_js::bundle_index_js,
    root_type::{
        DEFAULT_SCALARS,
        generate_root_type, //
    }, //
};
