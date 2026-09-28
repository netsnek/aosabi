// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the parser of a GET request.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/request-parser/get.js, MIT.
 */

//! The parser of a GET request.
//!
//! The query string is the URL after its first `?`, and the whole URL when
//! it has none, because `indexOf` answers -1 and the substring starts at
//! 0, so a URL without a query names one parameter, the URL itself, and no
//! `query`.

use std::rc::Rc;

use aosabi_execute::Value;
use aosabi_hooks::LocalBoxFuture;
use aosabi_http::Request;

use crate::{
    plugin::RequestParserResult,
    request_parser::utils::{
        handle_url_search_params,
        search_params, //
    }, //
};

/// Returns whether `request` is a GET, `isGETRequest(request)`.
#[must_use]
pub fn is_get_request(request: &Request) -> bool {
    request.method == "GET"
}

/// Parses the parameters of a GET, `parseGETRequest(request)`.
pub fn parse_get_request(
    request: Rc<Request>,
) -> LocalBoxFuture<'static, Result<RequestParserResult, Value>> {
    Box::pin(async move {
        let query_string = match request.url.find('?') {
            Some(index) => &request.url[index + 1..],
            None => request.url.as_str(),
        };
        handle_url_search_params(&search_params(query_string))
    })
}
