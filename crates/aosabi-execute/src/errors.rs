// SPDX-License-Identifier: MIT
/*
 * aosabi-execute errors: the GraphQLError constructor of the utilities and the located error.
 *
 * Derived from @graphql-tools/utils 11.0.0, cjs/errors.js, MIT.
 */

//! The error constructors of the executor.
//!
//! The reference passes one error object from the throw to the response and
//! shares it by reference. A [`GraphQLError`] of Aosabi is an owned value
//! with no `Clone`, because its original error is a boxed trait object, so
//! this module also holds the two functions that move an error out of a
//! thrown [`Value`] and copy it when the value is shared.

use core::fmt;
use std::rc::Rc;

use aosabi_json::{
    JsString,
    JsValue,
    Opaque, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    OriginalError,
    PathSegment,
    ast::{
        AstNode,
        FieldNode, //
    },
    js_string::to_string_lossy, //
};
use aosabi_type::DefinitionError;

use crate::{
    coerce_error::coerce_error,
    execute::FieldNodes,
    value::{
        ErrorObject,
        Value,
        to_error, //
    }, //
};

/// An error that is no `GraphQLError`, as the original error of a located error keeps it.
///
/// An [`ErrorObject`] belongs to one request and is `Rc` based, and the
/// original error of a [`GraphQLError`] is shared between threads, so the
/// located error keeps the two members a later layer reads, the `name` and
/// the `message`, and the host value of the error when it is an [`Opaque`]
/// handle, which another thread may hold. That host is how a layer that reads
/// the errors of a result finds the Rust value a resolver threw, as
/// `originalError instanceof` finds the instance in the reference. `Debug`
/// shows the name and the message as the JSON strings the writer of
/// `aosabi-json` makes of them, as the `Debug` of [`GraphQLError`] does, and
/// the host when there is one.
#[derive(Clone, PartialEq)]
pub struct ThrownError {
    /// The `name` of the error.
    pub name: JsString,
    /// The `message` of the error.
    pub message: JsString,
    /// The host value of the error, when the host slot [`ErrorObject::host`]
    /// reads holds an [`Opaque`] handle.
    pub host: Option<Opaque>,
}

/// Two hosts are equal when they are the same object, which is reflexive.
impl Eq for ThrownError {}

impl fmt::Display for ThrownError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&to_string_lossy(&self.message))
    }
}

impl fmt::Debug for ThrownError {
    /// Writes the name and the message, each as a JSON string, and the host
    /// when there is one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_struct("ThrownError");
        out.field("name", &JsValue::from(self.name.clone()))
            .field("message", &JsValue::from(self.message.clone()));
        if let Some(host) = &self.host {
            out.field("host", host);
        }
        out.finish()
    }
}

impl core::error::Error for ThrownError {}

/// Returns a `GraphQLError` with `message` and the blame of `options`.
///
/// The branch that rebuilds an original error which is an object and no
/// `Error` has no reading, because an [`OriginalError`] is an error by its
/// type, and the `coordinate` member is left out with
/// `schemaCoordinateInErrors`, which Yoga never asks for.
#[must_use]
pub fn create_graphql_error(
    message: impl Into<JsString>,
    options: GraphQLErrorOptions,
) -> GraphQLError {
    GraphQLError::new(message, options)
}

/// Returns the `GraphQLError` of `raw_error` that blames `nodes` and carries `path`.
///
/// A `GraphQLError` that already carries a path comes back as it is.
#[must_use]
pub fn located_error(raw_error: Value, nodes: &FieldNodes, path: Vec<PathSegment>) -> GraphQLError {
    let nodes = nodes
        .iter()
        .map(|node| AstNode::from(FieldNode::clone(node)))
        .collect();
    let error = match raw_error {
        Value::Error(error) => error,
        value => to_error(&value),
    };
    locate(unshare(error), Some(nodes), Some(path))
}

/// Returns the `GraphQLError` a thrown `value` is, for a position that pushes it unlocated.
///
/// A `GraphQLError` comes back as it is. Any other value is coerced and
/// wrapped without nodes and without a path, so the entry of the `errors`
/// array carries its message.
#[must_use]
pub fn into_graphql_error(value: Value) -> GraphQLError {
    match coerce_error(value) {
        Value::Error(error) => match unshare(error) {
            ErrorObject::GraphQL(error) => error,
            error => locate(error, None, None),
        },
        value => locate(unshare(to_error(&value)), None, None),
    }
}

