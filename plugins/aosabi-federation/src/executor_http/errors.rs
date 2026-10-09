// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the errors of a failed fetch and of an aborted request.
 *
 * Derived from @graphql-tools/executor-http 3.1.0, dist/index.cjs, MIT.
 * Derived from whatwg-node 0.8.5, cjs/fetchNodeHttp.js, MIT.
 */

//! The errors of the HTTP executor, `coerce_fetch_error` and
//! `create_graphql_error_for_abort`.
//!
//! A fetch that fails throws, and the executor answers a result whose one
//! error is that throw as a `GraphQLError`: a string becomes the error's
//! message, a `GraphQLError` stays itself, the `TypeError: fetch failed` of
//! the Fetch implementation becomes `fetch failed to <endpoint>`, an abort
//! becomes the error of its reason, and any other error keeps its message or
//! reads `Unknown error`, each with the upstream extensions of the request.
//! The network capability of `aosabi-core` throws that `TypeError` for every
//! failure below HTTP, so a `FetchError` of the capability is the original
//! error of `fetch failed to <endpoint>` (the plan, section DELEGATE, rule 2),
//! but for a refused connection, whose original error is the error of the
//! socket that [`fetch_rejection`] answers. That exception departs from rule 2
//! and from the rows `gw1-17` and `gw4-11` of the plan, which read
//! `fetch failed to <url>`. `createResultForAbort` of `:373-377` wraps the
//! abort error in a result and is left out with the executor that calls it,
//! which lands with the network half of the executor.
//!
//! The executor of the reference fetches through `@whatwg-node/node-fetch`,
//! which rejects with the error of the socket of Node where the connection
//! fails, `connect ECONNREFUSED <address>:<port>` for a refused one, and not
//! with the `TypeError` of the global fetch. [`fetch_rejection`] answers that
//! error for a refused connection, and the `TypeError` of the capability for
//! every other failure, so a reset connection or a name that does not resolve
//! reads `fetch failed to <endpoint>` here, where the request of Node rejects
//! with the error of its socket.

use core::net::SocketAddr;
use std::rc::Rc;

use aosabi_core::{
    Thrown,
    net::FetchError, //
};

