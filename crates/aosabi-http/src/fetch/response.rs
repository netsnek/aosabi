// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the Fetch Response, and behind the feature node-server the
 * lightweight Response of the Node adapter.
 *
 * It holds the mark of the coding compress() leaves to the host, which only
 * compress() under the feature platform-coding sets, and the substatus of
 * GraphTP, which no file of the reference has and which only the port of Yoga
 * sets (docs/graphtp-in-aosabi.md section 4).
 *
 * Derived from hono 1.19.9, dist/listener.js, MIT.
 * Derived from hono 1.19.9, dist/response.js, MIT.
 *
 * It carries the quirks Q52_NODE_ADAPTER_CACHED_LENGTH,
 * Q53_HONO_TEXT_DEFAULT_SPACE and Q57_HONO_HEAD_FROM_GET of
 * docs/byte-equality.md section 13.1.
 */

//! The Fetch `Response` and the lightweight answer of the Node adapter.
//!
//! A [`Response`] is a shared handle, as a JavaScript object is: the context
//! of a request, a middleware and the adapter hold the same response, and a
//! change of its headers through one of them is seen by all.
//!
//! Under the feature `node-server` a response is the adapter's lightweight
//! `Response`, which the adapter puts in the place of the platform's. It keeps
//! `[status, body, headers]` in a cache for a body that is a primitive string,
//! bytes or a stream, where `headers` are the headers of its init or
//! `content-type: text/plain; charset=UTF-8` with the space when it has none,
//! and it builds the platform's response from its body and its init only when
//! something reads a member the cache does not hold, the body or the status
//! text among them, which drops the cache (A4 of the plan's version 2 draft,
//! quirk `Q52_NODE_ADAPTER_CACHED_LENGTH`). While the cache holds, the adapter
//! writes the answer on its cached path. A response made from another one takes
//! that one's built response when it was built and copies its init otherwise,
//! and a response of `c.text` with nothing prepared has no init, so a response
//! made from it fails, which is the 500 of a `HEAD` on that path (A9, quirk
//! `Q57_HONO_HEAD_FROM_GET`).
//!
//! Without the feature, which is the Worker, a response is the platform's
//! `Response` of the Fetch Standard, built at once, and a string body with no
//! `content-type` gets `text/plain;charset=UTF-8` with no space, quirk
//! `Q53_HONO_TEXT_DEFAULT_SPACE`.
//!
//! A response also carries the substatus of `GraphTP`, one byte that refines
//! its status and that no response of the reference has. The port of Yoga sets
//! it where it decides the status, a response made from another one takes it
//! with the status, and the HTTP drivers never read or write it, so no byte of
//! an HTTP answer changes.

use core::{
    cell::{
        Cell,
        RefCell, //
    },
    fmt, //
};
use std::rc::Rc;

use crate::fetch::{
    body::{
        Body,
        BodyError,
        utf8_decode, //
    },
    headers::{
        Headers,
        HeadersError, //
    }, //
};

/// The `content-type` the platform gives a string body with no type, with no space.
pub const FETCH_TEXT_PLAIN: &str = "text/plain;charset=UTF-8";

thread_local! {
    /// Whether the platform lists the `content-type` it gives a string body
    /// among the header fields of its response, which undici and workerd do
    /// and Bun does not, writing its own type on the wire alone.
    static LISTED_TEXT_TYPE: Cell<bool> = const { Cell::new(true) };
}

/// Sets whether the platform the build runs on lists the `content-type` it
/// gives a string body among the header fields of its response, true for undici
/// and workerd, the default, and false for Bun, whose `Headers` of
/// `new Response(text)` hold no field and whose server writes
/// `text/plain;charset=utf-8` on the wire.
///
/// The host of the module `core.wasm` and of the native library sets it at
/// the boot, so a response of a string body reads as the platform of the host
/// reads it, in `compress()` and in the answer of `HEAD`, and its host writes
/// the type. A build under the feature `node-server` reads the cache of the
/// adapter and never this setting.
pub fn set_listed_text_type(listed: bool) {
    LISTED_TEXT_TYPE.set(listed);
}

