// SPDX-License-Identifier: MIT
/*
 * aosabi-sentry: the integrations of init that write into the scope of a
 * request, the scope itself and the request data it carries.
 *
 * Derived from @sentry/node 8.55.0, build/cjs, MIT.
 * Derived from @sentry/core 8.55.0, build/cjs, MIT.
 */

//! The scope of a request, its request data and the breadcrumbs of the
//! console.
//!
//! `Sentry.init` of `@sentry/node` installs default integrations beside the
//! client. Two of them write what an event of the plugin carries. The Http
//! integration forks the isolation scope for every incoming request, records
//! the request on it and names its transaction `METHOD /path`, and the
//! integration `RequestData` writes that record as the `request` of an event.
//! The console integration turns every call of the console into a breadcrumb
//! of the isolation scope in place, which is the scope of the request whose
//! code wrote the line, or the scope of the process outside a request. So an
//! event of the plugin carries the lines the logger wrote before the capture
//! in its request, after the lines the process wrote before the request, and
//! the request it was captured in.
//!
//! A request of Aosabi runs on one thread with its context in the ambient
//! cell of `aosabi-core`, so the scope of a request is a typed slot of its
//! context, read through `ambient_context`, which leaves the environment of
//! the context alone. The scope is forked from the scope of the process the
//! first time a line or a capture of the request needs it, where the
//! reference forks it when the request arrives, which writes the same scope
//! as long as no line outside every request falls between the two. The lines
//! reach this module through the watcher of `aosabi_core::console`, which
//! [`install`] sets, and the client check of `addBreadcrumb` keeps a line out
//! while no client exists.
//!
//! The request data is read from the Fetch request of the context: the URL is
//! the scheme of the request, the `host` field and the path with the query,
//! the headers are the fields of the request with their names in lower case,
//! and the cookies are those of the `cookie` field. The body, which the
//! reference reads while the request streams, is gone once the handler has
//! read it, so `data` is left out, a field the comparison of the plan reports
//! and never compares.

use core::cell::RefCell;
use std::rc::Rc;

use aosabi_core::{
    Context,
    console::{
        self,
        ConsoleCall, //
    },
    context::ambient_context,
};
use aosabi_http::utils::url::decode_uri_component;
use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::{
    envelope::Breadcrumb,
    hub::{
        Scope,
        client, //
    },
};

/// The longest message of a breadcrumb of the console in UTF-16 code units,
/// `2KB`.
const CONSOLE_MESSAGE_LIMIT: usize = 2048;

/// The host the request data names when the request has no `host` field.
const NO_HOST: &str = "<no host>";

/// The scope of one request, the isolation scope the Http integration forks,
/// with the request data it records.
#[derive(Debug)]
struct Isolation {
    /// The scope, which the lines of the request write into.
    scope: RefCell<Scope>,
    /// The request data, `normalizedRequest` as the integration `RequestData`
    /// writes it into an event.
    request: JsObject,
}

thread_local! {
    /// The isolation scope of the process, which the lines outside every
    /// request write into and every request forks.
    static PROCESS: RefCell<Scope> = RefCell::new(Scope::new());
}

/// Installs the integrations of `init` that watch the process, the console
/// integration, `consoleIntegration().setup(client)`.
pub(crate) fn install() {
    let watcher: console::ConsoleWatcher = Rc::new(on_console);
    console::set_watcher(Some(watcher));
}

/// Adds the breadcrumb of one call of the console, the handler of
/// `console.js:16-31`.
fn on_console(call: &ConsoleCall<'_>) {
    add_breadcrumb(Breadcrumb {
        category: Some(String::from("console")),
        level: Some(String::from(severity_level(call.level))),
        message: Some(truncate(call.message, CONSOLE_MESSAGE_LIMIT)),
        timestamp: None,
    });
}

/// Adds `breadcrumb` to the isolation scope in place, `Sentry.addBreadcrumb(breadcrumb)`.
///
/// With no client of `init` the crumb is dropped, and the scope keeps the
/// last 100. A scope that is borrowed when the line is written, which no code
/// of this crate leaves while it writes a line, drops the crumb as well.
pub(crate) fn add_breadcrumb(breadcrumb: Breadcrumb) {
    if client().is_none() {
        return;
    }
    if let Some(isolation) = request_isolation() {
        if let Ok(mut scope) = isolation.scope.try_borrow_mut() {
            scope.add_breadcrumb(breadcrumb);
        }
        return;
    }
    let _ = PROCESS.try_with(|scope| {
        if let Ok(mut scope) = scope.try_borrow_mut() {
            scope.add_breadcrumb(breadcrumb);
        }
    });
}

/// Returns a copy of the isolation scope in place and the request data it
/// records, none outside a request.
pub(crate) fn isolation_scope() -> (Scope, Option<JsObject>) {
    match request_isolation() {
        Some(isolation) => (
            isolation.scope.borrow().clone(),
            Some(isolation.request.clone()),
        ),
        None => (
            PROCESS
                .try_with(|scope| scope.borrow().clone())
                .unwrap_or_default(),
            None,
        ),
    }
}

