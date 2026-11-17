// SPDX-License-Identifier: MIT
/*
 * aosabi-core embed: the GraphQL requests of a host that holds the engine in its own process.
 *
 * No analogue in the reference.
 */

//! The GraphQL requests of a host that holds the engine in its own process.
//!
//! A served binary reads a request from a socket, and a twin reads the lines
//! of [`crate::request_stream`]. A host that links the engine and the roots of
//! its app into its own program, such as the module of WebAssembly that a
//! Web Worker of a page runs, hands each request over as the body of a
//! GraphQL request over HTTP, a JSON object with `query` and optionally
//! `variables` and `operationName`, and takes the response body back. There
//! is no socket, no HTTP and no thread here.
//!
//! Every request runs on a fresh agent with virtual timers, which the host
//! never has to drive, and the agent runs until it is idle after its answer,
//! as [`crate::request_stream`] runs an `execute`. A body that is no request
//! answers a response with one error and no data, as a GraphQL server over
//! HTTP answers a request it cannot read.
//!
//! The caller hands over the key `maskedErrors` of its app, which decides an
//! unexpected error, one that is no original GraphQL error, as it decides it
//! over HTTP: an app that sets no key masks no error, so such an error answers
//! its own message, and the mask a key turns on answers in its place, as an
//! answer over HTTP writes it, with no extension `unexpected` or `http`, a
//! named divergence.
//!
//! The caller hands over the key `introspection` of its app as well, which
//! refuses introspection where it is `false`, as over HTTP: a document that
//! selects a field of an introspection type answers the errors of the
//! validation the HTTP road answers, each with the code
//! `GRAPHQL_VALIDATION_FAILED`, a named divergence. Every other document, and
//! every document of an app that sets no key, answers as it answers with
//! introspection on.

use std::rc::Rc;

use aosabi_execute::{
    ErrorObject,
    ExecutionResult,
    errors::into_graphql_error,
    kernel_entry::{
        Request,
        RunFailure, //
    },
    promise::{
        AgentOptions,
        TimerMode, //
    },
    value::Value, //
};
use aosabi_hooks::plugins::use_masked_errors::DEFAULT_ERROR_MESSAGE;
use aosabi_json::{
    JsObject,
    JsValue,
    reader, //
};
use aosabi_lang::{
    GraphQLError,
    js_string::to_string_lossy,
    parser::{
        ParseOptions,
        parse, //
    }, //
};
use aosabi_validate::{
    SPECIFIED_RULES,
    ValidateOptions,
    ValidationRule,
    validate, //
};
use graphql_over_http::{
    checks::stamp_http_error,
    error::HttpMaskErrorFn,
    plugins::disable_introspection::NO_SCHEMA_INTROSPECTION_CUSTOM_RULE,
    result_processor::stringify::omit_internals_from_error,
    utils::mask_error::mask_error, //
};

use crate::{
    plugin::config::MaskedErrors,
    random::{
        NO_RANDOMNESS,
        seed_keys, //
    },
    request_stream::STALLED,
    service::Service, //
};

/// Answers the body `body` of one GraphQL request with the bytes of its response body.
///
/// A response is the JSON text of the execution result, `data` and `errors`
/// as the executor writes them. A body that is not a JSON object, that holds
/// no string `query`, or whose `operationName` is no string, answers a
/// response with one error naming the fault, and so do a request whose
/// resolver throws a value that is no GraphQL error and a request that stalls.
/// On a thread that [`seed_keys`] leaves without a key, every body answers
/// the one error [`NO_RANDOMNESS`] and is never read.
///
/// `masked_errors` is the key `maskedErrors` of the app, and
/// `MaskedErrors::Off` for an app that sets none, as the HTTP road reads it.
/// Where the key turns the mask on, an error of the result and a value the
/// request throws that the mask replaces answer the mask in their place.
///
/// `introspection` is `false` where the key `introspection` of the app is
/// `false`, and then a document that selects a field of an introspection type
/// answers the errors the HTTP road answers it with.
#[must_use]
pub fn answer(
    service: &Service,
    body: &[u8],
    masked_errors: &MaskedErrors,
    introspection: bool,
) -> Vec<u8> {
    // The maps of the body hash under the key of this thread, so the thread
    // holds one before the body is read.
    if seed_keys().is_err() {
        return failure(NO_RANDOMNESS);
    }
    let request = match reader::parse(body) {
        Ok(JsValue::Object(request)) => request,
        Ok(_) => return failure("the request body is not a JSON object"),
        Err(error) => return failure(&format!("the request body is not JSON: {error}")),
    };
    let Some(JsValue::String(query)) = request.get("query") else {
        return failure("the request carries no string field query");
    };
    let operation_name = match request.get("operationName") {
        None | Some(JsValue::Null | JsValue::Undefined) => None,
        Some(JsValue::String(name)) => Some(to_string_lossy(name)),
        Some(_) => return failure("the field operationName is no string"),
    };
    let variables = match request.get("variables") {
        None | Some(JsValue::Null | JsValue::Undefined) => None,
        Some(variables) => Some(variables.clone()),
    };
    // A document text reaches the pipeline as a Rust string, so a lone
    // surrogate in it reads as the replacement character.
    let document = to_string_lossy(query);
    if !introspection && let Some(refused) = refuse_introspection(service, &document) {
        return refused;
    }
    let run = Request {
        document: &document,
        variables,
        operation_name: operation_name.as_deref(),
        validate: true,
    };
    let options = AgentOptions {
        timers: TimerMode::Virtual,
        trace: false,
    };
    let mask = Mask::of(masked_errors);
    match service.execute_drained(&run, options).outcome {
        Ok(mut result) => {
            if let Some(mask) = &mask {
                mask.errors(&mut result.errors);
            }
            let mut bytes = Vec::new();
            result.write_json(&mut bytes);
            bytes
        }
        Err(RunFailure::Thrown(thrown)) => match mask.and_then(|mask| mask.replace(&thrown)) {
            Some(masked) => {
                let mut bytes = Vec::new();
                ExecutionResult::from_errors(vec![masked]).write_json(&mut bytes);
                bytes
            }
            None => failure(&thrown_message(&thrown)),
        },
        Err(RunFailure::Stalled) => failure(STALLED),
    }
}

