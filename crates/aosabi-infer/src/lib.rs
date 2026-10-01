// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-infer: the schema inference, which turns the types of an application
 * into the intermediate representation and the schema text.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/schema/, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * This file holds the module list and the names the crate exports at its
 * root.
 */

//! The schema inference of Aosabi.
//!
//! The inference derives a GraphQL schema from the types of an application in
//! five modules over one seam, the [`TypeSource`] trait, whose methods are
//! named after the calls of the TypeScript checker. The modules translate
//! their TypeScript sources literally, function by function and guard by
//! guard, because the order in which the walk registers
//! types is the order of the printed schema, and the exit number of the
//! milestone is byte equality of that text.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`type_source`] | no file, the seam | [`TypeSource`], [`TypeFlags`], [`ObjectFlags`], [`SymbolFlags`], [`ModifierFlags`], [`TupleElement`], [`JsDocTag`] and [`HeritageDeclaration`] |
//! | [`error`] | the `throw new Error` sites of the five files and the one read of `undefined` the engine refuses | [`InferError`] |
//! | [`scalars`] | `scalars.ts:1-16` | [`SCALARS`] and [`SCALAR_NAMES`] |
//! | [`types_helper`] | `types-helper.ts:1-238` | the predicates, such as [`is_list`] and [`safe_type_name`] |
//! | [`type_definition_builder`] | `type-definition-builder.ts:1-676` | [`TypeDefinitionBuilder`], [`FieldDefinition`], [`TypeDefinitionOptions`], [`Union`] and [`Enum`] |
//! | [`schema_parser`] | `schema-parser.ts:1-1390` | [`SchemaParser`], [`Schema`], [`Type`], [`Field`], [`Argument`], [`Input`], [`Interface`], [`TypeDefinition`], [`Index`], [`ResolveTypeRule`], [`ResolveTypeCheck`] and [`api_shape`] |
//! | [`builder`] | `builder.ts:347-446` | [`SchemaBuilder`] and [`Built`] |
//! | [`descriptor_source`] | no file | [`DescriptorSource`], [`Graph`] and [`GraphError`], the first type source, and [`AuthoringNode`], [`AuthoringHeritage`], [`AuthoringGuard`] and [`AuthoringGuardCall`], its authoring map |
//!
//! # The seam
//!
//! The five translated modules never import [`descriptor_source`] and never
//! import `aosabi_core`. They know a type as [`TypeSource::Type`] and nothing
//! more. The descriptor graph is the first implementation, and it runs over
//! an owned [`Graph`] that a linked application and a file can both feed, so
//! the devtool drives the inference with no application linked.
//!
//! # What the crate refuses
//!
//! The inference keeps the behaviour of its TypeScript sources and adds one
//! refusal of its own, which is
//! [`InferError::Sdl`] for a type name outside the alphabet of the collation
//! that orders an `implements` list. Every other refusal of Aosabi is a
//! compile error before the inference runs, through the `Described` trait and
//! the derive macros. The three warnings of the TypeScript sources keep their
//! text and are entries of [`Built::warnings`].
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3, and the judging notebook is
//! `tests/notebooks/10_schema_tier1.ipynb`.

pub mod builder;
pub mod descriptor_source;
pub mod error;
pub mod scalars;
pub mod schema_parser;
pub mod type_definition_builder;
pub mod type_source;
pub mod types_helper;

pub use crate::{
    builder::{
        Built,
        SchemaBuilder, //
    },
    descriptor_source::{
        AuthoringGuard,
        AuthoringGuardCall,
        AuthoringHeritage,
        AuthoringNode,
        DescriptorSource,
        Graph,
        GraphError, //
    },
    error::InferError,
    scalars::{
        SCALAR_NAMES,
        SCALARS, //
    },
    schema_parser::{
        Argument,
        Field,
        Index,
        Input,
        Interface,
        ResolveTypeCheck,
        ResolveTypeRule,
        Schema,
        SchemaParser,
        Type,
        TypeDefinition,
        api_shape, //
    },
    type_definition_builder::{
        Enum,
        FieldDefinition,
        TypeDefinitionBuilder,
        TypeDefinitionOptions,
        Union, //
    },
    type_source::{
        HeritageDeclaration,
        JsDocTag,
        ModifierFlags,
        ObjectFlags,
        SymbolFlags,
        TupleElement,
        TypeFlags,
        TypeSource, //
    },
    types_helper::{
        exclude_null_undefined_from_type,
        get_promise_type,
        get_public_properties_of_type,
        is_boolean,
        is_empty_object,
        is_function,
        is_list,
        is_literal_type,
        is_never,
        is_primitive,
        is_primitive_union,
        is_promise,
        is_subscription_repeater,
        safe_type_name, //
    }, //
};
