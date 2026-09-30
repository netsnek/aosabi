// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core scalars date: the bound `Date` scalar.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/pylon-handler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The bound `Date` scalar.
//!
//! The handler builds a new scalar around the three hooks of `DateTimeISO` of
//! graphql-scalars. The new scalar has the hooks, a description of its own
//! and nothing else, so it carries no `specifiedByURL` and no extensions of
//! the scalar the hooks come from.

use aosabi_execute::execute::LeafSerializer;
use aosabi_json::JsObject;
use aosabi_scalars::date_time_iso;
use aosabi_type::schema_bindings::ScalarBinding;

use crate::scalars::{
    hook_leaf_serializer,
    hooks_binding, //
};

/// The `name` of the scalar, source line 280.
pub const NAME: &str = "Date";

/// The `description` of the scalar, source line 281.
pub const DESCRIPTION: &str = "Date represented as an ISO-8601 string";

/// Returns the binding of the scalar for the type layer.
///
/// A parsed date is carried on as its `toISOString` text, because a hook of
/// the type layer answers a JSON value, and the glue of a resolver method
/// reads the text back into a date.
#[must_use]
pub fn binding() -> ScalarBinding {
    let hooks = (
        date_time_iso::serialize as _,
        date_time_iso::parse_value as _,
        date_time_iso::parse_literal as _,
    );
    hooks_binding(DESCRIPTION, None, JsObject::new(), hooks)
}

/// Returns the serializer over the value of the executor, which sees a
/// `Date` instance as a date and not as its time value.
#[must_use]
pub fn leaf_serializer() -> LeafSerializer {
    hook_leaf_serializer(date_time_iso::serialize)
}
