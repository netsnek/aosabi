// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the processor of a single JSON result.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/result-processor/regular.js, MIT.
 */

//! The processor of a single JSON result.
//!
//! The answer is the JSON of the result without its internals with
//! `Content-Type: <accepted>; charset=utf-8`, and under `application/json`
//! an error of the specification, one whose `http` extension has `spec`,
//! does not raise the status when the result is no batch and some error has
//! no `originalError` extension. A stream answers 406. The response carries
//! the substatus of `GraphTP` decided beside the status.

use std::borrow::Cow;

use aosabi_execute::Value;
use aosabi_http::Response;

use crate::{
    error::{
        are_graphql_errors,
        get_response_init_by_respecting_errors, //
    },
    plugin::HttpResult,
    result_processor::stringify::json_stringify_result_without_internals,
    server_adapter::fetch_api::{
        ResponseBody,
        new_response, //
    },
    utils::mask_error::truthy, //
};

/// Answers `result` as JSON of the media type `accepted`, `processRegularResult(executionResult, fetchAPI, acceptedHeader)`.
///
/// # Errors
///
/// Returns the `TypeError` of a header the response refuses.
pub fn process_regular_result(result: &HttpResult, accepted: &str) -> Result<Response, Value> {
    let headers = vec![(Cow::Borrowed("Content-Type"), content_type(accepted))];
    // Prefer 200 only if accepting application/json and all errors are
    // exclusively GraphQL errors.
    let is_application_json = accepted == "application/json"
        && match result {
            HttpResult::Single(outcome) => outcome.single().is_some_and(|result| {
                are_graphql_errors(&result.errors)
                    && result
                        .errors
                        .iter()
                        .any(|error| !error.extensions.get("originalError").is_some_and(truthy))
            }),
            HttpResult::Batch(_) => false,
        };
    let init = get_response_init_by_respecting_errors(result, headers, is_application_json);
    let body = json_stringify_result_without_internals(result);
    let response = new_response(ResponseBody::Text(body), init.status, None, &init.headers)?;
    response.set_substatus(init.substatus);
    Ok(response)
}

/// Returns the `Content-Type` of an answer of the media type `accepted`, `${acceptedHeader}; charset=utf-8`.
///
/// The two media types the processor offers take their header as a constant.
fn content_type(accepted: &str) -> Cow<'static, str> {
    match accepted {
        "application/json" => Cow::Borrowed("application/json; charset=utf-8"),
        "application/graphql-response+json" => {
            Cow::Borrowed("application/graphql-response+json; charset=utf-8")
        }
        _ => Cow::Owned(format!("{accepted}; charset=utf-8")),
    }
}
