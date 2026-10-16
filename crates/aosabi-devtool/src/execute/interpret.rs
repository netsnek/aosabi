// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool execute interpret: makes the value a spec of a behaviour
 * table describes, over the promise type of aosabi-execute.
 *
 * No analogue in the reference. The node side makes the same values with real
 * promises, and the rules both sides follow are those of
 * tests/sidecar/execute.md.
 */

//! The interpreter of a behaviour table.
//!
//! [`materialize`] makes what one [`Spec`] describes. Both sides make promises
//! and timers in one order, because the order of the `errors` array and the
//! key order of `data` follow from it.
//!
//! - A spec is made depth first, members and items in the order of the text.
//!   The `with` of a `resolved` and of an `after` spec is made before the
//!   promise that carries it, and so is the thrown value of a `rejected` and
//!   of an `after` spec.
//! - Everything is made eagerly, also a member no selection reads. A timer
//!   exists from the moment its `after` spec is made.
//! - A trace event is recorded when a `fn` starts, when a `throw` runs, when
//!   a `resolved` or a `rejected` spec is made and when the timer of an
//!   `after` spec fires. No reaction is attached to a promise for it, so a
//!   trace never adds a job.
//!
//! A function a `fn` spec makes has the empty name, which is the name of an
//! arrow function that is returned and never bound to a variable.
//!
//! The four kinds of the subscriptions follow rule 6 of the grammar. A
//! `repeater` makes a `Repeater` of aosabi-execute, whose executor runs its
//! steps on the first `next`, so the value of a push is made when its step
//! runs. A `pubsub_topic` subscribes on the pub sub of the request, and a
//! `pubsub_publish` makes its key and its payload in that order, publishes
//! them, and makes its `then` after the publish. A step of a repeater records
//! no trace event of its own.

use std::{
    cell::{
        Cell,
        RefCell, //
    },
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::{
    ServiceError,
    ServiceErrorExtensions,
    pub_sub::SharedPayload, //
};
use aosabi_execute::{
    promise::{
        Agent,
        Promise,
        PromiseFuture,
        TimerMode, //
    },
    repeater::{
        PushHandle,
        Repeater,
        StopHandle, //
    },
    value::{
        Completion,
        ErrorObject,
        Function,
        ObjectLike,
        PlainObject,
        Value, //
    }, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions, //
};
use aosabi_scalars::JsDate;

use crate::execute::{
    clock::WallClock,
    pub_sub::{
        Frozen,
        request_pub_sub, //
    },
    table::{
        Mode,
        Outcome,
        RepeaterStep,
        Spec,
        SpecKind,
        Throw, //
    }, //
};

/// The trace state of a resolver body or a `fn` that starts.
pub const CALLED: &str = "called";
/// The trace state of a `throw` that runs.
const THROWN: &str = "thrown";
/// The trace state of a promise that is made or becomes fulfilled.
const FULFILLED: &str = "fulfilled";
/// The trace state of a promise that is made or becomes rejected.
const REJECTED: &str = "rejected";

thread_local! {
    /// Whether the agents of the run read the clock of the host, `--timers host`.
    static HOST_TIMERS: Cell<bool> = const { Cell::new(false) };
    /// The window [`in_window`] holds open, while it is open.
    static WINDOW: RefCell<Option<Window>> = const { RefCell::new(None) };
}

/// What one window of [`in_window`] saw: the first agent a resolver ran on and its clock.
#[derive(Default)]
pub struct Window {
    /// The agent a resolver of the table ran on first.
    pub agent: Option<Agent>,
    /// The clock of the host that agent reads under `--timers host`.
    pub clock: Option<Rc<WallClock>>,
}

/// Sets whether the agents of the run read the clock of the host.
pub fn set_host_timers(host: bool) {
    HOST_TIMERS.set(host);
}

/// Returns the mode of the timers of every agent of the run.
pub fn timer_mode() -> TimerMode {
    if HOST_TIMERS.get() {
        TimerMode::Host
    } else {
        TimerMode::Virtual
    }
}

/// Runs `body`, one request, and returns what it returns with the window it opened.
///
/// A request runs on an agent its entry makes, and the first resolver of the
/// table that runs on it hands it to [`note_agent`]. Under `--timers host`
/// the window holds a fresh clock of the host, which that agent receives
/// before any timer of a spec is made, because a spec that makes a timer
/// runs inside a resolver. The seam reads the trace of a subscription that
/// answers a single result through the agent, and sleeps on the clock.
pub fn in_window<R>(body: impl FnOnce() -> R) -> (R, Window) {
    let clock = HOST_TIMERS.get().then(|| Rc::new(WallClock::new()));
    WINDOW.set(Some(Window { agent: None, clock }));
    let answer = body();
    (answer, WINDOW.take().unwrap_or_default())
}

/// Keeps `agent` as the agent of the open window when it holds none yet, and hands it the clock.
pub fn note_agent(agent: &Agent) {
    let clock = WINDOW.with_borrow_mut(|window| {
        let window = window.as_mut()?;
        if window.agent.is_some() {
            return None;
        }
        window.agent = Some(agent.clone());
        window.clock.clone()
    });
    if let Some(clock) = clock {
        agent.set_host_clock(clock);
    }
}

/// What a spec is made inside of: the agent of the request and the running call.
#[derive(Clone, Copy)]
pub struct Frame<'c> {
    /// The agent of the request, which owns the jobs, the timers and the trace.
    pub agent: &'c Agent,
    /// The mode of the table, which decides how an `arg` spec reads `args`.
    pub mode: Mode,
    /// The arguments of the running call.
    ///
    /// In [`Mode::Plain`] the first entry is the argument object of the field,
    /// for a resolver body and for a `fn` the default field resolver calls
    /// alike. In [`Mode::Service`] the entries are the positional arguments the
    /// resolver wrapping hands to a function.
    pub args: &'c [Value],
}

