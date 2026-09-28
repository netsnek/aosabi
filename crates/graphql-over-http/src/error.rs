// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the handling of an error that escapes the execution.
 *
 * The substatus of GraphTP beside the status has no analogue in the
 * reference, and docs/graphtp-in-aosabi.md section 4 gives its values.
 *
 * Derived from graphql-yoga 5.18.0, cjs/error.js, MIT.
 */

//! The error handling of Yoga.
//!
//! [`handle_error`] folds a thrown value into the `GraphQLError`s of a
//! result, and [`get_response_init_by_respecting_errors`] reads the status
//! and the headers of the answer off a result.
//!
//! Beside the status the same function decides the substatus of `GraphTP`,
//! which Yoga has no reading of, from the phase that made the result. The
//! notebook `tests/notebooks/32_graphtp_replay.ipynb` judges it.

use core::cell::RefCell;
use std::{
    borrow::Cow,
    rc::{
        Rc,
        Weak, //
    }, //
};

use aosabi_execute::{
    ErrorObject,
    ExecutionResult,
    Value,
    errors::into_graphql_error,
    value::to_js_string, //
};
use aosabi_hooks::{
    ExecutionOutcome,
    plugins::use_masked_errors::is_original, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    number_to_string, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions, //
};

use crate::{
    logger::HttpLogger,
    plugin::HttpResult,
    server_adapter::fetch_api::HeaderEntries,
    utils::mask_error::truthy, //
};

/// The mask of Yoga's options, `(error, message, isDev) => error`.
pub type HttpMaskErrorFn = Rc<dyn Fn(Value, &str, Option<bool>) -> Value>;

/// The masking options of the server, `maskedErrorsOpts`.
///
/// `mask_error` is the wrapper the server builds around the mask of its
/// options, which keeps a mask from running twice over its own output and
/// logs every error it replaces.
pub struct MaskedErrorsOpts {
    /// The message of a masked error, `Unexpected error.` unless the options name one.
    pub error_message: String,
    /// The mask of the options, `maskError`.
    pub mask: HttpMaskErrorFn,
    /// `isDev` of the options.
    pub is_dev: Option<bool>,
    /// The errors the wrapper made, `maskedErrorSet`, held weakly as a `WeakSet` holds them.
    masked: RefCell<Vec<Weak<ErrorObject>>>,
    /// The logger of the server.
    logger: HttpLogger,
}

impl MaskedErrorsOpts {
    /// Creates the options of `mask` with `error_message` and `is_dev`.
    #[must_use]
    pub fn new(
        error_message: String,
        mask: HttpMaskErrorFn,
        is_dev: Option<bool>,
        logger: HttpLogger,
    ) -> Self {
        Self {
            error_message,
            mask,
            is_dev,
            masked: RefCell::new(Vec::new()),
            logger,
        }
    }

    /// Masks `error` with `message` through the wrapper, `maskedErrorsOpts.maskError(error, message)`.
    ///
    /// An error the wrapper made comes back as it is, an error the mask
    /// replaces is logged, and the new error is remembered.
    #[must_use]
    pub fn mask_error(&self, error: Value, message: &str) -> Value {
        if let Value::Error(object) = &error
            && self
                .masked
                .borrow()
                .iter()
                .any(|masked| masked.as_ptr() == Rc::as_ptr(object))
        {
            return error;
        }
        let original = error.clone();
        let new_error = (self.mask)(error, message, self.is_dev);
        if !same_value(&new_error, &original) {
            self.logger.error_value(&original);
        }
        if let Value::Error(object) = &new_error {
            let mut masked = self.masked.borrow_mut();
            masked.retain(|masked| masked.strong_count() > 0);
            masked.push(Rc::downgrade(object));
        }
        new_error
    }
}

/// Returns whether `a` and `b` are the same value, `a === b` for the values a mask answers.
// Strict equality compares two numbers as doubles, NaN unequal to itself and
// the two zeros equal.
#[expect(
    clippy::float_cmp,
    reason = "strict equality of JavaScript compares doubles exactly"
)]
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Error(a), Value::Error(b)) => Rc::ptr_eq(a, b),
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        _ => false,
    }
}