/// Returns the isolation scope of the running request, forking it from the
/// scope of the process the first time, or `None` outside a request.
///
/// The fork records the request and names the transaction of the scope
/// `METHOD /path`, as the Http integration does when the request arrives.
fn request_isolation() -> Option<Rc<Isolation>> {
    let context = ambient_context()?;
    if let Some(isolation) = context.extension::<Isolation>() {
        return Some(isolation);
    }
    let mut scope = PROCESS
        .try_with(|scope| scope.borrow().clone())
        .unwrap_or_default();
    let (request, transaction) = request_data(&context);
    scope.set_transaction_name(transaction);
    let isolation = Rc::new(Isolation {
        scope: RefCell::new(scope),
        request,
    });
    context.set_extension(Rc::clone(&isolation));
    Some(isolation)
}

/// Returns the request data of the request of `c` as an event carries it,
/// and the name of its transaction.
fn request_data(c: &Context) -> (JsObject, String) {
    let req = c.req();
    let raw = req.raw();
    let (scheme, target) = scheme_and_target(&raw.url);
    let fields = req.headers();
    let host = fields
        .iter()
        .find(|(name, _)| name == "host")
        .map_or(NO_HOST, |(_, value)| value.as_str());
    let mut headers = JsObject::new();
    for (name, value) in &fields {
        headers.insert(name.as_str(), value.as_str());
    }
    let cookies = fields
        .iter()
        .find(|(name, _)| name == "cookie")
        .map(|(_, value)| parse_cookie(value))
        .unwrap_or_default();
    let mut data = JsObject::new();
    data.insert("headers", JsValue::Object(headers));
    data.insert("method", raw.method.as_ref());
    data.insert("url", format!("{scheme}://{host}{target}"));
    data.insert("cookies", JsValue::Object(cookies));
    let query = target
        .split_once('?')
        .map_or("", |(_, query)| query.split('#').next().unwrap_or_default());
    if !query.is_empty() {
        data.insert("query_string", query);
    }
    let path = target.split(['?', '#']).next().unwrap_or("/");
    let transaction = format!("{} {path}", raw.method.to_ascii_uppercase());
    (data, transaction)
}

/// Splits the serialized URL of a request into its scheme and its target,
/// the path with the query, `/` when the URL names no path.
fn scheme_and_target(href: &str) -> (&str, &str) {
    let Some((scheme, rest)) = href.split_once("://") else {
        return ("http", href);
    };
    let target = rest.find('/').map_or("/", |at| &rest[at..]);
    (scheme, target)
}

/// Returns the cookies of a `cookie` field, `parseCookie(text)`.
///
/// Each pair up to the next `;`, the name and the value trimmed, the first
/// value of a name kept, a value in quotes taken without them, and a value
/// with a `%` decoded by `decodeURIComponent`, kept as it is where that
/// throws.
fn parse_cookie(text: &str) -> JsObject {
    let mut cookies = JsObject::new();
    let mut index = 0;
    while index < text.len() {
        let Some(eq) = text[index..].find('=').map(|at| index + at) else {
            break;
        };
        let end = match text[index..].find(';').map(|at| index + at) {
            None => text.len(),
            Some(end) if end < eq => {
                index = text[..eq].rfind(';').map_or(0, |at| at + 1);
                continue;
            }
            Some(end) => end,
        };
        let key = text[index..eq].trim();
        if !cookies.contains_key(key) {
            let mut value = text[eq + 1..end].trim();
            if let Some(inner) = value.strip_prefix('"') {
                // `slice(1, -1)` drops the last character whatever it is.
                let mut characters = inner.chars();
                characters.next_back();
                value = characters.as_str();
            }
            let value = if value.contains('%') {
                decode_uri_component(value).unwrap_or_else(|| String::from(value))
            } else {
                String::from(value)
            };
            cookies.insert(key, value);
        }
        index = end + 1;
    }
    cookies
}

/// Returns `str.length <= max ? str : str.slice(0, max) + '...'` over UTF-16
/// code units, `truncate(str, max)`.
///
/// A cut that falls inside a pair of surrogates keeps the whole character,
/// where the reference keeps its first half, which a Rust string cannot
/// hold.
fn truncate(text: &str, max: usize) -> String {
    let mut units = 0;
    for (at, character) in text.char_indices() {
        if units >= max {
            return format!("{}...", &text[..at]);
        }
        units += character.len_utf16();
    }
    if units > max {
        format!("{text}...")
    } else {
        String::from(text)
    }
}

/// Returns the level of a breadcrumb for the method `level` of the console,
/// `severityLevelFromString(level)`.
fn severity_level(level: &str) -> &str {
    match level {
        "warn" => "warning",
        "fatal" | "error" | "warning" | "log" | "info" | "debug" => level,
        _ => "log",
    }
}
