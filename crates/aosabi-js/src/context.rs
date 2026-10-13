// SPDX-License-Identifier: MIT
/*
 * aosabi-js context: the context of a request of the host in the engine and
 * the members of it the host calls.
 *
 * No analogue in the reference.
 */

//! The context of a request of the host.
//!
//! The host runs the routes, the middleware and the context `c` of an app in
//! its own hono and hands the engine the requests the engine answers, the
//! GraphQL endpoint and the routes of the plugins of the engine, with the
//! request the host opened in the engine before, where a middleware of a
//! plugin of the engine asked for it, a client of Sentry or the claims of a
//! provider of Zitadel. The engine keeps for each request the context its
//! code had in place, the context of the dispatch of its app or of the open
//! request, and the infos of the resolvers it called the app in.
//!
//! The export [`ctx`] answers the members of that context the host calls: the
//! opening of the request, its access line and its end, `c.get(key)` and the
//! names of `c.var` for a key the app did not set, which the host reads here
//! while the engine request runs, so the bag of `@inContext` and the info a
//! root resolver stores under `graphqlResolveInfo` read on the `c` of hono as
//! they read in a resolver, the store of `getResolveInfo()`, the result a done hook
//! of a plugin of the app sets, the views of `crate::nodes`, the claims of a
//! provider of `aosabi/auth/zitadel`, the client of Sentry of the request and a
//! delegation of a federation of the app. The body of the request stays in the
//! stream of the host, which the engine reads through [`host_stream`] once a
//! request parser of `graphql-over-http` reads it, so a body a route of hono
//! reads never crosses.

#![forbid(unsafe_code)]

use core::{
    cell::RefCell,
    pin::Pin,
    task::{
        Context as TaskContext,
        Poll, //
    },
};
use std::rc::Rc;

use aosabi_core::{glue::Thrown, in_context::InContext, resolve_info::ExecutionContext};
use aosabi_execute::{
    execute::ResolveInfo,
    promise::{
        Agent,
        AgentOptions,
        Drive,
        PromiseState, //
    },
    value::{
        Completion,
        HostValue,
        Value, //
    },
};
use aosabi_http::{
    BodyStream,
    Context,
    Stream,
    fetch::body::BodyError, //
};
use aosabi_json::{JsObject as JsonObject, JsString, JsValue};
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    abi::import,
    host::{
        self,
        Reader, //
    },
    request::BOOT,
    value::{
        self,
        NORMAL,
        THROW, //
    },
};

/// The message of a call of `c` whose request the engine holds no context of.
const NO_CONTEXT: &str = "Context not defined";

/// The message of a broken record of a call of `c`.
const BROKEN: &str = "the host called c with a broken record";

/// The ops of the record of the export `ctx`, the member of `c` it calls.
mod op {
    /// `c.get(key)` of a key the app did not set.
    pub const GET: u8 = 17;
    /// `getResolveInfo()`, the info of the call the number names.
    pub const RESOLVE_INFO: u8 = 25;
    /// `setResult(result)` of the payload of a done hook of a plugin of the
    /// app, the result the app sets for the call the number names.
    pub const SET_RESULT: u8 = 26;
    /// `c.var`, the names of the variables of the store of the engine in the
    /// order they were first set.
    pub const VAR_NAMES: u8 = 27;
    // The ops 28 to 31 are the views of `crate::nodes`, the op 33, the
    // claims of a provider of `aosabi/auth/zitadel`, is
    // `crate::zitadel::CLAIMS`, the op 34, a member of the client of Sentry
    // of the request, `crate::sentry::OP`, and the op 35, a delegation of a
    // federation of the app, `crate::federation::OP`.
    /// Opens the request in the engine before it answers it, with the record
    /// of the request after the op, `crate::request::open`.
    pub const OPEN: u8 = 36;
    /// Writes the access line of the request the host opened and ends it,
    /// each where the record asks for it, [`end`].
    pub const END: u8 = 37;
}

/// A chunk of a body of the host.
pub struct Chunk(pub Vec<u8>);

/// The context `c` of the request, which the host answers with the `c` of
/// its hono where a function of the app takes it.
pub struct ContextMark;

/// What the engine holds of a request of the host beside its entry of
/// `crate::request`.
struct Held {
    /// The request.
    request: u32,
    /// The context of the request, once the engine had it in place.
    context: Option<Context>,
    /// The infos of the resolvers the request called the app in, by their
    /// number less one.
    infos: Vec<Rc<ExecutionContext>>,
}

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The contexts of the requests in flight.
    static CONTEXTS: RefCell<Vec<Held>> = const { RefCell::new(Vec::new()) };
}