/// Returns whether `value` is a `GraphQLError`, `isGraphQLError(val)`.
#[must_use]
pub fn is_graphql_error(value: &Value) -> bool {
    matches!(value, Value::Error(error) if matches!(error.as_ref(), ErrorObject::GraphQL(_)))
}

/// Returns whether `value` is an original `GraphQLError`, `isOriginalGraphQLError(val)`.
#[must_use]
pub fn is_original_graphql_error(value: &Value) -> bool {
    matches!(value, Value::Error(error) if matches!(error.as_ref(), ErrorObject::GraphQL(error) if is_original(error)))
}

/// Returns whether `error` is the `DOMException` of an abort or a timeout, `isAbortError(error)`.
///
/// No value of the model is a `DOMException`, because the app passes no
/// abort signal to Yoga, so every value answers `false`.
#[must_use]
pub fn is_abort_error(_error: &Value) -> bool {
    false
}

/// Returns the `GraphQLError` of `message` whose extensions mark an unexpected error.
fn unexpected_error(message: impl Into<aosabi_json::JsString>, with_code: bool) -> GraphQLError {
    let mut extensions = JsObject::new();
    if with_code {
        extensions.insert("code", JsValue::from("INTERNAL_SERVER_ERROR"));
    }
    extensions.insert("unexpected", JsValue::Bool(true));
    GraphQLError::new(
        message,
        GraphQLErrorOptions {
            extensions: Some(extensions),
            ..GraphQLErrorOptions::default()
        },
    )
}

/// Returns the `GraphQLError` a masked value is, `createGraphQLError(maskedError.message, { originalError: maskedError })` for one that is none.
fn graphql_error_of(value: Value) -> GraphQLError {
    match value {
        Value::Error(error) => into_graphql_error(Value::Error(error)),
        value => into_graphql_error(Value::from(ErrorObject::error(to_js_string(&value)))),
    }
}

/// Folds the thrown `error` into the errors of a result, `handleError(error, maskedErrorsOpts, logger)`.
///
/// An aggregate error gives one entry per inner error, an abort error none, a
/// masked error its mask, and any other value the error it is or a new one of
/// its text. The reference collects the entries in a `Set`, which drops a
/// second entry of the same error object, and an owned error of the model is
/// never the same object as another, so the list keeps every entry.
// Out of line, so a request compiles the handling of an error where one reaches the server.
#[must_use]
#[inline(never)]
pub fn handle_error(
    error: Value,
    masked_errors: Option<&MaskedErrorsOpts>,
    logger: &HttpLogger,
) -> Vec<GraphQLError> {
    let mut errors = Vec::new();
    if let Value::Error(object) = &error
        && let ErrorObject::Aggregate { errors: inner, .. } = object.as_ref()
    {
        for single in inner {
            errors.extend(handle_error(single.clone(), masked_errors, logger));
        }
    } else if is_abort_error(&error) {
        logger.debug("Request aborted");
    } else if let Some(masked_errors) = masked_errors {
        let original = error.clone();
        let masked = masked_errors.mask_error(error, &masked_errors.error_message);
        if !same_value(&masked, &original) {
            logger.error_value(&original);
        }
        errors.push(graphql_error_of(masked));
    } else if let Value::Error(object) = &error {
        match object.as_ref() {
            ErrorObject::GraphQL(_) => errors.push(into_graphql_error(error)),
            _ => errors.push(graphql_error_of(error)),
        }
    } else if let Value::String(text) = &error {
        errors.push(unexpected_error(text.clone(), true));
    } else if error.is_nullish() {
        logger.error_value(&error);
        errors.push(unexpected_error("Unexpected error.", false));
    } else {
        errors.push(unexpected_error(to_js_string(&error), true));
    }
    errors
}

/// The status and the headers of an answer, `{ status, headers }`, with the substatus of `GraphTP`.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseInitOfResult {
    /// The status.
    pub status: u16,
    /// The substatus of `GraphTP` that refines the status, 0 for none, which
    /// the reference has no reading of.
    pub substatus: u8,
    /// The headers, the entries of a plain object in the order of their keys.
    pub headers: HeaderEntries,
}

/// The substatus of a result with data and errors, 200.1 Partial.
const PARTIAL: u8 = 1;

/// The substatus of a result with null data and errors, 200.2 No Data.
const NO_DATA: u8 = 2;

