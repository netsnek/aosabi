// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the request parsers and the choice of one.
 *
 * Each file of the directory names the file of Yoga it translates.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/use-request-parser.js, MIT.
 * Derived from graphql-yoga 5.18.0, cjs/server.js, MIT.
 */

//! The request parsers.
//!
//! [`Parsers::choose`] answers the parser of a request before the hooks
//! `onRequestParse` of the app, which may replace it. Yoga sets the parser of
//! each of its five parsers whose matcher accepts the request, so the last one
//! that matches is the one that runs.

use std::rc::Rc;

use aosabi_http::Request;

use crate::plugin::RequestParser;

pub mod get;
pub mod json_parse;
pub mod post_form_url_encoded;
pub mod post_graphql_string;
pub mod post_json;
pub mod post_multipart;
pub mod utils;

/// The matcher of a parser, `(request) => boolean`.
pub type RequestMatcher = fn(&Request) -> bool;

/// The request parsers of the function, each with its matcher, in the order of Yoga's list.
pub struct Parsers(Vec<(RequestMatcher, RequestParser)>);

impl Parsers {
    /// Returns the parsers of GET, JSON, multipart when `multipart` holds,
    /// `application/graphql` and forms, in that order.
    ///
    /// The multipart parser stands in the list where `multipart` holds, which
    /// the function passes for the option `multipart: true` alone, the named
    /// divergence D49, where Yoga puts it in unless the option is `false`.
    #[must_use]
    pub fn new(multipart: bool) -> Self {
        let mut parsers: Vec<(RequestMatcher, RequestParser)> = Vec::with_capacity(5);
        parsers.push((get::is_get_request, Rc::new(get::parse_get_request)));
        parsers.push((
            post_json::is_post_json_request,
            Rc::new(post_json::parse_post_json_request),
        ));
        if multipart {
            parsers.push((
                post_multipart::is_post_multipart_request,
                Rc::new(post_multipart::parse_post_multipart_request),
            ));
        }
        parsers.push((
            post_graphql_string::is_post_graphql_string_request,
            Rc::new(post_graphql_string::parse_post_graphql_string_request),
        ));
        parsers.push((
            post_form_url_encoded::is_post_form_url_encoded_request,
            Rc::new(post_form_url_encoded::parse_post_form_url_encoded_request),
        ));
        Self(parsers)
    }

    /// Returns the parser of the last matcher that accepts `request`, or none.
    #[must_use]
    pub fn choose(&self, request: &Request) -> Option<RequestParser> {
        let mut chosen = None;
        for (matcher, parser) in &self.0 {
            if matcher(request) {
                chosen = Some(parser);
            }
        }
        chosen.map(Rc::clone)
    }
}
