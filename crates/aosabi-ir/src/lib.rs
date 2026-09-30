// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-ir: the intermediate representation and its SDL printer.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/ir/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * This file holds the module list and the names the crate exports at its root.
 */

//! The intermediate representation of Aosabi.
//!
//! The IR is the normalized, serializable description of an application that
//! sits between the inference and every generated artifact. The inference
//! fills it, the printer projects it to the text of `schema.graphql`, and its
//! JSON form is `ir.json`. The printer is a literal translation, because the
//! exit number of the milestone is byte equality of that text with the text
//! the reference writes.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`ir`] | `ir/ir.ts:13-30` and `:227-338` | [`SchemaIr`], [`TypeRef`], [`RootName`], [`Field`], [`Entity`], [`ObjectType`], [`InterfaceType`], [`EnumType`], [`UnionType`], [`InputType`], [`Operation`] and [`empty_ir()`] |
//! | [`record`] | no file, ECMA-262 `OrdinaryOwnPropertyKeys` | [`Record`], the JavaScript object of the model |
//! | [`sdl`] | `ir/sdl.ts:1-139` | [`to_sdl()`] and [`render_type()`] |
//! | [`collate`](mod@collate) | no file, stands in for `localeCompare` at `ir/sdl.ts:42` | [`collate()`] and [`CollateError`] |
//! | [`json`] | no file, follows the object literals of `cli/builder/schema/schema-parser.ts:641-756` | [`to_js_value()`], [`to_json()`], [`from_js_value()`] and [`IrError`] |
//! | [`in_context`] | `core/in-context.ts:36-44` | [`IN_CONTEXT_SDL`] |
//! | [`app_model`] | `cli/inspect.ts:19-49` and `:74-138` | the canonical application model of `inspect` |
//!
//! # What is not here
//!
//! `index.ts` of the reference exports four more files, `merge.ts`, `diff.ts`,
//! `ddl.ts` and `dialect.ts`. None of them is translated and none has a stub.
//! The helpers `tableSpecOf`, `pgIdent`, `joinTableName` and `joinColumn` and
//! the migration types `TableSpec`, `TableColumn`, `PhysicalTable`,
//! `PhysicalSchema` and `ForeignKeyChange` of `ir.ts:87-225` land with them.
//!
//! # The shape of the port
//!
//! A `Record<string, T>` of the reference is a [`Record`], an insertion
//! ordered map that reads in the key order of a JavaScript object, and every
//! map of the crate carries the hasher of the JSON writer. No serde derive
//! exists here, because `aosabi-json` is the only crate that writes JSON
//! bytes. A string of the model is a Rust `String`, and a lone surrogate is
//! out of contract.
//!
//! The crate builds with no standard library and needs an allocator. The `std`
//! feature is the configuration symbol of a host build and gates nothing
//! today. There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3, and the judging notebook is
//! `tests/notebooks/15_sdl_printer.ipynb`.

#![no_std]

extern crate alloc;

pub mod app_model;
pub mod collate;
pub mod in_context;
pub mod ir;
pub mod json;
pub mod record;
pub mod sdl;

pub use crate::{
    collate::{
        CollateError,
        collate, //
    },
    in_context::IN_CONTEXT_SDL,
    ir::{
        Entity,
        EnumType,
        Field,
        InputType,
        InterfaceType,
        ObjectType,
        Operation,
        RootName,
        SchemaIr,
        TypeRef,
        UnionType,
        empty_ir, //
    },
    json::{
        IrError,
        from_js_value,
        to_js_value,
        to_json, //
    },
    record::Record,
    sdl::{
        render_type,
        to_sdl, //
    }, //
};
