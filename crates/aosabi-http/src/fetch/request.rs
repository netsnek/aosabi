// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the Fetch Request a Node request becomes.
 *
 * Derived from hono 1.19.9, dist/request.js, MIT.
 */

//! The Fetch `Request`.
//!
//! A request carries its method, its URL, its `Headers` and its body. The body
//! is read once, and a second read answers the `TypeError` of a body in use. A
//! request of the Node adapter reads `TRACE` as its method and has no body, as
//! the adapter makes a `GET` whose method reads `TRACE` (A8 of the plan's
//! version 2 draft).
//!
//! An outbound request may also carry the field lines a client that writes
//! HTTP/1.1 itself sends in place of its `Headers`, each name as it was spelled
//! and each value untrimmed, the lines node:http writes for the object of a
//! request. Only the executor of Federation sets them, whose reference hands
//! node:http a plain object, and the native driver of `aosabi-fetch` writes
//! them, so a served request never carries them.
//!
//! Under the feature `host-platform` a served request may hold its body at
//! its host, which reads it once, at the first read of the body, through the
//! loader `Request::with_body_loader` sets, so a body no read takes never
//! crosses to the engine. The getter [`Request::body`] of such a request
//! answers no body until the read, since the bytes stay with the host.

use core::cell::{
    Cell,
    RefCell, //
};
use std::{
    borrow::Cow,
    rc::Rc, //
};

#[cfg(feature = "host-platform")]
use crate::fetch::body::LocalBoxFuture;
use crate::fetch::{
    body::{
        Body,
        BodyError,
        utf8_decode, //
    },
    headers::Headers, //
};

/// The read of a body the host holds, which answers its bytes once.
#[cfg(feature = "host-platform")]
pub type BodyLoader = Box<dyn FnOnce() -> LocalBoxFuture<'static, Result<Rc<[u8]>, BodyError>>>;

/// The loader of the body of a request whose host holds it, taken by the
/// first read.
#[cfg(feature = "host-platform")]
#[derive(Default)]
struct HostBody(RefCell<Option<BodyLoader>>);

#[cfg(feature = "host-platform")]
impl core::fmt::Debug for HostBody {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(if self.0.borrow().is_some() {
            "HostBody(held)"
        } else {
            "HostBody(none)"
        })
    }
}

/// The Fetch `Request` of one served request.
#[derive(Debug)]
pub struct Request {
    /// The method, as the request reads it.
    pub method: Cow<'static, str>,
    /// The URL, the WHATWG serialization of the request's URL.
    pub url: String,
    /// The headers.
    pub headers: RefCell<Headers>,
    /// The body, which a read takes.
    body: RefCell<Body>,
    /// Whether the body was read.
    body_used: Cell<bool>,
    /// The read of a body the host holds, which the first read takes.
    #[cfg(feature = "host-platform")]
    host_body: HostBody,
    /// The field lines a client that writes the request itself sends in
    /// place of the headers, none for a request that sets none, shared by
    /// the requests of one caller.
    wire_fields: Option<Rc<[(String, String)]>>,
}

impl Request {
    /// Creates a request with `method`, `url`, `headers` and `body`.
    pub fn new(
        method: impl Into<Cow<'static, str>>,
        url: impl Into<String>,
        headers: Headers,
        body: Body,
    ) -> Self {
        Self {
            method: method.into(),
            url: url.into(),
            headers: RefCell::new(headers),
            body: RefCell::new(body),
            body_used: Cell::new(false),
            #[cfg(feature = "host-platform")]
            host_body: HostBody::default(),
            wire_fields: None,
        }
    }

    /// Returns the request with its body held by its host, which `loader`
    /// reads once at the first read of the body.
    #[cfg(feature = "host-platform")]
    #[must_use]
    pub fn with_body_loader(self, loader: BodyLoader) -> Self {
        *self.host_body.0.borrow_mut() = Some(loader);
        self
    }

    /// Returns the request with `fields`, the lines a client that writes
    /// the request itself sends in place of its headers.
    ///
    /// Each name keeps its spelling and each value its outer whitespace,
    /// and the caller keeps the headers, which every other reader of the
    /// request reads, to the same fields in the form the Fetch standard
    /// normalizes, each name in lower case and each value trimmed.
    #[must_use]
    pub fn with_wire_fields(mut self, fields: Rc<[(String, String)]>) -> Self {
        self.wire_fields = Some(fields);
        self
    }

    /// Returns the field lines [`Request::with_wire_fields`] set, or none.
    #[must_use]
    pub fn wire_fields(&self) -> Option<&[(String, String)]> {
        self.wire_fields.as_deref()
    }

    /// Returns the value of the header `name`, `request.headers.get(name)`.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers.borrow().get(name)
    }

    /// Returns whether the body was read, `request.bodyUsed`.
    #[must_use]
    pub fn body_used(&self) -> bool {
        self.body_used.get()
    }

    /// Returns the body without reading it, `request.body`.
    #[must_use]
    pub fn body(&self) -> Body {
        self.body.borrow().clone()
    }

    /// Puts `body` in the place of the body, the body of
    /// `new Request(request, { body })`, which keeps every other part of the
    /// request.
    pub fn set_body(&self, body: Body) {
        *self.body.borrow_mut() = body;
    }

    /// Reads the body into bytes, `await request.arrayBuffer()`.
    ///
    /// # Errors
    ///
    /// Returns [`BodyError::Used`] when the body was read before, and the
    /// error a chunk failed with.
    pub async fn bytes(&self) -> Result<Rc<[u8]>, BodyError> {
        if self.body_used.replace(true) {
            return Err(BodyError::Used);
        }
        #[cfg(feature = "host-platform")]
        let loader = self.host_body.0.borrow_mut().take();
        #[cfg(feature = "host-platform")]
        if let Some(load) = loader {
            return load().await;
        }
        let body = self.body.replace(Body::Empty);
        body.bytes().await
    }

    /// Reads the body into a string, `await request.text()`.
    ///
    /// The bytes are decoded as UTF-8 with the replacement character, as the
    /// Fetch Standard decodes a body.
    ///
    /// # Errors
    ///
    /// Returns what [`Request::bytes`] returns.
    pub async fn text(&self) -> Result<String, BodyError> {
        let bytes = self.bytes().await?;
        Ok(utf8_decode(&bytes))
    }
}
