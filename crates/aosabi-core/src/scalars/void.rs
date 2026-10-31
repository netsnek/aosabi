// SPDX-License-Identifier: MIT
/*
 * aosabi-core scalars void: the bound `Void` scalar.
 *
 * Binds `GraphQLVoid` of graphql-scalars as it stands.
 */

//! The bound `Void` scalar, the `GraphQLVoid` of graphql-scalars.

use aosabi_execute::execute::LeafSerializer;
use aosabi_scalars::void;
use aosabi_type::schema_bindings::ScalarBinding;

use crate::scalars::{
    hook_leaf_serializer,
    hooks_binding, //
};

/// The `name` of the scalar.
pub const NAME: &str = void::NAME;

/// Returns the binding of the scalar for the type layer.
///
/// `GraphQLVoid` states no `specifiedByURL`, so the binding clears the one
/// of the built scalar.
#[must_use]
pub fn binding() -> ScalarBinding {
    let hooks = (
        void::serialize as _,
        void::parse_value as _,
        void::parse_literal as _,
    );
    hooks_binding(void::DESCRIPTION, None, void::extensions(), hooks)
}

/// Returns the serializer over the value of the executor.
#[must_use]
pub fn leaf_serializer() -> LeafSerializer {
    hook_leaf_serializer(void::serialize)
}