/// Returns the answer of the HTTP road with the key `introspection` off to
/// `document` where it selects a field of an introspection type, and `None`
/// for every other document, which runs as it runs with introspection on.
///
/// The HTTP road adds `NoSchemaIntrospectionCustomRule` after the specified
/// rules of the validation and stamps each error of a failed validation with
/// the code `GRAPHQL_VALIDATION_FAILED`, and the answer is written as an
/// answer over HTTP writes it, with no extension `http`. The parse takes the
/// cap on the tokens of the service, as every parse of a request document of
/// the app does, so a document past the cap of the app answers the syntax
/// error of the budget. A document that does not parse, or whose validation
/// throws, answers as it answers with introspection on.
fn refuse_introspection(service: &Service, document: &str) -> Option<Vec<u8>> {
    let document = parse(document, ParseOptions::request_capped(service.max_tokens())).ok()?;
    let errors = |rules: &[ValidationRule]| {
        validate(
            service.schema(),
            &document,
            Some(rules),
            ValidateOptions::default(),
            None,
        )
        .ok()
    };
    if errors(&[NO_SCHEMA_INTROSPECTION_CUSTOM_RULE])?.is_empty() {
        return None;
    }
    let mut rules = SPECIFIED_RULES.to_vec();
    rules.push(NO_SCHEMA_INTROSPECTION_CUSTOM_RULE);
    let refused = errors(&rules)?
        .into_iter()
        .map(|mut error| {
            stamp_http_error(&mut error, "GRAPHQL_VALIDATION_FAILED", true);
            omit_internals_from_error(&error)
        })
        .collect();
    let mut bytes = Vec::new();
    ExecutionResult::from_errors(refused).write_json(&mut bytes);
    Some(bytes)
}

/// The mask the key `maskedErrors` turns on, made of the key as the HTTP road
/// makes it.
struct Mask<'a> {
    /// The function `maskError` of the options, the default mask of
    /// `graphql-over-http` where they name none.
    function: Option<&'a HttpMaskErrorFn>,
    /// The message of a masked error, `Unexpected error.` unless the options
    /// name one.
    message: &'a str,
    /// `isDev` of the options, `false` where they leave it out.
    is_dev: Option<bool>,
}

impl<'a> Mask<'a> {
    /// Returns the mask `masked_errors` turns on, `None` for `MaskedErrors::Off`.
    fn of(masked_errors: &'a MaskedErrors) -> Option<Self> {
        match masked_errors {
            MaskedErrors::Off => None,
            MaskedErrors::On => Some(Self {
                function: None,
                message: DEFAULT_ERROR_MESSAGE,
                is_dev: None,
            }),
            MaskedErrors::Options(config) => Some(Self {
                function: config.mask_error.as_ref(),
                message: config
                    .error_message
                    .as_deref()
                    .filter(|message| !message.is_empty())
                    .unwrap_or(DEFAULT_ERROR_MESSAGE),
                is_dev: config.is_dev,
            }),
        }
    }

    /// Returns the mask of `error` as an answer over HTTP writes it, with no
    /// extension `http` or `unexpected`, and `None` where the mask keeps
    /// `error`.
    fn replace(&self, error: &Value) -> Option<GraphQLError> {
        let masked = match self.function {
            Some(mask) => mask(error.clone(), self.message, self.is_dev),
            None => mask_error(error.clone(), self.message, self.is_dev),
        };
        let kept = matches!((&masked, error),
            (Value::Error(masked), Value::Error(error)) if Rc::ptr_eq(masked, error));
        (!kept).then(|| omit_internals_from_error(&into_graphql_error(masked)))
    }

    /// Replaces each error of `errors` the mask replaces with its mask.
    fn errors(&self, errors: &mut Vec<GraphQLError>) {
        *errors = core::mem::take(errors)
            .into_iter()
            .map(|error| {
                let error = Value::from(ErrorObject::GraphQL(error));
                self.replace(&error)
                    .unwrap_or_else(|| into_graphql_error(error))
            })
            .collect();
    }
}

/// Returns the response body with the one error `message` and no data.
fn failure(message: &str) -> Vec<u8> {
    let mut error = JsObject::new();
    error.insert("message", JsValue::from(message));
    let mut response = JsObject::new();
    response.insert("errors", JsValue::Array(vec![JsValue::Object(error)]));
    let mut bytes = Vec::new();
    aosabi_json::write(&mut bytes, &JsValue::Object(response));
    bytes
}

/// Returns the message of a value the entry threw, its message for an error.
fn thrown_message(thrown: &Value) -> String {
    match thrown {
        Value::Error(error) => to_string_lossy(&error.message()).into_owned(),
        _ => String::from("the request threw a value that is no error"),
    }
}
