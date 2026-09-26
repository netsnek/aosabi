// SPDX-License-Identifier: MIT
/*
 * aosabi-scalars: the error a scalar hook throws.
 *
 * The RangeError beside it is not from that file: it is the error V8 throws
 * from Date.prototype.toISOString, which the DateTimeISO serialize hook lets
 * through untouched.
 *
 * Derived from graphql-scalars 1.25.0, ., MIT.
 */

//! The errors of the scalar hooks.

use alloc::{boxed::Box, string::String, vec};
use core::fmt;

use aosabi_json::JsString;
use aosabi_lang::{AstNode, GraphQLError, GraphQLErrorOptions, OriginalError, ast::ValueNode};

/// Builds the `GraphQLError` a scalar throws.
///
/// The reference reads `versionInfo.major` and passes the six option fields
/// positionally on graphql below 17, which the pinned 16.12.0 is, and passes
/// the options object on
/// 17. Both branches build one error from `message` and the same fields, so
/// this translation has one path. An `options` of `None` is the `undefined`
/// the reference reads every field from through the optional chain, and the
/// constructor treats every field as absent.
pub fn create_graphql_error(
    message: impl Into<JsString>,
    options: Option<GraphQLErrorOptions>,
) -> GraphQLError {
    GraphQLError::new(message, options.unwrap_or_default())
}

/// Builds the `{ nodes: ast }` options literal of the hooks.
///
/// The hooks pass one node under `nodes`, and the constructor of
/// `GraphQLError` wraps a single node into an array. The node is cloned into
/// the error, because the error outlives the borrow of the literal.
#[must_use]
pub fn nodes_option(ast: &ValueNode) -> GraphQLErrorOptions {
    GraphQLErrorOptions {
        nodes: Some(vec![AstNode::from(ast.clone())]),
        ..GraphQLErrorOptions::default()
    }
}

/// The `RangeError` V8 throws from `Date.prototype.toISOString`.
///
/// ECMA-262 says only that a `RangeError` is thrown when the time value is
/// not finite. The message text is V8's own, `Invalid time value`, and
/// capture row C37 of docs/byte-equality.md pins it. The execution layer
/// copies `message` into the located error it reports, so the display form is
/// the message alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeError {
    message: String,
}

impl RangeError {
    /// Creates a `RangeError` carrying `message`.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for RangeError {}

/// What a scalar hook throws.
///
/// Every message of the four scalars is a `GraphQLError` from
/// [`create_graphql_error()`], and the one exception is the [`RangeError`] of
/// `toISOString`. The execution layer wraps either through `locatedError`,
/// which reads only `message` from an error that is not already located, so
/// the two variants differ in the original error they carry and in nothing on
/// the wire.
// The feature host-errors of aosabi-lang, which the native build of aosabi-js
// turns on, grows GraphQLError past the bound of the lint on a target of 64
// bits. The error moves out of the hook at once into the error of the
// execution or of the definition, so its width is paid on one frame, and a
// box would add an allocation to each throw. Without that feature the lint
// does not fire, so the attribute allows the lint instead of expecting it.
#[cfg_attr(
    target_pointer_width = "64",
    allow(
        clippy::large_enum_variant,
        reason = "the feature host-errors of aosabi-lang grows GraphQLError on a target of 64 bits"
    )
)]
#[derive(Debug)]
pub enum ScalarError {
    /// A `GraphQLError` built by `createGraphQLError`.
    GraphQL(GraphQLError),
    /// The `RangeError` of `Date.prototype.toISOString`.
    Range(RangeError),
}

impl From<GraphQLError> for ScalarError {
    fn from(error: GraphQLError) -> Self {
        Self::GraphQL(error)
    }
}

impl From<RangeError> for ScalarError {
    fn from(error: RangeError) -> Self {
        Self::Range(error)
    }
}

impl fmt::Display for ScalarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GraphQL(error) => fmt::Display::fmt(error, f),
            Self::Range(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl core::error::Error for ScalarError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::GraphQL(error) => Some(error),
            Self::Range(error) => Some(error),
        }
    }
}

impl From<ScalarError> for OriginalError {
    fn from(error: ScalarError) -> Self {
        match error {
            ScalarError::GraphQL(error) => Self::GraphQL(Box::new(error)),
            ScalarError::Range(error) => Self::Other(Box::new(error)),
        }
    }
}