/// Returns whether the platform lists the type of a string body among the
/// header fields of its response.
fn listed_text_type() -> bool {
    LISTED_TEXT_TYPE.get()
}

/// Returns `body` as the getter `body` of the platform of the host answers
/// it, a string body as its bytes on a platform that lists no type of a
/// string body, see [`Response::body`].
#[cfg(feature = "host-platform")]
fn platform_body(body: Body) -> Body {
    match body {
        Body::Text(text) | Body::Html(text) if !listed_text_type() => {
            Body::Full(Rc::from(text.as_bytes()))
        }
        body => body,
    }
}

/// Returns `body` as it is, the getter `body` of a build of the Rust option.
#[cfg(not(feature = "host-platform"))]
fn platform_body(body: Body) -> Body {
    body
}

/// The `content-type` the adapter gives a cached body with no headers, with the space.
pub const NODE_TEXT_PLAIN: &str = "text/plain; charset=UTF-8";

/// The kind of a throw of the platform's constructors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchErrorKind {
    /// A `TypeError`.
    Type,
    /// A `RangeError`.
    Range,
}

/// A throw of the platform's `Response` or `Headers`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchError {
    /// The class of the error.
    pub kind: FetchErrorKind,
    /// The message of the error.
    pub message: String,
}

impl FetchError {
    /// Creates a `TypeError` with `message`.
    pub fn type_error(message: impl Into<String>) -> Self {
        Self {
            kind: FetchErrorKind::Type,
            message: message.into(),
        }
    }

    /// Returns the `name` of the error, `TypeError` or `RangeError`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self.kind {
            FetchErrorKind::Type => "TypeError",
            FetchErrorKind::Range => "RangeError",
        }
    }
}

impl From<HeadersError> for FetchError {
    fn from(error: HeadersError) -> Self {
        Self::type_error(error.message)
    }
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name(), self.message)
    }
}

impl core::error::Error for FetchError {}

/// The headers of an init, as the object gives them.
#[derive(Clone, Debug)]
pub enum HeadersInit {
    /// A plain object, its entries in the order of its keys.
    Plain(Vec<(String, String)>),
    /// A `Headers`.
    Fetch(Headers),
}

impl HeadersInit {
    /// Returns the `Headers` the init makes, `new Headers(init)`.
    fn to_headers(&self) -> Result<Headers, HeadersError> {
        match self {
            Self::Plain(entries) => Headers::from_entries(entries),
            Self::Fetch(headers) => Ok(headers.clone()),
        }
    }
}

/// The init of a response, `{ status, statusText, headers }`.
#[derive(Clone, Debug, Default)]
pub struct ResponseInit {
    /// The status, 200 when absent.
    pub status: Option<u16>,
    /// The status text, empty when absent.
    pub status_text: Option<String>,
    /// The headers, none when absent.
    pub headers: Option<HeadersInit>,
}

impl ResponseInit {
    /// Creates the init with `status` alone.
    #[must_use]
    pub fn status(status: u16) -> Self {
        Self {
            status: Some(status),
            ..Self::default()
        }
    }
}

/// The coding `compress()` leaves to the host, which the Worker's glue applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostCoding {
    /// The content coding `deflate`, `CompressionStream('deflate')`.
    Deflate,
}

/// The headers the adapter's cache holds.
#[derive(Clone, Debug)]
pub enum CachedHeaders {
    /// A plain object, written as given, the entries in the order of its keys.
    Plain(Vec<(String, String)>),
    /// A `Headers`, which a reading of the headers made of the plain object.
    Fetch(Headers),
}

/// The adapter's cache of a response, `[status, body, headers]`.
#[derive(Clone, Debug)]
struct NodeCache {
    /// The status.
    status: u16,
    /// The headers.
    headers: CachedHeaders,
}

