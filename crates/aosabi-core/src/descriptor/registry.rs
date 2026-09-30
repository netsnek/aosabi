// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the record of one type declaration of a program.
 *
 * No analogue on the reference side.
 */

//! The record of one type declaration of a program.
//!
//! Every `#[derive(Aosabi)]` and `#[aosabi::scalar]` emits one
//! [`Declaration`] as a constant beside its item, named
//! `__AOSABI_DECLARATION_<NAME>`. The build script of the crate, which calls
//! `aosabi_build::declarations_main`, scans the module tree and writes the
//! list of those constants for every target of the crate, and the derive
//! reads the list of its target into [`Described::DECLARATIONS`], so the
//! roots carry it as [`Roots::DECLARATIONS`]. The list is a constant with no
//! linker section, so it holds the same declarations on every target and in
//! every profile.
//!
//! [`Described::DECLARATIONS`]: super::Described::DECLARATIONS
//! [`Roots::DECLARATIONS`]: super::Roots::DECLARATIONS

use super::{
    info::{
        HeritageInfo,
        SourceKey, //
    },
    link::TypeLink,
};

/// One type declaration, which a macro emits as a constant.
///
/// The heritage sweep of a TypeScript build visits every class of the program, also one no
/// root reaches. A walk from the roots cannot find such a type, so every
/// macro emits a declaration of its own beside the descriptor, and the
/// declaration list of the crate names it.
#[derive(Debug)]
pub struct Declaration {
    /// The value of `CARGO_PKG_NAME` in the crate that declares the type, the
    /// tag the inference filters the application crate by.
    pub krate: &'static str,
    /// The place of the macro that emitted the declaration.
    pub source: SourceKey,
    /// The name of the declared type.
    pub name: &'static str,
    /// The link to the declared type, or `None` for a generic declaration,
    /// which names no type before it is instantiated.
    pub link: Option<TypeLink>,
    /// The base types the declaration names, in attribute order. A generic
    /// declaration lists none, because a base type may name a type parameter
    /// and a constant has none in scope.
    pub heritage: &'static [HeritageInfo],
}
