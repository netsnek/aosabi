// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the builder that turns a schema document, or its source text,
 * into a schema.
 *
 * Derived from graphql-js 16.12.0, src/utilities/buildASTSchema.ts, MIT.
 */

//! A schema built from SDL.
//!
//! `buildSchema` is the call the build path makes on the SDL it
//! generated, with no options, so the SDL validation runs inside it and a
//! problem of the SDL throws before `validateSchema` is reached. The builders
//! themselves are the ones of the extension of a schema, run over an empty
//! schema config.
//!
//! # The SDL validation seam
//!
//! The reference imports `assertValidSDL` from its validation directory. The
//! rules of that directory belong to the validation crate, which sits above this
//! one and which this crate therefore cannot name. The two builders take the
//! function as an argument of the type [`AssertValidSdl`], the validation crate
//! hands its `assert_valid_sdl` in, and the builders call it at the line the
//! reference calls it.

use alloc::{
    sync::Arc,
    vec::Vec, //
};

use aosabi_json::JsObject;
use aosabi_lang::{
    ParseOptions,
    Source,
    ast::DocumentNode,
    parse, //
};

use crate::{
    definition::DefinitionError,
    directives::{
        Directive,
        specified_directives, //
    },
    extend_schema::{
        ExtendSchemaOptions,
        extend_schema_impl_with_bindings, //
    },
    introspection::introspection_schema,
    schema::{
        Schema,
        SchemaConfig, //
    },
    schema_bindings::SchemaBindings, //
};

/// The options of `buildASTSchema` and `buildSchema`.
///
/// Extends `GraphQLSchemaValidationOptions` with `assumeValidSDL`. Both default
/// to false.
///
/// Derived from graphql-js 16.12.0, src/utilities/buildASTSchema.ts, MIT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuildSchemaOptions {
    /// When true, no type system validation runs on the built schema.
    pub assume_valid: bool,
    /// Set to true to assume the SDL is valid.
    pub assume_valid_sdl: bool,
}

impl From<BuildSchemaOptions> for ExtendSchemaOptions {
    fn from(options: BuildSchemaOptions) -> Self {
        Self {
            assume_valid: options.assume_valid,
            assume_valid_sdl: options.assume_valid_sdl,
        }
    }
}

/// The SDL validation of a schema document, which the validation crate
/// provides.
///
/// `assertValidSDL` validates the document with the SDL rules and throws a plain
/// `Error` whose message joins the message of every validation error with a
/// blank line. The function the validation crate hands in returns that error as
/// [`DefinitionError::Plain`] with the joined message, and `Ok(())` for a valid
/// document.
pub type AssertValidSdl = fn(&DocumentNode) -> Result<(), DefinitionError>;

/// This takes the ast of a schema document produced by the parse function of
/// the language layer.
///
/// If no schema definition is provided, then it looks for types named Query,
/// Mutation and Subscription. Given that AST it constructs a [`Schema`]. The
/// resulting schema has no resolve methods, so execution uses default resolvers.
///
/// Derived from graphql-js 16.12.0, src/utilities/buildASTSchema.ts, MIT.
///
/// `document_ast` is the document, `options` the two flags and
/// `assert_valid_sdl` the SDL validation of the validation crate, which runs
/// unless one of the flags is set. The `devAssert` of the reference over the
/// kind of the document has no reading, because the argument type carries
/// the kind.
///
/// # Errors
///
/// Returns what `assert_valid_sdl` returns, what a builder throws while it
/// reads the document and what the schema constructor throws. A type named
/// `Query`, `Mutation` or `Subscription` that is no object type is no error
/// here, and `validateSchema` reports it.
pub fn build_ast_schema(
    document_ast: &DocumentNode,
    options: BuildSchemaOptions,
    assert_valid_sdl: AssertValidSdl,
) -> Result<Schema, DefinitionError> {
    build_ast_schema_with_bindings(
        document_ast,
        options,
        assert_valid_sdl,
        &SchemaBindings::default(),
    )
}