/// Returns the value a throw of the type layer is.
///
/// The type layer reports a throw of the reference as a [`DefinitionError`],
/// a `GraphQLError` or a plain `Error` carried as its message. The executor
/// throws a [`Value`], so a `GraphQLError` stays one and a plain error
/// becomes `new Error(message)`.
#[must_use]
pub fn thrown_definition_error(error: DefinitionError) -> Value {
    match error {
        DefinitionError::GraphQL(error) => Value::from(ErrorObject::GraphQL(*error)),
        DefinitionError::Plain(message) => Value::from(ErrorObject::error(message)),
    }
}

/// Returns a copy of `error`, which shares the source and copies everything else.
///
/// An original error that is a [`ThrownError`] is copied with its shared
/// host, and any other original error that is no `GraphQLError` is copied as
/// a [`ThrownError`] of its message.
#[must_use]
pub fn copy_graphql_error(error: &GraphQLError) -> GraphQLError {
    GraphQLError {
        message: error.message.clone(),
        locations: error.locations.clone(),
        path: error.path.clone(),
        nodes: error.nodes.clone(),
        source: error.source.clone(),
        positions: error.positions.clone(),
        original_error: error.original_error.as_ref().map(copy_original_error),
        extensions: error.extensions.clone(),
        #[cfg(feature = "host-errors")]
        host_places: error.host_places.clone(),
    }
}

/// Calls [`aosabi_lang::located_error`] on `error` and keeps the exact message.
///
/// An error with own properties that holds an object at `extensions` hands it
/// to the located error, as the constructor of `GraphQLError` reads
/// `originalError.extensions` when it is an object.
fn locate(
    error: ErrorObject,
    nodes: Option<Vec<AstNode>>,
    path: Option<Vec<PathSegment>>,
) -> GraphQLError {
    let message = error.message();
    let original_extensions = match error.prop("extensions") {
        Some(JsValue::Object(extensions)) => Some(extensions.clone()),
        _ => None,
    };
    let original_error = match error {
        ErrorObject::GraphQL(error) => OriginalError::from(error),
        error => OriginalError::Other(Box::new(ThrownError {
            name: error.name(),
            message: message.clone(),
            host: error
                .host()
                .and_then(|host| host.downcast_ref::<Opaque>())
                .cloned(),
        })),
    };
    let is_graphql_error = matches!(original_error, OriginalError::GraphQL(_));
    let mut located = aosabi_lang::located_error(original_error, nodes, path);
    // The message of any other error travels through `Display`, which cannot
    // carry a lone surrogate, so the JavaScript string is put back.
    if !is_graphql_error {
        located.message = message;
    }
    if let Some(extensions) = original_extensions {
        located.extensions = extensions;
    }
    located
}

/// Returns the error behind `error`, moved out when nothing else holds it and copied otherwise.
fn unshare(error: Rc<ErrorObject>) -> ErrorObject {
    match Rc::try_unwrap(error) {
        Ok(error) => error,
        Err(shared) => match &*shared {
            ErrorObject::Plain { name, message } => ErrorObject::Plain {
                name: name.clone(),
                message: message.clone(),
            },
            ErrorObject::GraphQL(error) => ErrorObject::GraphQL(copy_graphql_error(error)),
            ErrorObject::Aggregate { message, errors } => ErrorObject::Aggregate {
                message: message.clone(),
                errors: errors.clone(),
            },
            ErrorObject::Custom {
                name,
                message,
                props,
                host,
            } => ErrorObject::Custom {
                name: name.clone(),
                message: message.clone(),
                props: Rc::clone(props),
                host: host.clone(),
            },
        },
    }
}

/// Returns a copy of `error`.
fn copy_original_error(error: &OriginalError) -> OriginalError {
    match error {
        OriginalError::GraphQL(error) => OriginalError::from(copy_graphql_error(error)),
        OriginalError::Other(error) => {
            let thrown = error.downcast_ref::<ThrownError>().cloned();
            OriginalError::Other(Box::new(thrown.unwrap_or_else(|| ThrownError {
                name: JsString::from("Error"),
                message: JsString::from(error.to_string()),
                host: None,
            })))
        }
    }
}
