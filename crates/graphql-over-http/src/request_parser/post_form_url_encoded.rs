// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the parser of a form body.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/request-parser/post-form-url-encoded.js, MIT.
 */

//! The parser of a form body.
//!
//! The body is read as text and parsed as search parameters.

use std::rc::Rc;

use aosabi_execute::Value;
use aosabi_hooks::LocalBoxFuture;
use aosabi_http::Request;

use crate::{
    plugin::RequestParserResult,
    request_parser::utils::{
        is_content_type_match,
        parse_url_search_params,
        thrown_body_error, //
    }, //
};

/// Returns whether `request` is a POST of a form, `isPOSTFormUrlEncodedRequest(request)`.
#[must_use]
pub fn is_post_form_url_encoded_request(request: &Request) -> bool {
    request.method == "POST" && is_content_type_match(request, "application/x-www-form-urlencoded")
}

/// Parses the parameters of a form body, `parsePOSTFormUrlEncodedRequest(request)`.
pub fn parse_post_form_url_encoded_request(
    request: Rc<Request>,
) -> LocalBoxFuture<'static, Result<RequestParserResult, Value>> {
    Box::pin(async move {
        let text = request
            .text()
            .await
            .map_err(|error| thrown_body_error(&error))?;
        parse_url_search_params(&text)
    })
}
