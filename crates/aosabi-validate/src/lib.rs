// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the GraphQL validation layer of Aosabi.
 *
 * This file holds the module list and the names the crate exports at its root.
 *
 * Derived from graphql-js 16.12.0, src/validation, MIT.
 */

//! The GraphQL validation layer of Aosabi.
//!
//! The crate is a literal translation of the validation directory of graphql-js
//! 16.12.0, the version the wire is pinned to. Every function keeps the
//! graphql-js name in snake case and follows the TypeScript line by line,
//! because the exit number of the milestone is byte equality of the error text
//! and the locations of every specified rule.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`validate`](mod@validate) | `validate.ts` | [`validate()`], [`validate_sdl()`], [`assert_valid_sdl()`] and [`assert_valid_sdl_extension()`] |
//! | [`validation_context`] | `ValidationContext.ts` | [`AstValidationContext`], [`SdlValidationContext`], [`ValidationContext`], the three rule values and [`Rule`] |
//! | [`specified_rules`] | `specifiedRules.ts` | [`SPECIFIED_RULES`], [`SPECIFIED_SDL_RULES`], [`RECOMMENDED_RULES`] and the lookup of a rule by name |
//! | [`rules`] | `rules/*.ts` | one module for each rule file |
//! | [`group_by`](mod@group_by) | `jsutils/groupBy.ts` | [`group_by()`] |
//! | [`key_map`](mod@key_map) | `jsutils/keyMap.ts` | [`key_map()`] |
//! | [`build_schema`](mod@build_schema) | the call of `assertValidSDL` in `buildASTSchema.ts` and `extendSchema.ts` | [`build_schema()`], [`build_ast_schema()`] and [`extend_schema()`] with the SDL rules handed in |
//! | [`pass`] | no analogue | the single pass, [`pass::check()`] over a field table of the schema and the tokens of a document |
//!
//! # The shape of the port
//!
//! A rule of the reference is a function from a context to a visitor object. A
//! rule here is a struct that implements [`Rule`] and the visitor of the
//! language layer, beside a function with the name of the reference that boxes
//! it, and a rule list holds that function with its name. The contexts are
//! shared by every rule of one walk and work behind `&self`.
//!
//! An exception of the reference becomes a stop state of the context. The error
//! limit of [`validate()`] and a failing thunk of the schema both set it, and
//! the walk breaks at the next hook.
//!
//! The crate builds with no standard library and needs an allocator, because
//! the bare metal build of a microcontroller card links it. The `std` feature
//! is the configuration symbol of a host build and gates nothing today. There
//! is no test in this crate, which is the rule of CONTRIBUTING.md section 3,
//! and the judging notebook is `tests/notebooks/02_language.ipynb`.

#![no_std]

extern crate alloc;

pub mod build_schema;
pub mod group_by;
pub mod key_map;
pub mod pass;
pub mod rules;
pub mod specified_rules;
pub mod validate;
pub mod validation_context;

pub use crate::{
    build_schema::{
        build_ast_schema,
        build_ast_schema_with_bindings,
        build_schema,
        build_schema_with_bindings,
        extend_schema, //
    },
    group_by::group_by,
    key_map::key_map,
    specified_rules::{
        RECOMMENDED_RULES,
        SPECIFIED_RULES,
        SPECIFIED_SDL_RULES,
        specified_rule_named,
        specified_sdl_rule_named, //
    },
    validate::{
        DEFAULT_MAX_ERRORS,
        ValidateOptions,
        assert_valid_sdl,
        assert_valid_sdl_extension,
        validate,
        validate_sdl, //
    },
    validation_context::{
        AstValidationContext,
        AstValidationRule,
        NodeWithSelectionSet,
        OnError,
        Rule,
        SdlOrValidationContext,
        SdlValidationContext,
        SdlValidationRule,
        ValidationAbort,
        ValidationContext,
        ValidationRule,
        VariableUsage, //
    }, //
};
