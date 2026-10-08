// SPDX-License-Identifier: MIT
/*
 * aosabi-auth-zitadel oidc/errors: the errors both halves throw.
 *
 * Derived from openid-client 6.8.2, ., MIT.
 * Derived from oauth4webapi 3.8.4, build/index.js, MIT.
 */

//! `ClientError`, `OperationProcessingError`, `ResponseBodyError`,
//! `AuthorizationResponseError` and `WWWAuthenticateChallengeError`.
//!
//! The functions of `oauth4webapi` throw a coded `TypeError`, an
//! [`OperationProcessingError`], an [`UnsupportedOperationError`] or one of
//! the three classes that carry an answer of the server, and a failed call of
//! the network rejects with the `TypeError` `fetch failed`. Every function
//! of this subset answers those as an [`OauthError`]. The functions of
//! `openid-client` run each failure through [`error_handler`], which maps an
//! [`OperationProcessingError`] and an [`UnsupportedOperationError`] to a
//! [`ClientError`] by its code and passes the rest on, and answer the thrown
//! value a route and the identity middleware log with its name and its
//! message.
//!
//! Each class converts into the thrown value as the instance of its class,
//! with the reference's name and its own enumerable properties in the order
//! its constructor makes them: the class fields first, then `name`, which
//! the constructor assigns. The Rust value rides in the host slot of the
//! error, so a layer of this crate reads it back whole. A property no layer
//! reads holds `{}` where the reference holds a host object, a response or
//! the parameters of a URL, and `undefined` where it holds the `SyntaxError`
//! of a body that does not parse.

use std::rc::Rc;

use aosabi_core::{
    Thrown,
    execute::value::{
        ErrorName,
        ErrorObject,
        Value, //
    },
    net::FetchError, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};

/// The code of a `TypeError` whose argument has the wrong type.
pub const ERR_INVALID_ARG_TYPE: &str = "ERR_INVALID_ARG_TYPE";
/// The code of a `TypeError` whose argument has the wrong value.
pub const ERR_INVALID_ARG_VALUE: &str = "ERR_INVALID_ARG_VALUE";
/// The code of a `WWW-Authenticate` challenge in an answer.
pub const WWW_AUTHENTICATE_CHALLENGE: &str = "OAUTH_WWW_AUTHENTICATE_CHALLENGE";
/// The code of an error in the body of an answer.
pub const RESPONSE_BODY_ERROR: &str = "OAUTH_RESPONSE_BODY_ERROR";
/// The code of an operation the subset does not support.
pub const UNSUPPORTED_OPERATION: &str = "OAUTH_UNSUPPORTED_OPERATION";
/// The code of an authorization response that is an error.
pub const AUTHORIZATION_RESPONSE_ERROR: &str = "OAUTH_AUTHORIZATION_RESPONSE_ERROR";
/// The code of a body that does not parse.
pub const PARSE_ERROR: &str = "OAUTH_PARSE_ERROR";
/// The code of an answer that breaks its specification.
pub const INVALID_RESPONSE: &str = "OAUTH_INVALID_RESPONSE";
/// The code of an answer whose content type is not the expected one.
pub const RESPONSE_IS_NOT_JSON: &str = "OAUTH_RESPONSE_IS_NOT_JSON";
/// The code of an answer whose status is not the expected one.
pub const RESPONSE_IS_NOT_CONFORM: &str = "OAUTH_RESPONSE_IS_NOT_CONFORM";
/// The code of a request to a URL that is not `https:`.
pub const HTTP_REQUEST_FORBIDDEN: &str = "OAUTH_HTTP_REQUEST_FORBIDDEN";
/// The code of a request to a URL that is neither `http:` nor `https:`.
pub const REQUEST_PROTOCOL_FORBIDDEN: &str = "OAUTH_REQUEST_PROTOCOL_FORBIDDEN";
/// The code of a timestamp claim of a JWT that fails its check.
pub const JWT_TIMESTAMP_CHECK: &str = "OAUTH_JWT_TIMESTAMP_CHECK_FAILED";
/// The code of a claim of a JWT that is not the expected one.
pub const JWT_CLAIM_COMPARISON: &str = "OAUTH_JWT_CLAIM_COMPARISON_FAILED";
/// The code of an attribute of a JSON answer that is not the expected one.
pub const JSON_ATTRIBUTE_COMPARISON: &str = "OAUTH_JSON_ATTRIBUTE_COMPARISON_FAILED";
/// The code of server metadata that lacks an endpoint.
pub const MISSING_SERVER_METADATA: &str = "OAUTH_MISSING_SERVER_METADATA";
/// The code of server metadata whose endpoint is no URL.
pub const INVALID_SERVER_METADATA: &str = "OAUTH_INVALID_SERVER_METADATA";
/// The code of an operation that ran past its timeout.
pub const OAUTH_TIMEOUT: &str = "OAUTH_TIMEOUT";