use aosabi_execute::{
    errors::ThrownError,
    value::{
        ErrorObject,
        Value,
        to_js_string, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    Opaque, //
};
use aosabi_lang::{
    GraphQLErrorOptions,
    OriginalError, //
};

use crate::utils::{
    create_graphql_error,
    is_truthy, //
};

/// Returns the value the fetch of the executor rejects with for the failure `error`.
///
/// Rejects with the error of the socket. A refused connection to an address
/// reads `connect ECONNREFUSED <address>:<port>`, the address written as Node
/// writes it with no brackets, and every other failure is the `TypeError`
/// `fetch failed` of the network capability, whose message
/// [`coerce_fetch_error`] reads.
#[must_use]
pub fn fetch_rejection(error: &FetchError) -> Value {
    if let FetchError::Connect(reason) = error
        && let Some((address, cause)) = reason.split_once(": ")
        && cause.starts_with("Connection refused")
        && let Ok(address) = address.parse::<SocketAddr>()
    {
        return Value::from(ErrorObject::error(format!(
            "connect ECONNREFUSED {}:{}",
            address.ip(),
            address.port()
        )));
    }
    Thrown::from(error.clone()).0
}

/// Returns the `GraphQLError` the result of a failed fetch carries.
///
/// `e` is the thrown value, `signal_reason` the reason of the request's abort
/// signal, `endpoint` the URL of the remote, and `upstream_error_extensions`
/// the extensions every error of the request carries.
#[must_use]
pub fn coerce_fetch_error(
    e: &Value,
    signal_reason: Option<&Value>,
    endpoint: &str,
    upstream_error_extensions: Option<&JsObject>,
) -> Value {
    if let Value::String(message) = e {
        return graphql_error(message.clone(), upstream_error_extensions, None);
    }
    let name = member_text(e, "name");
    let message = member(e, "message");
    if name.as_deref() == Some("GraphQLError") {
        return e.clone();
    }
    if name.as_deref() == Some("TypeError")
        && matches!(&message, Value::String(text) if text.as_str() == Some("fetch failed"))
    {
        return graphql_error(
            JsString::from(format!("fetch failed to {endpoint}")),
            upstream_error_extensions,
            Some(original_of(e)),
        );
    }
    if name.as_deref() == Some("AbortError") {
        let reason = signal_reason
            .filter(|reason| is_truthy(reason))
            .unwrap_or(e);
        return create_graphql_error_for_abort(reason, upstream_error_extensions);
    }
    if is_truthy(&message) {
        return graphql_error(
            to_js_string(&message),
            upstream_error_extensions,
            Some(original_of(e)),
        );
    }
    graphql_error(
        JsString::from("Unknown error"),
        upstream_error_extensions,
        Some(original_of(e)),
    )
}

/// Returns the `GraphQLError` of an aborted request.
///
/// A `GraphQLError` stays itself, a `TimeoutError` gets the extensions
/// `{http: {status: 504, ...http}, code: 'TIMEOUT_ERROR', ...extensions}`, and
/// any other reason keeps its message and the extensions.
#[must_use]
pub fn create_graphql_error_for_abort(reason: &Value, extensions: Option<&JsObject>) -> Value {
    if matches!(reason, Value::Error(error) if matches!(error.as_ref(), ErrorObject::GraphQL(_))) {
        return reason.clone();
    }
    let message = to_js_string(&member(reason, "message"));
    if member_text(reason, "name").as_deref() == Some("TimeoutError") {
        let mut http = JsObject::new();
        http.insert("status", JsValue::Number(504.0));
        if let Some(JsValue::Object(more)) =
            extensions.and_then(|extensions| extensions.get("http"))
        {
            for (key, value) in more.iter() {
                http.insert(key.clone(), value.clone());
            }
        }
        let mut timeout_extensions = JsObject::new();
        timeout_extensions.insert("http", JsValue::Object(http));
        timeout_extensions.insert("code", "TIMEOUT_ERROR");
        for (key, value) in extensions.into_iter().flat_map(JsObject::iter) {
            timeout_extensions.insert(key.clone(), value.clone());
        }
        return graphql_error(
            message,
            Some(&timeout_extensions),
            Some(original_of(reason)),
        );
    }
    graphql_error(message, extensions, Some(original_of(reason)))
}

/// Returns `createGraphQLError(message, {extensions, originalError})` as a thrown value.
fn graphql_error(
    message: JsString,
    extensions: Option<&JsObject>,
    original_error: Option<OriginalError>,
) -> Value {
    Value::from(ErrorObject::GraphQL(create_graphql_error(
        message,
        GraphQLErrorOptions {
            extensions: extensions.cloned(),
            original_error,
            ..GraphQLErrorOptions::default()
        },
    )))
}

/// Returns the original error a `GraphQLError` keeps of a thrown value.
///
/// The name and the message travel, and the host of an error, the
/// `FetchError` of the network capability among them, rides along as the
/// handle it is.
fn original_of(e: &Value) -> OriginalError {
    let host = match e {
        Value::Error(error) => error
            .host()
            .and_then(|host| host.downcast_ref::<Opaque>())
            .cloned(),
        _ => None,
    };
    OriginalError::Other(Box::new(ThrownError {
        name: member_text(e, "name").map_or_else(|| JsString::from("Error"), JsString::from),
        message: to_js_string(&member(e, "message")),
        host,
    }))
}

/// Returns the member `key` of a thrown value, `e[key]`, an error's name and message among them.
fn member(e: &Value, key: &str) -> Value {
    match e {
        Value::Error(error) => match key {
            "name" => Value::String(error.name()),
            "message" => Value::String(error.message()),
            _ => error.prop(key).map_or(Value::Undefined, Value::from_js),
        },
        Value::Object(object) => Rc::clone(object).get(key),
        _ => Value::Undefined,
    }
}

/// Returns the member `key` of a thrown value as text, when it is a string.
fn member_text(e: &Value, key: &str) -> Option<String> {
    match member(e, key) {
        Value::String(text) => Some(String::from_utf8_lossy(text.as_wtf8()).into_owned()),
        _ => None,
    }
}
