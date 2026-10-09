// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the HTTP executor a configured federation sends its
 * requests through.
 *
 * Derived from @graphql-tools/executor-http 3.1.0, dist/index.cjs, MIT.
 */

//! The HTTP executor, `build_http_executor` and the requests in flight it
//! shares.
//!
//! The explicit headers are `accept`, then the headers the headers function
//! answers in their order, then `content-type` for a text body that no
//! header of that name set, and the body is the JSON text of [`serialize`]
//! or the form of [`form_data`]. The headers function takes the request, so
//! the introspection, whose request carries no context, hands the
//! federation's function none (quirk Q77). The headers leave as the lines
//! node:http writes for that object, and the fetch follows the redirects the
//! fetch of the reference follows, both in [`transport`].
//!
//! Two requests with the same key, `<url>|<method>|<body>` followed by the
//! JSON of the headers when a headers function is configured, that overlap
//! in time share one fetch and one result, the entry leaves the map of the
//! thread when its result settles, and a form body is never shared. The map
//! is per runtime thread, which is per process for a served card with its
//! one runtime thread, so two federations over one URL share it as the
//! module wide map of the reference does.
//!
//! The answer is read as text whole. An empty text answers
//! `No response returned`, a text that is no JSON
//! `Unexpected response: <JSON of the text>` and a JSON value with no `data`
//! and no `errors`
//! `Unexpected empty "data" and "errors" fields in result: <text>`, each
//! with the code `RESPONSE_VALIDATION_FAILED`, and a failed fetch answers
//! the message of its rejection with the code `SUBREQUEST_HTTP_ERROR`
//! through [`errors::coerce_fetch_error`],
//! `connect ECONNREFUSED <address>:<port>` for a refused connection and
//! `fetch failed to <endpoint>` for any other failure of the capability
//! ([`errors::fetch_rejection`]). The extensions of each are
//! `{request: {method, body}, response: {status, statusText, body}, code}`
//! in that key order with the absent members left out, the response headers
//! are never written into them, the status is read for a redirect alone, and
//! no timeout of the executor's own is set but the bound of a chain of
//! redirects, the bound of the capability being the bound of each request.
//! The `errors` of a JSON answer are rebuilt as `GraphQLError`s from their
//! message, their path and their extensions, and the `locations` a remote
//! writes is not taken, as the constructor of graphql 16 does not take it.
//!
//! The options `fetch`, `method`, `useGETForQueries`, `timeout`, `apq`,
//! `retry`, `credentials`, `print`, `File`, `FormData` and
//! `deduplicateInflightRequests` and the members `endpoint`, `method`,
//! `fetch`, `headers` and `useGETForQueries` of the request's extensions
//! are never set by the federation, so the branches they open are left
//! out, which is the one entry of `buildHTTPExecutor` in the `NOT_PORTED`
//! register of `aosabi-compat`. No abort signal is read, since the requests of a delegation
//! carry none and the executor has no dispose controller a caller can reach.

pub mod errors;
pub mod form_data;
pub mod serialize;
pub mod transport;

use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        Mutex,
        PoisonError, //
    },
    task::{
        Context,
        Poll,
        Wake,
        Waker, //
    }, //
};

use aosabi_execute::value::{
    Completion,
    ErrorObject,
    ObjectLike,
    PlainObject,
    Value, //
};
use aosabi_http::LocalBoxFuture;
use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLErrorOptions,
    OriginalError, //
};

use crate::{
    delegate::subschema::Executor,
    executor_http::{
        errors::coerce_fetch_error,
        form_data::{
            RequestBody,
            create_form_data_from_variables, //
        },
        serialize::serialize_execution_request,
        transport::Outbound, //
    },
    headers::Headers,
    transforms::ExecutionRequest,
    utils::{
        create_graphql_error,
        create_graphql_error_from_object,
        get_operation_ast_from_request,
        is_nullish, //
    }, //
};

/// The `accept` header of every request the executor sends.
pub const ACCEPT: &str = "application/graphql-response+json, application/json, multipart/mixed";

/// The code of an answer the executor refuses, `RESPONSE_VALIDATION_FAILED`.
pub const RESPONSE_VALIDATION_FAILED: &str = "RESPONSE_VALIDATION_FAILED";

/// The code of a request that failed below GraphQL, `SUBREQUEST_HTTP_ERROR`.
pub const SUBREQUEST_HTTP_ERROR: &str = "SUBREQUEST_HTTP_ERROR";