/// Makes what `spec` describes.
///
/// The completion is a throw for a `throw` spec and for nothing else, which
/// the grammar admits only as a whole resolver body and as the whole
/// `returns` of a `fn`.
pub fn materialize(spec: &Spec, frame: Frame<'_>) -> Completion {
    Completion::from(make(spec, frame))
}

/// Makes what `spec` describes, with a throw as the error.
fn make(spec: &Spec, frame: Frame<'_>) -> Result<Value, Value> {
    match &spec.kind {
        SpecKind::Value(json) => Ok(Value::from_js(json)),
        SpecKind::Undefined => Ok(Value::Undefined),
        SpecKind::Number(number) => Ok(Value::Number(*number)),
        SpecKind::Date(iso) => Ok(Value::Date(date(iso.as_deref()))),
        SpecKind::Object(members) => object(members, frame),
        SpecKind::List(items) => {
            let items: Result<Vec<Value>, Value> =
                items.iter().map(|item| make(item, frame)).collect();
            items.map(Value::from)
        }
        SpecKind::Fn(returns) => Ok(function(&spec.label, returns, frame.mode)),
        SpecKind::Arg { name, index } => Ok(argument(frame, name, *index)),
        SpecKind::Resolved(with) => {
            let value = make(with, frame)?;
            frame.agent.record(&spec.label, FULFILLED);
            Ok(Value::from(Promise::resolve(frame.agent, value)))
        }
        SpecKind::Rejected(error) => {
            let reason = thrown(error, frame)?;
            frame.agent.record(&spec.label, REJECTED);
            Ok(Value::from(Promise::reject(frame.agent, reason)))
        }
        SpecKind::After { units, outcome } => after(&spec.label, *units, outcome, frame),
        SpecKind::Throw(error) => {
            let reason = thrown(error, frame)?;
            frame.agent.record(&spec.label, THROWN);
            Err(reason)
        }
        SpecKind::ErrorValue(error) => thrown(error, frame),
        SpecKind::Repeater(steps) => Ok(repeater(steps, frame)),
        SpecKind::NotIterable(value) => make(value, frame),
        SpecKind::PubSubTopic { name, key } => {
            let key = match key {
                Some(key) => Some(make(key, frame)?.to_js()),
                None => None,
            };
            let stream: Rc<dyn ObjectLike> = request_pub_sub().subscribe_value(name, key);
            Ok(Value::Object(stream))
        }
        SpecKind::PubSubPublish {
            name,
            key,
            payload,
            then,
        } => {
            let mut args: Vec<SharedPayload> = Vec::with_capacity(2);
            if let Some(key) = key {
                args.push(Arc::new(Frozen::of(&make(key, frame)?)));
            }
            args.push(Arc::new(Frozen::of(&make(payload, frame)?)));
            request_pub_sub().publish_args(name, &args);
            make(then, frame)
        }
    }
}

/// Makes the `Repeater` of a `repeater` spec, whose executor runs `steps` in order.
///
/// The executor is an async function spawned on the agent of the first
/// `next`, so it runs synchronously up to its first await, as the async
/// arrow function of the node side does. It keeps the arguments of the call
/// that made the repeater, so a spec of a push reads them as the call would.
fn repeater(steps: &Arc<[RepeaterStep]>, frame: Frame<'_>) -> Value {
    let steps = Arc::clone(steps);
    let mode = frame.mode;
    let args: Rc<[Value]> = frame.args.into();
    let executor = Box::new(move |agent: &Agent, push: PushHandle, stop: StopHandle| {
        let body = async move {
            for step in steps.iter() {
                match start(step, mode, &args, &push, &stop) {
                    Ok(Some(awaited)) => {
                        if let Err(error) = awaited.await {
                            return Completion::Throw(error);
                        }
                    }
                    Ok(None) => {}
                    Err(thrown) => return Completion::Throw(thrown),
                }
            }
            Completion::Normal(Value::Undefined)
        };
        Completion::Normal(Value::Promise(agent.spawn(Box::pin(body))))
    });
    let repeater: Rc<dyn ObjectLike> = Repeater::new(executor);
    Value::Object(repeater)
}

