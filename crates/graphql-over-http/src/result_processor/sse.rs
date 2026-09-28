// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the processor of a server sent event stream.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/result-processor/sse.js, MIT.
 */

//! The processor of a server sent event stream.
//!
//! The answer carries `Content-Type: text/event-stream`,
//! `Connection: keep-alive`, `Cache-Control: no-cache` and
//! `Content-Encoding: none`, and its body is the prelude `:\n\n`, then
//! `event: next\n` and `data: <json>\n\n` per result, then `event: complete\n`
//! and `data:\n\n`. Each piece the reference enqueues is one item of the body
//! stream, so the chunk boundaries on the wire are the reference's (section
//! YOGA of the plan's version 2).
//!
//! A stream, the answer of a subscription, is the readable stream of
//! `sse.js:26-78` over [`ReadableStream`]. Its `start` enqueues the prelude,
//! starts the ping as an interval of the timers of the agent, 12000 ms and 300
//! ms when `NODE_ENV` is exactly `test`, read once per result, and reads the
//! iterator of the stream. Each tick of the ping enqueues `:\n\n` while the
//! desired size of the stream is neither 0 nor null and clears the interval
//! for good otherwise, so a queue of exactly one chunk stops it. Its `pull`
//! writes a result whose `value != null` as two chunks, writes the terminator
//! for a done result, clears the interval and closes, and errors the stream
//! when `next` rejects. Its `cancel` clears the interval and calls `return` of
//! the iterator with the reason.

use core::cell::Cell;
use std::rc::Rc;

use aosabi_execute::{
    Agent,
    ExecutionResult,
    Value,
    handle_maybe_promise,
    promise::TimerId,
    value::{
        Completion,
        ErrorObject,
        get_async_iterator,
        get_method,
        iter_result_parts, //
    }, //
};
use aosabi_hooks::ExecutionOutcome;
use aosabi_http::{
    BodyStream,
    Response,
    fetch::body::Chunks, //
};
use aosabi_json::JsValue;

use crate::{
    error::get_response_init_by_respecting_errors,
    plugin::HttpResult,
    readable_stream::{
        ReadableStream,
        ReadableStreamController,
        UnderlyingSource, //
    },
    result_processor::stringify::{
        write_execution_result,
        write_result_without_internals, //
    },
    server_adapter::fetch_api::{
        ResponseBody,
        header_entries,
        new_response, //
    }, //
};

/// The ping and the prelude of a stream, a comment line of the event stream format.
pub const PING: &[u8] = b":\n\n";

/// The interval of the ping in milliseconds.
pub const PING_INTERVAL_MS: u64 = 12_000;

/// The interval of the ping under `NODE_ENV=test`.
pub const TEST_PING_INTERVAL_MS: u64 = 300;

/// Answers `result` as a server sent event stream, `getSSEProcessor()(result, fetchAPI)`.
///
/// # Errors
///
/// Returns the `TypeError` of a header the response refuses.
pub fn process_sse_result(result: &HttpResult, _accepted: &str) -> Result<Response, Value> {
    let ping_interval_ms = ping_interval_ms();
    let headers = header_entries(&[
        ("Content-Type", "text/event-stream"),
        ("Connection", "keep-alive"),
        ("Cache-Control", "no-cache"),
        ("Content-Encoding", "none"),
    ]);
    let init = get_response_init_by_respecting_errors(result, headers, true);
    let stream = match result {
        HttpResult::Single(ExecutionOutcome::Stream(stream)) => {
            let source = StreamSource {
                result: stream.iterable().clone(),
                iterator: Value::Undefined,
                ping_interval: Rc::new(Cell::new(None)),
                ping_interval_ms,
            };
            let readable = ReadableStream::new(stream.agent(), Box::new(source));
            BodyStream::new(readable.into_body())
        }
        _ => single_result_stream(result),
    };
    let response = new_response(
        ResponseBody::Stream(stream),
        init.status,
        None,
        &init.headers,
    )?;
    response.set_substatus(init.substatus);
    Ok(response)
}

/// Returns the interval of the ping, `pingIntervalMs`, which `NODE_ENV=test` shortens.
fn ping_interval_ms() -> u64 {
    // For testing the pings, reduce the timeout.
    if node_env().is_some_and(|env| env == "test") {
        return TEST_PING_INTERVAL_MS;
    }
    PING_INTERVAL_MS
}

/// Returns `NODE_ENV` of the process, `globalThis.process?.env?.NODE_ENV`.
#[cfg(not(feature = "host-env"))]
fn node_env() -> Option<String> {
    std::env::var("NODE_ENV").ok()
}

/// Returns `NODE_ENV` of the process, the one the host handed in first.
#[cfg(feature = "host-env")]
fn node_env() -> Option<String> {
    crate::host_env::var("NODE_ENV")
}

/// Returns the body of a single result, the prelude, its one frame and the terminator.
///
/// The JSON of the result is written into the buffer of its frame, after
/// `data: ` and before its blank line.
fn single_result_stream(result: &HttpResult) -> BodyStream {
    let mut frame = b"data: ".to_vec();
    write_result_without_internals(&mut frame, result);
    frame.extend_from_slice(b"\n\n");
    let pieces = [
        b":\n\n".to_vec(),
        b"event: next\n".to_vec(),
        frame,
        b"event: complete\n".to_vec(),
        b"data:\n\n".to_vec(),
    ];
    BodyStream::new(Chunks::new(pieces.map(Ok)))
}