/// A headers function of the executor, called with the request it sends.
pub type HttpHeadersFn = Rc<dyn Fn(&ExecutionRequest) -> Headers>;

/// The options of `buildHTTPExecutor` the gateway passes, `{endpoint, headers}`.
#[derive(Clone)]
pub struct HttpExecutorOptions {
    /// The URL every request goes to, `endpoint`, `/graphql` when empty.
    pub endpoint: String,
    /// The function that answers the headers of a request, `headers`.
    pub headers: Option<HttpHeadersFn>,
}

thread_local! {
    /// The requests in flight by their key, the map `inflightRequests`.
    static INFLIGHT_REQUESTS: RefCell<IndexMap<String, Rc<InFlight>, KeyHasherBuilder>> =
        RefCell::new(IndexMap::default());
}

/// Returns the HTTP executor of `options`, which sends through the network capability.
///
/// The executor throws the refusal of [`get_operation_ast_from_request`] at
/// once, and answers every other request with a promise of its result, which
/// carries a failure of the request as its `errors`.
#[must_use]
pub fn build_http_executor(options: HttpExecutorOptions) -> Executor {
    let options = Rc::new(options);
    Rc::new(
        move |agent, request| match base_executor(&options, &request) {
            Ok(result) => Completion::Normal(Value::Promise(
                agent.spawn(Box::pin(async move { Completion::Normal(result.await) })),
            )),
            Err(error) => Completion::Throw(error),
        },
    )
}

/// The extensions every error of one request carries, `upstreamErrorExtensions`.
#[derive(Clone)]
struct Upstream {
    /// The member `request`, `{method, body?}`.
    request: JsObject,
    /// The member `response`, `{status, statusText, body?}`, once a response arrived.
    response: Option<JsObject>,
    /// The member `code`, once a failure set it.
    code: Option<&'static str>,
}

impl Upstream {
    /// Returns the extensions object in the key order the reference fills it in.
    fn to_object(&self) -> JsObject {
        let mut extensions = JsObject::with_capacity(3);
        extensions.insert("request", JsValue::Object(self.request.clone()));
        if let Some(response) = &self.response {
            extensions.insert("response", JsValue::Object(response.clone()));
        }
        if let Some(code) = self.code {
            extensions.insert("code", code);
        }
        extensions
    }

    /// Sets the member `body` of the response, `upstreamErrorExtensions.response.body`.
    fn set_response_body(&mut self, body: JsValue) {
        self.response
            .get_or_insert_with(JsObject::new)
            .insert("body", body);
    }
}

/// Builds one request and answers the future of its result, the body of `baseExecutor`.
///
/// # Errors
///
/// Returns the refusal of [`get_operation_ast_from_request`], which the
/// reference throws before anything else.
fn base_executor(
    options: &HttpExecutorOptions,
    request: &ExecutionRequest,
) -> Result<LocalBoxFuture<'static, Value>, Value> {
    get_operation_ast_from_request(request)?;
    let method = "POST";
    let endpoint = if options.endpoint.is_empty() {
        String::from("/graphql")
    } else {
        options.endpoint.clone()
    };
    let is_custom_header = options.headers.is_some();
    let mut headers = Headers::new();
    headers.set("accept", ACCEPT);
    if let Some(headers_fn) = &options.headers {
        for (name, value) in headers_fn(request).iter() {
            headers.set(name, value);
        }
    }
    let mut upstream = Upstream {
        request: JsObject::new(),
        response: None,
        code: None,
    };
    upstream.request.insert("method", method);
    let body = match serialize_execution_request(request, false, None) {
        Ok(body) => body,
        Err(error) => return Ok(ready(handle_error(&error, &endpoint, &mut upstream))),
    };
    upstream.request.insert("body", body.to_js_value());
    let body = create_form_data_from_variables(&body);
    if let RequestBody::Json(text) = &body
        && headers.get("content-type").is_none_or(str::is_empty)
    {
        upstream.request.insert("body", text.as_str());
        headers.set("content-type", "application/json");
    }
    Ok(handle_inflight_request(InflightRequest {
        url: endpoint,
        method,
        body,
        headers,
        upstream,
        is_custom_header,
    }))
}

