// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool graphtp substatus: the answer the processor of a single JSON
 * result of graphql-over-http makes of a result, with the substatus of GraphTP the
 * port decides beside the status.
 *
 * No analogue in the reference.
 */

//! The operation `substatus` of the primitive `graphtp`.
//!
//! The operation `substatus {result, batch, accepted}` builds the result of Yoga
//! that `result` describes, or the batch of the results that `batch` describes,
//! and answers what `process_regular_result` of `graphql-over-http` makes of it
//! under the media type `accepted`: the `status`, the `substatus`, the
//! `content_type` and the `body`. The media type is
//! `application/graphql-response+json` by default, the one the fixed `accept` of
//! the listener selects for every result that is no stream.
//!
//! A result is `{errors, data, extensions}` as the processor meets it, before
//! the stringifier drops the internals of its errors: each error is
//! `{message, locations, path, extensions}` with its `http` and `unexpected`
//! extensions, `data` is absent for a result with no `data` key, and
//! `extensions` is absent for none.

use core::{
    future::Future,
    pin::pin,
    task::{
        Context,
        Poll,
        Waker, //
    }, //
};

use aosabi_execute::ExecutionResult;
use aosabi_hooks::ExecutionOutcome;
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    PathSegment,
    SourceLocation, //
};
use graphql_over_http::{
    HttpResult,
    result_processor::regular::process_regular_result, //
};

/// The media type of the answer to every result that is no stream under the
/// fixed `accept` of the listener.
const ANSWER_TYPE: &str = "application/graphql-response+json";

/// Answers the operation `substatus`.
pub(super) fn op_substatus(request: &JsObject) -> Result<JsObject, JsString> {
    let result = match (request.get("result"), request.get("batch")) {
        (Some(result), None) => HttpResult::Single(outcome_of(result)?),
        (None, Some(JsValue::Array(results))) => HttpResult::Batch(
            results
                .iter()
                .map(outcome_of)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        _ => return Err(refusal("the request names either result or batch")),
    };
    let accepted = match request.get("accepted") {
        None => ANSWER_TYPE,
        Some(JsValue::String(accepted)) => accepted
            .as_str()
            .ok_or_else(|| refusal("the field accepted holds a lone surrogate"))?,
        Some(_) => return Err(refusal("the field accepted is no string")),
    };
    let response = process_regular_result(&result, accepted)
        .map_err(|error| refusal(&format!("the processor threw {error:?}")))?;
    let body = settled(response.text())
        .ok_or_else(|| refusal("the body did not settle in one poll"))?
        .map_err(|error| refusal(&format!("the body failed with {error:?}")))?;
    let mut payload = JsObject::with_capacity(4);
    payload.insert("status", JsValue::from(u32::from(response.status())));
    payload.insert("substatus", JsValue::from(u32::from(response.substatus())));
    payload.insert(
        "content_type",
        response
            .header("content-type")
            .map_or(JsValue::Null, JsValue::from),
    );
    payload.insert("body", JsValue::from(body));
    Ok(payload)
}

/// Returns the single result `value` describes.
fn outcome_of(value: &JsValue) -> Result<ExecutionOutcome, JsString> {
    let JsValue::Object(result) = value else {
        return Err(refusal("a result is no object"));
    };
    let errors = match result.get("errors") {
        None => Vec::new(),
        Some(JsValue::Array(errors)) => {
            errors.iter().map(error_of).collect::<Result<Vec<_>, _>>()?
        }
        Some(_) => return Err(refusal("the errors of a result are no array")),
    };
    let extensions = match result.get("extensions") {
        None => None,
        Some(JsValue::Object(extensions)) => Some(extensions.clone()),
        Some(_) => return Err(refusal("the extensions of a result are no object")),
    };
    Ok(ExecutionOutcome::Single(ExecutionResult::new(
        errors,
        result.get("data").cloned(),
        extensions,
    )))
}

/// Returns the `GraphQLError` `value` describes.
fn error_of(value: &JsValue) -> Result<GraphQLError, JsString> {
    let JsValue::Object(error) = value else {
        return Err(refusal("an error is no object"));
    };
    let Some(JsValue::String(message)) = error.get("message") else {
        return Err(refusal("an error has no message"));
    };
    let path = match error.get("path") {
        None => None,
        Some(JsValue::Array(segments)) => Some(
            segments
                .iter()
                .map(segment_of)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(_) => return Err(refusal("the path of an error is no array")),
    };
    let extensions = match error.get("extensions") {
        None => None,
        Some(JsValue::Object(extensions)) => Some(extensions.clone()),
        Some(_) => return Err(refusal("the extensions of an error are no object")),
    };
    let mut made = GraphQLError::new(
        message.clone(),
        GraphQLErrorOptions {
            path,
            extensions,
            ..GraphQLErrorOptions::default()
        },
    );
    made.locations = match error.get("locations") {
        None => None,
        Some(JsValue::Array(locations)) => Some(
            locations
                .iter()
                .map(location_of)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(_) => return Err(refusal("the locations of an error are no array")),
    };
    Ok(made)
}

/// Returns the segment of a path `value` describes, a key or an index.
fn segment_of(value: &JsValue) -> Result<PathSegment, JsString> {
    match value {
        JsValue::String(key) => key
            .as_str()
            .map(|key| PathSegment::Key(key.to_owned()))
            .ok_or_else(|| refusal("a key of a path holds a lone surrogate")),
        JsValue::Number(index) => whole(*index)
            .map(PathSegment::Index)
            .ok_or_else(|| refusal("an index of a path is no whole number")),
        _ => Err(refusal("a segment of a path is neither a key nor an index")),
    }
}

/// Returns the location `{line, column}` that `value` describes.
fn location_of(value: &JsValue) -> Result<SourceLocation, JsString> {
    let JsValue::Object(location) = value else {
        return Err(refusal("a location is no object"));
    };
    let line = match location.get("line") {
        Some(JsValue::Number(line)) => whole(*line),
        _ => None,
    };
    let column = match location.get("column") {
        Some(JsValue::Number(column)) => whole(*column),
        _ => None,
    };
    match (line, column) {
        (Some(line), Some(column)) => Ok(SourceLocation { line, column }),
        _ => Err(refusal("a location has no whole line and column")),
    }
}

/// Returns the double `value` as an index, or `None` when it is not a whole
/// number of 0 to 2^32.
fn whole(value: f64) -> Option<usize> {
    if !(value.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&value)) {
        return None;
    }
    // The cast is exact, because the value is a whole number of 0 to 2^32,
    // which a double carries without a gap.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = value as u32;
    usize::try_from(index).ok()
}

/// Returns the output of `future` when it settles in its first poll.
///
/// The processor writes a body of one chunk that is ready at once, so its
/// reading settles without a wake.
fn settled<F: Future>(future: F) -> Option<F::Output> {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(output) => Some(output),
        Poll::Pending => None,
    }
}

/// Returns the `fatal` of a request the operation cannot read.
fn refusal(reason: &str) -> JsString {
    JsString::from(format!("substatus: {reason}"))
}