/// The underlying source of the stream of a subscription, the object literal of `sse.js:26-78`.
struct StreamSource {
    /// The async iterable the subscription answers, `result`.
    result: Value,
    /// Its iterator once `start` read it, `iterator`.
    iterator: Value,
    /// The ping, `pingInterval`, which its own callback clears.
    ping_interval: Rc<Cell<Option<TimerId>>>,
    /// The interval of the ping, `pingIntervalMs`.
    ping_interval_ms: u64,
}

impl UnderlyingSource for StreamSource {
    fn start(&mut self, agent: &Agent, controller: &ReadableStreamController) {
        // Always start with a ping because some browsers dont accept a header
        // flush causing the fetch to stall until something is streamed
        // through the response.
        controller.enqueue(PING.to_vec());
        // Ping client every 12 seconds to keep the connection alive.
        let ping = Rc::clone(&self.ping_interval);
        let pinger = controller.clone();
        let id = agent.set_interval(
            self.ping_interval_ms,
            Rc::new(move |agent| {
                if matches!(pinger.desired_size(), None | Some(0)) {
                    clear_interval(agent, &ping);
                    return;
                }
                pinger.enqueue(PING.to_vec());
            }),
        );
        self.ping_interval.set(Some(id));
        // The iterator of every stream Yoga answers is the stream itself, so
        // this read does not throw, and a throw errors the stream where the
        // constructor of the reference would throw it.
        match get_async_iterator(agent, &self.result) {
            Completion::Normal(iterator) => self.iterator = iterator,
            Completion::Throw(error) => controller.error(error),
        }
    }

    fn pull(&mut self, agent: &Agent, controller: &ReadableStreamController) -> Completion {
        let iterator = self.iterator.clone();
        let on_next = controller.clone();
        let on_error = controller.clone();
        let ping = Rc::clone(&self.ping_interval);
        handle_maybe_promise(
            agent,
            move |agent| match get_method(&iterator, "next") {
                Some(next) => next.call(agent, &[]),
                None => Completion::Throw(Value::from(ErrorObject::type_error(
                    "iterator.next is not a function",
                ))),
            },
            Box::new(move |agent, result| write_result(agent, &on_next, &ping, &result)),
            Some(Box::new(move |_, error| {
                on_error.error(error);
                Completion::Normal(Value::Undefined)
            })),
        )
    }

    fn cancel(&mut self, agent: &Agent, reason: Value) -> Completion {
        clear_interval(agent, &self.ping_interval);
        let Some(r#return) = get_method(&self.iterator, "return") else {
            return Completion::Normal(Value::Undefined);
        };
        handle_maybe_promise(
            agent,
            move |agent| r#return.call(agent, &[reason]),
            Box::new(|_, _| Completion::Normal(Value::Undefined)),
            None,
        )
    }
}

/// Writes one iterator result into the stream, the success handler of the pull at `sse.js:56-67`.
fn write_result(
    agent: &Agent,
    controller: &ReadableStreamController,
    ping: &Cell<Option<TimerId>>,
    result: &Value,
) -> Completion {
    if result.is_nullish() {
        let target = if matches!(result, Value::Null) {
            "null"
        } else {
            "undefined"
        };
        return Completion::Throw(Value::from(ErrorObject::type_error(format!(
            "Cannot read properties of {target} (reading 'value')"
        ))));
    }
    let (value, done) = iter_result_parts(result);
    if !value.is_nullish() {
        controller.enqueue(b"event: next\n".to_vec());
        let mut frame = b"data: ".to_vec();
        write_stream_result(&mut frame, &value);
        frame.extend_from_slice(b"\n\n");
        controller.enqueue(frame);
    }
    if done {
        controller.enqueue(b"event: complete\n".to_vec());
        controller.enqueue(b"data:\n\n".to_vec());
        clear_interval(agent, ping);
        controller.close();
    }
    Completion::Normal(Value::Undefined)
}

/// Clears the ping, `clearInterval(pingInterval)`, which does nothing before the ping is set.
fn clear_interval(agent: &Agent, ping: &Cell<Option<TimerId>>) {
    if let Some(id) = ping.get() {
        agent.clear_timer(id);
    }
}

/// Appends the JSON of one result of a stream without its internals to `out`, `jsonStringifyResultWithoutInternals(result.value)`.
///
/// A result of a stream is a value of the agent, the object
/// `{ errors?, data?, extensions? }` the executor, envelop and the error
/// handler of Yoga write in that key order, and it goes through the
/// stringifier of a single result, [`write_execution_result`], so the frame of
/// a result is the text of the same result answered alone.
fn write_stream_result(out: &mut Vec<u8>, value: &Value) {
    let mut result = ExecutionResult::from_value(value);
    if let Value::Object(object) = value
        && let JsValue::Object(extensions) = Rc::clone(object).get("extensions").to_js()
    {
        result.extensions = Some(extensions);
    }
    write_execution_result(out, &result);
}
