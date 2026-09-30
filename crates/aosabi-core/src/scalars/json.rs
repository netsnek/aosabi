// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core scalars json: the bound `JSON` scalar.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/pylon-handler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * Binds `JSONResolver` of graphql-scalars as it stands.
 */

//! The bound `JSON` scalar, the `JSONResolver` of graphql-scalars.

use aosabi_execute::execute::LeafSerializer;
use aosabi_scalars::json;
use aosabi_type::schema_bindings::ScalarBinding;

use crate::scalars::{
    hook_leaf_serializer,
    hooks_binding, //
};

/// The `name` of the scalar.
pub const NAME: &str = json::NAME;

/// Returns the binding of the scalar for the type layer.
#[must_use]
pub fn binding() -> ScalarBinding {
    let hooks = (
        json::serialize as _,
        json::parse_value as _,
        json::parse_literal as _,
    );
    hooks_binding(
        json::DESCRIPTION,
        Some(json::SPECIFIED_BY_URL),
        json::extensions(),
        hooks,
    )
}

/// Returns the serializer over the value of the executor.
#[must_use]
pub fn leaf_serializer() -> LeafSerializer {
    hook_leaf_serializer(json::serialize)
}
