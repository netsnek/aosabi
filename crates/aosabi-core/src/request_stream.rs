// SPDX-License-Identifier: MIT
/*
 * aosabi-core request_stream: the request lines a twin answers without a socket.
 *
 * No analogue in the reference.
 */

//! The request stream of a twin.
//!
//! A judging notebook compares what a Rust twin answers with what the built
//! TypeScript fixture answers, below HTTP. The twin binary therefore reads a
//! file of request lines and writes one response line per request. A request
//! is one JSON object with an `id`, the `op` `execute`, a `document` and
//! optionally `variables`, `operationName`, `validate` and `trace`. The
//! response carries `id` first, `op` second and then `result`, the result
//! text as a string, or `thrown`, the name and the message of what the entry
//! threw, and `trace` last when the request asked for it. A request that
//! never reaches the service answers `fatal`.
//!
//! Three operations beside `execute` run a subscription through
//! [`Execute`] of the service, on its own agent with virtual timers, named by
//! a handle the stream gives out, `h1`, `h2` and on.
//!
//! | `op` | Request fields | Response payload |
//! | --- | --- | --- |
//! | `subscribe` | `document`, optional `variables`, `operationName`, `carrier` (`network` when absent), `carrier_allow` (a list of field names, all fields when absent) and `depth` (1024 when absent) | `{"handle":"h1"}` for a stream, `{"result":"<text>"}` for a single result |
//! | `next` | `handle`, optional `wait_ms` (0 when absent) | `{"result":"<text>","revision":<n>}` with `revision` present when the pump noted one, `{"pending":true}` after `wait_ms` with no result, or `{"done":true}` |
//! | `return` | `handle` | `{"done":true}` |
//!
//! The carrier is named in snake case, `network`, `unix_socket`, `usb`,
//! `bluetooth`, `serial_line` or `lora`. A `next` with `wait_ms` parks the
//! reading thread on a waker that unparks it, so a publish of another thread
//! reaches a waiting `next`, and no thread is spawned. `Allow::Only` takes a
//! `'static` list, so the stream leaks the list it parses once per
//! `subscribe` line, a cost a test binary carries and a served binary never
//! meets, because no served path parses an allowlist.
//!
//! Every request runs on a fresh agent with virtual timers, so the clock of
//! each request starts at zero. The agent of an `execute` runs until it is
//! idle after its answer, so a timer a resolver set fires before the next
//! line is read, as node fires it after the answer. There is no socket and
//! no HTTP type here.

use core::{
    pin::Pin,
    task::{
        Context,
        Poll,
        Waker, //
    },
    time::Duration, //
};
use std::{
    env,
    fs,
    io::{
        self,
        Write, //
    },
    process::ExitCode,
    sync::Arc,
    task::Wake,
    thread::{
        self,
        Thread, //
    },
    time::Instant, //
};

use aosabi_execute::{
    kernel_entry::{
        Executed,
        Request,
        RunFailure, //
    },
    promise::{
        AgentOptions,
        TimerMode,
        TraceEvent, //
    },
    result::ExecutionResult,
    value::Value, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    reader, //
};
use aosabi_kernel::execute_closed::{
    Execute,
    Operation,
    ResultStream, //
};
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    carrier::{
        Allow,
        CarrierId,
        CarrierPolicy, //
    },
    service::{
        Service,
        ServiceStream, //
    }, //
};

/// The `fatal` of a request whose run ends with nothing left to wait for.
pub(crate) const STALLED: &str =
    "the request stalled: no job, no timer and no waker is left and the result is pending";

/// The operation that runs a query or a mutation to its one result.
const OP_EXECUTE: &str = "execute";

/// The operation that starts a subscription.
const OP_SUBSCRIBE: &str = "subscribe";

/// The operation that pulls the next result of a subscription.
const OP_NEXT: &str = "next";

/// The operation that ends a subscription.
const OP_RETURN: &str = "return";

/// The operations of the protocol.
const OPS: [&str; 4] = [OP_EXECUTE, OP_SUBSCRIBE, OP_NEXT, OP_RETURN];

