// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the error that carries its own answer.
 *
 * Derived from hono 4.11.9, dist/http-exception.js, MIT.
 */

//! `HTTPException`.
//!
//! An `HTTPException` is an `Error` with a status and an optional response,
//! and its `getResponse()` answers a response of the status over the body and
//! the headers of that response, or over the message. The default error
//! handler of hono and the error handler of the app answer it with that response.

use core::fmt;

use crate::fetch::{
    body::Body,
    response::{
        HeadersInit,
        Response,
        ResponseInit, //
    }, //
};

/// An error that carries its own answer, `HTTPException` of hono.
#[derive(Clone)]
pub struct HttpException {
    /// The status, 500 by default.
    pub status: u16,
    /// The message, empty when none is given.
    pub message: String,
    /// The response to answer with, when one is given.
    pub res: Option<Response>,
}

impl HttpException {
    /// Creates `new HTTPException(status, { message })`.
    #[must_use]
    pub fn new(status: u16, message: Option<String>) -> Self {
        Self {
            status,
            message: message.unwrap_or_default(),
            res: None,
        }
    }

    /// Creates `new HTTPException(status, { res })`.
    #[must_use]
    pub fn with_response(status: u16, res: Response) -> Self {
        Self {
            status,
            message: String::new(),
            res: Some(res),
        }
    }

    /// Returns the answer of the error, `getResponse()`.
    ///
    /// A response of the status over the body and the headers of the
    /// response given, or over the message.
    #[must_use]
    pub fn get_response(&self) -> Response {
        let (body, headers) = match &self.res {
            Some(res) => (res.body(), Some(HeadersInit::Fetch(res.headers()))),
            None => (Body::Text(self.message.as_str().into()), None),
        };
        let init = ResponseInit {
            status: Some(self.status),
            status_text: None,
            headers,
        };
        Response::new(body, Some(init)).unwrap_or_else(|_| Response::empty(self.status))
    }
}

impl fmt::Debug for HttpException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpException")
            .field("status", &self.status)
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for HttpException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for HttpException {}
