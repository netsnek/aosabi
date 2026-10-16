// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool batch: answers a file of request lines one response line per
 * request, in the protocol of the graphql-js oracle.
 *
 * Follows the answer function and the read loop of
 * tests/sidecar/graphql-js.mjs, whose protocol is tests/sidecar/README.md. No
 * analogue in the reference.
 */

//! The batch form of a primitive, which is the `--requests PATH` flag.
//!
//! A judging notebook sends a whole corpus through one process on each side
//! and compares the two response streams as bytes. A request is one JSON
//! object per line with an `id` and an `op`, and the response is one JSON
//! object per line with `id` first, `op` second and the payload of the
//! operation after them. A request that never reaches the crate under test
//! answers `fatal` in place of the payload. A blank line draws no response.
//!
//! Every response line ends in a newline, which is the one exception to the
//! rule that a primitive writes no trailing newline. The line is written by
//! aosabi-json, so the seam owns no JSON of its own.

use std::path::Path;

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    ParseOptions,
    Source, //
};

use crate::{
    json::reader,
    seam::{
        self,
        Answer,
        Failure, //
    }, //
};

/// One operation of the protocol.
///
/// The argument is the request object. The answer is the payload of the
/// response, or the text of the `fatal` that refuses the request. The text is
/// a JavaScript string, because the oracle writes the message of a syntax
/// error into it, and such a message may quote the body up to a leading
/// surrogate.
pub type Operation = fn(&JsObject) -> Result<JsObject, JsString>;

/// The operations one sub command answers, by the `op` that names each.
pub type Operations = [(&'static str, Operation)];

/// Answers every request line of the file `path` with `operations`.
///
/// # Errors
///
/// Returns [`Failure::NoInput`] when the file cannot be read. A line that
/// cannot be answered is never a failure of the call, it is a `fatal`
/// response line.
pub fn run(operations: &Operations, path: &Path) -> Result<Answer, Failure> {
    let bytes = seam::read_bytes(path)?;
    let mut out = Vec::new();
    for line in bytes.split(|&byte| byte == b'\n') {
        if line.trim_ascii().is_empty() {
            continue;
        }
        aosabi_json::write(&mut out, &JsValue::Object(answer(operations, line)));
        out.push(b'\n');
    }
    Ok(Answer::Payload(out))
}

/// Returns the response object for one request line.
///
/// The order of the refusals is the order of `answer` in the oracle: a line
/// that is not JSON, a line that is not an object, an unknown `op`, then a
/// missing `id`.
fn answer(operations: &Operations, line: &[u8]) -> JsObject {
    let request = match reader::parse(line) {
        Ok(JsValue::Object(request)) => request,
        Ok(_) => {
            return refuse(JsValue::Null, JsValue::Null, not_an_object());
        }
        Err(error) => {
            let fatal = format!("the request line is not JSON: {error}");
            return refuse(JsValue::Null, JsValue::Null, fatal);
        }
    };
    let id = request.get("id").cloned().unwrap_or(JsValue::Null);
    let Some(JsValue::String(op)) = request.get("op") else {
        return refuse(id, JsValue::Null, unknown_op(request.get("op")));
    };
    let operation = operations.iter().find(|(name, _)| *op == *name);
    let op = JsValue::String(op.clone());
    let Some((_, operation)) = operation else {
        let fatal = unknown_op(Some(&op));
        return refuse(id, op, fatal);
    };
    if !request.contains_key("id") {
        return refuse(id, op, "the request carries no id");
    }
    respond(id, op, operation(&request))
}

/// Returns the response that refuses a request with the text `fatal`.
fn refuse(id: JsValue, op: JsValue, fatal: impl Into<JsString>) -> JsObject {
    respond(id, op, Err(fatal.into()))
}

/// Returns the response with `id` first, `op` second and the payload after.
fn respond(id: JsValue, op: JsValue, outcome: Result<JsObject, JsString>) -> JsObject {
    let mut response = JsObject::new();
    response.insert("id", id);
    response.insert("op", op);
    match outcome {
        Ok(payload) => {
            for (key, value) in &payload {
                response.insert(key.clone(), value.clone());
            }
        }
        Err(fatal) => {
            response.insert("fatal", fatal);
        }
    }
    response
}

/// Returns the `fatal` of a request line that holds no object.
fn not_an_object() -> String {
    String::from("the request line is not a JSON object")
}

/// Returns the `fatal` of an `op` this sub command does not answer.
///
/// The oracle prints the `op` through `JSON.stringify`, which answers
/// `undefined` for a missing key, and a template literal prints that as the
/// word.
fn unknown_op(op: Option<&JsValue>) -> String {
    let printed = op
        .and_then(aosabi_json::to_string)
        .unwrap_or_else(|| String::from("undefined"));
    format!("unknown op {printed}")
}

/// Returns the name of the operation `request` asks for.
fn op_name(request: &JsObject) -> &str {
    match request.get("op") {
        Some(JsValue::String(op)) => op.as_str().unwrap_or("?"),
        _ => "?",
    }
}

/// Returns the string field `name` of `request`, as a JavaScript string.
///
/// # Errors
///
/// Returns the `fatal` of the oracle when the field is missing or is not a
/// string.
fn require_js_string<'a>(request: &'a JsObject, name: &str) -> Result<&'a JsString, String> {
    match request.get(name) {
        Some(JsValue::String(value)) => Ok(value),
        _ => Err(format!(
            "{}: the request carries no string field {name}",
            op_name(request)
        )),
    }
}

