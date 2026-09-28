// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the stringifier of a result without its internals.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/result-processor/stringify.js, MIT.
 */

//! The stringifier of a result without its internals.
//!
//! The stringifier drops `extensions.http` of the result and `http` and
//! `unexpected` of every error, leaves out `extensions` when nothing is left,
//! and joins a batch with `,` inside `[...]`. A result with no error and no
//! `http` extension is written as it is, so its empty `extensions` stays
//! `{}`.
//!
//! The reference makes a new `GraphQLError` of every error, whose extensions
//! fall back to those of its original error when nothing is left of its own.
//! The original error of the model is a `GraphQLError`, whose own extensions
//! are stripped the same way, or an error that carries none, so the fallback
//! leaves `extensions` out here as it does there.
//!
//! The JSON is the bytes the writer of `aosabi-json` appends, which are UTF-8
//! by construction, and the string the reference answers is those bytes here.
//! A body is handed on as the writer's buffer, and a frame of a stream is
//! written into the buffer of its frame, so no pass checks the text of an
//! answer again and no copy of it is made before the connection copies it
//! into its buffer of writes.
//!
//! The data of a result of the executor is written from the arena of its
//! execution, and the errors and the extensions without their internals are
//! written around it, so the data is neither copied nor built as a tree on
//! the way to the body.

use aosabi_execute::{
    ExecutionResult,
    errors::copy_graphql_error, //
};
use aosabi_hooks::ExecutionOutcome;
use aosabi_json::JsObject;
use aosabi_lang::GraphQLError;

use crate::{
    plugin::HttpResult,
    utils::mask_error::truthy, //
};

/// Returns the JSON of `result` without its internals, `jsonStringifyResultWithoutInternals(result)`.
///
/// The result is the UTF-8 bytes of the string the reference answers.
#[must_use]
pub fn json_stringify_result_without_internals(result: &HttpResult) -> Vec<u8> {
    let mut out = Vec::new();
    write_result_without_internals(&mut out, result);
    out
}

/// Appends the JSON of `result` without its internals to `out`.
///
/// A batch is the JSON of each of its results joined with `,` inside `[...]`,
/// each written into `out` in turn, and an outcome that holds no single
/// result appends nothing.
pub fn write_result_without_internals(out: &mut Vec<u8>, result: &HttpResult) {
    match result {
        HttpResult::Single(outcome) => write_outcome(out, outcome),
        HttpResult::Batch(outcomes) => {
            out.push(b'[');
            for (position, outcome) in outcomes.iter().enumerate() {
                if position > 0 {
                    out.push(b',');
                }
                write_outcome(out, outcome);
            }
            out.push(b']');
        }
    }
}

/// Appends the JSON of one outcome, nothing for an outcome that holds no single result.
fn write_outcome(out: &mut Vec<u8>, outcome: &ExecutionOutcome) {
    if let Some(result) = outcome.single() {
        write_execution_result(out, result);
    }
}

/// Appends the JSON of one execution result without its internals to `out`.
///
/// The errors and the extensions are those
/// [`omit_internals_from_result_errors`] leaves, and the data is the result's
/// own, written where it stands.
pub fn write_execution_result(out: &mut Vec<u8>, result: &ExecutionResult) {
    match omitted_internals(result) {
        Some((errors, extensions)) => result.write_json_with(out, &errors, extensions.as_ref()),
        None => result.write_json(out),
    }
}

/// Returns the result with the internals of its errors and extensions left out, `omitInternalsFromResultErrors(result)`.
///
/// `None` is the result as it is, which the reference answers when the
/// result has no error and no `http` extension. The data of the new result
/// is a copy of the tree of the data. With the feature `host-results` the new
/// result keeps the members a host set in their order, as the spread
/// `{ ...result }` of the reference keeps them.
#[must_use]
pub fn omit_internals_from_result_errors(result: &ExecutionResult) -> Option<ExecutionResult> {
    let (errors, extensions) = omitted_internals(result)?;
    #[cfg(feature = "host-results")]
    {
        let mut sanitized = ExecutionResult::new(errors, result.data().cloned(), extensions);
        sanitized.host_members.clone_from(&result.host_members);
        Some(sanitized)
    }
    #[cfg(not(feature = "host-results"))]
    Some(ExecutionResult::new(
        errors,
        result.data().cloned(),
        extensions,
    ))
}

/// Returns the errors and the extensions of `result` without their internals.
///
/// `None` is a result with no error and no `http` extension, which the
/// reference answers as it is. Each error drops the extensions `http` and
/// `unexpected`, and the extensions drop `http` and are left out once
/// nothing is left of them.
fn omitted_internals(result: &ExecutionResult) -> Option<(Vec<GraphQLError>, Option<JsObject>)> {
    let has_http = result
        .extensions
        .as_ref()
        .and_then(|extensions| extensions.get("http"))
        .is_some_and(truthy);
    if result.errors.is_empty() && !has_http {
        return None;
    }
    let errors = result
        .errors
        .iter()
        .map(omit_internals_from_error)
        .collect();
    let extensions = result.extensions.as_ref().and_then(|extensions| {
        let mut extensions = extensions.clone();
        extensions.remove("http");
        (!extensions.is_empty()).then_some(extensions)
    });
    Some((errors, extensions))
}

/// Returns `error` without the extensions `http` and `unexpected`, `omitInternalsFromError(err)`.
#[must_use]
pub fn omit_internals_from_error(error: &GraphQLError) -> GraphQLError {
    let mut copy = copy_graphql_error(error);
    copy.extensions.remove("http");
    copy.extensions.remove("unexpected");
    copy
}