/// The substatus of a document that does not parse, 400.1 Syntax Error.
const SYNTAX_ERROR: u8 = 1;

/// The substatus of a document that fails validation, 400.2 Validation Error.
const VALIDATION_ERROR: u8 = 2;

/// The substatus of variables that fail coercion, 400.3 Invalid Variables.
const INVALID_VARIABLES: u8 = 3;

/// The substatus of a request that selects no operation, 400.4 Operation Not Found.
const OPERATION_NOT_FOUND: u8 = 4;

/// The substatus of a content the request parser refuses, 400.6 Invalid Request Body.
const INVALID_REQUEST_BODY: u8 = 6;

/// Returns the status and the headers of the answer to `result`, `getResponseInitByRespectingErrors(result, headers, isApplicationJson)`.
///
/// A batch and a stream have neither `extensions` nor `errors` and answer 200.
/// The substatus comes from the error whose `http` extension gave the status
/// and from the data of the result, by the phases of
/// `docs/graphtp-in-aosabi.md` section 4, and a batch and a stream have none.
#[must_use]
pub fn get_response_init_by_respecting_errors(
    result: &HttpResult,
    headers: HeaderEntries,
    is_application_json: bool,
) -> ResponseInitOfResult {
    let mut headers = headers;
    let single = match result {
        HttpResult::Single(outcome) => outcome.single(),
        HttpResult::Batch(_) => None,
    };
    let Some(result) = single else {
        return ResponseInitOfResult {
            status: 200,
            substatus: 0,
            headers,
        };
    };
    let mut status: Option<f64> = None;
    let mut decided_by: Option<&GraphQLError> = None;
    let mut unexpected_error_exists = false;
    if let Some(http) = result
        .extensions
        .as_ref()
        .and_then(|extensions| extensions.get("http"))
        .filter(|http| truthy(http))
    {
        assign_headers(&mut headers, http);
        if let Some(value) = http_status(http) {
            status = Some(value);
        }
    }
    if result.errors.is_empty() {
        status = status.or(Some(200.0));
    } else {
        for error in &result.errors {
            if let Some(http) = error.extensions.get("http").filter(|http| truthy(http)) {
                assign_headers(&mut headers, http);
                if is_application_json && http_member(http, "spec").is_some_and(truthy) {
                    continue;
                }
                if let Some(value) = http_status(http)
                    && status.is_none_or(|status| value > status)
                {
                    status = Some(value);
                    decided_by = Some(error);
                }
            } else if !is_original(error) || error.extensions.get("unexpected").is_some_and(truthy)
            {
                unexpected_error_exists = true;
            }
        }
    }
    let status = status.unwrap_or(if unexpected_error_exists && !result.has_data() {
        500.0
    } else {
        200.0
    });
    let status = to_status(status);
    ResponseInitOfResult {
        status,
        substatus: substatus_of(status, decided_by, result),
        headers,
    }
}

/// Returns the substatus of `GraphTP` that the phase that made `result` gives `status`.
///
/// Each phase before execution answers a result with errors and no `data`,
/// and stamps the `http` extension of each error with 400, so the error that
/// gave the status names the phase by its `code`: `GRAPHQL_PARSE_FAILED` the
/// parse of the document, `GRAPHQL_VALIDATION_FAILED` validation,
/// `OPERATION_RESOLUTION_FAILURE` the choice of the operation and
/// `BAD_REQUEST` the request parser, the checks of the parameters and the
/// refusal of a batch. The coercion of the variables names no code, and the
/// executor stamps its errors with 400, the first of them with a message that
/// begins `Variable "$`, since only the error that ends the list at its bound
/// of 50 opens otherwise. Execution answers a result with `data`, which holds
/// data for 200.1 and is null for 200.2, read by the kind of the data with no
/// tree of it. Every other result has the substatus 0, so its code is the
/// status alone.
fn substatus_of(status: u16, decided_by: Option<&GraphQLError>, result: &ExecutionResult) -> u8 {
    match (status, result.has_data()) {
        (400, false) => decided_by.map_or(0, phase_substatus),
        (200, true) if !result.errors.is_empty() => {
            if result.data_is_null() {
                NO_DATA
            } else {
                PARTIAL
            }
        }
        _ => 0,
    }
}