/// The exit code of a wrong call, `EX_USAGE` of sysexits.h.
const EX_USAGE: u8 = 64;

/// The exit code of a request file that cannot be read, `EX_NOINPUT`.
const EX_NOINPUT: u8 = 66;

/// The exit code of a standard output that cannot be written, `EX_IOERR`.
const EX_IOERR: u8 = 74;

/// Answers every request line of `requests`, one response line per request.
///
/// A blank line draws no response. Every response line ends in a newline.
/// A line that cannot be answered is never a failure of the call, it is a
/// `fatal` response line.
///
/// The subscriptions a file starts live until their `return` or the end of
/// the file, whichever comes first, and a stream still open at the end is
/// closed, which calls `return` on its iterator.
#[must_use]
pub fn answer_requests(service: &Service, requests: &[u8]) -> Vec<u8> {
    let mut streams = Streams::default();
    let mut out = Vec::new();
    for line in requests.split(|&byte| byte == b'\n') {
        if line.trim_ascii().is_empty() {
            continue;
        }
        let response = answer(service, &mut streams, line);
        aosabi_json::write(&mut out, &JsValue::Object(response));
        out.push(b'\n');
    }
    out
}

/// Runs the binary of a twin over `service`.
///
/// With no argument the schema text is printed, which is what a twin printed
/// before it could execute. With `--requests PATH` the request lines of the
/// file are answered on standard output. Any other call is a usage error.
#[must_use]
pub fn main_with(service: &Service) -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => emit(service.type_defs().as_bytes()),
        ["--requests", path] => match fs::read(path) {
            Ok(requests) => emit(&answer_requests(service, &requests)),
            Err(error) => {
                eprintln!("{path}: {error}");
                ExitCode::from(EX_NOINPUT)
            }
        },
        _ => {
            eprintln!("usage: [--requests PATH]");
            ExitCode::from(EX_USAGE)
        }
    }
}

/// Writes `bytes` to standard output.
fn emit(bytes: &[u8]) -> ExitCode {
    let mut stdout = io::stdout().lock();
    match stdout.write_all(bytes).and_then(|()| stdout.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(EX_IOERR),
    }
}

/// Returns the response object for one request line.
///
/// The order of the refusals is the order of `answer` in the oracle: a line
/// that is not JSON, a line that is not an object, an unknown `op`, then a
/// missing `id`.
fn answer(service: &Service, streams: &mut Streams, line: &[u8]) -> JsObject {
    let request = match reader::parse(line) {
        Ok(JsValue::Object(request)) => request,
        Ok(_) => {
            let fatal = "the request line is not a JSON object";
            return respond(JsValue::Null, JsValue::Null, Err(JsString::from(fatal)));
        }
        Err(error) => {
            let fatal = format!("the request line is not JSON: {error}");
            return respond(JsValue::Null, JsValue::Null, Err(JsString::from(fatal)));
        }
    };
    let id = request.get("id").cloned().unwrap_or(JsValue::Null);
    let op = request.get("op");
    let Some(name) = OPS
        .into_iter()
        .find(|name| matches!(op, Some(JsValue::String(op)) if *op == *name))
    else {
        let fatal = JsString::from(unknown_op(op));
        let op = match op {
            Some(JsValue::String(op)) => JsValue::String(op.clone()),
            _ => JsValue::Null,
        };
        return respond(id, op, Err(fatal));
    };
    let op = JsValue::from(name);
    if !request.contains_key("id") {
        return respond(id, op, Err(JsString::from("the request carries no id")));
    }
    let payload = match name {
        OP_SUBSCRIBE => subscribe(service, streams, &request),
        OP_NEXT => next(streams, &request),
        OP_RETURN => close(streams, &request),
        _ => execute(service, &request),
    };
    respond(id, op, payload)
}

/// Returns the response with `id` first, `op` second and the payload after.
fn respond(id: JsValue, op: JsValue, outcome: Result<JsObject, JsString>) -> JsObject {
    let mut response = JsObject::new();
    response.insert("id", id);
    response.insert("op", op);
    match outcome {
        Ok(payload) => {
            for (key, value) in &payload {
                response.insert(key.clone(), value.clone());
            }
        }
        Err(fatal) => {
            response.insert("fatal", fatal);
        }
    }
    response
}

