// SPDX-License-Identifier: MIT
/*
 * aosabi-sentry: the Sentry client of one request.
 *
 * Derived from toucan-js 4.1.1, ., MIT.
 */

//! `Toucan`, `ToucanOptions` and `RequestDataOptions`.
//!
//! Toucan is a scope with a client of its own, made once per request by the
//! middleware, so the data a route sets on it reaches the error the
//! middleware captures and no other request. Its `withScope` hands its
//! callback a clone with a copy of the scope data, whose captures carry that
//! copy and leave the scope of the request as it was, which [`Toucan::fork`]
//! and [`Toucan::capture_in`] stand for, and its captures of an error and of
//! a message are those of the scope of `@sentry/core` 8.9.2. Its client adds
//! the data of the request to an event through the integration `RequestData`,
//! under the allowlists of [`RequestDataOptions`].
//!
//! A capture mints its event id first and sends only where the options name a
//! DSN. A client with no DSN has no transport, so `sendEnvelope` drops the
//! event it builds, and this port builds none. The other default integrations
//! of Toucan, the linked errors and the errors of Zod, and the options this
//! port does not carry are no part of it, which `NOT_PORTED` of
//! `aosabi-compat` names.

use core::cell::RefCell;
use std::rc::Rc;

use aosabi_http::fetch::{
    request::Request,
    url::url_search_params, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    sort::sort_by_key_stable,
    value::array_index, //
};

use crate::{
    envelope::{
        Breadcrumb,
        Exception,
        event_id, //
    },
    hub::{
        Client,
        ClientOptions,
        Report,
        Scope, //
    },
};

/// The allowlists of the integration `RequestData`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestDataOptions {
    /// `allowedHeaders`, the names a header of the event may carry, compared
    /// without case. `None` is no list, which keeps no header.
    pub allowed_headers: Option<Vec<String>>,
    /// Whether `allowedSearchParams` keeps every parameter, the expression
    /// `/(.*)/`. `false` is no list, which keeps no query.
    pub allow_all_search_params: bool,
}

impl Default for RequestDataOptions {
    /// Returns `defaultRequestDataOptions`, the headers `CF-RAY` and
    /// `CF-Worker`.
    fn default() -> Self {
        Self {
            allowed_headers: Some(vec![String::from("CF-RAY"), String::from("CF-Worker")]),
            allow_all_search_params: false,
        }
    }
}

/// The options of [`Toucan::new`], the keys of Toucan this port carries.
#[derive(Clone, Debug, Default)]
pub struct ToucanOptions {
    /// The options of the client, `dsn`, `environment`, `release` and `sampleRate`.
    pub client: ClientOptions,
    /// `request`, the request whose data an event carries.
    pub request: Option<Rc<Request>>,
    /// `requestDataOptions`.
    pub request_data_options: RequestDataOptions,
}

/// The client of one request, `new Toucan(options)`.
#[derive(Debug)]
pub struct Toucan {
    /// The client.
    client: Client,
    /// The scope the request writes into.
    scope: RefCell<Scope>,
    /// The request.
    request: Option<Rc<Request>>,
    /// The allowlists of the request data.
    request_data: RequestDataOptions,
}

impl Toucan {
    /// Returns the client of `options`.
    #[must_use]
    pub fn new(options: ToucanOptions) -> Self {
        Self {
            client: Client::new(options.client),
            scope: RefCell::new(Scope::new()),
            request: options.request,
            request_data: options.request_data_options,
        }
    }

    /// Returns whether the client sends events, which needs a DSN.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.client.is_enabled()
    }

    /// Runs `f` over the scope of the request.
    pub fn configure_scope<R>(&self, f: impl FnOnce(&mut Scope) -> R) -> R {
        f(&mut self.scope.borrow_mut())
    }

    /// Adds `breadcrumb` to the scope of the request, `toucan.addBreadcrumb(breadcrumb)`.
    pub fn add_breadcrumb(&self, breadcrumb: Breadcrumb) {
        self.scope.borrow_mut().add_breadcrumb(breadcrumb);
    }

    /// Captures `exception` with the data of the request, `toucan.captureException(exception)`.
    ///
    /// The id is minted first and answered whether or not the client sends,
    /// and `None` when the process has no source of random bytes.
    pub fn capture_exception(&self, exception: Exception) -> Option<String> {
        let scope = self.fork();
        self.capture_in(&scope, Report::Exception(exception))
    }

    /// Captures `message` at `level` with the data of the request,
    /// `toucan.captureMessage(message, level)`, the level `info` where the
    /// call names none.
    pub fn capture_message(&self, message: &str, level: Option<&str>) -> Option<String> {
        let scope = self.fork();
        self.capture_in(&scope, message_of(message, level))
    }

    /// Returns a copy of the scope of the request, the scope of the clone
    /// `toucan.withScope(callback)` hands its callback.
    #[must_use]
    pub fn fork(&self) -> Scope {
        self.scope.borrow().clone()
    }

    /// Captures `report` under `scope`, the scope of the request or a fork of
    /// it, with the data of the request, and answers its id.
    ///
    /// The id is minted first and answered whether or not the client sends,
    /// and `None` when the process has no source of random bytes.
    pub fn capture_in(&self, scope: &Scope, report: Report) -> Option<String> {
        let id = event_id().ok()?;
        if self.client.is_enabled() {
            let request = self
                .request
                .as_deref()
                .map(|request| event_request(request, &self.request_data));
            self.client.send(id.clone(), report, scope, request);
        }
        Some(id)
    }
}

