// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the helpers of the request parsers.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/request-parser/utils.js, MIT.
 */

//! The helpers of the request parsers.
//!
//! [`handle_url_search_params`] reads `operationName`, `query`, `variables`
//! and `extensions` off search parameters, an empty value as absent, and
//! parses the last two with an unguarded `JSON.parse`, so a value that is no
//! JSON throws the `SyntaxError` of V8 out of the parser.
//! [`is_content_type_match`] cuts a list of content types at its first
//! comma.

use aosabi_execute::{
    ErrorObject,
    Value,
    value::error::ErrorName, //
};
use aosabi_http::{
    Request,
    fetch::url::url_search_params, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};

use crate::{
    plugin::{
        GraphQLParams,
        RequestParserResult, //
    },
    request_parser::json_parse::{
        JsonParseError,
        json_parse, //
    },
    utils::header::with_header, //
};

/// Returns the pairs of `init`, `new URLSearchParams(init)` of a string.
///
/// The constructor drops one leading `?` of a string.
#[must_use]
pub fn search_params(init: &str) -> Vec<(String, String)> {
    url_search_params(init.strip_prefix('?').unwrap_or(init))
}

/// Returns the value of the first pair named `name`, `searchParams.get(name)`.
fn get<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// Returns the thrown value of a failed `JSON.parse`.
#[must_use]
pub fn thrown_parse_error(error: JsonParseError) -> Value {
    let name = match error.name {
        "RangeError" => ErrorName::RangeError,
        name => ErrorName::Other(JsString::from(name)),
    };
    Value::from(ErrorObject::Plain {
        name,
        message: error.message,
    })
}

/// Returns the parameters of search parameters, `handleURLSearchParams(searchParams)`.
///
/// # Errors
///
/// Returns the `SyntaxError` of a `variables` or `extensions` value that is no JSON.
pub fn handle_url_search_params(pairs: &[(String, String)]) -> Result<RequestParserResult, Value> {
    let present = |name: &str| get(pairs, name).filter(|value| !value.is_empty());
    let text = |name: &str| present(name).map_or(JsValue::Undefined, JsValue::from);
    let parsed = |name: &str| -> Result<JsValue, Value> {
        match present(name) {
            Some(value) => json_parse(value).map_err(thrown_parse_error),
            None => Ok(JsValue::Undefined),
        }
    };
    let mut params = JsObject::new();
    params.insert("operationName", text("operationName"));
    params.insert("query", text("query"));
    params.insert("variables", parsed("variables")?);
    params.insert("extensions", parsed("extensions")?);
    Ok(RequestParserResult::Single(GraphQLParams::new(
        JsValue::Object(params),
    )))
}

/// Returns the parameters of a form body, `parseURLSearchParams(requestBody)`.
///
/// # Errors
///
/// Returns what [`handle_url_search_params`] returns.
pub fn parse_url_search_params(request_body: &str) -> Result<RequestParserResult, Value> {
    handle_url_search_params(&search_params(request_body))
}

/// Returns whether the content type of `request` is `expected`, `isContentTypeMatch(request, expectedContentType)`.
///
/// The header is read where the request holds it, and the test of
/// `contentType.startsWith(expectedContentType + ';')` is the test of the
/// character `;` after `expected`, so the test makes no string.
#[must_use]
pub fn is_content_type_match(request: &Request, expected: &str) -> bool {
    with_header(request, "content-type", |content_type| {
        let Some(content_type) = content_type else {
            return false;
        };
        // A list of content-types is not valid as per HTTP spec, but some
        // clients dont care.
        let content_type = content_type.split(',').next().unwrap_or_default();
        !content_type.is_empty()
            && (content_type == expected
                || content_type
                    .strip_prefix(expected)
                    .is_some_and(|rest| rest.starts_with(';')))
    })
}

/// Returns the thrown value of a body the platform failed to read.
#[must_use]
pub fn thrown_body_error(error: &aosabi_http::fetch::body::BodyError) -> Value {
    Value::from(ErrorObject::type_error(error.to_string()))
}