/// A `TypeError` with a `code`, which `CodedTypeError(message, code)` makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodedTypeError {
    /// The `message` property.
    pub message: String,
    /// The `code` property.
    pub code: &'static str,
}

impl CodedTypeError {
    /// Creates the error, `CodedTypeError(message, code)`.
    pub fn new(message: impl Into<String>, code: &'static str) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }
}

/// An operation the subset does not support, `UnsupportedOperationError`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedOperationError {
    /// The `message` property.
    pub message: String,
}

impl UnsupportedOperationError {
    /// Creates the error, `new UnsupportedOperationError(message)`.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// A failure of an operation of `oauth4webapi`, `OperationProcessingError`.
#[derive(Clone, Debug, PartialEq)]
pub struct OperationProcessingError {
    /// The `message` property.
    pub message: String,
    /// The `code` property, absent when the thrower names none.
    pub code: Option<&'static str>,
    /// The `cause` property, the value the thrower names.
    pub cause: JsValue,
}

impl OperationProcessingError {
    /// Creates the error, `OPE(message, code, cause)`.
    pub fn new(message: impl Into<String>, code: &'static str, cause: JsValue) -> Self {
        Self {
            message: message.into(),
            code: Some(code),
            cause,
        }
    }
}

/// An answer whose body holds an error, `ResponseBodyError`.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseBodyError {
    /// The `message` property.
    pub message: String,
    /// The `cause` property, the body of the answer.
    pub cause: JsObject,
    /// The `status` property, the status of the answer.
    pub status: u16,
}

impl ResponseBodyError {
    /// Creates the error over the body and the status of an answer.
    pub fn new(message: impl Into<String>, cause: JsObject, status: u16) -> Self {
        Self {
            message: message.into(),
            cause,
            status,
        }
    }
}

/// An authorization response that is an error, `AuthorizationResponseError`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizationResponseError {
    /// The `message` property.
    pub message: String,
    /// The `cause` property, the parameters of the response in order.
    pub cause: Vec<(String, String)>,
}

impl AuthorizationResponseError {
    /// Creates the error over the parameters of a response.
    pub fn new(message: impl Into<String>, cause: Vec<(String, String)>) -> Self {
        Self {
            message: message.into(),
            cause,
        }
    }