/// Returns the report of the message `message` at `level`, `info` where the
/// call names none, as `eventFromMessage` makes its event.
#[must_use]
pub fn message_of(message: &str, level: Option<&str>) -> Report {
    Report::Message {
        message: String::from(message),
        level: String::from(level.unwrap_or("info")),
    }
}

/// Returns the request data of an event, `toEventRequest(request, options)`.
///
/// The method, the headers but the cookie, the URL as its protocol, host name
/// and path, and the query, then the allowlists. A list of headers that keeps
/// none leaves the key out, cookies are left out with no list of cookies, and
/// the query is left out with no list of parameters and is otherwise the kept
/// parameters written again as a form query.
fn event_request(request: &Request, options: &RequestDataOptions) -> JsObject {
    let mut data = JsObject::new();
    data.insert("method", request.method.as_ref());
    if let Some(allowed) = &options.allowed_headers {
        let mut headers = JsObject::new();
        for (name, value) in request.headers.borrow().entries() {
            if name != "cookie" && allowed.iter().any(|one| one.eq_ignore_ascii_case(&name)) {
                headers.insert(name.as_str(), value.as_str());
            }
        }
        if !headers.is_empty() {
            data.insert("headers", JsValue::Object(headers));
        }
    }
    let (url, search) = split_url(&request.url);
    data.insert("url", url);
    if options.allow_all_search_params {
        let query = search.strip_prefix('?').unwrap_or(search);
        data.insert("query_string", serialize_params(&kept_params(query)));
    }
    data
}

/// Splits a serialized URL into `protocol//hostname pathname` and its `search`.
///
/// A URL of a request is serialized already, so its parts are read by a
/// split: the scheme with its colon, the host without its user and its port,
/// the path, and the query with its `?`, empty when it holds nothing. A text
/// that names no authority is split at its first `?`, which is the fallback
/// of the reference for a URL that does not parse.
fn split_url(href: &str) -> (String, &str) {
    let href = href.split('#').next().unwrap_or_default();
    let (before, search) = match href.find('?') {
        Some(at) => href.split_at(at),
        None => (href, ""),
    };
    let search = if search == "?" { "" } else { search };
    let Some((scheme, rest)) = before.split_once("://") else {
        return (String::from(before), search);
    };
    let end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let hostname = if host.starts_with('[') {
        host.find(']').map_or(host, |close| &host[..=close])
    } else {
        host.split(':').next().unwrap_or_default()
    };
    let path = if path.is_empty() { "/" } else { path };
    (format!("{scheme}://{hostname}{path}"), search)
}

/// Returns the parameters of `query` in the order of the keys of
/// `Object.fromEntries(new URLSearchParams(query))`, each name once with its
/// last value.
///
/// The keys of that object come in `OrdinaryOwnPropertyKeys` order: the names
/// that are array indices first, in ascending numeric order, then the other
/// names in the order they first appear. So `b=1&2=x&1=y&b=3` keeps `1=y`,
/// `2=x` and `b=3`, and `z=1&10=a&a=2` keeps `10=a`, `z=1` and `a=2`.
///
/// [own-keys]: https://tc39.es/ecma262/#sec-ordinaryownpropertykeys
fn kept_params(query: &str) -> Vec<(String, String)> {
    let mut kept: Vec<(String, String)> = Vec::new();
    for (name, value) in url_search_params(query) {
        match kept.iter_mut().find(|(known, _)| *known == name) {
            Some(entry) => entry.1 = value,
            None => kept.push((name, value)),
        }
    }
    // A name that is no array index sorts after every index, and the sort is
    // stable, so those names keep the order they first appear in.
    sort_by_key_stable(&mut kept, |(name, _)| {
        array_index(name.as_bytes()).map_or(u64::MAX, u64::from)
    });
    kept
}

/// Writes `params` as `new URLSearchParams(params).toString()` writes them.
///
/// The serializer of `application/x-www-form-urlencoded`: a space becomes
/// `+`, the bytes `*-._`, the digits and the ASCII letters stay, and every
/// other byte of the UTF-8 form becomes `%XX` with upper case digits.
fn serialize_params(params: &[(String, String)]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
    for (index, (name, value)) in params.iter().enumerate() {
        if index > 0 {
            out.push('&');
        }
        for (at, part) in [name, value].into_iter().enumerate() {
            if at == 1 {
                out.push('=');
            }
            for byte in part.bytes() {
                match byte {
                    b' ' => out.push('+'),
                    b'*' | b'-' | b'.' | b'_' => out.push(char::from(byte)),
                    _ if byte.is_ascii_alphanumeric() => out.push(char::from(byte)),
                    _ => {
                        out.push('%');
                        out.push(char::from(HEX[usize::from(byte >> 4)]));
                        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
                    }
                }
            }
        }
    }
    out
}
