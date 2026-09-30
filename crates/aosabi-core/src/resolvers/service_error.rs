// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core resolvers service_error: the error an application throws on
 * purpose.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/define-pylon.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The service error.
//!
//! A `ServiceError` is a `GraphQLError` whose extensions are the three
//! members an application states, `code`, `statusCode` and `details`. The
//! executor keeps the extensions of an original error that is a
//! `GraphQLError`, so the three members reach the response.
//!
//! Two constructors make one. An application of the native layer writes
//! [`ServiceError::msg`] with the builder methods [`ServiceError::code`],
//! [`ServiceError::status`] and [`ServiceError::details`], so
//! `ServiceError::msg("Author not found").code("AUTHOR_NOT_FOUND").status(404)`
//! is
//! `new ServiceError('Author not found', {code: 'AUTHOR_NOT_FOUND', statusCode: 404})`.
//! The extensions of a builder keep the order `code`, `statusCode`, `details`
//! whatever the order of the calls, the order the object literal of the
//! reference writes them in.

use core::fmt;

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    OriginalError, //
};

/// The extensions of a [`ServiceError`], the second argument of its constructor.
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceErrorExtensions {
    /// The member `code`.
    pub code: String,
    /// The member `statusCode`, a JavaScript number.
    pub status_code: f64,
    /// The member `details`, left out of the extensions when it is `None`.
    pub details: Option<JsObject>,
}

/// An error a resolver throws with a code and a status of its own.
///
/// `Debug` shows the message and the extensions as the JSON the writer of
/// `aosabi-json` makes of them, the status code with the digits of
/// `aosabi_json::number::write_f64`, and the cause when there is one.
pub struct ServiceError {
    /// The message.
    pub message: JsString,
    /// The extensions.
    pub extensions: ServiceErrorExtensions,
    /// The error that caused this one, the third argument of the constructor.
    pub cause: Option<OriginalError>,
}

impl ServiceError {
    /// Creates `new ServiceError(message, extensions)`.
    pub fn new(message: impl Into<JsString>, extensions: ServiceErrorExtensions) -> Self {
        Self {
            message: message.into(),
            extensions,
            cause: None,
        }
    }

    /// Creates the error of the message `message` that the builder methods
    /// complete, the constructor of the native layer.
    ///
    /// The code is `INTERNAL_SERVER_ERROR` and the status 500 until
    /// [`ServiceError::code`] and [`ServiceError::status`] set them, the code
    /// Yoga gives an unexpected error and the status the error handler
    /// gives an error with no status of its own, since the TypeScript runtime
    /// has no `ServiceError` without both.
    pub fn msg(message: impl Into<JsString>) -> Self {
        Self::new(
            message,
            ServiceErrorExtensions {
                code: String::from("INTERNAL_SERVER_ERROR"),
                status_code: 500.0,
                details: None,
            },
        )
    }

    /// Returns the error with the member `code` of its extensions set to
    /// `code`.
    #[must_use]
    pub fn code(mut self, code: impl Into<String>) -> Self {
        self.extensions.code = code.into();
        self
    }

    /// Returns the error with the member `statusCode` of its extensions set
    /// to `status`, an HTTP status as a JavaScript number.
    #[must_use]
    pub fn status(mut self, status: u16) -> Self {
        self.extensions.status_code = f64::from(status);
        self
    }

    /// Returns the error with the member `details` of its extensions set to
    /// `details`, a JSON object, `Map::from([("id", id)])` for `details: {id}`.
    #[must_use]
    pub fn details(mut self, details: impl Into<JsObject>) -> Self {
        self.extensions.details = Some(details.into());
        self
    }

    /// Returns the error with `cause` as the third argument of the constructor.
    #[must_use]
    pub fn with_cause(mut self, cause: OriginalError) -> Self {
        self.cause = Some(cause);
        self
    }

    /// Returns the `GraphQLError` the error is.
    ///
    /// The constructor calls `super(message, {originalError})` and then
    /// assigns `this.extensions`, so the extensions an original error carries
    /// are replaced as a whole (source lines 500-503). The members are
    /// written in the order `code`, `statusCode`, `details`, and `details` is
    /// left out when it is `None`, because `JSON.stringify` skips a member
    /// that is `undefined`.
    #[must_use]
    pub fn into_graphql_error(self) -> GraphQLError {
        let options = GraphQLErrorOptions {
            original_error: self.cause,
            ..GraphQLErrorOptions::default()
        };
        let mut error = GraphQLError::new(self.message, options);
        error.extensions = self.extensions.into_js_object();
        error
    }
}

impl fmt::Debug for ServiceError {
    /// Writes the message, the extensions as the object the error carries,
    /// and the cause when there is one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut members = f.debug_struct("ServiceError");
        members.field("message", &JsValue::from(self.message.clone()));
        members.field(
            "extensions",
            &JsValue::Object(self.extensions.clone().into_js_object()),
        );
        if let Some(cause) = &self.cause {
            members.field("cause", cause);
        }
        members.finish()
    }
}

impl ServiceErrorExtensions {
    /// Returns the extensions as the object the error carries.
    fn into_js_object(self) -> JsObject {
        let mut extensions = JsObject::new();
        extensions.insert("code", JsValue::from(self.code));
        extensions.insert("statusCode", JsValue::from(self.status_code));
        if let Some(details) = self.details {
            extensions.insert("details", JsValue::Object(details));
        }
        extensions
    }
}