/// Returns the `fatal` of an `op` the stream does not answer.
///
/// The oracle prints the `op` through `JSON.stringify`, which answers
/// `undefined` for a missing key, and a template literal prints that as the
/// word.
fn unknown_op(op: Option<&JsValue>) -> String {
    let printed = op
        .and_then(aosabi_json::to_string)
        .unwrap_or_else(|| String::from("undefined"));
    format!("unknown op {printed}")
}

/// Runs the operation `execute` and returns its payload.
///
/// The agent of the request runs until it is idle after the answer, so a
/// detached timer of a resolver fires before the next line, as
/// `bumpLater` of `pubsub-app` needs.
///
/// # Errors
///
/// Returns the `fatal` of a request without a `document` string, with an
/// `operationName` that is no string, and of a run that stalls.
fn execute(service: &Service, request: &JsObject) -> Result<JsObject, JsString> {
    let Some(JsValue::String(document)) = request.get("document") else {
        return Err(JsString::from(
            "execute: the request carries no string field document",
        ));
    };
    let operation_name = match request.get("operationName") {
        None | Some(JsValue::Null | JsValue::Undefined) => None,
        Some(JsValue::String(name)) => Some(to_string_lossy(name)),
        Some(_) => return Err(JsString::from("execute: operationName is no string")),
    };
    let trace = flag(request, "trace", false);
    // A document text reaches the pipeline as a Rust string, so a lone
    // surrogate in it reads as the replacement character.
    let document = to_string_lossy(document);
    let run = Request {
        document: &document,
        variables: request.get("variables").cloned(),
        operation_name: operation_name.as_deref(),
        validate: flag(request, "validate", true),
    };
    let options = AgentOptions {
        timers: TimerMode::Virtual,
        trace,
    };
    payload(service.execute_drained(&run, options), trace)
}

/// Returns the boolean field `name` of `request`, `default` when it is absent.
fn flag(request: &JsObject, name: &str, default: bool) -> bool {
    match request.get(name) {
        Some(JsValue::Bool(value)) => *value,
        _ => default,
    }
}

/// The subscriptions of one request file, a slot per handle, emptied by `return`.
#[derive(Default)]
struct Streams(Vec<Option<ServiceStream>>);

impl Streams {
    /// Keeps `stream` and returns its handle, `h1` for the first.
    fn insert(&mut self, stream: ServiceStream) -> String {
        self.0.push(Some(stream));
        format!("h{}", self.0.len())
    }

    /// Returns the open stream of the handle field of `request`.
    ///
    /// # Errors
    ///
    /// Returns the `fatal` of a request without a handle string and of a
    /// handle the stream did not give out or that was returned.
    fn get(&mut self, op: &str, request: &JsObject) -> Result<&mut ServiceStream, JsString> {
        let index = handle_index(op, request)?;
        self.0
            .get_mut(index)
            .and_then(Option::as_mut)
            .ok_or_else(|| JsString::from(format!("{op}: the handle names no open stream")))
    }
}

/// Returns the index of the handle field of `request`, `h1` being 0.
fn handle_index(op: &str, request: &JsObject) -> Result<usize, JsString> {
    let index = match request.get("handle") {
        Some(JsValue::String(handle)) => to_string_lossy(handle)
            .strip_prefix('h')
            .and_then(|number| number.parse::<usize>().ok())
            .and_then(|number| number.checked_sub(1)),
        _ => None,
    };
    index.ok_or_else(|| JsString::from(format!("{op}: the request carries no handle h1, h2 or on")))
}

