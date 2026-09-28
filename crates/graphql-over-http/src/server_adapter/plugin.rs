// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the plugin type of the server adapter.
 *
 * Derived from whatwg-node 0.10.18, cjs/plugins/types.js, MIT.
 */

//! `ServerAdapterPlugin`.
//!
//! A plugin of the server adapter has the hooks `onRequest`, `onResponse`
//! and `onDispose`. As a plugin of envelop, a hook it lacks has a body that
//! does nothing. The hooks of Yoga and of the app answer no promise, so a hook
//! here answers at once, and a throw is the thrown [`Value`].

use core::cell::OnceCell;
use std::rc::Rc;

use aosabi_execute::{
    ErrorObject,
    Value, //
};
use aosabi_hooks::HookContext;
use aosabi_http::{
    Request,
    Response,
    fetch::url::{
        Url,
        parse_url, //
    }, //
};

use crate::server_adapter::RequestHandler;

/// A plugin of the server adapter.
pub trait ServerAdapterPlugin: 'static {
    /// The hook `onRequest`, which runs before the request handler and may end the request with a response.
    ///
    /// # Errors
    ///
    /// Returns the value the hook throws, which rejects the fetch.
    fn on_request(&self, _p: &mut OnRequest<'_>) -> Result<(), Value> {
        Ok(())
    }

    /// The hook `onResponse`, which runs after the request handler on whatever response came out.
    ///
    /// # Errors
    ///
    /// Returns the value the hook throws, which rejects the fetch.
    fn on_response(&self, _p: &mut OnResponse<'_>) -> Result<(), Value> {
        Ok(())
    }

    /// The hook `onDispose`, which the disposal of the adapter runs.
    fn on_dispose(&self) {}
}

/// The URL of the request, parsed at its first read, the `url` proxy of the adapter.
///
/// The reference parses `new URL(request.url, 'http://localhost')` the first
/// time a hook reads a member of the proxy.
#[derive(Default)]
pub struct LazyUrl(OnceCell<Result<Url, ()>>);

impl LazyUrl {
    /// Creates the proxy, which parses nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the URL of `href` against `http://localhost`, parsed at the first call.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` `Invalid URL` of a URL the parser refuses,
    /// which a request the adapter built never has.
    pub fn get(&self, href: &str) -> Result<&Url, Value> {
        self.0
            .get_or_init(|| parse_url(href, Some("http://localhost")).map_err(|_| ()))
            .as_ref()
            .map_err(|()| Value::from(ErrorObject::type_error("Invalid URL")))
    }
}

/// The payload of `onRequest`.
pub struct OnRequest<'a> {
    /// The request, which `setRequest` replaces.
    pub(crate) request: &'a mut Rc<Request>,
    /// The server context of the request.
    pub server_context: &'a HookContext,
    /// The URL of the request.
    pub(crate) url: &'a LazyUrl,
    /// The request handler, which `setRequestHandler` replaces.
    pub(crate) request_handler: &'a mut RequestHandler,
    /// The response the hook ends the request with.
    pub(crate) response: &'a mut Option<Response>,
}

impl OnRequest<'_> {
    /// Returns the request, `request`.
    #[must_use]
    pub fn request(&self) -> &Rc<Request> {
        self.request
    }

    /// Replaces the request, `setRequest(request)`.
    pub fn set_request(&mut self, request: Rc<Request>) {
        *self.request = request;
    }

    /// Returns the URL of the request, `url`.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` of a URL the parser refuses.
    pub fn url(&self) -> Result<&Url, Value> {
        self.url.get(&self.request.url)
    }

    /// Returns the request handler, `requestHandler`.
    #[must_use]
    pub fn request_handler(&self) -> &RequestHandler {
        self.request_handler
    }

    /// Replaces the request handler, `setRequestHandler(handler)`.
    pub fn set_request_handler(&mut self, handler: RequestHandler) {
        *self.request_handler = handler;
    }

    /// Ends the request with `response` and skips every later `onRequest`, `endResponse(response)`.
    pub fn end_response(&mut self, response: Response) {
        *self.response = Some(response);
    }
}

/// The payload of `onResponse`.
pub struct OnResponse<'a> {
    /// The request.
    pub request: &'a Rc<Request>,
    /// The server context of the request.
    pub server_context: &'a HookContext,
    /// The response, which `setResponse` replaces.
    pub(crate) response: &'a mut Response,
}

impl OnResponse<'_> {
    /// Returns the response, `response`.
    #[must_use]
    pub fn response(&self) -> &Response {
        self.response
    }

    /// Replaces the response, `setResponse(response)`.
    pub fn set_response(&mut self, response: Response) {
        *self.response = response;
    }
}