/// The platform's response once built.
#[derive(Clone, Debug)]
struct Built {
    /// The status.
    status: u16,
    /// The status text.
    status_text: String,
    /// The headers.
    headers: Headers,
}

/// The state every handle of a response shares.
#[derive(Debug)]
struct State {
    /// The body the response was made with.
    body: Body,
    /// The init the response was made with, `None` for none.
    init: Option<ResponseInit>,
    /// The adapter's cache while it holds.
    cache: Option<NodeCache>,
    /// The platform's response once built.
    built: Option<Built>,
    /// The coding left to the host.
    host_coding: Option<HostCoding>,
    /// The substatus of `GraphTP`, 0 for none.
    substatus: u8,
}

/// The Fetch `Response`, a shared handle.
#[derive(Clone)]
pub struct Response(Rc<RefCell<State>>);

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.borrow().fmt(f)
    }
}

impl Response {
    /// Creates `new Response(body, init)`.
    ///
    /// Under the feature `node-server` this is the adapter's constructor,
    /// which keeps a cache for a string, bytes or a stream and builds nothing
    /// yet. Without it the platform's response is built at once.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` of a header the init names and the platform
    /// refuses, and without the feature `node-server` the `RangeError` of a
    /// status outside 200 to 599 and the `TypeError` of a body on a status
    /// that has none.
    pub fn new(body: Body, init: Option<ResponseInit>) -> Result<Self, FetchError> {
        if let Some(HeadersInit::Plain(entries)) =
            init.as_ref().and_then(|init| init.headers.as_ref())
        {
            Headers::from_entries(entries)?;
        }
        let mut state = State {
            body,
            init,
            cache: None,
            built: None,
            host_coding: None,
            substatus: 0,
        };
        if cfg!(feature = "node-server") {
            state.cache = node_cache(&state.body, state.init.as_ref(), None);
        } else {
            state.built = Some(build(&state.body, state.init.as_ref())?);
        }
        Ok(Self(Rc::new(RefCell::new(state))))
    }

    /// Creates `new Response(text)`, a string body with no init.
    ///
    /// Under the feature `node-server` the adapter's cache holds
    /// `content-type: text/plain; charset=UTF-8` with the space, and without it
    /// the platform writes `text/plain;charset=UTF-8` with none, quirk
    /// `Q53_HONO_TEXT_DEFAULT_SPACE`.
    #[must_use]
    pub fn from_text(text: impl Into<Rc<str>>) -> Self {
        let state = State {
            body: Body::Text(text.into()),
            init: None,
            cache: None,
            built: None,
            host_coding: None,
            substatus: 0,
        };
        let response = Self(Rc::new(RefCell::new(state)));
        {
            let mut state = response.0.borrow_mut();
            if cfg!(feature = "node-server") {
                state.cache = node_cache(&state.body, None, None);
            } else {
                let mut headers = Headers::new();
                if listed_text_type() {
                    // The name and the value are the platform's own constants.
                    let _ = headers.append("content-type", FETCH_TEXT_PLAIN);
                }
                state.built = Some(Built {
                    status: 200,
                    status_text: String::new(),
                    headers,
                });
            }
        }
        response
    }

    /// Creates the response of `status` with no body and no headers.
    #[must_use]
    pub fn empty(status: u16) -> Self {
        let init = ResponseInit::status(status);
        let state = State {
            body: Body::Empty,
            built: Some(Built {
                status,
                status_text: String::new(),
                headers: Headers::new(),
            }),
            init: Some(init),
            cache: None,
            host_coding: None,
            substatus: 0,
        };
        Self(Rc::new(RefCell::new(state)))
    }