    /// Returns the first value of the parameter `name`, `cause.get(name)`.
    fn get(&self, name: &str) -> Option<&str> {
        self.cause
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// An answer that carries a `WWW-Authenticate` challenge, `WWWAuthenticateChallengeError`.
#[derive(Clone, Debug, PartialEq)]
pub struct WWWAuthenticateChallengeError {
    /// The `message` property.
    pub message: String,
    /// The `cause` property, the parsed challenges.
    pub cause: JsValue,
    /// The `status` property, the status of the answer.
    pub status: u16,
}

impl WWWAuthenticateChallengeError {
    /// Creates the error over the challenges and the status of an answer.
    pub fn new(message: impl Into<String>, cause: JsValue, status: u16) -> Self {
        Self {
            message: message.into(),
            cause,
            status,
        }
    }
}

/// The error class of `openid-client`, `ClientError`.
#[derive(Debug)]
pub struct ClientError {
    /// The `message` property.
    pub message: String,
    /// The `code` property, absent when the thrower names none.
    pub code: Option<&'static str>,
    /// The `cause` property, the error the class wraps, which is no enumerable property.
    pub cause: Option<Box<OauthError>>,
}

impl ClientError {
    /// Creates the error, `new ClientError(message, {cause, code})`.
    pub fn new(message: impl Into<String>, code: Option<&'static str>) -> Self {
        Self {
            message: message.into(),
            code,
            cause: None,
        }
    }

    /// Returns the error with `cause` as its cause.
    #[must_use]
    pub fn with_cause(mut self, cause: OauthError) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }
}

/// Every failure a function of the subset answers before `errorHandler` maps it.
#[derive(Debug)]
pub enum OauthError {
    /// A coded `TypeError`.
    Type(CodedTypeError),
    /// An `OperationProcessingError`.
    Operation(OperationProcessingError),
    /// An `UnsupportedOperationError`.
    Unsupported(UnsupportedOperationError),
    /// A `ResponseBodyError`.
    ResponseBody(ResponseBodyError),
    /// An `AuthorizationResponseError`.
    AuthorizationResponse(AuthorizationResponseError),
    /// A `WWWAuthenticateChallengeError`.
    Challenge(WWWAuthenticateChallengeError),
    /// A `ClientError` thrown as it is.
    Client(ClientError),
    /// A failed call of the network, the `TypeError` `fetch failed`.
    Fetch(FetchError),
    /// The `DOMException` `TimeoutError` of an operation whose signal ran out.
    TimedOut,
    /// The `DOMException` `OperationError` of a `WebCrypto` operation that failed.
    OperationFailed,
    /// Any other thrown value, which passes as it is.
    Thrown(Thrown),
}

impl From<CodedTypeError> for OauthError {
    fn from(error: CodedTypeError) -> Self {
        Self::Type(error)
    }
}

impl From<OperationProcessingError> for OauthError {
    fn from(error: OperationProcessingError) -> Self {
        Self::Operation(error)
    }
}

impl From<UnsupportedOperationError> for OauthError {
    fn from(error: UnsupportedOperationError) -> Self {
        Self::Unsupported(error)
    }
}

impl From<ResponseBodyError> for OauthError {
    fn from(error: ResponseBodyError) -> Self {
        Self::ResponseBody(error)
    }
}

impl From<AuthorizationResponseError> for OauthError {
    fn from(error: AuthorizationResponseError) -> Self {
        Self::AuthorizationResponse(error)
    }
}

impl From<WWWAuthenticateChallengeError> for OauthError {
    fn from(error: WWWAuthenticateChallengeError) -> Self {
        Self::Challenge(error)
    }
}

impl From<ClientError> for OauthError {
    fn from(error: ClientError) -> Self {
        Self::Client(error)
    }
}

impl From<FetchError> for OauthError {
    fn from(error: FetchError) -> Self {
        Self::Fetch(error)
    }
}

impl From<Thrown> for OauthError {
    fn from(error: Thrown) -> Self {
        Self::Thrown(error)
    }
}

/// The message of the `DOMException` `OperationError` of a failed `WebCrypto` operation.
const OPERATION_FAILED: &str = "The operation failed for an operation-specific reason";

/// Returns the message of `code` that `errorHandler` throws for an
/// `OperationProcessingError`, or `None` for a code it passes through.
fn client_message(code: Option<&str>) -> Option<&'static str> {
    Some(match code? {
        HTTP_REQUEST_FORBIDDEN => "only requests to HTTPS are allowed",
        REQUEST_PROTOCOL_FORBIDDEN => "only requests to HTTP or HTTPS are allowed",
        RESPONSE_IS_NOT_CONFORM => "unexpected HTTP response status code",
        RESPONSE_IS_NOT_JSON => "unexpected response content-type",
        PARSE_ERROR => "parsing error occured",
        INVALID_RESPONSE => "invalid response encountered",
        JWT_CLAIM_COMPARISON => "unexpected JWT claim value encountered",
        JSON_ATTRIBUTE_COMPARISON => "unexpected JSON attribute value encountered",
        JWT_TIMESTAMP_CHECK => "JWT timestamp claim value failed validation",
        _ => return None,
    })
}

/// Maps a failure to the value `openid-client` throws, `errorHandler(err)`.
///
/// A `TypeError`, a [`ClientError`] and the three classes that carry an
/// answer of the server pass as they are. An [`OperationProcessingError`]
/// becomes a [`ClientError`] whose message its code chooses and whose code is
/// its own, an [`UnsupportedOperationError`] becomes the [`ClientError`]
/// `unsupported operation`, and the timeout of a signal becomes the
/// [`ClientError`] `operation timed out` with the code `OAUTH_TIMEOUT`, and
/// the failure of a `WebCrypto` operation the [`ClientError`] `runtime
/// operation error`.
#[must_use]
pub fn error_handler(error: OauthError) -> Thrown {
    match error {
        OauthError::Operation(ope) => {
            let message =
                client_message(ope.code).map_or_else(|| ope.message.clone(), String::from);
            let code = ope.code;
            Thrown::from(ClientError::new(message, code).with_cause(OauthError::Operation(ope)))
        }
        OauthError::Unsupported(unsupported) => Thrown::from(
            ClientError::new("unsupported operation", Some(UNSUPPORTED_OPERATION))
                .with_cause(OauthError::Unsupported(unsupported)),
        ),
        OauthError::TimedOut => Thrown::from(
            ClientError::new("operation timed out", Some(OAUTH_TIMEOUT))
                .with_cause(OauthError::TimedOut),
        ),
        OauthError::OperationFailed => Thrown::from(
            ClientError::new("runtime operation error", Some(UNSUPPORTED_OPERATION))
                .with_cause(OauthError::OperationFailed),
        ),
        other => Thrown::from(other),
    }
}