/// What one request in flight carries, `inflightRequestOptions` with the state of its closure.
struct InflightRequest {
    /// The URL, `url`.
    url: String,
    /// The method, `method`.
    method: &'static str,
    /// The body, a text or a form.
    body: RequestBody,
    /// The explicit headers in their order.
    headers: Headers,
    /// The extensions of its errors.
    upstream: Upstream,
    /// Whether a headers function is configured, `isCustomHeader`.
    is_custom_header: bool,
}

/// Answers the result of a request, shared with an identical request in flight.
///
/// A form body runs on its own. Any other body looks its key up in the map,
/// joins the request found there, or runs and enters the map until its
/// result settles.
fn handle_inflight_request(inflight: InflightRequest) -> LocalBoxFuture<'static, Value> {
    let RequestBody::Json(body) = &inflight.body else {
        return Box::pin(run_inflight_request(inflight));
    };
    let mut inflight_request_id = format!("{}|{}", inflight.url, inflight.method);
    if !body.is_empty() {
        inflight_request_id.push('|');
        inflight_request_id.push_str(body);
    }
    if inflight.is_custom_header {
        inflight_request_id.push('|');
        inflight_request_id.push_str(&headers_json(&inflight.headers));
    }
    let shared = INFLIGHT_REQUESTS.with(|map| {
        let mut map = map.borrow_mut();
        if let Some(shared) = map.get(&inflight_request_id) {
            return Rc::clone(shared);
        }
        let shared = Rc::new(InFlight::new(
            inflight_request_id.clone(),
            Box::pin(run_inflight_request(inflight)),
        ));
        map.insert(inflight_request_id, Rc::clone(&shared));
        shared
    });
    Box::pin(SharedResult(shared))
}

/// Returns `JSON.stringify(headers)`, the headers as an object in their order.
fn headers_json(headers: &Headers) -> String {
    let mut object = JsObject::with_capacity(headers.len());
    for (name, value) in headers.iter() {
        object.insert(name, value);
    }
    aosabi_json::to_string(&JsValue::Object(object)).unwrap_or_default()
}

/// Sends one request and reads its answer, `runInflightRequest`.
///
/// A fetch that fails, a header node:http refuses and a body that breaks
/// while it is read go to [`handle_error`], and the text that arrives goes
/// to [`read_result`]. The answer is the one the last redirect of the fetch
/// led to.
async fn run_inflight_request(inflight: InflightRequest) -> Value {
    let InflightRequest {
        url,
        method,
        body,
        headers,
        mut upstream,
        is_custom_header: _,
    } = inflight;
    let outbound = match Outbound::new(method, &body, &headers) {
        Ok(outbound) => outbound,
        Err(error) => return handle_error(&error, &url, &mut upstream),
    };
    let fetch_result = match outbound.send(&url).await {
        Ok(fetch_result) => fetch_result,
        Err(error) => return handle_error(&error, &url, &mut upstream),
    };
    let response = upstream.response.get_or_insert_with(JsObject::new);
    response.insert("status", JsValue::Number(f64::from(fetch_result.status())));
    response.insert("statusText", fetch_result.status_text().as_str());
    match fetch_result.text().await {
        Ok(text) => read_result(&text, &mut upstream),
        Err(error) => handle_error(
            &Value::from(ErrorObject::type_error(error.to_string())),
            &url,
            &mut upstream,
        ),
    }
}