/// Runs the operation `subscribe` and returns its payload.
///
/// The subscription runs through [`Execute`] of the service under the
/// policy the request names. A stream is kept under a fresh handle, and a
/// single result answers as `execute` answers one.
///
/// # Errors
///
/// Returns the `fatal` of a request without a `document` string, with an
/// `operationName` that is no string, and with a carrier, an allowlist or a
/// depth the stream does not read.
fn subscribe(
    service: &Service,
    streams: &mut Streams,
    request: &JsObject,
) -> Result<JsObject, JsString> {
    let Some(JsValue::String(document)) = request.get("document") else {
        return Err(JsString::from(
            "subscribe: the request carries no string field document",
        ));
    };
    let operation_name = match request.get("operationName") {
        None | Some(JsValue::Null | JsValue::Undefined) => None,
        Some(JsValue::String(name)) => Some(to_string_lossy(name)),
        Some(_) => return Err(JsString::from("subscribe: operationName is no string")),
    };
    let policy = carrier_policy(request)?;
    let document = to_string_lossy(document);
    let operation = Operation::Document {
        text: &document,
        operation_name: operation_name.as_deref(),
    };
    let variables = request.get("variables").cloned().unwrap_or_default();
    let mut payload = JsObject::new();
    match Execute::execute(service, operation, &variables, &policy) {
        ServiceStream::One(result) => {
            let text = result.as_ref().map(result_text).unwrap_or_default();
            payload.insert("result", text);
        }
        stream @ ServiceStream::Events { .. } => {
            payload.insert("handle", streams.insert(stream));
        }
    }
    Ok(payload)
}