/// Returns the string field `name` of `request`, which names something.
///
/// This reads a field whose value a Rust string carries on the far side, such
/// as the name of a rule.
///
/// # Errors
///
/// Returns the `fatal` of the oracle when the field is missing or is not a
/// string. Returns a `fatal` of its own when the string carries a lone
/// surrogate, which a Rust string cannot hold.
pub fn require_string<'a>(request: &'a JsObject, name: &str) -> Result<&'a str, String> {
    require_js_string(request, name)?.as_str().ok_or_else(|| {
        let op = op_name(request);
        format!("{op}: the string field {name} carries a lone surrogate, which a Rust string cannot hold")
    })
}

/// Returns the source whose body is the string field `name` of `request`.
///
/// This reads a field that holds GraphQL text, a document or an SDL. The
/// oracle hands such a string to graphql-js as it arrives, a lone surrogate
/// included, and the lexer of the reference answers that code unit with a
/// syntax error. The source carries every code unit of the field, so the same
/// text reaches the lexer under test.
///
/// # Errors
///
/// Returns the `fatal` of the oracle when the field is missing or is not a
/// string.
pub fn require_source(request: &JsObject, name: &str) -> Result<Source, String> {
    require_js_string(request, name).map(|body| Source::new(body.clone()))
}

/// Returns the object that travels as `options` in `request`, when one does.
///
/// # Errors
///
/// Returns a `fatal` when `options` is neither absent, `null` nor an object.
pub fn options_of(request: &JsObject) -> Result<Option<&JsObject>, String> {
    match request.get("options") {
        None | Some(JsValue::Undefined | JsValue::Null) => Ok(None),
        Some(JsValue::Object(options)) => Ok(Some(options)),
        Some(_) => Err(format!("{}: options is not an object", op_name(request))),
    }
}

/// Returns the parser options of `request`, which travel as `options`.
///
/// The oracle hands `options` to `parse` verbatim. `noLocation` and
/// `allowLegacyFragmentVariables` take effect only when they are `true`,
/// because the reference compares them with `=== true`.
///
/// # Errors
///
/// Returns a `fatal` when `options` is not an object or `maxTokens` is not a
/// number.
pub fn parse_options(request: &JsObject) -> Result<ParseOptions, String> {
    let Some(options) = options_of(request)? else {
        return Ok(ParseOptions::default());
    };
    Ok(ParseOptions {
        no_location: matches!(options.get("noLocation"), Some(JsValue::Bool(true))),
        max_tokens: max_tokens(request, options.get("maxTokens"))?,
        allow_legacy_fragment_variables: matches!(
            options.get("allowLegacyFragmentVariables"),
            Some(JsValue::Bool(true))
        ),
    })
}

/// Returns the token budget `value` names.
///
/// The oracle hands the number to `parse` as it arrives, and the reference
/// compares it with the count of tokens and prints it into the message of the
/// abort, a fraction and a negative included. The budget therefore travels as
/// the double the request carries.
fn max_tokens(request: &JsObject, value: Option<&JsValue>) -> Result<Option<f64>, String> {
    match value {
        None | Some(JsValue::Undefined) => Ok(None),
        Some(JsValue::Number(budget)) => Ok(Some(*budget)),
        Some(_) => Err(format!(
            "{}: options.maxTokens is not a number",
            op_name(request)
        )),
    }
}
