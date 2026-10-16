// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool pubsub script: one script of steps over the pub sub and the
 * repeater, on one agent with virtual timers.
 *
 * Follows `Run`, `executorOf`, `runStep` and `opScript` of
 * tests/sidecar/pubsub.mjs, whose protocol is tests/sidecar/pubsub.md. No
 * analogue in the reference.
 */

//! One script of the pub sub sidecar's protocol.
//!
//! A script runs on a fresh agent with virtual timers, its own pub subs and
//! its own streams, named by the strings the steps give them. The steps run
//! in order, each at once inside the agent, and only `settle` runs the jobs
//! and the timers, until nothing is left that can run. A step that throws,
//! such as the 1025th pending `next`, stops the script, the work still
//! pending settles, and the answer carries the throw. Every script ends with
//! that settle.
//!
//! | Step | What it does |
//! | --- | --- |
//! | `{"create":"p"}` | makes a pub sub named `p` |
//! | `{"subscribe":"s","pubsub":"p","key":"k","id":<json>}` | `subscribe(key, id)`, and `subscribe(key)` when `id` is absent |
//! | `{"publish":"p","key":"k","args":[<json>...]}` | `publish(key, ...args)` |
//! | `{"publish_on_thread":"p","key":"k","args":[<json>...]}` | the same from a second thread of the process |
//! | `{"repeater":"r","script":[...]}` | a repeater whose executor runs the inner steps |
//! | `{"pull":"s","count":n}` | calls `next()` `n` times without awaiting |
//! | `{"return":"s"}` | calls `return()` |
//! | `{"throw_into":"s","error":"message"}` | calls `throw(new Error(message))` |
//! | `{"settle":true}` | runs jobs and timers until nothing is left |
//!
//! An inner step of a repeater is `{"push":<json>}`, which awaits what `push`
//! answers unless it carries `"await":false`, `{"sleep":n}`, `{"tick":true}`,
//! `{"stop":null}`, `{"stop":"message"}` or `{"throw":"message"}`. A JSON
//! value of `id`, `args` and `push` is taken as it is, except the object
//! `{"$undefined":true}`, which stands for `undefined`.
//!
//! Every call of `next`, `return` and `throw` on one stream takes the next
//! number of that stream from 1, and its settlement appends one observation,
//! `{"on","n","value","done"}` or `{"on","n","error":{"name","message"}}`, in
//! the order the settlements happen.
//!
//! The sidecar's step `sleep` takes any number, and this side takes an
//! integer from 0 alone, which is every sleep of the corpus.

use std::{
    cell::RefCell,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{
        Context,
        Waker, //
    },
    thread, //
};