    /// Creates `new Response(body, other)`, a response made from the response `other`.
    ///
    /// Under the feature `node-server` the adapter's constructor takes the
    /// platform's response of `other` when it was built, which builds this one
    /// at once and keeps no cache, and copies the init of `other` otherwise,
    /// with a cache over `new Headers(init.headers)` and the status of `other`
    /// for a string, bytes or a stream. Without the feature the platform reads
    /// the status, the status text and the headers of `other` as an init.
    /// Either way the response takes the substatus of `other`, which refines
    /// its status.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` the adapter's constructor throws when `other`
    /// has no init, the response of `c.text` with nothing prepared, whose
    /// `init.#init.headers` reads a property of `undefined`, quirk
    /// `Q57_HONO_HEAD_FROM_GET`.
    pub fn from_response(body: Body, other: &Response) -> Result<Self, FetchError> {
        if !cfg!(feature = "node-server") {
            let init = ResponseInit {
                status: Some(other.status()),
                status_text: Some(other.status_text()),
                headers: Some(HeadersInit::Fetch(other.headers())),
            };
            let made = Self::new(body, Some(init))?;
            made.set_substatus(other.substatus());
            return Ok(made);
        }
        let (built, init) = {
            let state = other.0.borrow();
            (state.built.clone(), state.init.clone())
        };
        if let Some(built) = built {
            let init = ResponseInit {
                status: Some(built.status),
                status_text: Some(built.status_text),
                headers: Some(HeadersInit::Fetch(built.headers)),
            };
            let built = build(&body, Some(&init))?;
            let state = State {
                body,
                init: Some(init),
                cache: None,
                built: Some(built),
                host_coding: None,
                substatus: other.substatus(),
            };
            return Ok(Self(Rc::new(RefCell::new(state))));
        }
        let Some(init) = init else {
            return Err(FetchError::type_error(
                "Cannot read properties of undefined (reading 'headers')",
            ));
        };
        let headers = match &init.headers {
            Some(headers) => headers.to_headers()?,
            None => Headers::new(),
        };
        let status = other.status();
        let cache = node_cache(&body, Some(&init), Some((status, headers)));
        let state = State {
            body,
            init: Some(init),
            cache,
            built: None,
            host_coding: None,
            substatus: other.substatus(),
        };
        Ok(Self(Rc::new(RefCell::new(state))))
    }

    /// Returns whether `self` and `other` are the same response.
    #[must_use]
    pub fn ptr_eq(&self, other: &Response) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// Returns the status, `response.status`.
    ///
    /// The adapter answers the status of its cache, and otherwise the status
    /// of the platform's response, which is the status of the init or 200.
    #[must_use]
    pub fn status(&self) -> u16 {
        let state = self.0.borrow();
        if let Some(cache) = &state.cache {
            return cache.status;
        }
        if let Some(built) = &state.built {
            return built.status;
        }
        state
            .init
            .as_ref()
            .and_then(|init| init.status)
            .unwrap_or(200)
    }

    /// Returns the status text, `response.statusText`, which drops the adapter's cache.
    #[must_use]
    pub fn status_text(&self) -> String {
        self.with_built(|built| built.status_text.clone())
            .unwrap_or_default()
    }

    /// Returns a copy of the headers, `response.headers`.
    ///
    /// A plain object of the adapter's cache becomes a `Headers` and stays
    /// cached, as the adapter's getter turns it into one.
    #[must_use]
    pub fn headers(&self) -> Headers {
        self.with_headers(|headers| headers.clone())
    }

