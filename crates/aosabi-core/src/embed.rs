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

use aosabi_execute::{
    kernel_entry::{
        Request,
        RunFailure, //
    },
    promise::{
        AgentOptions,
        TimerMode, //
    },
    value::Value,
};
use aosabi_json::{
    JsObject,
    JsValue,
    reader, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::{
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
#[must_use]
pub fn answer(service: &Service, body: &[u8]) -> Vec<u8> {
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
    match service.execute_drained(&run, options).outcome {
        Ok(result) => {
            let mut bytes = Vec::new();
            result.write_json(&mut bytes);
            bytes
        }
        Err(RunFailure::Thrown(thrown)) => failure(&thrown_message(&thrown)),
        Err(RunFailure::Stalled) => failure(STALLED),
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