/// Runs `body` on what the engine holds of `request`, made at the first use.
fn with_held<R>(request: u32, body: impl FnOnce(&mut Held) -> R) -> R {
    CONTEXTS.with_borrow_mut(|held| {
        let at = held
            .iter()
            .position(|held| held.request == request)
            .unwrap_or_else(|| {
                held.push(Held {
                    request,
                    context: None,
                    infos: Vec::new(),
                });
                held.len() - 1
            });
        body(&mut held[at])
    })
}

/// Notes `context` as the context of `request`, unless one is noted already.
pub fn note_context(request: u32, context: &Context) {
    if request == BOOT {
        return;
    }
    with_held(request, |held| {
        if held.context.is_none() {
            held.context = Some(context.clone());
        }
    });
}

/// Notes `context` as the context of `request` in place of the one noted
/// before, the context of the dispatch of the app of an open request, whose
/// calls of `c` read the context the app runs in from then on.
pub fn replace_context(request: u32, context: &Context) {
    if request == BOOT {
        return;
    }
    with_held(request, |held| held.context = Some(context.clone()));
}

/// Notes `info`, the info of the resolver a call of `request` runs in, and
/// returns its number, counted from 1, the number of the info noted last
/// where it is the same.
pub fn note_info(request: u32, info: &Rc<ExecutionContext>) -> u32 {
    if request == BOOT {
        return 0;
    }
    with_held(request, |held| {
        if !held.infos.last().is_some_and(|last| Rc::ptr_eq(last, info)) {
            held.infos.push(Rc::clone(info));
        }
        host::length(held.infos.len())
    })
}

/// Returns the info `number` of `request`.
pub fn info_of(request: u32, number: u32) -> Option<Rc<ExecutionContext>> {
    CONTEXTS.with_borrow(|held| {
        let held = held.iter().find(|held| held.request == request)?;
        let index = usize::try_from(number).ok()?.checked_sub(1)?;
        held.infos.get(index).cloned()
    })
}

/// Returns the context noted for `request`.
pub fn context_of(request: u32) -> Option<Context> {
    CONTEXTS.with_borrow(|held| {
        held.iter()
            .find(|held| held.request == request)
            .and_then(|held| held.context.clone())
    })
}

/// Drops what the engine holds of `request` and of its views.
pub fn release(request: u32) {
    let gone = CONTEXTS.with_borrow_mut(|held| {
        held.iter()
            .position(|held| held.request == request)
            .map(|at| held.swap_remove(at))
    });
    drop(gone);
    crate::nodes::release(request);
}

/// Hands the app the value `result` of a call of the engine the app waits
/// on under the number `number` of `request`, through the import `js_next`:
/// the value, or the thrown value.
pub(crate) fn hand_value(request: u32, number: u32, result: &Result<Value, Thrown>) {
    let mut record = Vec::new();
    match result {
        Ok(value) => {
            record.push(NORMAL);
            host::encode(&mut record, value);
        }
        Err(thrown) => {
            record.push(THROW);
            host::encode(&mut record, &thrown.0);
        }
    }
    import::js_next(
        request,
        number,
        host::address(&mut record),
        host::length(record.len()),
    );
}

/// Returns the stream of the reader `handle` of the table of `request`, the
/// body of the request the host holds, pulled chunk by chunk through the
/// import `iter_next`.
pub fn host_stream(request: u32, handle: u32) -> BodyStream {
    BodyStream::new(HostStream {
        request,
        handle,
        agent: Agent::new(AgentOptions::default()),
        waiting: None,
        done: false,
    })
}

/// The body of a request of the host, pulled chunk by chunk from the reader
/// `handle` of the table of `request` through the import `iter_next`, each
/// chunk a promise of the host waited for on an agent of its own.
struct HostStream {
    /// The request whose table holds the reader.
    request: u32,
    /// The handle of the reader.
    handle: u32,
    /// The agent the promises of the reads wait on.
    agent: Agent,
    /// The read that waits on the host.
    waiting: Option<Drive>,
    /// Whether the stream has ended.
    done: bool,
}

