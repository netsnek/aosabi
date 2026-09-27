// SPDX-License-Identifier: MIT
/*
 * aosabi-execute value error: an instance of Error as the executor reads it.
 *
 * The error classes are the ones of https://tc39.es/ecma262/#sec-error-objects
 * the executor and the scalar hooks throw.
 *
 * Derived from graphql-js 16.12.0, src/jsutils/toError.ts, MIT.
 */

//! The error value.
//!
//! The executor asks a thrown value whether it is an instance of `Error`,
//! reads its `message`, its `name` and, for an `AggregateError`, its
//! `errors`, and keeps a `GraphQLError` as it is. [`ErrorObject`] carries
//! what those reads need and nothing else, so the stack of the reference has
//! no reading here.
//!
//! An instance of a class that extends `Error` with properties of its own,
//! `ForbiddenError` of the auth contract with its `code`, `statusCode` and
//! `feature`, or `HTTPException` of hono with its `status` and `res`, is
//! [`ErrorObject::Custom`]. Its own enumerable properties are the object the
//! layers above read, and the Rust value behind it rides in its host slot, so
//! the layer that threw it gets it back whole.

use core::any::Any;
use std::rc::Rc;

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    js_string::to_string_lossy, //
};

use crate::value::{
    Value,
    inspect, //
};

/// The `name` of an error, which tells the classes apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorName {
    /// `Error`.
    Error,
    /// `TypeError`.
    TypeError,
    /// `RangeError`.
    RangeError,
    /// `AggregateError`.
    AggregateError,
    /// `NonErrorThrown`, the class [`to_error`] wraps a thrown value in.
    NonErrorThrown,
    /// Any other name, which an error coerced from an object keeps.
    Other(JsString),
}

impl ErrorName {
    /// Returns the name as the string the `name` property holds.
    #[must_use]
    pub fn to_js_string(&self) -> JsString {
        match self {
            Self::Error => JsString::from("Error"),
            Self::TypeError => JsString::from("TypeError"),
            Self::RangeError => JsString::from("RangeError"),
            Self::AggregateError => JsString::from("AggregateError"),
            Self::NonErrorThrown => JsString::from("NonErrorThrown"),
            Self::Other(name) => name.clone(),
        }
    }
}

/// An instance of `Error` or of a subclass of it.
pub enum ErrorObject {
    /// An error that carries a name and a message.
    Plain {
        /// The `name` property.
        name: ErrorName,
        /// The `message` property.
        message: JsString,
    },
    /// A `GraphQLError`, which is also what a `ServiceError` is.
    GraphQL(GraphQLError),
    /// An `AggregateError`.
    Aggregate {
        /// The `message` property.
        message: JsString,
        /// The `errors` property, in order.
        errors: Vec<Value>,
    },
    /// The error of a class that extends `Error` with properties of its own.
    ///
    /// `props` are the own enumerable properties in the order the instance
    /// holds them, which is what `JSON.stringify` and `Object.entries` read of
    /// it, and `host` is the Rust value the error stands for, which the layer
    /// that threw it downcasts. The `name` and the `message` of a class are
    /// no enumerable properties unless the class assigns them, and then
    /// `props` holds them too.
    Custom {
        /// The `name` property.
        name: ErrorName,
        /// The `message` property.
        message: JsString,
        /// The own enumerable properties, in order.
        props: Rc<JsObject>,
        /// The Rust value behind the error.
        host: Option<Rc<dyn Any>>,
    },
}

impl ErrorObject {
    /// Creates `new Error(message)`.
    pub fn error(message: impl Into<JsString>) -> Self {
        Self::Plain {
            name: ErrorName::Error,
            message: message.into(),
        }
    }

    /// Creates `new TypeError(message)`.
    pub fn type_error(message: impl Into<JsString>) -> Self {
        Self::Plain {
            name: ErrorName::TypeError,
            message: message.into(),
        }
    }

    /// Creates `new RangeError(message)`.
    pub fn range_error(message: impl Into<JsString>) -> Self {
        Self::Plain {
            name: ErrorName::RangeError,
            message: message.into(),
        }
    }

    /// Creates the error of a class with own properties.
    ///
    /// `props` are its own enumerable properties in order and `host` the Rust
    /// value behind it.
    pub fn custom(
        name: ErrorName,
        message: impl Into<JsString>,
        props: JsObject,
        host: Option<Rc<dyn Any>>,
    ) -> Self {
        Self::Custom {
            name,
            message: message.into(),
            props: Rc::new(props),
            host,
        }
    }

    /// Returns the `name` property.
    ///
    /// A `GraphQLError` sets its name in its constructor.
    #[must_use]
    pub fn name(&self) -> JsString {
        match self {
            Self::Plain { name, .. } | Self::Custom { name, .. } => name.to_js_string(),
            Self::GraphQL(_) => JsString::from("GraphQLError"),
            Self::Aggregate { .. } => ErrorName::AggregateError.to_js_string(),
        }
    }

    /// Returns the `message` property.
    #[must_use]
    pub fn message(&self) -> JsString {
        match self {
            Self::Plain { message, .. }
            | Self::Aggregate { message, .. }
            | Self::Custom { message, .. } => message.clone(),
            Self::GraphQL(error) => error.message.clone(),
        }
    }

    /// Returns the own property `key` of an error with own properties.
    ///
    /// This is `error[key]` for a key the class holds as an own enumerable
    /// property, and `None` for any other key and for every other error.
    #[must_use]
    pub fn prop(&self, key: &str) -> Option<&JsValue> {
        match self {
            Self::Custom { props, .. } => props.get(key),
            _ => None,
        }
    }

    /// Returns the Rust value behind an error with own properties, when it has one.
    #[must_use]
    pub fn host(&self) -> Option<&Rc<dyn Any>> {
        match self {
            Self::Custom { host, .. } => host.as_ref(),
            _ => None,
        }
    }

    /// Returns `error.toString()`.
    ///
    /// A `GraphQLError` overrides the method and answers its message followed
    /// by its locations. Every other error takes `Error.prototype.toString`,
    /// which answers the name when the message is empty, the message when the
    /// name is empty, and the two joined by a colon and a space otherwise.
    #[must_use]
    pub fn to_js_string(&self) -> String {
        if let Self::GraphQL(error) = self {
            return error.to_string();
        }
        error_prototype_to_string(&self.name(), &self.message())
    }
}

/// Returns `Error.prototype.toString` for an error with `name` and `message`.
///
/// Translation of steps 7 to 9 of the clause. A lone surrogate has no text and
/// prints as the replacement character.
pub(crate) fn error_prototype_to_string(name: &JsString, message: &JsString) -> String {
    if name.is_empty() {
        return to_string_lossy(message).into_owned();
    }
    if message.is_empty() {
        return to_string_lossy(name).into_owned();
    }
    format!("{}: {}", to_string_lossy(name), to_string_lossy(message))
}

/// Sometimes a non-error is thrown, wrap it as an Error instance to ensure a consistent Error interface.
///
/// `thrown_value` comes back as it is when it is an instance of `Error`. The
/// `thrownValue` property of the wrapper has no reading, because nothing in
/// the executor reads it.
///
/// Derived from graphql-js 16.12.0, src/jsutils/toError.ts, MIT.
#[must_use]
pub fn to_error(thrown_value: &Value) -> Rc<ErrorObject> {
    if let Value::Error(error) = thrown_value {
        return Rc::clone(error);
    }
    Rc::new(ErrorObject::Plain {
        name: ErrorName::NonErrorThrown,
        message: JsString::from(format!("Unexpected error value: {}", inspect(thrown_value))),
    })
}