/// Returns the instance of the class `name` with `props` and `host`.
fn instance(name: &str, message: &str, props: JsObject, host: Rc<dyn core::any::Any>) -> Thrown {
    let name = match name {
        "TypeError" => ErrorName::TypeError,
        "Error" => ErrorName::Error,
        other => ErrorName::Other(JsString::from(other)),
    };
    Thrown(Value::from(ErrorObject::custom(
        name,
        message,
        props,
        Some(host),
    )))
}

/// Returns `value` as a property value, `undefined` when it is absent.
fn optional(value: Option<&str>) -> JsValue {
    value.map_or(JsValue::Undefined, JsValue::from)
}

/// Returns the property `key` of a body as the class field reads it.
fn field(body: &JsObject, key: &str) -> JsValue {
    body.get(key).cloned().unwrap_or(JsValue::Undefined)
}

impl From<OauthError> for Thrown {
    fn from(error: OauthError) -> Self {
        match error {
            OauthError::Type(error) => Thrown::from(error),
            OauthError::Operation(error) => Thrown::from(error),
            OauthError::Unsupported(error) => Thrown::from(error),
            OauthError::ResponseBody(error) => Thrown::from(error),
            OauthError::AuthorizationResponse(error) => Thrown::from(error),
            OauthError::Challenge(error) => Thrown::from(error),
            OauthError::Client(error) => Thrown::from(error),
            OauthError::Fetch(error) => Thrown::from(error),
            OauthError::TimedOut => instance(
                "TimeoutError",
                "The operation was aborted due to timeout",
                JsObject::new(),
                Rc::new(()),
            ),
            OauthError::OperationFailed => instance(
                "OperationError",
                OPERATION_FAILED,
                JsObject::new(),
                Rc::new(()),
            ),
            OauthError::Thrown(thrown) => thrown,
        }
    }
}

impl From<CodedTypeError> for Thrown {
    fn from(error: CodedTypeError) -> Self {
        let mut props = JsObject::new();
        props.insert("code", error.code);
        let message = error.message.clone();
        instance("TypeError", &message, props, Rc::new(error))
    }
}

impl From<UnsupportedOperationError> for Thrown {
    fn from(error: UnsupportedOperationError) -> Self {
        let mut props = JsObject::new();
        props.insert("code", UNSUPPORTED_OPERATION);
        props.insert("name", "UnsupportedOperationError");
        let message = error.message.clone();
        instance("UnsupportedOperationError", &message, props, Rc::new(error))
    }
}

impl From<OperationProcessingError> for Thrown {
    fn from(error: OperationProcessingError) -> Self {
        let mut props = JsObject::new();
        props.insert("code", optional(error.code));
        props.insert("name", "OperationProcessingError");
        let message = error.message.clone();
        instance("OperationProcessingError", &message, props, Rc::new(error))
    }
}

impl From<ResponseBodyError> for Thrown {
    fn from(error: ResponseBodyError) -> Self {
        let mut props = JsObject::new();
        props.insert("cause", JsValue::Object(error.cause.clone()));
        props.insert("code", RESPONSE_BODY_ERROR);
        props.insert("error", field(&error.cause, "error"));
        props.insert("status", u32::from(error.status));
        props.insert(
            "error_description",
            field(&error.cause, "error_description"),
        );
        props.insert("name", "ResponseBodyError");
        let message = error.message.clone();
        instance("ResponseBodyError", &message, props, Rc::new(error))
    }
}

impl From<AuthorizationResponseError> for Thrown {
    fn from(error: AuthorizationResponseError) -> Self {
        let mut props = JsObject::new();
        props.insert("cause", JsObject::new());
        props.insert("code", AUTHORIZATION_RESPONSE_ERROR);
        props.insert(
            "error",
            error.get("error").map_or(JsValue::Null, JsValue::from),
        );
        props.insert(
            "error_description",
            optional(error.get("error_description")),
        );
        props.insert("name", "AuthorizationResponseError");
        let message = error.message.clone();
        instance(
            "AuthorizationResponseError",
            &message,
            props,
            Rc::new(error),
        )
    }
}

impl From<WWWAuthenticateChallengeError> for Thrown {
    fn from(error: WWWAuthenticateChallengeError) -> Self {
        let mut props = JsObject::new();
        props.insert("cause", error.cause.clone());
        props.insert("code", WWW_AUTHENTICATE_CHALLENGE);
        props.insert("status", u32::from(error.status));
        props.insert("name", "WWWAuthenticateChallengeError");
        let message = error.message.clone();
        instance(
            "WWWAuthenticateChallengeError",
            &message,
            props,
            Rc::new(error),
        )
    }
}

impl From<ClientError> for Thrown {
    fn from(error: ClientError) -> Self {
        let mut props = JsObject::new();
        props.insert("code", optional(error.code));
        props.insert("name", "ClientError");
        let message = error.message.clone();
        instance("ClientError", &message, props, Rc::new(error))
    }
}