impl Stream for HostStream {
    type Item = Result<Vec<u8>, BodyError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        if this.waiting.is_none() {
            let (request, handle) = (this.request, this.handle);
            let completion = this.agent.enter(|| value::pull(request, handle));
            match completion {
                Completion::Normal(Value::Promise(promise)) => {
                    this.waiting = Some(this.agent.drive(&promise));
                }
                Completion::Normal(value) => return Poll::Ready(this.chunk(&value)),
                Completion::Throw(thrown) => return Poll::Ready(Some(this.fail(&thrown))),
            }
        }
        let Some(waiting) = this.waiting.as_mut() else {
            return Poll::Ready(None);
        };
        match Pin::new(waiting).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(state) => {
                this.waiting = None;
                Poll::Ready(match state {
                    Ok(PromiseState::Fulfilled(value)) => this.chunk(&value),
                    Ok(PromiseState::Rejected(reason)) => Some(this.fail(&reason)),
                    Ok(PromiseState::Pending) | Err(_) => Some(this.fail(&Value::Undefined)),
                })
            }
        }
    }
}

impl HostStream {
    /// Returns the item of a read that answered `value`: its bytes, or the
    /// end of the stream for `null` and for any value that holds no bytes.
    fn chunk(&mut self, value: &Value) -> Option<Result<Vec<u8>, BodyError>> {
        if let Value::Host(HostValue::Opaque(opaque)) = value
            && let Some(chunk) = opaque.downcast_ref::<Chunk>()
        {
            return Some(Ok(chunk.0.clone()));
        }
        self.done = true;
        None
    }

    /// Returns the item of a read that threw or rejected with `reason`, the
    /// failure of the stream with the message of the reason, and ends it.
    fn fail(&mut self, reason: &Value) -> Result<Vec<u8>, BodyError> {
        self.done = true;
        let message = match reason {
            Value::Error(error) => to_string_lossy(&error.message()).into_owned(),
            other => to_string_lossy(&aosabi_execute::value::to_js_string(other)).into_owned(),
        };
        Err(BodyError::Failed(message))
    }
}

/// Answers the call of a member of `c` of `request` the record in the first
/// `len` bytes of the input buffer names, and returns the address of its
/// completion record in the output buffer.
///
/// The record of the completion is the byte of a normal completion, a byte
/// that says how its value is written, 0 for a value of the engine and
/// [`crate::nodes::VIEW`] for a view record, and the value, or the byte of a
/// throw completion and the thrown value.
pub fn ctx(request: u32, len: u32) -> usize {
    let record = host::input(len);
    let mut reader = Reader::new(&record);
    let completion = match record.first().copied() {
        Some(op::OPEN) => crate::request::open(request, &record[1..]).map(Answered::Value),
        Some(op::END) => {
            reader.byte();
            end(request, &mut reader)
                .map(|()| Answered::Value(Value::Undefined))
                .ok_or_else(|| Thrown::error(BROKEN))
        }
        Some(op::RESOLVE_INFO) => {
            reader.byte();
            Ok(resolve_info(request, &mut reader))
        }
        Some(code) if crate::nodes::answers(code) => crate::nodes::answer(request, &record)
            .map(Answered::View)
            .ok_or_else(|| Thrown::error(BROKEN)),
        Some(op::SET_RESULT) => {
            reader.byte();
            crate::plugin::set_result(&mut reader)
                .map(|()| Answered::Value(Value::Undefined))
                .ok_or_else(|| Thrown::error(BROKEN))
        }
        _ => match context_of(request) {
            None => Err(Thrown::error(NO_CONTEXT)),
            Some(c) => {
                call_ctx(request, &c, &mut reader).unwrap_or_else(|| Err(Thrown::error(BROKEN)))
            }
        },
    };
    let mut out = Vec::new();
    match completion {
        Ok(Answered::Value(value)) => {
            out.push(NORMAL);
            out.push(0);
            host::encode(&mut out, &value);
        }
        Ok(Answered::View(bytes)) => {
            out.push(NORMAL);
            out.push(crate::nodes::VIEW);
            out.extend_from_slice(&bytes);
        }
        Err(thrown) => {
            out.push(THROW);
            host::encode(&mut out, &thrown.0);
        }
    }
    host::output(&out)
}

/// What a call of `c` answers: a value of the engine or a view record of
/// `crate::nodes`.
enum Answered {
    /// A value, written for the host.
    Value(Value),
    /// A view record.
    View(Vec<u8>),
}

