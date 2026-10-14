// SPDX-License-Identifier: MIT
/*
 * aoviewer-core transform builtin: the recognition of the twelve scalars every
 * build prints and of the introspection types.
 *
 * Twelve scalars are printed in every build in one order.
 */

//! The built in types.
//!
//! A type is built in when its name starts with `__`, which the GraphQL
//! specification reserves for the introspection system, and when it is a
//! scalar the schema does not define for itself. The five scalars of the
//! specification, `Int`, `Float`, `String`, `Boolean` and `ID`, are built in
//! in every schema. The seven a build adds are built in only when the schema
//! declares all twelve as scalars, which every build does, so a `Date`
//! or a `JSON` scalar of another service keeps its own place in the index and
//! its specification URL.
//!
//! [specification]: https://spec.graphql.org/October2021/

use alloc::vec::Vec;

use crate::model::{
    Kind,
    Model,
    TypeId, //
};

/// The twelve scalars every build prints, in the order it prints them.
pub const SCALARS: [&str; 12] = [
    "String",
    "ID",
    "Int",
    "Float",
    "Boolean",
    "Number",
    "Any",
    "Void",
    "JSONObject",
    "File",
    "Date",
    "JSON",
];

/// The count of scalars at the head of [`SCALARS`] that the GraphQL
/// specification defines.
const SPECIFIED: usize = 5;

/// The prefix the specification reserves for the introspection types.
const INTROSPECTION: &str = "__";

/// Returns every built in type of `model`, sorted.
pub(super) fn builtin(model: &Model) -> Vec<TypeId> {
    let scalars = SCALARS.map(|name| {
        model
            .type_by_name(name)
            .filter(|&t| model.ty(t).kind == Kind::Scalar)
    });
    let all_twelve = scalars.iter().all(Option::is_some);
    let count = if all_twelve { scalars.len() } else { SPECIFIED };
    let mut out: Vec<TypeId> = scalars[..count].iter().flatten().copied().collect();
    out.extend(
        model
            .types()
            .iter()
            .copied()
            .filter(|&t| model.name(model.ty(t).name).starts_with(INTROSPECTION)),
    );
    out.sort_unstable();
    out
}
