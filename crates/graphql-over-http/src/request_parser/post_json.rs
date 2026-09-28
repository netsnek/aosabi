// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the parser of a JSON body.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/request-parser/post-json.js, MIT.
 */

//! The parser of a JSON body.
//!
//! A POST whose content type is `application/json` or
//! `application/graphql+json` is read with `request.json()`. A body that is no
//! JSON throws `POST body sent invalid JSON.` with `http.spec`, status 400,
//! code `BAD_REQUEST` and the `originalError` extension `{ name, message }` of
//! the platform's error, and a body that is `null` or no object throws
//! `POST body is expected to be object but received <type>` with status 400
//! and no `spec`. The second throw happens in the success step of the read,
//! which the failure step of the same `handleMaybePromise` does not catch, so
//! it keeps its own extensions. [`parse_json_content`] reads the parameters of
//! a content that has arrived, so a carrier with no request of HTTP reads them
//! with the same rules.

use std::rc::Rc;

use aosabi_execute::{
    ErrorObject,
    Value,
    errors::create_graphql_error, //
};
use aosabi_hooks::LocalBoxFuture;
use aosabi_http::{
    Request,
    fetch::body::utf8_decode, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::GraphQLErrorOptions;

use crate::{
    plugin::RequestParserResult,
    request_parser::{
        json_parse::json_parse,
        utils::is_content_type_match, //
    }, //
};

/// Returns whether `request` is a POST of JSON, `isPOSTJsonRequest(request)`.
#[must_use]
pub fn is_post_json_request(request: &Request) -> bool {
    request.method == "POST"
        && (is_content_type_match(request, "application/json")
            || is_content_type_match(request, "application/graphql+json"))
}

/// Returns the extension `http` with `status`, and `spec: true` first when `spec` holds.
fn http_extension(spec: bool, status: u16) -> JsValue {
    let mut http = JsObject::new();
    if spec {
        http.insert("spec", JsValue::Bool(true));
    }
    http.insert("status", JsValue::from(u32::from(status)));
    JsValue::Object(http)
}

/// Returns the refusal of a body that is no object.
fn object_expected(received: &str) -> Value {
    let mut extensions = JsObject::new();
    extensions.insert("http", http_extension(false, 400));
    extensions.insert("code", JsValue::from("BAD_REQUEST"));
    Value::from(ErrorObject::GraphQL(create_graphql_error(
        format!("POST body is expected to be object but received {received}"),
        GraphQLErrorOptions {
            extensions: Some(extensions),
            ..GraphQLErrorOptions::default()
        },
    )))
}

/// Returns the refusal of a body that is no JSON, whose read threw `name` with `message`.
fn invalid_json(name: &str, message: JsString) -> Value {
    let mut extensions = JsObject::new();
    extensions.insert("http", http_extension(true, 400));
    extensions.insert("code", JsValue::from("BAD_REQUEST"));
    let mut original = JsObject::new();
    original.insert("name", JsValue::from(name));
    original.insert("message", JsValue::String(message));
    extensions.insert("originalError", JsValue::Object(original));
    Value::from(ErrorObject::GraphQL(create_graphql_error(
        "POST body sent invalid JSON.",
        GraphQLErrorOptions {
            extensions: Some(extensions),
            ..GraphQLErrorOptions::default()
        },
    )))
}

/// Parses the parameters of a JSON body, `parsePOSTJsonRequest(request)`.
pub fn parse_post_json_request(
    request: Rc<Request>,
) -> LocalBoxFuture<'static, Result<RequestParserResult, Value>> {
    Box::pin(async move {
        match request.bytes().await {
            Ok(content) => parse_json_content(&content),
            Err(error) => Err(invalid_json("TypeError", JsString::from(error.to_string()))),
        }
    })
}

/// Parses the parameters of the JSON content `content`, the read of
/// `parsePOSTJsonRequest(request)` once its body has arrived.
///
/// The content is decoded as `request.text()` decodes a body, as UTF-8 with
/// the replacement character, and parsed and checked with the refusals of
/// the module.
///
/// # Errors
///
/// Returns the refusal of a content that is no JSON and of a value that is
/// no object.
pub fn parse_json_content(content: &[u8]) -> Result<RequestParserResult, Value> {
    let text = utf8_decode(content);
    let body = json_parse(&text).map_err(|error| invalid_json(error.name, error.message))?;
    match &body {
        JsValue::Undefined | JsValue::Null => return Err(object_expected("null")),
        JsValue::Bool(_) => return Err(object_expected("boolean")),
        JsValue::Number(_) => return Err(object_expected("number")),
        JsValue::String(_) => return Err(object_expected("string")),
        JsValue::Array(_) | JsValue::Object(_) | JsValue::Opaque(_) => {}
    }
    Ok(RequestParserResult::from_value(body, Vec::new()))
}