/// Returns the result of a text the remote answered.
///
/// The reference sets the body of the response's extensions to the text and
/// then to the JSON value the text is, both by reference, and reads `data`
/// and `errors` in place. Only a refusal reads those extensions, so each
/// refusal sets the body it reads, the text before the parse and the parsed
/// value after it, and an answer the remote makes copies neither, as the
/// reference copies neither.
fn read_result(result: &str, upstream: &mut Upstream) -> Value {
    if result.is_empty() {
        upstream.set_response_body(JsValue::from(result));
        let message = "No response returned";
        upstream.code = Some(RESPONSE_VALIDATION_FAILED);
        return errors_result(vec![refusal(message, upstream, plain_original(message))]);
    }
    let parsed_result = match aosabi_json::parse(result.as_bytes()) {
        Ok(parsed_result) => parsed_result,
        Err(error) => {
            upstream.set_response_body(JsValue::from(result));
            return unexpected_response(result, upstream, "SyntaxError", &error.to_string());
        }
    };
    if matches!(parsed_result, JsValue::Null) {
        upstream.set_response_body(JsValue::Null);
        return unexpected_response(
            result,
            upstream,
            "TypeError",
            "Cannot read properties of null (reading 'data')",
        );
    }
    let data = js_member(&parsed_result, "data");
    let errors = js_member(&parsed_result, "errors");
    if is_nullish(data) && (is_nullish(errors) || js_length_is_zero(errors)) {
        let message =
            format!("Unexpected empty \"data\" and \"errors\" fields in result: {result}");
        upstream.set_response_body(parsed_result.clone());
        upstream.code = Some(RESPONSE_VALIDATION_FAILED);
        return errors_result(vec![refusal(&message, upstream, plain_original(&message))]);
    }
    let (JsValue::Array(remote_errors), JsValue::Object(object)) = (errors, &parsed_result) else {
        return Value::from_js(&parsed_result);
    };
    let mut rebuilt = Vec::with_capacity(remote_errors.len());
    for remote_error in remote_errors {
        if is_nullish(remote_error) {
            let kind = if matches!(remote_error, JsValue::Null) {
                "null"
            } else {
                "undefined"
            };
            upstream.set_response_body(parsed_result.clone());
            return unexpected_response(
                result,
                upstream,
                "TypeError",
                &format!("Cannot destructure property 'message' of 'object null' as it is {kind}."),
            );
        }
        rebuilt.push(remote_graphql_error(remote_error));
    }
    let entries = object
        .own_keys()
        .map(|(key, value)| {
            let key = String::from_utf8_lossy(key.as_wtf8()).into_owned();
            let value = if key == "errors" {
                Value::from(core::mem::take(&mut rebuilt))
            } else {
                Value::from_js(value)
            };
            (key, value)
        })
        .collect();
    Value::Object(PlainObject::from_entries(entries) as Rc<dyn ObjectLike>)
}

/// Returns the result of a text that does not read as a JSON result, `Unexpected response: <JSON of the text>`.
///
/// `name` and `message` are those of the error the reading threw, which
/// becomes the original error of the refusal.
fn unexpected_response(result: &str, upstream: &mut Upstream, name: &str, message: &str) -> Value {
    upstream.code = Some(RESPONSE_VALIDATION_FAILED);
    let quoted = aosabi_json::to_string(&JsValue::from(result)).unwrap_or_default();
    let original = OriginalError::Other(Box::new(aosabi_execute::errors::ThrownError {
        name: JsString::from(name),
        message: JsString::from(message),
        host: None,
    }));
    errors_result(vec![refusal(
        &format!("Unexpected response: {quoted}"),
        upstream,
        original,
    )])
}

/// Returns the `GraphQLError` of a refusal with the extensions of the request and `original_error`.
fn refusal(message: &str, upstream: &Upstream, original_error: OriginalError) -> Value {
    Value::from(ErrorObject::GraphQL(create_graphql_error(
        message,
        GraphQLErrorOptions {
            extensions: Some(upstream.to_object()),
            original_error: Some(original_error),
            ..GraphQLErrorOptions::default()
        },
    )))
}

/// Returns the original error `new Error(message)`.
fn plain_original(message: &str) -> OriginalError {
    OriginalError::Other(Box::new(aosabi_execute::errors::ThrownError {
        name: JsString::from("Error"),
        message: JsString::from(message),
        host: None,
    }))
}

/// Returns the result whose one member is `errors`, `{errors}`.
fn errors_result(errors: Vec<Value>) -> Value {
    Value::Object(
        PlainObject::from_entries(vec![(String::from("errors"), Value::from(errors))])
            as Rc<dyn ObjectLike>,
    )
}

/// Returns the result of a request that failed below GraphQL, `handleError`.
///
/// The code becomes `SUBREQUEST_HTTP_ERROR`, and an `AggregateError` answers
/// one error per error it holds.
fn handle_error(e: &Value, endpoint: &str, upstream: &mut Upstream) -> Value {
    upstream.code = Some(SUBREQUEST_HTTP_ERROR);
    let extensions = upstream.to_object();
    if let Value::Error(error) = e
        && let ErrorObject::Aggregate { errors, .. } = error.as_ref()
    {
        return errors_result(
            errors
                .iter()
                .map(|e2| coerce_fetch_error(e2, None, endpoint, Some(&extensions)))
                .collect(),
        );
    }
    errors_result(vec![coerce_fetch_error(
        e,
        None,
        endpoint,
        Some(&extensions),
    )])
}

