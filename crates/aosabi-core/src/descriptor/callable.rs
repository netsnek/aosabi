// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the description of a member whose value is a function.
 *
 * No analogue on the reference side.
 */

//! The description of a function valued member, its parameters and its
//! return.
//!
//! A TypeScript property may hold a function, `org: () => Org` or
//! `orders(status?: string, limit?: number): Order[]`, and the checker then
//! answers its type with one call signature. The walk of a TypeScript build reads the
//! parameters of that signature as the arguments of the field and its return
//! as the type of the field. A Rust field holds no such type with a
//! description, because a closure and a trait object implement
//! [󰀂](super::Described) nowhere, so a named type that stands for a function
//! describes itself through [󰀃](super::TypeExpr::Callable) and the record
//! below. `Lazy` of the run time is the one such type of this crate, a
//! callable with no parameter.

use super::{
    info::ParamInfo,
    link::TypeLink, //
};

/// The description of a function valued member, the twin of a property whose
/// TypeScript type has one call signature.
///
/// The record is a promoted constant a type writes into its
/// [`Described::TYPE`](super::Described::TYPE), so every list is `'static`.
#[derive(Debug)]
pub struct CallableInfo {
    /// The parameters of the signature, in declaration order.
    pub params: &'static [ParamInfo],
    /// The return type of the signature.
    pub returns: TypeLink,
}