/// Runs the synchronous half of one step of a repeater and returns what the step awaits.
///
/// The agent is read here and dropped before the await, because the agent
/// keeps the future of the executor, and a future that kept the agent would
/// keep both alive. `Err` is the value the step throws out of the executor.
fn start(
    action: &RepeaterStep,
    mode: Mode,
    args: &[Value],
    push: &PushHandle,
    stop: &StopHandle,
) -> Result<Option<PromiseFuture>, Value> {
    let Some(agent) = Agent::current() else {
        return Ok(None);
    };
    let frame = Frame {
        agent: &agent,
        mode,
        args,
    };
    match action {
        RepeaterStep::Push { value, awaited } => {
            let pushed = push.push(&agent, make(value, frame)?)?;
            Ok(awaited.then(|| pushed.awaited(&agent).into_future()))
        }
        RepeaterStep::Sleep(units) => Ok(Some(agent.sleep(*units).into_future())),
        RepeaterStep::Stop(None) => {
            stop.stop(&agent, None);
            Ok(None)
        }
        RepeaterStep::Stop(Some(error)) => {
            stop.stop(&agent, Some(thrown(error, frame)?));
            Ok(None)
        }
        RepeaterStep::Throw(error) => Err(thrown(error, frame)?),
    }
}

/// Returns `new Date(iso)`, and `new Date(NaN)` for `None`.
fn date(iso: Option<&str>) -> JsDate {
    iso.map_or_else(|| JsDate::from_time_value(f64::NAN), JsDate::parse)
}

/// Makes the object of an `object` spec, the members in the order of the text.
fn object(members: &[(String, Arc<Spec>)], frame: Frame<'_>) -> Result<Value, Value> {
    let mut entries = Vec::with_capacity(members.len());
    for (key, member) in members {
        entries.push((key.clone(), make(member, frame)?));
    }
    Ok(Value::from(PlainObject::from_entries(entries)))
}

/// Makes the function of a `fn` spec, which makes `returns` on every call.
fn function(label: &str, returns: &Arc<Spec>, mode: Mode) -> Value {
    let label = String::from(label);
    let returns = Arc::clone(returns);
    Value::from(Function::new("", move |agent, args| {
        note_agent(agent);
        agent.record(&label, CALLED);
        let frame = Frame { agent, mode, args };
        materialize(&returns, frame)
    }))
}

/// Returns the argument an `arg` spec names, undefined when the call has none.
fn argument(frame: Frame<'_>, name: &str, index: usize) -> Value {
    match frame.mode {
        Mode::Service => frame.args.get(index).cloned().unwrap_or(Value::Undefined),
        Mode::Plain => match frame.args.first() {
            Some(Value::Object(args)) => Rc::clone(args).get(name),
            _ => Value::Undefined,
        },
    }
}

/// Makes the promise of an `after` spec and the timer that settles it.
///
/// The outcome is made first and the timer second, so a timer nested in the
/// outcome is older than the timer that carries it and fires first on an
/// equal deadline.
fn after(label: &str, units: u64, outcome: &Outcome, frame: Frame<'_>) -> Result<Value, Value> {
    let settlement = match outcome {
        Outcome::With(with) => Ok(make(with, frame)?),
        Outcome::Error(error) => Err(thrown(error, frame)?),
    };
    let label = String::from(label);
    let (promise, functions) = Promise::with_resolvers(frame.agent);
    let fire = move |agent: &Agent| match settlement {
        Ok(value) => {
            agent.record(&label, FULFILLED);
            functions.resolve(agent, value);
        }
        Err(reason) => {
            agent.record(&label, REJECTED);
            functions.reject(agent, reason);
        }
    };
    frame.agent.set_timeout(units, Box::new(fire));
    Ok(Value::from(promise))
}

/// Makes the value a throw describes.
fn thrown(error: &Throw, frame: Frame<'_>) -> Result<Value, Value> {
    match error {
        Throw::Plain { message } => Ok(Value::from(ErrorObject::error(message.clone()))),
        Throw::Service {
            message,
            code,
            status_code,
            details,
        } => {
            let extensions = ServiceErrorExtensions {
                code: code.clone(),
                status_code: *status_code,
                details: details.clone(),
            };
            let error = ServiceError::new(message.clone(), extensions).into_graphql_error();
            Ok(Value::from(ErrorObject::GraphQL(error)))
        }
        Throw::GraphQL {
            message,
            extensions,
        } => {
            let options = GraphQLErrorOptions {
                extensions: extensions.clone(),
                ..GraphQLErrorOptions::default()
            };
            let error = GraphQLError::new(message.clone(), options);
            Ok(Value::from(ErrorObject::GraphQL(error)))
        }
        Throw::Value(spec) => make(spec, frame),
        Throw::Aggregate { message, errors } => {
            let errors: Result<Vec<Value>, Value> =
                errors.iter().map(|error| thrown(error, frame)).collect();
            Ok(Value::from(ErrorObject::Aggregate {
                message: message.clone(),
                errors: errors?,
            }))
        }
    }
}