/// Returns the carrier policy the fields `carrier`, `carrier_allow` and `depth` of `request` name.
///
/// The list of `carrier_allow` is leaked once per call, because
/// `Allow::Only` takes a `'static` list.
fn carrier_policy(request: &JsObject) -> Result<CarrierPolicy, JsString> {
    let carrier = match request.get("carrier") {
        None => CarrierId::NETWORK,
        Some(JsValue::String(name)) => carrier_id(&to_string_lossy(name)).ok_or_else(|| {
            JsString::from("subscribe: carrier is none of network, unix_socket, usb, bluetooth, serial_line and lora")
        })?,
        Some(_) => return Err(JsString::from("subscribe: carrier is no string")),
    };
    let allow = match request.get("carrier_allow") {
        None => Allow::All,
        Some(JsValue::Array(fields)) => {
            let fields = fields
                .iter()
                .map(|field| match field {
                    JsValue::String(field) => {
                        let field: &'static str = String::leak(to_string_lossy(field).into_owned());
                        Ok(field)
                    }
                    _ => Err(JsString::from(
                        "subscribe: carrier_allow holds a field name that is no string",
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Allow::Only(Vec::leak(fields))
        }
        Some(_) => return Err(JsString::from("subscribe: carrier_allow is no list")),
    };
    let depth = match request.get("depth") {
        None => CarrierPolicy::NETWORK.depth,
        Some(JsValue::Number(depth)) => depth_of(*depth)
            .ok_or_else(|| JsString::from("subscribe: depth is no whole number from 0 to 65535"))?,
        Some(_) => return Err(JsString::from("subscribe: depth is no number")),
    };
    Ok(CarrierPolicy {
        carrier,
        allow,
        depth,
    })
}

/// Returns the carrier the snake case name `name` names.
fn carrier_id(name: &str) -> Option<CarrierId> {
    match name {
        "network" => Some(CarrierId::NETWORK),
        "unix_socket" => Some(CarrierId::UNIX_SOCKET),
        "usb" => Some(CarrierId::USB),
        "bluetooth" => Some(CarrierId::BLUETOOTH),
        "serial_line" => Some(CarrierId::SERIAL_LINE),
        "lora" => Some(CarrierId::LORA),
        _ => None,
    }
}

/// Returns `depth` as the depth of a slot, `None` for a number that is no `u16`.
fn depth_of(depth: f64) -> Option<u16> {
    if depth.fract() != 0.0 || !(0.0..=f64::from(u16::MAX)).contains(&depth) {
        return None;
    }
    // The number is whole and within the range of a `u16`, so the cast is exact.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(depth as u16)
}

/// Runs the operation `next` and returns its payload.
///
/// The stream is polled once and, while it is pending and `wait_ms` has not
/// passed, again after each wake of the reading thread or once the time is
/// up.
///
/// # Errors
///
/// Returns the `fatal` of a request without an open handle and with a
/// `wait_ms` that is no number.
fn next(streams: &mut Streams, request: &JsObject) -> Result<JsObject, JsString> {
    let wait = match request.get("wait_ms") {
        None => Some(Duration::ZERO),
        Some(JsValue::Number(wait)) => Duration::try_from_secs_f64(wait / 1000.0).ok(),
        Some(_) => None,
    };
    let Some(wait) = wait else {
        return Err(JsString::from(
            "next: wait_ms is no number of milliseconds from zero up",
        ));
    };
    let stream = streams.get(OP_NEXT, request)?;
    let mut payload = JsObject::new();
    match poll_until(stream, wait) {
        Poll::Ready(Some(result)) => {
            payload.insert("result", result_text(&result));
            if let Some(revision) = stream.last_revision() {
                payload.insert("revision", JsValue::from(revision.0));
            }
        }
        Poll::Ready(None) => {
            payload.insert("done", true);
        }
        Poll::Pending => {
            payload.insert("pending", true);
        }
    }
    Ok(payload)
}

/// Polls `stream` until it answers or `wait` has passed, parking the thread in between.
fn poll_until(stream: &mut ServiceStream, wait: Duration) -> Poll<Option<ExecutionResult>> {
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let start = (!wait.is_zero()).then(Instant::now);
    loop {
        let poll = Pin::new(&mut *stream).poll_next(&mut context);
        let Some(start) = start else {
            return poll;
        };
        let left = wait.saturating_sub(start.elapsed());
        if poll.is_ready() || left.is_zero() {
            return poll;
        }
        thread::park_timeout(left);
    }
}

/// The waker of a reading thread, which unparks it.
struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// Runs the operation `return`, which closes the stream of the handle and answers `{"done":true}`.
///
/// A stream that has not ended calls `return` on its iterator as it is
/// dropped.
///
/// # Errors
///
/// Returns the `fatal` of a request without an open handle.
fn close(streams: &mut Streams, request: &JsObject) -> Result<JsObject, JsString> {
    let index = handle_index(OP_RETURN, request)?;
    let stream = streams.0.get_mut(index).and_then(Option::take);
    if stream.is_none() {
        return Err(JsString::from("return: the handle names no open stream"));
    }
    drop(stream);
    let mut payload = JsObject::new();
    payload.insert("done", true);
    Ok(payload)
}

/// Returns the payload of one run: `result` or `thrown`, and `trace` last.
fn payload(executed: Executed, trace: bool) -> Result<JsObject, JsString> {
    let mut payload = JsObject::new();
    match executed.outcome {
        Ok(result) => {
            payload.insert("result", result_text(&result));
        }
        Err(RunFailure::Thrown(thrown)) => {
            payload.insert("thrown", JsValue::Object(thrown_record(&thrown)));
        }
        Err(RunFailure::Stalled) => return Err(JsString::from(STALLED)),
    }
    if trace {
        let events = executed.trace.iter().map(TraceEvent::to_js_value);
        payload.insert("trace", JsValue::Array(events.collect()));
    }
    Ok(payload)
}

/// Returns the text of `result`, the bytes `JSON.stringify` writes of it.
///
/// The data of a result of the executor is written from the arena of its
/// execution, as a served answer writes it, and no tree of it is built (see
/// [`ExecutionResult::write_json`]). The writer escapes a lone surrogate and
/// writes every other code point as its UTF-8, so the bytes are UTF-8 and the
/// text takes them with no copy.
// A `String::from_utf8_unchecked` would skip this pass.
fn result_text(result: &ExecutionResult) -> String {
    let mut bytes = Vec::new();
    result.write_json(&mut bytes);
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

/// Returns `{name, message}` of a thrown value.
///
/// The oracle reads the two members off whatever was thrown, and
/// `JSON.stringify` leaves a member out that is `undefined`, so a thrown
/// value that is no error answers an empty object.
fn thrown_record(thrown: &Value) -> JsObject {
    let mut record = JsObject::new();
    if let Value::Error(error) = thrown {
        record.insert("name", error.name());
        record.insert("message", error.message());
    }
    record
}