/// Returns the substatus of the phase that stamped `error` with its status.
fn phase_substatus(error: &GraphQLError) -> u8 {
    match error.extensions.get("code") {
        Some(JsValue::String(code)) => match code.as_wtf8() {
            b"GRAPHQL_PARSE_FAILED" => SYNTAX_ERROR,
            b"GRAPHQL_VALIDATION_FAILED" => VALIDATION_ERROR,
            b"OPERATION_RESOLUTION_FAILURE" => OPERATION_NOT_FOUND,
            b"BAD_REQUEST" => INVALID_REQUEST_BODY,
            _ => 0,
        },
        None if error.message.as_wtf8().starts_with(b"Variable \"$") => INVALID_VARIABLES,
        _ => 0,
    }
}

/// Returns whether the errors are `GraphQLError`s, `areGraphQLErrors(obj)`.
///
/// Every error of a result of the model is one, so the test is that there is one.
#[must_use]
pub fn are_graphql_errors(errors: &[GraphQLError]) -> bool {
    !errors.is_empty()
}

/// Returns the result whose errors are the handled `errors`, `{ errors }`.
///
/// With the feature `host-results` a result of no error keeps its member
/// `errors`, so the result writes the empty array Yoga writes for a value
/// `handleError` makes no error of, the abort a hook of a plugin of an app of
/// the TypeScript option throws among them. No value a Rust service throws
/// folds into no error.
// Out of line, so a request compiles the result of errors where one reaches the server.
#[must_use]
#[inline(never)]
pub fn errors_result(errors: Vec<GraphQLError>) -> ExecutionOutcome {
    #[cfg(feature = "host-results")]
    if errors.is_empty() {
        let mut result = ExecutionResult::from_errors(errors);
        result.host_members = Some(vec![aosabi_execute::result::HostMember::Errors]);
        return ExecutionOutcome::Single(result);
    }
    ExecutionOutcome::Single(ExecutionResult::from_errors(errors))
}

/// Returns the member `key` of the extension `http` when it is an object.
fn http_member<'a>(http: &'a JsValue, key: &str) -> Option<&'a JsValue> {
    match http {
        JsValue::Object(http) => http.get(key),
        _ => None,
    }
}

/// Returns the truthy `status` of the extension `http` as a number.
fn http_status(http: &JsValue) -> Option<f64> {
    match http_member(http, "status") {
        Some(JsValue::Number(status)) if *status != 0.0 && !status.is_nan() => Some(*status),
        _ => None,
    }
}

/// Assigns the headers of the extension `http` onto `headers`, `Object.assign(headers, http.headers)`.
fn assign_headers(headers: &mut HeaderEntries, http: &JsValue) {
    let Some(JsValue::Object(assigned)) =
        http_member(http, "headers").filter(|value| truthy(value))
    else {
        return;
    };
    for (name, value) in assigned.iter() {
        let name = aosabi_lang::js_string::to_string_lossy(name).into_owned();
        let value = Cow::Owned(header_value(value));
        match headers.iter_mut().find(|(key, _)| *key == name) {
            Some((_, slot)) => *slot = value,
            None => headers.push((Cow::Owned(name), value)),
        }
    }
}

/// Returns the string a header value becomes, `String(value)`.
fn header_value(value: &JsValue) -> String {
    match value {
        JsValue::Undefined => String::from("undefined"),
        JsValue::Null => String::from("null"),
        JsValue::Bool(value) => value.to_string(),
        JsValue::Number(number) => number_to_string(*number),
        JsValue::String(text) => aosabi_lang::js_string::to_string_lossy(text).into_owned(),
        JsValue::Array(items) => items
            .iter()
            .map(|item| {
                if matches!(item, JsValue::Undefined | JsValue::Null) {
                    String::new()
                } else {
                    header_value(item)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        JsValue::Object(_) | JsValue::Opaque(_) => String::from("[object Object]"),
    }
}

/// Returns the status `value` as the status of a response.
///
/// A status of the extension is an integer between 100 and 599 on every
/// path of Yoga and of the app, and any other number is clamped into that
/// range.
fn to_status(value: f64) -> u16 {
    let value = value.clamp(100.0, 599.0);
    // The value is clamped into the range of a status, so the cast keeps it.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is clamped into 100 to 599 first"
    )]
    let status = value as u16;
    status
}
