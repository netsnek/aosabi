// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool execute subscribe: subscribes to one document against a
 * loaded behaviour table and pulls its stream.
 *
 * Follows `opSubscribe`, `pullStream` and `runInterleave` of
 * tests/sidecar/execute.mjs, whose protocol is tests/sidecar/README.md, the
 * operation subscribe. No analogue in the reference.
 */

//! The operation `subscribe` of the batch form.
//!
//! A request carries `table`, `document` and optionally `variables`,
//! `operationName`, `validate`, true when absent, `trace`, `pull`,
//! `return_after` and `interleave`. The subscription runs through
//! `run_subscription` of aosabi-execute on a fresh agent with virtual timers,
//! which a service table reaches through `Service::subscribe` of
//! aosabi-core with the policy of Network.
//!
//! - `pull` is the most calls of `next`, from 0 to 1024, and 0, the default,
//!   pulls until the stream answers done.
//! - `return_after` is the count of results after which `return` is called
//!   and the pull ends, from 1 to 1024.
//! - `interleave` is a list of `{"after_pull": n, "document", "variables"?,
//!   "operationName"?}`, each run on the same table after the `n`-th `next`
//!   was called, with validation on, on an agent of its own and with the pub
//!   sub of the request, and what it answers is dropped.
//!
//! The answer is `{"frames": [...], "done": true}` when the stream ended,
//! `{"frames": [...], "done": false}` when the pull ended first, and the same
//! with `thrown` when a `next` rejected, the frames before it kept. A frame is
//! the result text of one result the stream yielded. An answer that is no
//! stream is `{"result": "<text>"}`, a throw of the entry is `{"thrown":
//! {name, message}}`, and `trace` follows last when the request asks for it.
//!
//! Under `--timers host` every agent reads the clock of the host, and a pull
//! that waits on a timer sleeps until it is due.
//!
//! The stream is the one a carrier reads, so a call of `next` and the run of
//! the agent until it settles are one poll. An `interleave` request runs
//! after that poll, and a poll that waits on a publish polls once more after
//! the request ran. A rejected `next` reaches the seam as the stream ends it,
//! as a result with one error and no `data`, which no result an event
//! produces has, so its `thrown` carries the name `GraphQLError` and the
//! message of that error. The trace holds the events of the agent of the
//! subscription up to each `interleave` request, the events of that request
//! on its own clock, and the rest after it. A `return` after `return_after`
//! closes the stream, and what that call answers does not reach the seam.

use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
        Waker, //
    },
};

use aosabi_core::kernel::execute_closed::ResultStream;
use aosabi_execute::{
    kernel_entry::{
        Request,
        RunFailure,
        Subscribed,
        SubscriptionStream, //
    },
    promise::{
        AgentOptions,
        TraceEvent, //
    },
    result::ExecutionResult, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    ParseOptions,
    js_string::to_string_lossy, //
};
use aosabi_validate::ValidateOptions;

use crate::{
    batch,
    execute::{
        Call,
        SESSION,
        error_record,
        interpret::{
            Window,
            in_window,
            timer_mode, //
        },
        no_table,
        pub_sub::begin_request,
        result_text,
        trace_json, //
    }, //
};

/// The most calls of `next` one request makes, the bound of the queue of `next` calls of
/// the repeater.
const MAX_PULL: f64 = 1024.0;

/// One request of an `interleave` list.
struct Interleave {
    /// The count of calls of `next` after which the request runs.
    after_pull: u32,
    /// The document text.
    document: String,
    /// The raw variable values, when the request carries any.
    variables: Option<JsValue>,
    /// The name of the operation to run, when the request names one.
    operation_name: Option<String>,
}

/// What the pull of one stream asks for.
struct Plan {
    /// The table the request runs against.
    table: String,
    /// The most calls of `next`, 0 for until done.
    pull: u32,
    /// The count of results after which `return` is called, when the request names one.
    return_after: Option<u32>,
    /// The requests run between a call of `next` and its settlement.
    interleave: Vec<Interleave>,
    /// Whether the trace is recorded.
    trace: bool,
}

