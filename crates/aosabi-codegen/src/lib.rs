// SPDX-License-Identifier: MIT
/*
 * aosabi-codegen: the emission of the run time glue of a type, which the
 * macros of the derive path and the builder of the source path share.
 *
 * No analogue on the reference side, where a class instance is an object of
 * the executor already and a method is a function already. This file holds
 * the module list, the description of an item the glue reads and the rule of
 * the names of the derive path.
 */

//! The emission of the run time glue of a type.
//!
//! The glue is what lets the executor read a Rust value and call a Rust
//! method: the implementations of the traits of the module `glue` of
//! `aosabi-core`, `ObjectLike`, `SharedObject`, `FieldValue`, `IntoValue`,
//! `ResolverMethods` and `FromArgument`, and the functions of the executor of
//! the methods of a block. Two writers emit it. The procedural macros of
//! `aosabi-macros` emit it for the twins and the registries of the derive
//! path, the mirror path, beside the descriptor of each item, and the builder
//! of `aosabi-build` emits it for every root, type and inherent block the
//! roots of a plain crate reach, into the module the generated copy of the
//! crate adds to its crate root. This crate is the one library both call,
//! written in the project, so the glue of a member is written once.
//!
//! | Module | Holds |
//! | --- | --- |
//! | [`glue`] | the glue of the derive path, of a derived struct, enum and newtype and of a resolvers block, and the bodies of the functions of the executor `graphql!` shares |
//! | [`plain`] | the glue of the source path, of a struct, an enum and a payload enum of a plain crate and of the entry with its registered parts |
//! | [`spread`] | the object of a struct with a field under `#[aosabi(spread)]` |
//! | [`remote`] | the reading of a delegated value into a type of a pulled registry |
//! | [`lints`] | the lint allowances every emitted item carries |
//! | `unify` | whether two payload types of a generic enum can be one type |
//!
//! Every generated path opens with `::aosabi::`, `::core::` or, in the glue
//! of the derive path, `::std::`, so the glue names the facade by the one
//! name an application gives it. The crate emits tokens and parses nothing
//! but the helper attributes of a resolvers block, and it links into the
//! procedural macros and into the build host alone, never into a card.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebooks are
//! `tests/notebooks/10_schema_tier1.ipynb`,
//! `tests/notebooks/30_http_tier1.ipynb` and
//! `tests/notebooks/31_http_errors.ipynb` for the glue of both paths.

use syn::{
    Generics,
    Ident,
    ext::IdentExt, //
};

pub mod glue;
pub mod lints;
pub mod plain;
pub mod remote;
pub mod spread;
mod unify;

/// The parts of a struct or an enum the glue of the derive path reads: its
/// identifier and its generics.
///
/// The derive reads them from the input of the macro, and the glue reads the
/// fields and the variants beside them.
#[derive(Clone, Copy)]
pub struct TypeItem<'a> {
    /// The identifier of the type.
    pub ident: &'a Ident,
    /// The generics of the type, as the source writes them.
    pub generics: &'a Generics,
}

/// Returns the name of `ident` with a raw prefix removed and nothing else
/// changed, which is the field name policy of the derive path,
/// `docs/schema-inference.md` section 5.3.
#[must_use]
pub fn unraw(ident: &Ident) -> String {
    ident.unraw().to_string()
}
