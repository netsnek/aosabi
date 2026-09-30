// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue thrown: what a resolver throws.
 *
 * No analogue in the reference, where `throw` takes any value.
 */

//! The thrown value of a resolver.
//!
//! A JavaScript `throw` takes any value, an error or not. A Rust resolver
//! returns `Result<T, Thrown>` for the same freedom: [`Thrown`] wraps the
//! value that is thrown, and its constructors are the throw statements a
//! fixture writes, `throw new Error(message)`, `throw new TypeError(message)`
//! and `throw value`.
//!
//! `Err(error)?` is the throw of an error type: `?` converts the error into
//! the thrown value. A value of the executor converts as it is, so the
//! rejection of an awaited promise is rethrown with `?`. A `ServiceError` and
//! every type with `From` into `ServiceError` convert, which is how a
//! subclass of `ServiceError` of an application reads, so
//! `Err(AuthRequiredError::new())?` is `throw new AuthRequiredError()`. A
//! `GraphQLError` converts as the error it is. The blanket conversion stands
//! beside the one of `GraphQLError`, because `ServiceError` is a type of this
//! crate, so no other crate can write `From<GraphQLError>` for it, and this
//! crate writes none.
//!
//! A `ForbiddenError` converts as the instance of its class, named
//! `ForbiddenError` with the own properties `feature`, `code`, `statusCode`
//! and `name` in the order the TypeScript build gives them, the parameter
//! property `feature` before the fields, which is what the route error
//! handler reads and what `JSON.stringify` and the inspect of graphql-js
//! show, the message and the feature as the error holds them. The error
//! itself rides in the host slot as an [`Opaque`] handle, which the located
//! error of the executor keeps, so [`original_host`] finds it among the
//! errors of a result as `err.originalError instanceof ForbiddenError` finds
//! it in the reference, and a class of an application that carries the same
//! name is not found. An `HTTPException` converts as the instance of its
//! class with the exception in the host slot, so the route error handler
//! answers its response. A `CookieError` of the cookie helpers converts as
//! `new Error(message)`, the error hono throws.
//!
//! # The error of the native layer
//!
//! [`Error`] is the error a resolver of the native layer answers, the type
//! the `Result<T, E = Error>` of the prelude of the facade defaults to. It
//! has the form of the error of `anyhow`: it implements `Display` and `Debug`
//! and not `std::error::Error`, so every type that implements
//! `std::error::Error` converts into it through one blanket `From`, and `?`
//! throws any error of the standard library or of a crate. Such an error
//! converts as `new Error(message)` with the text its `Display` writes, the
//! unexpected error the masking of Yoga hides in production, and the errors
//! of the engine that are `std::error::Error` keep the conversion of
//! [`Thrown`]: a `GraphQLError`, a `ForbiddenError`, an `HTTPException`, a
//! `CookieError` and a `FetchError` convert as the instance of their class
//! above. A `ServiceError` converts as itself. An own error type of an
//! application converts into `Error` by the `From<Own> for Error` it writes
//! for its own type, which the blanket `From` leaves free for a type that is
//! no `std::error::Error`, or it is thrown as `ServiceError::from(own)`
//! through the `From<Own> for ServiceError` it writes, the form a class of an
//! application that extends `ServiceError` takes. A type with a `From` into
//! `ServiceError` alone throws through `?` on the derive path, whose
//! [`Thrown`] converts every such type, and not into `Error`, since a blanket
//! `From` over `Into<ServiceError>` cannot stand beside the one over
//! `std::error::Error`. [`Error::msg`] is `new Error(message)`.

use core::{
    any::Any,
    fmt, //
};
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_auth_contract::ForbiddenError;
use aosabi_execute::{
    errors::ThrownError,
    value::{
        ErrorName,
        ErrorObject,
        Value, //
    }, //
};
use aosabi_http::{
    HttpException,
    utils::cookie::CookieError, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    Opaque, //
};
use aosabi_lang::{
    GraphQLError,
    OriginalError,
    js_string::to_string_lossy, //
};

use crate::{
    glue::{
        completion::IntoThrown,
        into_value::IntoValue, //
    },
    net::FetchError,
    resolvers::ServiceError,
};

/// A value a resolver throws.
#[derive(Clone, Debug)]
pub struct Thrown(pub Value);

impl Thrown {
    /// Returns `new Error(message)` as a thrown value.
    #[must_use]
    pub fn error(message: &str) -> Self {
        Self(Value::from(ErrorObject::error(message)))
    }

    /// Returns `new Error(message)` as a thrown value for a message that is
    /// any JavaScript string.
    ///
    /// A JavaScript string may hold a lone surrogate among its code units,
    /// which the `&str` of [`Thrown::error`] cannot hold, so
    /// `new Error('\uD842...')` of a fixture is this constructor over
    /// [`JsString::from_utf16`].
    #[must_use]
    pub fn error_js(message: JsString) -> Self {
        Self(Value::from(ErrorObject::error(message)))
    }

    /// Returns `new TypeError(message)` as a thrown value.
    #[must_use]
    pub fn type_error(message: &str) -> Self {
        Self(Value::from(ErrorObject::type_error(message)))
    }

    /// Returns `value` as a thrown value, which is `throw value` for a value
    /// that is no error.
    pub fn value(value: impl IntoValue) -> Self {
        Self(value.into_value())
    }
}