/// Answers the operation `subscribe`.
pub fn op_subscribe(request: &JsObject) -> Result<JsObject, JsString> {
    let table = batch::require_string(request, "table")?;
    let call = Call::of(request).map_err(|reason| format!("subscribe: {reason}"))?;
    let plan = Plan {
        table: String::from(table),
        pull: count(request, "pull", 0.0)?.unwrap_or(0),
        return_after: count(request, "return_after", 1.0)?,
        interleave: interleave_of(request)?,
        trace: call.trace,
    };
    begin_request();
    let subscribe_request = Request {
        document: &call.document,
        variables: call.variables.clone(),
        operation_name: call.operation_name.as_deref(),
        validate: call.validate,
    };
    let options = AgentOptions {
        timers: timer_mode(),
        trace: call.trace,
    };
    let (subscribed, window) = in_window(|| {
        SESSION.with_borrow(|session| session.subscribe(table, &subscribe_request, options))
    });
    let subscribed = subscribed.ok_or_else(|| format!("subscribe: {}", no_table(table)))?;
    let mut events = Vec::new();
    let mut payload = JsObject::new();
    match subscribed {
        Ok(Subscribed::One(result)) => {
            payload.insert("result", result_text(&result));
        }
        Ok(Subscribed::Stream(mut stream)) => {
            payload = pull_stream(&mut stream, &plan, &window, &mut events)?;
            events.extend(stream.agent().take_trace());
        }
        Err(RunFailure::Thrown(error)) => {
            payload.insert("thrown", JsValue::Object(error_record(&error)));
        }
        Err(RunFailure::Stalled) => {
            return Err(JsString::from(
                "subscribe: the subscription is pending with no job and no timer left",
            ));
        }
    }
    if plan.trace {
        if let Some(agent) = window.agent {
            let mut all = agent.take_trace();
            all.extend(events);
            events = all;
        }
        payload.insert("trace", trace_json(&events));
    }
    Ok(payload)
}

/// Pulls `stream` as `plan` asks and returns the payload of the answer.
///
/// Every interleave request appends the events of the subscription so far
/// and its own to `events`.
fn pull_stream(
    stream: &mut SubscriptionStream,
    plan: &Plan,
    window: &Window,
    events: &mut Vec<TraceEvent>,
) -> Result<JsObject, String> {
    let mut frames = Vec::new();
    let mut calls = 0_u32;
    let mut payload = JsObject::new();
    while plan.pull == 0 || calls < plan.pull {
        calls += 1;
        let mut poll = poll_next(stream, window);
        let mut interleaved = false;
        for step in plan
            .interleave
            .iter()
            .filter(|step| step.after_pull == calls)
        {
            events.extend(stream.agent().take_trace());
            events.extend(run_interleave(&plan.table, step, plan.trace)?);
            interleaved = true;
        }
        if interleaved && poll.is_pending() {
            poll = poll_next(stream, window);
        }
        let Poll::Ready(item) = poll else {
            return Err(format!(
                "subscribe: next {calls} is pending with no job and no timer left"
            ));
        };
        let Some(result) = item else {
            payload.insert("frames", frames);
            payload.insert("done", true);
            return Ok(payload);
        };
        if !result.has_data() {
            payload.insert("frames", frames);
            payload.insert("done", false);
            payload.insert("thrown", JsValue::Object(rejection_record(&result)));
            return Ok(payload);
        }
        frames.push(JsValue::from(result_text(&result)));
        if plan
            .return_after
            .is_some_and(|after| usize::try_from(after).ok() == Some(frames.len()))
        {
            stream.close();
            break;
        }
    }
    stream.close();
    payload.insert("frames", frames);
    payload.insert("done", false);
    Ok(payload)
}

/// Polls `stream` once, and once more after each deadline of the clock of the host it waits on.
///
/// A stream on the virtual clock answers pending only when nothing can run
/// any more. On the clock of the host it answers pending while a timer is
/// not due, and the seam sleeps until the timer is due and polls again.
fn poll_next(stream: &mut SubscriptionStream, window: &Window) -> Poll<Option<ExecutionResult>> {
    let mut context = Context::from_waker(Waker::noop());
    loop {
        let poll = Pin::new(&mut *stream).poll_next(&mut context);
        let waited = poll.is_pending() && window.clock.as_ref().is_some_and(|clock| clock.wait());
        if !waited {
            return poll;
        }
    }
}