    /// Returns the value of the header `name`, `response.headers.get(name)`.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<String> {
        self.with_headers(|headers| headers.get(name))
    }

    /// Returns whether the headers hold `name`, `response.headers.has(name)`.
    #[must_use]
    pub fn has_header(&self, name: &str) -> bool {
        self.with_headers(|headers| headers.has(name))
    }

    /// Runs `f` on the headers of the response, the ones a change through `response.headers` reaches.
    pub fn with_headers<R>(&self, f: impl FnOnce(&mut Headers) -> R) -> R {
        let mut state = self.0.borrow_mut();
        if let Some(cache) = &mut state.cache {
            if let CachedHeaders::Plain(entries) = &cache.headers {
                // The entries were checked when the response was made.
                let headers = Headers::from_entries(entries).unwrap_or_default();
                cache.headers = CachedHeaders::Fetch(headers);
            }
            if let CachedHeaders::Fetch(headers) = &mut cache.headers {
                return f(headers);
            }
        }
        if state.built.is_none() {
            let built =
                build(&state.body, state.init.as_ref()).unwrap_or_else(|_| empty_built(&state));
            state.built = Some(built);
        }
        match &mut state.built {
            Some(built) => f(&mut built.headers),
            None => f(&mut Headers::new()),
        }
    }

    /// Sets the header `name` to `value`, `response.headers.set(name, value)`.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` of a name or a value the platform refuses.
    pub fn set_header(&self, name: &str, value: &str) -> Result<(), FetchError> {
        self.with_headers(|headers| headers.set(name, value))
            .map_err(FetchError::from)
    }

    /// Appends `value` to the header `name`, `response.headers.append(name, value)`.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` of a name or a value the platform refuses.
    pub fn append_header(&self, name: &str, value: &str) -> Result<(), FetchError> {
        self.with_headers(|headers| headers.append(name, value))
            .map_err(FetchError::from)
    }

    /// Removes the header `name`, `response.headers.delete(name)`.
    pub fn delete_header(&self, name: &str) {
        self.with_headers(|headers| headers.delete(name));
    }

    /// Returns the body, `response.body`, which drops the adapter's cache.
    ///
    /// No body gives [`Body::Empty`], and a stream gives the same stream. Under
    /// the feature `host-platform`, on a platform that lists no type of a
    /// string body among the header fields of its response, Bun, a string body
    /// gives its bytes, since the getter of the platform answers a stream of
    /// the bytes of its body, which the platform writes with the type
    /// `application/octet-stream` once a response is made of it, as it writes a
    /// body of bytes.
    #[must_use]
    pub fn body(&self) -> Body {
        platform_body(self.made_body())
    }

    /// Returns the body as the response was made, which drops the adapter's
    /// cache as [`Response::body`] does, the body a host writes for the
    /// response itself.
    #[must_use]
    pub fn made_body(&self) -> Body {
        let mut state = self.0.borrow_mut();
        state.cache = None;
        if state.built.is_none() {
            let built =
                build(&state.body, state.init.as_ref()).unwrap_or_else(|_| empty_built(&state));
            state.built = Some(built);
        }
        state.body.clone()
    }

    /// Reads the body into bytes, `await response.arrayBuffer()`, which drops the adapter's cache.
    ///
    /// # Errors
    ///
    /// Returns the error a chunk of a stream failed with.
    pub async fn bytes(&self) -> Result<Rc<[u8]>, BodyError> {
        let body = self.body();
        body.bytes().await
    }

    /// Reads the body into a string, `await response.text()`, which drops the adapter's cache.
    ///
    /// # Errors
    ///
    /// Returns the error a chunk of a stream failed with.
    pub async fn text(&self) -> Result<String, BodyError> {
        let bytes = self.bytes().await?;
        Ok(utf8_decode(&bytes))
    }

    /// Returns the adapter's cache, `[status, body, headers]`, while it holds.
    ///
    /// The adapter writes an answer whose response has a cache on its cached
    /// path (A4 of the plan's version 2 draft, quirk
    /// `Q52_NODE_ADAPTER_CACHED_LENGTH`).
    #[must_use]
    pub fn node_cache(&self) -> Option<(u16, Body, CachedHeaders)> {
        let state = self.0.borrow();
        let cache = state.cache.as_ref()?;
        Some((cache.status, state.body.clone(), cache.headers.clone()))
    }

    /// Returns the coding `compress()` left to the host.
    #[must_use]
    pub fn host_coding(&self) -> Option<HostCoding> {
        self.0.borrow().host_coding
    }

    /// Marks the coding `compress()` leaves to the host.
    pub fn set_host_coding(&self, coding: HostCoding) {
        self.0.borrow_mut().host_coding = Some(coding);
    }

    /// Returns the substatus of `GraphTP`, 0 for none.
    ///
    /// The port of Yoga decides it where it decides the status, and the
    /// `GraphTP` listener answers it beside the status. HTTP has no substatus,
    /// so the HTTP drivers never read or write it.
    #[must_use]
    pub fn substatus(&self) -> u8 {
        self.0.borrow().substatus
    }

    /// Sets the substatus of `GraphTP`.
    pub fn set_substatus(&self, substatus: u8) {
        self.0.borrow_mut().substatus = substatus;
    }

    /// Runs `f` on the platform's response, which it builds and which drops the adapter's cache.
    fn with_built<R>(&self, f: impl FnOnce(&Built) -> R) -> Option<R> {
        let mut state = self.0.borrow_mut();
        state.cache = None;
        if state.built.is_none() {
            let built = build(&state.body, state.init.as_ref()).ok()?;
            state.built = Some(built);
        }
        state.built.as_ref().map(f)
    }
}