/// Builds a [`Schema`] from the ast of a schema document with `bindings`
/// laid over its types.
///
/// With empty bindings it is [`build_ast_schema`], and the other arguments are
/// the same.
///
/// # Errors
///
/// Returns what [`build_ast_schema`] returns.
pub fn build_ast_schema_with_bindings(
    document_ast: &DocumentNode,
    options: BuildSchemaOptions,
    assert_valid_sdl: AssertValidSdl,
    bindings: &SchemaBindings,
) -> Result<Schema, DefinitionError> {
    if !options.assume_valid && !options.assume_valid_sdl {
        assert_valid_sdl(document_ast)?;
    }

    let empty_schema_config = SchemaConfig {
        description: None,
        query: None,
        mutation: None,
        subscription: None,
        types: Vec::new(),
        directives: Some(Vec::new()),
        extensions: JsObject::new(),
        ast_node: None,
        extension_ast_nodes: Vec::new(),
        assume_valid: false,
    };
    // The reference gets the config it passed in back for a document with no
    // definition, which is the empty one.
    let mut config = extend_schema_impl_with_bindings(
        &empty_schema_config,
        document_ast,
        options.into(),
        bindings,
    )?
    .unwrap_or(empty_schema_config);

    if config.ast_node.is_none() {
        for ty in &config.types {
            // Note: While this could make early assertions to get the correctly
            // typed values below, that would throw immediately while type system
            // validation with validateSchema() will produce more actionable results.
            match ty.name() {
                "Query" => config.query = Some(ty.clone()),
                "Mutation" => config.mutation = Some(ty.clone()),
                "Subscription" => config.subscription = Some(ty.clone()),
                _ => {}
            }
        }
    }

    let directives = with_specified_directives(config.directives.take().unwrap_or_default());

    Schema::new(
        SchemaConfig {
            directives: Some(directives),
            ..config
        },
        &introspection_schema()?,
    )
}

/// Appends every specified directive the document does not declare itself.
///
/// A directive of the document that carries the name of a specified one replaces
/// it, and the specified ones that remain follow in their own order.
fn with_specified_directives(config_directives: Vec<Arc<Directive>>) -> Vec<Arc<Directive>> {
    // If specified directives were not explicitly declared, add them.
    let missing: Vec<Arc<Directive>> = specified_directives()
        .iter()
        .filter(|std_directive| {
            config_directives
                .iter()
                .all(|directive| directive.name != std_directive.name)
        })
        .cloned()
        .collect();

    let mut directives = config_directives;
    directives.extend(missing);
    directives
}

/// A helper function to build a [`Schema`] directly from a source document.
///
/// `source` is the SDL text or a source, `options` the two flags of the build,
/// `parse_options` the options of the parser and `assert_valid_sdl` the SDL
/// validation of the validation crate. The reference forwards `noLocation` and
/// `allowLegacyFragmentVariables` to the parser and nothing else, so
/// `max_tokens` of `parse_options` is not read.
///
/// Derived from graphql-js 16.12.0, src/utilities/buildASTSchema.ts, MIT.
///
/// # Errors
///
/// Returns the syntax error of the parser as [`DefinitionError::GraphQL`],
/// and what [`build_ast_schema`] returns.
pub fn build_schema(
    source: impl Into<Source>,
    options: BuildSchemaOptions,
    parse_options: ParseOptions,
    assert_valid_sdl: AssertValidSdl,
) -> Result<Schema, DefinitionError> {
    build_schema_with_bindings(
        source,
        options,
        parse_options,
        assert_valid_sdl,
        &SchemaBindings::default(),
    )
}

/// Builds a [`Schema`] directly from a source document with `bindings` laid
/// over its types.
///
/// With empty bindings it is [`build_schema`], and the other arguments are the
/// same.
///
/// # Errors
///
/// Returns what [`build_schema`] returns.
pub fn build_schema_with_bindings(
    source: impl Into<Source>,
    options: BuildSchemaOptions,
    parse_options: ParseOptions,
    assert_valid_sdl: AssertValidSdl,
    bindings: &SchemaBindings,
) -> Result<Schema, DefinitionError> {
    let document = parse(
        source,
        ParseOptions {
            no_location: parse_options.no_location,
            allow_legacy_fragment_variables: parse_options.allow_legacy_fragment_variables,
            ..ParseOptions::default()
        },
    )?;

    build_ast_schema_with_bindings(&document, options, assert_valid_sdl, bindings)
}
