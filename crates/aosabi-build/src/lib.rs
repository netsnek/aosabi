// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-build: the build step, which runs inside the build script of a server
 * crate and turns the descriptors of an application crate into the schema
 * files.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/bundler/emit-server-glue.ts and
 * packages/pylon/src/cli/builder/update-file-if-changed.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * This file holds the module list and the names the crate exports at its root.
 */

//! The build step of Aosabi.
//!
//! A build infers the schema from the types of an application, validates it,
//! and writes `.aosabi/schema.graphql` only when the text changed. This crate
//! is that step for a cargo build. It reads the roots descriptor of the
//! application, runs the inference of `aosabi-infer` over it, validates the
//! schema text the way the runtime builds it, and writes five artifacts. Before
//! that, the build script of the application crate writes the declaration list
//! the roots carry.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`declarations`](mod@declarations) | the source file sweep of `schema-parser.ts:1270-1284` | [`declarations_main`], [`write_declarations`], [`Declared`] and [`DECLARATIONS_DIR`] |
//! | [`emit`](mod@emit) | `emit-server-glue.ts:162-192` | [`emit_from`], [`emit_cli_from`], [`emit()`], [`emit_schema`], [`resolvers_js`], [`resolve_types_rs`], [`take_artifacts`], [`Emitted`] and the file name constants, and behind the feature `source` `build_source`, `emit_source` and `SourceBuild` |
//! | [`assert_schema_is_valid`](mod@assert_schema_is_valid) | `emit-server-glue.ts:36-59` | [`assert_schema_is_valid()`], [`schema_invalid_message`] and [`SchemaInvalid`] |
//! | [`prepare_object_injection`](mod@prepare_object_injection) | `emit-server-glue.ts:61-71` | [`prepare_object_injection()`] and [`Injected`] |
//! | [`update_file_if_changed`](mod@update_file_if_changed) | `update-file-if-changed.ts:1-15` | [`update_file_if_changed()`] |
//! | [`lean_link`](mod@lean_link) | no file | [`lean_link()`], [`LinuxCard`], [`Linker`], [`LINUX_CARD_ENV`], [`LINKER_ENV`] and [`DISCARD_UNWIND_FILE`] |
//! | [`authoring`](mod@authoring) | no file | [`authoring_json`] |
//! | [`inspect`](mod@inspect) | no file, `introspectAppData` of `cli/project-bridge.ts` and `cli/inspect.ts:50-70` | [`inspect_from`], the inspect runner entry, and behind the feature `source` `introspect_source` and `inspect_source_from`, the runner of a plain crate |
//! | [`artifact_dirs`] | no file | [`ArtifactDirs`] and [`ARTIFACT_DIR_ENV`] |
//! | [`error`] | no file | [`BuildError`], [`ScanRefusal`] and [`ModuleRefusal`] |
//! | `source` | `cli/builder/schema/builder.ts:233-394`, behind the feature `source` | the reader of the plain source of a crate, its entry, its reach, `source::names::graphql_name`, `source::SourceTypes`, the third type source, `source::glue`, the generated module of its glue, and `source::write_copy`, the generated copy of the crate |
//!
//! The file `schema_invalid_message.rs` beside these modules is no module. It
//! is the constant text of the invalid schema message, written by
//! `tools/provenance` and read with `include_str!`.
//!
//! # The protocol
//!
//! The application is a library crate. It names `aosabi-build` under
//! `[build-dependencies]`, and its whole build script is one call, which
//! writes the declaration list of the crate:
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     aosabi_build::declarations_main()
//! }
//! ```
//!
//! The server crate names the application under `[dependencies]` and under
//! `[build-dependencies]`, names `aosabi-build` under `[build-dependencies]`,
//! and its whole build script is one call as well, over the entry of the
//! application:
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     aosabi_build::emit_from(my_app::default)
//! }
//! ```
//!
//! The server package of a Linux card calls [`lean_link()`] before it, which
//! takes the unwind tables out of a binary for `aarch64-unknown-linux-gnu` or
//! `x86_64-unknown-linux-gnu` built with `-C panic=abort` in `RUSTFLAGS` and
//! links the card [`LinuxCard::Server`] with packed relocations:
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     aosabi_build::lean_link(aosabi_build::LinuxCard::Server);
//!     aosabi_build::emit_from(my_app::default)
//! }
//! ```
//!
//! The type the entry returns carries the roots, from which the inference
//! reaches every type through links, and the declaration list of the
//! application crate, which the derive of every root and the macro
//! `graphql!` read from the build script of that crate.
//! [`BuildError::NoDeclarationList`] reports roots whose crate has no such
//! build script.
//!
//! A program that is no build script, such as the runner of a case of the
//! builder corpus, is the post compile route of the same step. Its whole
//! `main` is one call, and its one argument names the artifact directory:
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     aosabi_build::emit_cli_from(case_x::default)
//! }
//! ```
//!
//! The inspect runner of the command line is a program of the same kind. Its
//! whole `main` is one call as well, and its one argument names the file it
//! writes, the introspection of the app:
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     aosabi_build::inspect_from(my_app::default)
//! }
//! ```
//!
//! # The artifacts
//!
//! | File | Bytes |
//! | --- | --- |
//! | [`SCHEMA_FILE`] | `to_sdl(ir)`, `"\n\n"`, `IN_CONTEXT_SDL` and `"\n"`, which is `Built::type_defs` of `aosabi-infer` |
//! | [`RESOLVERS_FILE`] | `export const resolvers = `, the resolver of every union and interface as [`prepare_object_injection()`] writes them, and `"\n"` |
//! | [`IR_FILE`] | `JSON.stringify(ir)` and one `"\n"` |
//! | [`DESCRIPTORS_FILE`] | the dump of the descriptor graph and one `"\n"` |
//! | [`AUTHORING_FILE`] | the authoring map of the build as [`authoring_json`] writes it, which check C2 of `12_authoring_parity` reads |
//!
//! They land in [`ArtifactDirs::artifact_dir`], which is `target/aosabi/` of
//! the workspace that builds, and a second copy of the schema lands in
//! `OUT_DIR`, where the server binary embeds it, beside
//! [`RESOLVE_TYPES_FILE`] and [`NAME_FILE`]. `resolvers.js` is the text
//! the reference writes, a copy that no Rust server imports, so the type
//! resolver of every union and interface compares as bytes. `schema.mjs`,
//! `server.mjs` and `worker.mjs` of the reference have no twin, because a
//! Rust server links its application and imports nothing at run time.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3, and the judging notebook is
//! `tests/notebooks/10_schema_tier1.ipynb`, with
//! `tests/notebooks/17_framework_size.ipynb` and
//! `tests/notebooks/31_http_errors.ipynb` for the lean link.

