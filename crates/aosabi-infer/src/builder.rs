// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-infer: the schema builder, which reads the three roots of an
 * application, runs the parser and assembles the schema text.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/schema/builder.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * Nothing of that file outside `build` is translated. The TypeScript program
 * cache of `:23-56` has no counterpart, because no program is built here. The
 * entry discovery of `:233-345` and the root lookup of `:348-394` are
 * `TypeSource::type_of_root`.
 */

//! The schema builder of the inference.
//!
//! [`SchemaBuilder::build`] is `build` of the reference without an
//! authoritative contribution, the path every application without the object
//! relational mapper takes. The schema text is the printed intermediate
//! representation, two newlines, the definition of the context directive and
//! one newline. The directive is appended to the text and is not part of the
//! intermediate representation, so an application cannot shadow it with a
//! type of the same name.

use aosabi_ir::{
    IN_CONTEXT_SDL,
    RootName,
    SchemaIr, //
};
use aosabi_json::{
    map::IndexMap,
    value::KeyHasherBuilder, //
};

use crate::{
    error::InferError,
    schema_parser::{
        Index,
        ResolveTypeRule,
        SchemaParser, //
    },
    type_source::TypeSource, //
};

/// What one build of a schema yields, the object literal of
/// `builder.ts:440-445`.
///
/// The working schema of the reference result is left out, because it carries
/// type handles of one source and no reader of a build needs it.
#[derive(Debug, Clone)]
pub struct Built {
    /// The schema text, which is the bytes of `schema.graphql`.
    pub type_defs: String,
    /// The intermediate representation the text was printed from.
    pub ir: SchemaIr,
    /// The source text of the type resolver of each union and interface,
    /// keyed by type name.
    pub resolvers: IndexMap<String, String, KeyHasherBuilder>,
    /// The type resolver of each union and interface as the model of its
    /// text, keyed by type name, which the table of a server is written from.
    pub resolve_type_rules: IndexMap<String, ResolveTypeRule, KeyHasherBuilder>,
    /// The text of every warning the reference prints, in emission order.
    pub warnings: Vec<String>,
}

/// Every output type an inference named, each with its GraphQL name, in the
/// order the inference named them.
pub type TypeNames<T> = Vec<(T, String)>;

/// Builds the schema of the application a [`TypeSource`] describes,
/// `builder.ts:233-446`.
pub struct SchemaBuilder<'s, S: TypeSource> {
    /// The type source, which stands in for the program and the checker.
    source: &'s S,
}

impl<'s, S: TypeSource> SchemaBuilder<'s, S> {
    /// Creates a builder over `source`.
    #[must_use]
    pub fn new(source: &'s S) -> Self {
        Self { source }
    }

    /// Infers the schema, `builder.ts:347-446` without the `contributeIR`
    /// branch.
    ///
    /// # Errors
    ///
    /// Returns every error of [`SchemaParser::parse`], and
    /// [`InferError::Sdl`] when the printer refuses a type name.
    pub fn build(&self) -> Result<Built, InferError> {
        self.build_named().map(|(built, _)| built)
    }

    /// Infers the schema as [`Self::build`] does, and answers beside it every
    /// output type the inference named with its GraphQL name, so the caller
    /// that holds the types of the source finds the name each of them prints
    /// under.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::build`] returns.
    pub fn build_named(&self) -> Result<(Built, TypeNames<S::Type>), InferError> {
        let query_type = self.source.type_of_root(RootName::Query);
        let mutation_type = self.source.type_of_root(RootName::Mutation);
        let subscription_type = self.source.type_of_root(RootName::Subscription);

        let mut parser = SchemaParser::new(self.source);

        parser.parse(Index {
            query: query_type,
            mutation: mutation_type,
            subscription: subscription_type,
        })?;

        let ir = parser.to_ir();
        let type_defs = [&parser.to_sdl_string()?, "\n\n", IN_CONTEXT_SDL, "\n"].concat();
        let resolve_type_rules = parser.get_resolve_type_rules();
        let resolvers = resolve_type_rules
            .iter()
            .map(|(name, rule)| (name.clone(), rule.to_js()))
            .collect();

        Ok((
            Built {
                type_defs,
                ir,
                resolvers,
                resolve_type_rules,
                warnings: parser.warnings().to_vec(),
            },
            parser.type_names(),
        ))
    }
}
