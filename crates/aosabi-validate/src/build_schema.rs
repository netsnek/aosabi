// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the schema builders of the type layer with the SDL
 * validation of this crate handed in.
 */

//! The schema builders with their SDL validation.
//!
//! The reference imports `assertValidSDL` and `assertValidSDLExtension` from
//! its validation directory into its schema builders. The type layer sits
//! below this crate and cannot name it, so its builders take the two
//! functions as an argument, of the types `aosabi_type::AssertValidSdl` and
//! `aosabi_type::AssertValidSdlExtension`.
//!
//! The functions here close that seam. Each one has the name and the
//! parameters of the reference, calls the builder of the type layer, and
//! hands in [`assert_valid_sdl()`] or [`assert_valid_sdl_extension()`]. A
//! caller that builds a schema from SDL calls these and never the builders
//! of the type layer, because a builder that receives another function skips
//! the SDL rules, which the reference does only under `assumeValid` and
//! `assumeValidSDL`. The two bound forms take the bindings of a resolver map
//! beside the document, which is the way `makeExecutableSchema` reaches a
//! type that takes its hooks at construction.

use aosabi_lang::{
    ParseOptions,
    Source,
    ast::DocumentNode, //
};
use aosabi_type::{
    BuildSchemaOptions,
    DefinitionError,
    ExtendSchemaOptions,
    Schema,
    SchemaBindings, //
};

use crate::validate::{
    assert_valid_sdl,
    assert_valid_sdl_extension, //
};

/// This takes the ast of a schema document produced by the parse function of
/// the language layer, and builds a [`Schema`] from it after the SDL rules
/// pass.
///
/// It is `aosabi_type::build_ast_schema()` with [`assert_valid_sdl()`]
/// handed in. `document_ast` is the document and `options` the two flags
/// that skip the SDL rules.
///
/// Derived from graphql-js 16.12.0, src/utilities/buildASTSchema.ts, MIT.
///
/// # Errors
///
/// Returns the plain error of [`assert_valid_sdl()`], and what the builder
/// of the type layer returns.
pub fn build_ast_schema(
    document_ast: &DocumentNode,
    options: BuildSchemaOptions,
) -> Result<Schema, DefinitionError> {
    aosabi_type::build_ast_schema(document_ast, options, assert_valid_sdl)
}

/// Builds a [`Schema`] from the ast of a schema document with `bindings`
/// laid over its types, after the SDL rules pass.
///
/// `aosabi_type::build_ast_schema_with_bindings()` with
/// [`assert_valid_sdl()`] handed in. `document_ast` is the document,
/// `options` the two flags that skip the SDL rules and `bindings` the hooks
/// of the resolver map.
///
/// # Errors
///
/// Returns what [`build_ast_schema()`] returns.
pub fn build_ast_schema_with_bindings(
    document_ast: &DocumentNode,
    options: BuildSchemaOptions,
    bindings: &SchemaBindings,
) -> Result<Schema, DefinitionError> {
    aosabi_type::build_ast_schema_with_bindings(document_ast, options, assert_valid_sdl, bindings)
}

/// A helper function to build a [`Schema`] directly from a source document.
///
/// It is `aosabi_type::build_schema()` with [`assert_valid_sdl()`] handed
/// in. `source` is the SDL text or a source, `options` the two flags of the
/// build and `parse_options` the options of the parser.
///
/// Derived from graphql-js 16.12.0, src/utilities/buildASTSchema.ts, MIT.
///
/// # Errors
///
/// Returns the syntax error of the parser, the plain error of
/// [`assert_valid_sdl()`], and what the builder of the type layer returns.
pub fn build_schema(
    source: impl Into<Source>,
    options: BuildSchemaOptions,
    parse_options: ParseOptions,
) -> Result<Schema, DefinitionError> {
    aosabi_type::build_schema(source, options, parse_options, assert_valid_sdl)
}

/// Builds a [`Schema`] directly from a source document with `bindings` laid
/// over its types, after the SDL rules pass.
///
/// `aosabi_type::build_schema_with_bindings()` with [`assert_valid_sdl()`]
/// handed in. `source` is the SDL text or a source, `options` the two flags
/// of the build, `parse_options` the options of the parser and `bindings`
/// the hooks of the resolver map.
///
/// # Errors
///
/// Returns what [`build_schema()`] returns.
pub fn build_schema_with_bindings(
    source: impl Into<Source>,
    options: BuildSchemaOptions,
    parse_options: ParseOptions,
    bindings: &SchemaBindings,
) -> Result<Schema, DefinitionError> {
    aosabi_type::build_schema_with_bindings(
        source,
        options,
        parse_options,
        assert_valid_sdl,
        bindings,
    )
}

/// Produces a new schema given an existing schema and a document which may
/// contain GraphQL type extensions and definitions.
///
/// It is `aosabi_type::extend_schema()` with
/// [`assert_valid_sdl_extension()`] handed in. The answer is `None` when
/// `document_ast` changes nothing, in which case the reference returns
/// `schema` itself.
///
/// Derived from graphql-js 16.12.0, src/utilities/extendSchema.ts, MIT.
///
/// # Errors
///
/// Returns the plain error of [`assert_valid_sdl_extension()`], and what the
/// builder of the type layer returns.
pub fn extend_schema(
    schema: &Schema,
    document_ast: &DocumentNode,
    options: ExtendSchemaOptions,
) -> Result<Option<Schema>, DefinitionError> {
    aosabi_type::extend_schema(schema, document_ast, options, assert_valid_sdl_extension)
}