/// Returns `createGraphQLError(message, options)` of one error of a remote's answer.
fn remote_graphql_error(remote_error: &JsValue) -> Value {
    create_graphql_error_from_object(&Value::from_js(remote_error))
}

/// Returns `value[key]` of a JSON value, an own member of an object and `undefined` otherwise, in place.
fn js_member<'v>(value: &'v JsValue, key: &str) -> &'v JsValue {
    match value {
        JsValue::Object(object) => object.get(key).unwrap_or(&JsValue::Undefined),
        _ => &JsValue::Undefined,
    }
}

/// Returns `value.length === 0`, which holds for an empty array, an empty string and an object whose `length` is 0.
fn js_length_is_zero(value: &JsValue) -> bool {
    match value {
        JsValue::Array(items) => items.is_empty(),
        JsValue::String(text) => text.is_empty(),
        JsValue::Object(object) => {
            matches!(object.get("length"), Some(JsValue::Number(length)) if *length == 0.0)
        }
        _ => false,
    }
}

/// Returns a future that answers `value` at once.
fn ready(value: Value) -> LocalBoxFuture<'static, Value> {
    Box::pin(core::future::ready(value))
}

/// The wakers of every request that waits on one request in flight.
// An `unsafe` `Sync` cell would skip the lock.
#[derive(Default)]
struct Waiters(Mutex<Vec<Waker>>);

impl Waiters {
    /// Adds `waker` unless a waker that wakes the same task is in already.
    fn register(&self, waker: &Waker) {
        let mut wakers = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if !wakers.iter().any(|known| known.will_wake(waker)) {
            wakers.push(waker.clone());
        }
    }
}

impl Wake for Waiters {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let wakers = core::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner));
        for waker in wakers {
            waker.wake();
        }
    }
}

/// The state of a request in flight.
enum InFlightState {
    /// The fetch and the reading of its answer run.
    Running(LocalBoxFuture<'static, Value>),
    /// The future is out for a poll further up the stack.
    Polling,
    /// The result settled.
    Done(Value),
}

/// One request in flight that every identical request joins, the promise the map of the reference holds.
struct InFlight {
    /// The key of the request in the map.
    key: String,
    /// The future or its result.
    state: RefCell<InFlightState>,
    /// The tasks that wait on the result.
    waiters: Arc<Waiters>,
}

impl InFlight {
    /// Returns the request in flight of `future` under `key`.
    fn new(key: String, future: LocalBoxFuture<'static, Value>) -> Self {
        Self {
            key,
            state: RefCell::new(InFlightState::Running(future)),
            waiters: Arc::new(Waiters::default()),
        }
    }

    /// Polls the request for one waiter, which any waiter may drive.
    ///
    /// The future is polled with a waker that wakes every waiter, so the
    /// request moves on while any of them is alive. The result leaves the
    /// map once it settles, the `finally` of the reference.
    fn poll_for(self: &Rc<Self>, cx: &mut Context<'_>) -> Poll<Value> {
        let taken = core::mem::replace(&mut *self.state.borrow_mut(), InFlightState::Polling);
        let mut future = match taken {
            InFlightState::Done(value) => {
                let answer = value.clone();
                self.state.replace(InFlightState::Done(value));
                return Poll::Ready(answer);
            }
            InFlightState::Polling => {
                self.state.replace(InFlightState::Polling);
                self.waiters.register(cx.waker());
                return Poll::Pending;
            }
            InFlightState::Running(future) => future,
        };
        self.waiters.register(cx.waker());
        let waker = Waker::from(Arc::clone(&self.waiters));
        let mut inner = Context::from_waker(&waker);
        match future.as_mut().poll(&mut inner) {
            Poll::Ready(value) => {
                self.state.replace(InFlightState::Done(value.clone()));
                INFLIGHT_REQUESTS.with(|map| {
                    let mut map = map.borrow_mut();
                    if map
                        .get(&self.key)
                        .is_some_and(|known| Rc::ptr_eq(known, self))
                    {
                        map.shift_remove(&self.key);
                    }
                });
                self.waiters.wake_by_ref();
                Poll::Ready(value)
            }
            Poll::Pending => {
                self.state.replace(InFlightState::Running(future));
                Poll::Pending
            }
        }
    }
}

/// The result of a request in flight as one waiter awaits it.
struct SharedResult(Rc<InFlight>);

impl Future for SharedResult {
    type Output = Value;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Value> {
        self.0.poll_for(cx)
    }
}