/// Returns the adapter's cache of a response made with `body` and `init`.
///
/// A primitive string, bytes and a stream are kept, with the status of the init
/// or 200 and the headers of the init or the default `content-type` with the
/// space. A response made from another one passes the status of that one and
/// the `Headers` of its init as `made_from`.
fn node_cache(
    body: &Body,
    init: Option<&ResponseInit>,
    made_from: Option<(u16, Headers)>,
) -> Option<NodeCache> {
    if !matches!(body, Body::Text(_) | Body::Full(_) | Body::Stream(_)) {
        return None;
    }
    if let Some((status, headers)) = made_from {
        return Some(NodeCache {
            status: if status == 0 { 200 } else { status },
            headers: CachedHeaders::Fetch(headers),
        });
    }
    let status = init
        .and_then(|init| init.status)
        .filter(|&status| status != 0)
        .unwrap_or(200);
    let headers = match init.and_then(|init| init.headers.clone()) {
        Some(HeadersInit::Plain(entries)) => CachedHeaders::Plain(entries),
        Some(HeadersInit::Fetch(headers)) => CachedHeaders::Fetch(headers),
        None => CachedHeaders::Plain(vec![(
            String::from("content-type"),
            String::from(NODE_TEXT_PLAIN),
        )]),
    };
    Some(NodeCache { status, headers })
}

/// Returns the platform's response of `body` and `init`, `new Response(body, init)` of the Fetch Standard.
///
/// A status outside 200 to 599 is a `RangeError`, a body on a status of no body
/// a `TypeError`, and a string body with no `content-type` takes
/// `text/plain;charset=UTF-8`.
fn build(body: &Body, init: Option<&ResponseInit>) -> Result<Built, FetchError> {
    let status = init.and_then(|init| init.status).unwrap_or(200);
    if !(200..=599).contains(&status) {
        return Err(FetchError {
            kind: FetchErrorKind::Range,
            message: String::from(
                "init[\"status\"] must be in the range of 200 to 599, inclusive.",
            ),
        });
    }
    if !body.is_empty() && matches!(status, 101 | 103 | 204 | 205 | 304) {
        return Err(FetchError::type_error(format!(
            "Response constructor: Invalid response status code {status}"
        )));
    }
    let mut headers = match init.and_then(|init| init.headers.as_ref()) {
        Some(headers) => headers.to_headers()?,
        None => Headers::new(),
    };
    if matches!(body, Body::Text(_) | Body::Html(_))
        && !headers.has("content-type")
        && listed_text_type()
    {
        headers.append("content-type", FETCH_TEXT_PLAIN)?;
    }
    Ok(Built {
        status,
        status_text: init
            .and_then(|init| init.status_text.clone())
            .unwrap_or_default(),
        headers,
    })
}

/// Returns a platform's response with the status of `state` and nothing else.
///
/// A build that throws in the reference throws where a member is read, and
/// the handle reads the member of this response instead, so a reading never
/// panics.
fn empty_built(state: &State) -> Built {
    Built {
        status: state
            .init
            .as_ref()
            .and_then(|init| init.status)
            .unwrap_or(200),
        status_text: String::new(),
        headers: Headers::new(),
    }
}