/// Runs one interleave request on the table `table` and returns the events of its agent.
///
/// The document is parsed and validated first, and a document either refuses
/// is a mistake of the case, which answers `fatal`. What the request answers
/// is dropped.
fn run_interleave(table: &str, step: &Interleave, trace: bool) -> Result<Vec<TraceEvent>, String> {
    let refused = SESSION.with_borrow(|session| {
        let schema = session.schema(table)?;
        let errors = match aosabi_lang::parse(step.document.as_str(), ParseOptions::default()) {
            Ok(document) => {
                aosabi_validate::validate(schema, &document, None, ValidateOptions::default(), None)
                    .unwrap_or_default()
            }
            Err(error) => vec![error],
        };
        (!errors.is_empty()).then(|| result_text(&ExecutionResult::from_errors(errors)))
    });
    if let Some(refused) = refused {
        return Err(format!(
            "subscribe: an interleave document is refused: {refused}"
        ));
    }
    let request = Request {
        document: &step.document,
        variables: step.variables.clone(),
        operation_name: step.operation_name.as_deref(),
        validate: true,
    };
    let options = AgentOptions {
        timers: timer_mode(),
        trace,
    };
    let (executed, _) =
        in_window(|| SESSION.with_borrow(|session| session.execute(table, &request, options)));
    Ok(executed.map(|executed| executed.trace).unwrap_or_default())
}

/// Returns `{name, message}` of the rejection a stream ended with.
///
/// The stream carries the rejection as the one error of its last result, a
/// `GraphQLError`.
fn rejection_record(result: &ExecutionResult) -> JsObject {
    let mut record = JsObject::new();
    if let Some(error) = result.errors.first() {
        record.insert("name", "GraphQLError");
        record.insert("message", error.message.clone());
    } else {
        record.insert("name", "Error");
        record.insert("message", "");
    }
    record
}

/// Returns the integer field `name` of `request` from `least` to 1024, `None` when it is absent.
///
/// Only an absent field is unset. A `null` is a field of the wrong type and
/// answers `fatal`, as on the node side.
fn count(request: &JsObject, name: &str, least: f64) -> Result<Option<u32>, String> {
    let Some(value) = request.get(name) else {
        return Ok(None);
    };
    match value {
        JsValue::Number(count) if count.fract() == 0.0 && (least..=MAX_PULL).contains(count) => {
            // The checks above keep the value a whole number up to 1024.
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Ok(Some(*count as u32))
        }
        _ => Err(format!(
            "subscribe: the field {name} is no integer from {least} to {MAX_PULL}"
        )),
    }
}

/// Returns the `interleave` list of `request`, each entry checked.
fn interleave_of(request: &JsObject) -> Result<Vec<Interleave>, String> {
    let Some(list) = request.get("interleave") else {
        return Ok(Vec::new());
    };
    let JsValue::Array(list) = list else {
        return Err(String::from("subscribe: the field interleave is no list"));
    };
    let mut steps = Vec::with_capacity(list.len());
    for (index, step) in list.iter().enumerate() {
        let wrong = |reason: &str| format!("subscribe: interleave {index} carries {reason}");
        let JsValue::Object(step) = step else {
            return Err(wrong("no string field document"));
        };
        let Some(JsValue::String(document)) = step.get("document") else {
            return Err(wrong("no string field document"));
        };
        let after_pull = match step.get("after_pull") {
            Some(JsValue::Number(after))
                if after.fract() == 0.0 && (1.0..=MAX_PULL).contains(after) =>
            {
                // The checks above keep the value a whole number up to 1024.
                #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let after = *after as u32;
                after
            }
            _ => return Err(wrong("no after_pull from 1 to 1024")),
        };
        let operation_name = match step.get("operationName") {
            Some(JsValue::String(name)) => Some(to_string_lossy(name).into_owned()),
            _ => None,
        };
        steps.push(Interleave {
            after_pull,
            document: to_string_lossy(document).into_owned(),
            variables: step.get("variables").cloned(),
            operation_name,
        });
    }
    Ok(steps)
}