pub mod artifact_dirs;
pub mod assert_schema_is_valid;
pub mod authoring;
pub mod declarations;
pub mod emit;
pub mod error;
pub mod inspect;
pub mod lean_link;
pub mod prepare_object_injection;
#[cfg(feature = "source")]
pub mod source;
pub mod update_file_if_changed;

pub use crate::{
    artifact_dirs::{
        ARTIFACT_DIR_ENV,
        ArtifactDirs, //
    },
    assert_schema_is_valid::{
        SchemaInvalid,
        assert_schema_is_valid,
        schema_invalid_message, //
    },
    authoring::authoring_json,
    declarations::{
        DECLARATIONS_DIR,
        Declared,
        declarations_main,
        write_declarations, //
    },
    emit::{
        AUTHORING_FILE,
        DESCRIPTORS_FILE,
        Emitted,
        IR_FILE,
        NAME_FILE,
        RESOLVE_TYPES_FILE,
        RESOLVERS_FILE,
        SCHEMA_FILE,
        emit,
        emit_cli_from,
        emit_from,
        emit_schema,
        resolve_type_rules,
        resolve_types_rs,
        resolvers_js,
        take_artifacts, //
    },
    error::{
        BuildError,
        ModuleRefusal,
        ScanRefusal, //
    },
    inspect::inspect_from,
    lean_link::{
        DISCARD_UNWIND_FILE,
        LINKER_ENV,
        LINUX_CARD_ENV,
        Linker,
        LinuxCard,
        lean_link, //
    },
    prepare_object_injection::{
        Injected,
        prepare_object_injection, //
    },
    update_file_if_changed::update_file_if_changed, //
};
#[cfg(feature = "source")]
pub use crate::{
    emit::{
        SourceBuild,
        build_source,
        emit_source, //
    },
    inspect::{
        inspect_source_from,
        introspect_source, //
    }, //
};