use aosabi_core::{
    execute::{
        promise::{
            Agent,
            AgentOptions,
            Promise,
            PromiseFuture,
            TimerMode,
            tick, //
        },
        repeater::{
            Executor,
            PushHandle,
            Repeater,
            StopHandle, //
        },
        value::{
            Completion,
            ErrorObject,
            Value,
            iter_result_parts,
            to_js_string, //
        }, //
    },
    pub_sub::{
        PubSub,
        SharedPayload,
        experimental_create_pub_sub, //
    },
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::js_string::to_string_lossy;
use aosabi_type::ObjMap;

use crate::execute::error_record;

/// The key of the object that stands for `undefined` in a JSON value of a step.
const UNDEFINED: &str = "$undefined";

/// The keys of an inner step, exactly one of which it carries.
const INNER_KEYS: [&str; 5] = ["push", "sleep", "tick", "stop", "throw"];

/// Why the steps of a script stop before their end.
enum Halt {
    /// A step breaks the grammar, which answers `fatal` with the reason.
    Request(String),
    /// A step threw the value, which the answer carries as `thrown`.
    Thrown(Value),
}

/// Returns the refusal of the step `index` for `reason`.
fn refuse(index: usize, reason: &str) -> Halt {
    Halt::Request(format!("step {index}: {reason}"))
}

/// The method of a stream a step calls.
enum Method {
    /// `next()`.
    Next,
    /// `return()`.
    Return,
    /// `throw(new Error(message))`.
    Throw(JsString),
}

/// One inner step of the executor of a repeater.
enum Inner {
    /// `push(value)`, awaited unless `awaited` is false.
    Push {
        /// The value pushed.
        value: JsValue,
        /// Whether the executor awaits what `push` answers.
        awaited: bool,
    },
    /// Awaits a timer of the units.
    Sleep(u64),
    /// Awaits one job.
    Tick,
    /// `stop()`, or `stop(new Error(message))`.
    Stop(Option<JsString>),
    /// Throws `new Error(message)` out of the executor.
    Throw(JsString),
}

/// Runs the script of `request` and answers its observations and its throw.
///
/// # Errors
///
/// Returns the text of the `fatal` of a request without a list `steps` and
/// of a step that breaks the grammar.
pub fn run(request: &JsObject) -> Result<JsObject, String> {
    let Some(JsValue::Array(steps)) = request.get("steps") else {
        return Err(String::from("script: the request carries no list steps"));
    };
    let script = Script::new();
    let mut thrown = JsValue::Null;
    for (index, step) in steps.iter().enumerate() {
        match script.step(step, index) {
            Ok(()) => {}
            Err(Halt::Request(reason)) => return Err(reason),
            Err(Halt::Thrown(error)) => {
                thrown = JsValue::Object(error_record(&error));
                break;
            }
        }
    }
    script.settle();
    let mut payload = JsObject::new();
    payload.insert("observations", JsValue::Array(script.observations.take()));
    payload.insert("thrown", thrown);
    Ok(payload)
}

/// One run of a script: its agent, its pub subs, its streams and what it observed.
struct Script {
    /// The agent every step, job and timer of the script runs on.
    agent: Agent,
    /// The pub subs by the names the steps gave them.
    pub_subs: RefCell<ObjMap<&'static PubSub>>,
    /// The streams by the names the steps gave them.
    streams: RefCell<ObjMap<Rc<Repeater>>>,
    /// The count of calls made on each stream.
    calls: RefCell<ObjMap<u32>>,
    /// The observations in the order the settlements happened.
    observations: Rc<RefCell<Vec<JsValue>>>,
}

impl Script {
    /// Returns a script with a fresh agent on the virtual clock.
    fn new() -> Self {
        Self {
            agent: Agent::new(AgentOptions {
                timers: TimerMode::Virtual,
                trace: false,
            }),
            pub_subs: RefCell::new(ObjMap::default()),
            streams: RefCell::new(ObjMap::default()),
            calls: RefCell::new(ObjMap::default()),
            observations: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Runs one step, `settle` included.
    fn step(&self, step: &JsValue, index: usize) -> Result<(), Halt> {
        let JsValue::Object(step) = step else {
            return Err(refuse(index, "a step is no object"));
        };
        if step.contains_key("settle") {
            self.settle();
            return Ok(());
        }
        self.agent.enter(|| self.run_step(step, index))
    }

    /// Runs one step other than `settle`, `runStep` of the sidecar.
    fn run_step(&self, step: &JsObject, index: usize) -> Result<(), Halt> {
        if step.contains_key("create") {
            let name = string(step, "create", index)?;
            let pub_sub: &'static PubSub = Box::leak(Box::new(experimental_create_pub_sub()));
            self.pub_subs.borrow_mut().insert(name, pub_sub);
        } else if step.contains_key("subscribe") {
            let pub_sub = self.pub_sub(step, "pubsub", index)?;
            let key = string(step, "key", index)?;
            let stream = pub_sub.subscribe_value(&key, step.get("id").map(decode));
            let name = string(step, "subscribe", index)?;
            self.streams.borrow_mut().insert(name, stream);
        } else if step.contains_key("publish") {
            let pub_sub = self.pub_sub(step, "publish", index)?;
            let args = payloads(step, index)?;
            pub_sub.publish_args(&string(step, "key", index)?, &args);
        } else if step.contains_key("repeater") {
            let name = string(step, "repeater", index)?;
            let steps = inner_steps(step.get("script"), index)?;
            self.streams
                .borrow_mut()
                .insert(name, Repeater::new(executor(steps)));
        } else if step.contains_key("pull") {
            let name = string(step, "pull", index)?;
            let count = match step.get("count") {
                Some(JsValue::Number(count)) if count.fract() == 0.0 && *count >= 1.0 => *count,
                _ => return Err(refuse(index, "count is no integer from 1")),
            };
            let mut called = 0.0;
            while called < count {
                self.call(&name, &Method::Next, index)?;
                called += 1.0;
            }
        } else if step.contains_key("return") {
            self.call(&string(step, "return", index)?, &Method::Return, index)?;
        } else if step.contains_key("throw_into") {
            let name = string(step, "throw_into", index)?;
            let Some(JsValue::String(message)) = step.get("error") else {
                return Err(refuse(index, "error is no string"));
            };
            self.call(&name, &Method::Throw(message.clone()), index)?;
        } else if step.contains_key("publish_on_thread") {
            let pub_sub = self.pub_sub(step, "publish_on_thread", index)?;
            let args = payloads(step, index)?;
            let key = string(step, "key", index)?;
            publish_on_thread(pub_sub, key, args)
                .map_err(|()| refuse(index, "the publishing thread panicked"))?;
        } else {
            let first = step
                .iter()
                .next()
                .and_then(|(key, _)| aosabi_json::to_string(&JsValue::String(key.clone())))
                .unwrap_or_else(|| String::from("undefined"));
            return Err(refuse(index, &format!("unknown step {first}")));
        }
        Ok(())
    }

    /// Returns the pub sub the string field `key` of `step` names.
    fn pub_sub(&self, step: &JsObject, key: &str, index: usize) -> Result<&'static PubSub, Halt> {
        let name = string(step, key, index)?;
        let pub_sub = self.pub_subs.borrow().get(&name).copied();
        pub_sub.ok_or_else(|| refuse(index, &format!("no pub sub is named {name}")))
    }

    /// Calls one method of the stream `name` and observes its settlement.
    ///
    /// The call takes the next number of the stream whatever the method, and
    /// a synchronous throw of the call stops the script.
    fn call(&self, name: &str, method: &Method, index: usize) -> Result<(), Halt> {
        let stream = self.streams.borrow().get(name).cloned();
        let Some(stream) = stream else {
            return Err(refuse(index, &format!("no stream is named {name}")));
        };
        let n = {
            let mut calls = self.calls.borrow_mut();
            let n = calls.get(name).copied().unwrap_or(0) + 1;
            calls.insert(String::from(name), n);
            n
        };
        let agent = &self.agent;
        let completion = match method {
            Method::Next => stream.next(agent, Value::Undefined),
            Method::Return => stream.r#return(agent, Value::Undefined),
            Method::Throw(message) => {
                let error = Value::from(ErrorObject::error(message.clone()));
                stream.throw(agent, error)
            }
        };
        match completion {
            Completion::Normal(answer) => {
                self.observe(name, n, answer);
                Ok(())
            }
            Completion::Throw(error) => Err(Halt::Thrown(error)),
        }
    }

    /// Appends the settlement of `answer` to the observations when it happens.
    ///
    /// This is `Promise.resolve(answer).then(...)`, one reaction for either
    /// settlement, so an observation costs the job it costs in the sidecar.
    fn observe(&self, name: &str, n: u32, answer: Value) {
        let agent = &self.agent;
        let on = JsString::from(name);
        let fulfilled = {
            let observations = Rc::clone(&self.observations);
            let on = on.clone();
            Box::new(move |_: &Agent, result: Value| {
                let (value, done) = iter_result_parts(&result);
                let mut record = JsObject::new();
                record.insert("on", on);
                record.insert("n", n);
                record.insert("value", encode(&value));
                record.insert("done", done);
                observations.borrow_mut().push(JsValue::Object(record));
                Completion::Normal(Value::Undefined)
            })
        };
        let rejected = {
            let observations = Rc::clone(&self.observations);
            Box::new(move |_: &Agent, error: Value| {
                let mut record = JsObject::new();
                record.insert("on", on);
                record.insert("n", n);
                record.insert("error", JsValue::Object(error_record(&error)));
                observations.borrow_mut().push(JsValue::Object(record));
                Completion::Normal(Value::Undefined)
            })
        };
        let _ = Promise::resolve(agent, answer).then(agent, Some(fulfilled), Some(rejected));
    }

    /// Runs jobs and timers until nothing is left that can run.
    ///
    /// The agent is polled once until it is idle. A pump that waits on an
    /// empty slot holds a waker, which keeps the agent from being idle, and
    /// the poll then answers pending, which is the end of the settle too,
    /// because nothing of the script can wake the pump any more.
    fn settle(&self) {
        let mut idle = self.agent.drain_until_idle();
        let mut context = Context::from_waker(Waker::noop());
        let _ = Pin::new(&mut idle).poll(&mut context);
    }
}

/// Returns the string field `key` of `step`, `stepString` of the sidecar.
fn string(step: &JsObject, key: &str, index: usize) -> Result<String, Halt> {
    match step.get(key) {
        Some(JsValue::String(value)) => Ok(to_string_lossy(value).into_owned()),
        _ => Err(refuse(index, &format!("{key} is no string"))),
    }
}

/// Returns the list `args` of `step` as payloads of the pub sub.
fn payloads(step: &JsObject, index: usize) -> Result<Vec<SharedPayload>, Halt> {
    let Some(JsValue::Array(args)) = step.get("args") else {
        return Err(refuse(index, "args is no list"));
    };
    Ok(args
        .iter()
        .map(|arg| Arc::new(decode(arg)) as SharedPayload)
        .collect())
}

/// Returns a JSON value of a step with the marker of `undefined` read back.
fn decode(value: &JsValue) -> JsValue {
    match value {
        JsValue::Object(object)
            if object.len() == 1 && matches!(object.get(UNDEFINED), Some(JsValue::Bool(true))) =>
        {
            JsValue::Undefined
        }
        other => other.clone(),
    }
}

/// Returns a value as JSON writes it, with `undefined` as its marker.
fn encode(value: &Value) -> JsValue {
    if matches!(value, Value::Undefined) {
        let mut marker = JsObject::new();
        marker.insert(UNDEFINED, true);
        return JsValue::Object(marker);
    }
    value.to_js()
}

/// Returns the inner steps of a `repeater` step, each checked, `executorOf` of the sidecar.
fn inner_steps(script: Option<&JsValue>, index: usize) -> Result<Vec<Inner>, Halt> {
    let Some(JsValue::Array(script)) = script else {
        return Err(refuse(index, "script is no list"));
    };
    let mut steps = Vec::with_capacity(script.len());
    for (position, step) in script.iter().enumerate() {
        let wrong =
            |reason: &str| refuse(index, &format!("inner step {position} carries {reason}"));
        let JsValue::Object(step) = step else {
            return Err(wrong(
                "not exactly one of push, sleep, tick, stop and throw",
            ));
        };
        let known: Vec<&str> = INNER_KEYS
            .into_iter()
            .filter(|key| step.contains_key(key))
            .collect();
        let [key] = known.as_slice() else {
            return Err(wrong(
                "not exactly one of push, sleep, tick, stop and throw",
            ));
        };
        let awaited = match step.get("await") {
            None => true,
            Some(JsValue::Bool(awaited)) => *awaited,
            Some(_) => return Err(wrong("an await that is no boolean")),
        };
        let value = step.get(key).cloned().unwrap_or(JsValue::Undefined);
        steps.push(match *key {
            "push" => Inner::Push {
                value: decode(&value),
                awaited,
            },
            "sleep" => match value {
                JsValue::Number(units) if units.fract() == 0.0 && units >= 0.0 => {
                    // The check above keeps the value a whole number from 0,
                    // and a sleep longer than a `u64` is no sleep of a script.
                    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    Inner::Sleep(units as u64)
                }
                _ => return Err(wrong("a sleep that is no integer from 0")),
            },
            "tick" => Inner::Tick,
            "stop" if matches!(value, JsValue::Null) => Inner::Stop(None),
            "stop" => Inner::Stop(Some(message_of(&value))),
            _ => Inner::Throw(message_of(&value)),
        });
    }
    Ok(steps)
}

/// Returns `String(value)` of a JSON value, the message of the error a step makes.
fn message_of(value: &JsValue) -> JsString {
    to_js_string(&Value::from_js(value))
}

/// Returns the executor of a repeater that runs `steps` in order.
///
/// The executor is an async function, so it runs synchronously up to its
/// first await on the first call of `next`, as the executor of the sidecar
/// does, and what it throws rejects its promise, the execution of the
/// repeater.
fn executor(steps: Vec<Inner>) -> Executor {
    Box::new(move |agent, push, stop| {
        let body = async move {
            for step in &steps {
                match start(step, &push, &stop) {
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
    })
}

/// Runs the synchronous half of one inner step and returns what the step awaits.
///
/// The agent is read here and dropped before the await, because the agent
/// keeps the future of the executor, and a future that kept the agent would
/// keep both alive. `Err` is the value the step throws out of the executor.
fn start(
    action: &Inner,
    push: &PushHandle,
    stop: &StopHandle,
) -> Result<Option<PromiseFuture>, Value> {
    let Some(agent) = Agent::current() else {
        return Ok(None);
    };
    match action {
        Inner::Push { value, awaited } => {
            let pushed = push.push(&agent, Value::from_js(value))?;
            Ok(awaited.then(|| pushed.awaited(&agent).into_future()))
        }
        Inner::Sleep(units) => Ok(Some(agent.sleep(*units).into_future())),
        Inner::Tick => Ok(Some(tick())),
        Inner::Stop(None) => {
            stop.stop(&agent, None);
            Ok(None)
        }
        Inner::Stop(Some(message)) => {
            stop.stop(
                &agent,
                Some(Value::from(ErrorObject::error(message.clone()))),
            );
            Ok(None)
        }
        Inner::Throw(message) => Err(Value::from(ErrorObject::error(message.clone()))),
    }
}

/// Publishes `args` to `key` of `pub_sub` from a second operating system thread and waits for it.
///
/// `Err` answers a thread that panicked or that the host refused to start.
fn publish_on_thread(
    pub_sub: &'static PubSub,
    key: String,
    args: Vec<SharedPayload>,
) -> Result<(), ()> {
    // The devtool is a tool of the build host and part of no served binary,
    // so the thread rule of Documentation/architecture.md section 14 does not reach it, and
    // this thread is the one publish from a foreign thread that the pub sub
    // of a service has to carry.
    #[allow(
        clippy::disallowed_methods,
        reason = "a tool of the build host that publishes from a second thread on purpose"
    )]
    let publisher = thread::Builder::new()
        .name(String::from("aosabi-devtool publisher"))
        .spawn(move || pub_sub.publish_args(&key, &args));
    publisher.map_err(|_| ())?.join().map_err(|_| ())
}