/// The rejection of an awaited promise, which is a value of the executor, as the thrown value.
///
/// This is `throw value` for a value the executor holds already, so
/// `sleep(ms).await?` inside an async resolver or the executor of a
/// repeater rethrows the rejection of the timer as the reference does.
impl From<Value> for Thrown {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

impl<E: Into<ServiceError>> From<E> for Thrown {
    fn from(error: E) -> Self {
        Self::from(error.into().into_graphql_error())
    }
}

impl From<GraphQLError> for Thrown {
    fn from(error: GraphQLError) -> Self {
        Self(Value::from(ErrorObject::GraphQL(error)))
    }
}

impl From<ForbiddenError> for Thrown {
    fn from(error: ForbiddenError) -> Self {
        let mut props = JsObject::new();
        props.insert(
            "feature",
            error.feature.clone().map_or(JsValue::Undefined, Into::into),
        );
        props.insert("code", ForbiddenError::CODE);
        props.insert("statusCode", ForbiddenError::STATUS_CODE);
        props.insert("name", ForbiddenError::NAME);
        let name = ErrorName::Other(ForbiddenError::NAME.into());
        let message = error.message.clone();
        let host: Rc<dyn Any> = Rc::new(Opaque(Arc::new(error)));
        Self(Value::from(ErrorObject::custom(
            name,
            message,
            props,
            Some(host),
        )))
    }
}

impl From<CookieError> for Thrown {
    fn from(error: CookieError) -> Self {
        Self::error(error.message())
    }
}

impl From<HttpException> for Thrown {
    fn from(exception: HttpException) -> Self {
        let mut props = JsObject::new();
        props.insert("res", JsValue::Undefined);
        props.insert("status", f64::from(exception.status));
        let message = exception.message.clone();
        Self(Value::from(ErrorObject::custom(
            ErrorName::Error,
            message.as_str(),
            props,
            Some(Rc::new(exception)),
        )))
    }
}

/// The error a resolver of the native layer answers, `aosabi::Error`.
///
/// The value is the thrown value of a JavaScript `throw`, as in [`Thrown`],
/// and the conversions are those of the module documentation, section The
/// error of the native layer. `Display` writes the message of an error, and
/// of any other thrown value the text `String(value)` gives, and `Debug`
/// writes the value as the executor shows it.
pub struct Error(Value);

impl Error {
    /// Returns `new Error(message)`, the unexpected error the mask hides, with
    /// the text `Display` writes of `message`.
    ///
    /// It is the twin of `throw new Error(message)` and of `Thrown::error`,
    /// and of `anyhow::Error::msg` in a Rust crate.
    pub fn msg(message: impl fmt::Display) -> Self {
        Self(Value::from(ErrorObject::error(message.to_string())))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match &self.0 {
            Value::Error(error) => error.message(),
            value => aosabi_execute::value::to_js_string(value),
        };
        f.write_str(&to_string_lossy(&text))
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Error").field(&self.0).finish()
    }
}

/// Any error of the standard library or of a crate, the unexpected error
/// `new Error(message)`, and an error of the engine as the instance of its
/// class.
///
/// The type of `error` is known where the conversion is monomorphised, so
/// each test of a class of the engine below is a comparison of two constant
/// type ids that the compiler folds.
impl<E: core::error::Error + 'static> From<E> for Error {
    fn from(error: E) -> Self {
        let mut held = Some(error);
        let slot: &mut dyn Any = &mut held;
        let engine = taken::<GraphQLError>(slot)
            .map(Thrown::from)
            .or_else(|| taken::<ForbiddenError>(slot).map(Thrown::from))
            .or_else(|| taken::<HttpException>(slot).map(Thrown::from))
            .or_else(|| taken::<CookieError>(slot).map(Thrown::from))
            .or_else(|| taken::<FetchError>(slot).map(Thrown::from));
        match (engine, held) {
            (Some(thrown), _) => Self(thrown.0),
            (None, Some(error)) => Self::msg(error),
            // A class of the engine takes the error out of `held` and answers
            // it, so `held` is empty only beside an answer.
            (None, None) => Self(Value::Undefined),
        }
    }
}

/// Takes the error out of `slot` when the slot holds an `Option<T>`.
fn taken<T: 'static>(slot: &mut dyn Any) -> Option<T> {
    slot.downcast_mut::<Option<T>>().and_then(Option::take)
}

/// A `ServiceError`, the error an application throws on purpose, as itself.
///
/// An own error type with a `From` into `ServiceError` is thrown as
/// `ServiceError::from(own)`, or converts by its own `From<Own> for Error`,
/// as the module documentation says.
impl From<ServiceError> for Error {
    fn from(error: ServiceError) -> Self {
        Self(Thrown::from(error).0)
    }
}

/// The thrown value of the forms of the derive path, as it is.
impl From<Thrown> for Error {
    fn from(thrown: Thrown) -> Self {
        Self(thrown.0)
    }
}

/// The error of the native layer as the thrown value of the forms of the
/// derive path, so `?` throws it in a resolver that answers
/// `aosabi::mirror::Result<T>`.
impl From<Error> for Thrown {
    fn from(error: Error) -> Self {
        Self(error.0)
    }
}

impl IntoThrown for Error {
    fn into_thrown(self) -> Value {
        self.0
    }
}

/// Returns the host value of type `T` the original error of `error` carries.
///
/// The original error of a located error keeps the host of the thrown error
/// when the host slot held an [`Opaque`] handle, which is how a
/// `ForbiddenError` is thrown, so `original_host::<ForbiddenError>(error)` is
/// `error.originalError instanceof ForbiddenError` with the instance in hand.
/// An error whose original error is a `GraphQLError`, an error of another
/// class and an error with no original error answer `None`.
#[must_use]
pub fn original_host<T: Any>(error: &GraphQLError) -> Option<&T> {
    let Some(OriginalError::Other(original)) = &error.original_error else {
        return None;
    };
    original
        .downcast_ref::<ThrownError>()?
        .host
        .as_ref()?
        .downcast_ref::<T>()
}
