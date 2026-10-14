// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the JSON-RPC lines of the protocol server.
 *
 * Derived from @modelcontextprotocol/sdk 1.29.0, dist/cjs/shared/stdio.js, MIT.
 * Derived from @modelcontextprotocol/sdk 1.29.0, dist/cjs/server/stdio.js, MIT.
 */

//! The JSON-RPC lines.
//!
//! The SDK frames one JSON message per line: it reads up to a line feed,
//! decodes the line as UTF-8, drops one trailing carriage return, runs
//! `JSON.parse` and checks the value against the union of a request, a
//! notification, a result and an error response, and writes every message it
//! sends as `JSON.stringify(message)` and a line feed. A line that does not
//! parse or does not pass the check is consumed and handed to `onerror`, and
//! nothing is written. The request and the notification are strict objects,
//! an `id` is a string or an integer, and `params` is an object when it is
//! present.
//!
//! [`read_line`] and [`message`] are that framing and that check, and
//! [`write()`] writes one message. The case `mcp-bad-line` pins it.
//!
//! A result goes out as `{result, jsonrpc, id}` and an error as
//! `{jsonrpc, id, error: {code, message}}`, the key orders of the SDK.

use std::io::{
    self,
    BufRead,
    Write, //
};

use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::cargo;

/// The version of JSON-RPC every message names.
pub const JSONRPC_VERSION: &str = "2.0";

/// The code of a method no handler answers.
pub const METHOD_NOT_FOUND: i32 = -32601;

/// The code of a handler that failed with no code of its own.
pub const INTERNAL_ERROR: i32 = -32603;

/// The largest integer `Number.isSafeInteger` accepts, the bound of an
/// integer `id`.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// One message that passed the check of the SDK.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// A request, which takes an answer.
    Request {
        /// The id the answer echoes.
        id: JsValue,
        /// The method.
        method: String,
        /// The parameters, an empty object when the request carries none.
        params: JsObject,
    },
    /// A notification, which takes none.
    Notification,
    /// A result or an error response, which the server sends no request for
    /// and so consumes.
    Response,
}

/// Reads the next line of `input` with its line feed, `Ok(None)` at the end
/// of the input.
///
/// A last line with no line feed is no message, as the buffer of the SDK
/// keeps it until a line feed that never comes, so it reads as the end.
///
/// # Errors
///
/// Returns the error of the read.
pub fn read_line(input: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    input.read_until(b'\n', &mut bytes)?;
    if bytes.last() != Some(&b'\n') {
        return Ok(None);
    }
    bytes.pop();
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Returns the message `line` holds, or `None` for a line that does not
/// parse as JSON or does not pass the check of the SDK.
#[must_use]
pub fn message(line: &str) -> Option<Message> {
    let value = aosabi_json::parse(line.as_bytes()).ok()?;
    let JsValue::Object(object) = &value else {
        return None;
    };
    if cargo::text(&value, "jsonrpc") != Some(JSONRPC_VERSION) {
        return None;
    }
    let keys: Vec<&str> = object.iter().filter_map(|(key, _)| key.as_str()).collect();
    if keys.len() != object.len() {
        return None;
    }
    let only = |allowed: &[&str]| keys.iter().all(|key| allowed.contains(key));
    let method = cargo::text(&value, "method");
    let id = object.get("id");
    match (method, id) {
        (Some(method), Some(id)) if only(&["jsonrpc", "id", "method", "params"]) => {
            let params = params_of(object.get("params"))?;
            is_id(id).then(|| Message::Request {
                id: id.clone(),
                method: method.to_owned(),
                params,
            })
        }
        (Some(_), None) if only(&["jsonrpc", "method", "params"]) => {
            params_of(object.get("params")).map(|_| Message::Notification)
        }
        _ if only(&["jsonrpc", "id", "result"]) && object.contains_key("result") => {
            let result_ok = matches!(object.get("result"), Some(JsValue::Object(_)));
            (result_ok && id.is_some_and(is_id)).then_some(Message::Response)
        }
        _ if only(&["jsonrpc", "id", "error"]) && is_error(object.get("error")) => {
            id.is_none_or(is_id).then_some(Message::Response)
        }
        _ => None,
    }
}

/// Returns the answer of the request `id` with `result`, `{result, jsonrpc,
/// id}`.
#[must_use]
pub fn result(id: &JsValue, result: JsValue) -> JsValue {
    let mut object = JsObject::with_capacity(3);
    object.insert("result", result);
    object.insert("jsonrpc", JSONRPC_VERSION);
    object.insert("id", id.clone());
    JsValue::Object(object)
}

/// Returns the error answer of the request `id`, `{jsonrpc, id, error:
/// {code, message}}`.
#[must_use]
pub fn error(id: &JsValue, code: i32, message: &str) -> JsValue {
    let mut inner = JsObject::with_capacity(2);
    inner.insert("code", code);
    inner.insert("message", message);
    let mut object = JsObject::with_capacity(3);
    object.insert("jsonrpc", JSONRPC_VERSION);
    object.insert("id", id.clone());
    object.insert("error", JsValue::Object(inner));
    JsValue::Object(object)
}

/// Writes `message` to `out` as `JSON.stringify` writes it and a line feed,
/// and flushes.
///
/// # Errors
///
/// Returns the error of the write.
pub fn write(out: &mut impl Write, message: &JsValue) -> io::Result<()> {
    let mut line = aosabi_json::to_bytes(message).unwrap_or_default();
    line.push(b'\n');
    out.write_all(&line)?;
    out.flush()
}

/// Returns whether `id` is a string or a safe integer.
fn is_id(id: &JsValue) -> bool {
    match id {
        JsValue::String(_) => true,
        JsValue::Number(number) => number.fract() == 0.0 && number.abs() <= MAX_SAFE_INTEGER,
        _ => false,
    }
}

/// Returns the parameters `params`, an empty object when they are absent,
/// and `None` when they are present and no object, or carry a `_meta` that
/// is no object.
fn params_of(params: Option<&JsValue>) -> Option<JsObject> {
    match params {
        None => Some(JsObject::new()),
        Some(JsValue::Object(object)) => match object.get("_meta") {
            None | Some(JsValue::Object(_)) => Some(object.clone()),
            Some(_) => None,
        },
        Some(_) => None,
    }
}

/// Returns whether `error` is the error object of a response, an integer
/// `code` and a string `message`.
fn is_error(error: Option<&JsValue>) -> bool {
    let Some(error @ JsValue::Object(_)) = error else {
        return false;
    };
    let code = matches!(
        cargo::field(error, "code"),
        Some(JsValue::Number(code)) if code.fract() == 0.0 && code.abs() <= MAX_SAFE_INTEGER
    );
    code && matches!(cargo::field(error, "message"), Some(JsValue::String(_)))
}