/// Writes the access line of the request `request` the host opened and ends
/// it, the op `END`, each where the record at the cursor of `reader` asks for
/// it, or answers `None` for a broken record.
///
/// The record is a byte 1 where the line is written, followed by the status of
/// the answer as four bytes and the milliseconds the request took as a number,
/// or a byte 0, and then a byte 1 where the request ends. The line is the
/// record `request` at `info` with those two fields, which the logger of the
/// request writes where the access line is on, as the access logger of the
/// base pipeline writes it once the rest of the chain has run, the step of the
/// request logger of the host noting the two fields and the end of the request
/// writing the line with the end, in one call.
fn end(request: u32, reader: &mut Reader<'_>) -> Option<()> {
    if reader.byte()? == 1 {
        let status = reader.word()?;
        let duration = reader.number()?;
        if let Some(raw) = crate::request::logger_of(request)
            && aosabi_core::logger::access_log_enabled()
        {
            let line = || {
                aosabi_core::logger::Logger::from_raw(raw).info(
                    "request",
                    aosabi_json::json!({ status: f64::from(status), durationMs: duration }),
                );
            };
            match context_of(request) {
                Some(context) => aosabi_core::context::run_with_context(context, line),
                None => line(),
            }
        }
    }
    if reader.byte()? == 1 {
        crate::request::close(request);
    }
    Some(())
}

/// Answers the call of `c` the record at the cursor of `reader` names over
/// the context `c` of `request`, or `None` for a broken record.
fn call_ctx(
    request: u32,
    c: &Context,
    reader: &mut Reader<'_>,
) -> Option<Result<Answered, Thrown>> {
    let code = reader.byte()?;
    let answered = match code {
        op::GET => return Some(Ok(get_var(request, c, reader.text()?))),
        op::VAR_NAMES => js(&JsValue::Array(
            c.var_names().into_iter().map(JsValue::from).collect(),
        )),
        crate::zitadel::CLAIMS => {
            return crate::zitadel::start(request, c, reader)
                .map(|claims| claims.map(Answered::Value));
        }
        crate::sentry::OP => {
            return crate::sentry::call(request, c, reader)
                .map(|answer| answer.map(Answered::Value));
        }
        crate::federation::OP => {
            return crate::federation::start(request, c, reader)
                .map(|answer| answer.map(Answered::Value));
        }
        _ => return None,
    };
    Some(Ok(Answered::Value(answered)))
}

/// Answers `c.get(key)` of `request` for a key the app did not set on the
/// `c` of its hono: a string a Rust key set, a value of the engine, the view
/// of the info a root resolver stored under `graphqlResolveInfo`, the bag of
/// `@inContext`, and `undefined` for a variable the store does not hold or
/// holds as a value no record writes.
fn get_var(request: u32, c: &Context, key: &str) -> Answered {
    let Some(held) = c.var(key) else {
        return Answered::Value(Value::Undefined);
    };
    if let Some(info) = held.downcast_ref::<Rc<ResolveInfo>>() {
        return Answered::View(crate::nodes::info_view(request, info));
    }
    if let Some(text) = held.downcast_ref::<String>() {
        return Answered::Value(js(&JsValue::from(text.as_str())));
    }
    if let Some(value) = held.downcast_ref::<Value>() {
        return Answered::Value(value.clone());
    }
    if let Some(value) = held.downcast_ref::<JsValue>() {
        return Answered::Value(Value::from_js(value));
    }
    if let Some(in_context) = held.downcast_ref::<InContext>() {
        let mut object = JsonObject::new();
        if let Some(locale) = &in_context.locale {
            object.insert(JsString::from("locale"), JsValue::from(locale.as_str()));
        }
        if let Some(bag) = &in_context.context {
            object.insert(JsString::from("context"), bag.clone());
        }
        return Answered::Value(js(&JsValue::Object(object)));
    }
    Answered::Value(Value::Undefined)
}

/// Answers `getResolveInfo()` for the info whose number follows at the cursor
/// of `reader`, the view record of the store of
/// `executionAsyncContext`, its info and its selected fields, or `undefined`
/// for a call outside a resolver.
// Out of line, so a request compiles the view of the resolve info where it reads it.
#[inline(never)]
fn resolve_info(request: u32, reader: &mut Reader<'_>) -> Answered {
    let Some((number, context)) = reader
        .word()
        .and_then(|number| Some((number, info_of(request, number)?)))
    else {
        return Answered::Value(Value::Undefined);
    };
    Answered::View(crate::nodes::call_view(request, number, &context))
}

/// Returns the value of the engine of the JSON value `value`.
fn js(value: &JsValue) -> Value {
    Value::from_js(value)
}
