// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the view of a request, the view of a Fetch request the handlers
 * of the chain read.
 *
 * Derived from hono 4.11.9, dist/request.js, MIT.
 */

//! The view of a request, `c.req`.
//!
//! A view of a request reads the raw Fetch request, the path the router matched and
//! the parameters of the route that runs, which the index of the route in the chain
//! names. A parameter that holds a `%` is decoded with `decodeURIComponent`, and a
//! query parameter is read from the URL string as `utils/url.js` reads it.

use core::cell::{
    Cell,
    RefCell, //
};
use std::rc::Rc;

use aosabi_json::JsValue;

use crate::{
    fetch::{
        body::BodyError,
        request::Request, //
    },
    router::Params,
    utils::url::{
        decode_uri_component,
        get_query_param,
        get_query_param_all,
        get_query_params,
        try_decode, //
    }, //
    web_app::RouteRecord,
};

/// The state every handle of a view of a request shares.
struct Inner {
    /// The raw request.
    raw: Rc<Request>,
    /// The path the router matched.
    path: String,
    /// The matched routes with their parameters, in the order they run.
    routes: Rc<[(RouteRecord, Params)]>,
    /// The index of the route that runs.
    route_index: Cell<usize>,
    /// The body read once as text, which a later reading takes.
    text: RefCell<Option<Result<String, BodyError>>>,
}

/// The view of a request, a shared handle.
#[derive(Clone)]
pub struct RequestView(Rc<Inner>);

impl RequestView {
    /// Creates the request over `raw` routed at `path` with the matched routes `routes`.
    #[must_use]
    pub fn new(raw: Rc<Request>, path: String, routes: Rc<[(RouteRecord, Params)]>) -> Self {
        Self(Rc::new(Inner {
            raw,
            path,
            routes,
            route_index: Cell::new(0),
            text: RefCell::new(None),
        }))
    }

    /// Returns the raw Fetch request, `c.req.raw`.
    #[must_use]
    pub fn raw(&self) -> Rc<Request> {
        Rc::clone(&self.0.raw)
    }

    /// Returns the path the router matched, `c.req.path`.
    #[must_use]
    pub fn path(&self) -> String {
        self.0.path.clone()
    }

    /// Returns the URL, `c.req.url`.
    #[must_use]
    pub fn url(&self) -> String {
        self.0.raw.url.clone()
    }

    /// Returns the method, `c.req.method`.
    #[must_use]
    pub fn method(&self) -> String {
        String::from(&*self.0.raw.method)
    }

    /// Returns the index of the route that runs, `c.req.routeIndex`.
    #[must_use]
    pub fn route_index(&self) -> usize {
        self.0.route_index.get()
    }

    /// Sets the index of the route that runs.
    pub fn set_route_index(&self, index: usize) {
        self.0.route_index.set(index);
    }

    /// Returns the value of the parameter `key` of the route that runs, `c.req.param(key)`.
    ///
    /// A value that holds a `%` is decoded with `decodeURIComponent`, and a name
    /// the route does not declare answers `None`.
    #[must_use]
    pub fn param(&self, key: &str) -> Option<String> {
        let (_, params) = self.0.routes.get(self.route_index())?;
        let (_, value) = params.iter().find(|(name, _)| name == key)?;
        Some(decode_param(value))
    }

    /// Returns every parameter of the route that runs, decoded, `c.req.param()`.
    #[must_use]
    pub fn params(&self) -> Vec<(String, String)> {
        let Some((_, params)) = self.0.routes.get(self.route_index()) else {
            return Vec::new();
        };
        params
            .iter()
            .map(|(name, value)| (name.clone(), decode_param(value)))
            .collect()
    }

    /// Returns the first value of the query parameter `key`, `c.req.query(key)`.
    #[must_use]
    pub fn query(&self, key: &str) -> Option<String> {
        get_query_param(&self.0.raw.url, key)
    }

    /// Returns every query parameter with its first value, `c.req.query()`.
    #[must_use]
    pub fn query_all(&self) -> Vec<(String, String)> {
        get_query_param_all(&self.0.raw.url)
    }

    /// Returns every value of the query parameter `key`, `c.req.queries(key)`.
    #[must_use]
    pub fn queries(&self, key: &str) -> Option<Vec<String>> {
        get_query_params(&self.0.raw.url, key)
    }

    /// Returns the value of the header `name`, `c.req.header(name)`.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<String> {
        self.0.raw.header(name)
    }

    /// Returns every header, `c.req.header()`, the names in lower case in Fetch iteration order.
    #[must_use]
    pub fn headers(&self) -> Vec<(String, String)> {
        self.0.raw.headers.borrow().entries()
    }

    /// Returns the routes the router matched, `c.req.matchedRoutes`.
    #[must_use]
    pub fn matched_routes(&self) -> Vec<RouteRecord> {
        self.0
            .routes
            .iter()
            .map(|(record, _)| record.clone())
            .collect()
    }

    /// Returns the path the route that runs was registered with, `c.req.routePath`.
    #[must_use]
    pub fn route_path(&self) -> Option<String> {
        self.0
            .routes
            .get(self.route_index())
            .map(|(record, _)| record.path.clone())
    }

    /// Reads the body as text once and keeps it, `await c.req.text()`.
    ///
    /// # Errors
    ///
    /// Returns the error the body failed with.
    pub async fn text(&self) -> Result<String, BodyError> {
        if let Some(text) = self.0.text.borrow().clone() {
            return text;
        }
        let text = self.0.raw.text().await;
        *self.0.text.borrow_mut() = Some(text.clone());
        text
    }

    /// Reads the body as JSON, `await c.req.json()`.
    ///
    /// # Errors
    ///
    /// Returns the error the body failed with, or the error of a text that is
    /// no JSON as [`BodyError::Failed`] with its message.
    pub async fn json(&self) -> Result<JsValue, BodyError> {
        let text = self.text().await?;
        aosabi_json::parse(text.as_bytes()).map_err(|error| BodyError::Failed(error.to_string()))
    }
}

/// Returns `value` decoded when it holds a `%`, as `tryDecodeURIComponent` does.
fn decode_param(value: &str) -> String {
    if value.contains('%') {
        try_decode(value, decode_uri_component)
    } else {
        String::from(value)
    }
}
