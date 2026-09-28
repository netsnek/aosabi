// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the parser of an application/graphql body.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/request-parser/post-graphql-string.js, MIT.
 */

//! The parser of an application/graphql body.
//!
//! The body is the query, `{ query: await request.text() }`.

use std::rc::Rc;

use aosabi_execute::Value;
use aosabi_hooks::LocalBoxFuture;
use aosabi_http::Request;
use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::{
    plugin::{
        GraphQLParams,
        RequestParserResult, //
    },
    request_parser::utils::{
        is_content_type_match,
        thrown_body_error, //
    }, //
};

/// Returns whether `request` is a POST of a query string, `isPOSTGraphQLStringRequest(request)`.
#[must_use]
pub fn is_post_graphql_string_request(request: &Request) -> bool {
    request.method == "POST" && is_content_type_match(request, "application/graphql")
}

/// Parses the parameters of a query string body, `parsePOSTGraphQLStringRequest(request)`.
pub fn parse_post_graphql_string_request(
    request: Rc<Request>,
) -> LocalBoxFuture<'static, Result<RequestParserResult, Value>> {
    Box::pin(async move {
        let query = request
            .text()
            .await
            .map_err(|error| thrown_body_error(&error))?;
        let mut params = JsObject::new();
        params.insert("query", JsValue::from(query));
        Ok(RequestParserResult::Single(GraphQLParams::new(
            JsValue::Object(params),
        )))
    })
}
