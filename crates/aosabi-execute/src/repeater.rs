// SPDX-License-Identifier: MIT
/*
 * aosabi-execute repeater: the Repeater class of @repeaterjs/repeater 3.0.6.
 *
 * The overflow error, the states, the push and stop functions and the class.
 * The buffers at :123-235 and the statics race, merge, zip and latest at
 * :542-916 are not ported, because no construct of a service reaches them.
 *
 * Derived from @repeaterjs/repeater 3.0.6, cjs/repeater.js, MIT.
 */

//! The `Repeater` of `@repeaterjs/repeater`.
//!
//! A subscription resolver answers a `Repeater`, and
//! `@graphql-yoga/subscription` re-exports the class and none of its buffers.
//! A repeater runs its executor `(push, stop) => ...` on the first call of
//! `next`, queues what the executor pushes until a `next` takes it, and queues
//! a `next` until a push answers it. Each queue holds at most
//! [`MAX_QUEUE_LENGTH`] calls, and the call after that throws the
//! `RepeaterOverflowError`.
//!
//! Every function keeps the name it has in `repeater.js`, in snake case, and
//! every promise of a record is a [`Promise`] of the agent the call runs on,
//! so each settlement costs the job it costs in the reference. The record of
//! the reference lives in a `WeakMap` keyed by the instance, and here the
//! instance holds it, so the error `WeakMap error` has no reading. The buffer
//! of the reference is always `undefined` here, so every branch that reads it
//! takes its `undefined` arm.
//!
//! | Reference | Here |
//! | --- | --- |
//! | `RepeaterOverflowError`, `:101-121` | [`repeater_overflow_error`] |
//! | the states, `swallow`, `consumeExecution` and `createIteration`, `:236-281` | `State` and private functions of the same names |
//! | `stop`, `finish` and `reject`, `:282-365` | private functions of the same names, and [`StopHandle::stop`] |
//! | `push`, `:366-432` | [`PushHandle::push`] and [`PushResult`] |
//! | `createStop` and `execute`, `:433-459` | private functions of the same names |
//! | the class, `:460-540` | [`Repeater`] |
//!
//! The module names `core` and `alloc` and no item of `std`, and its state is
//! shared through `Rc`.

use alloc::{
    boxed::Box,
    collections::VecDeque,
    format,
    rc::Rc,
    string::String,
    vec::Vec, //
};
use core::cell::{
    Cell,
    RefCell, //
};

use aosabi_json::JsString;

use crate::{
    promise::{
        Agent,
        Job,
        Promise,
        Reaction,
        ResolvingFunctions, //
    },
    value::{
        Completion,
        ErrorName,
        ErrorObject,
        Function,
        ObjectLike,
        Value,
        iter_result, //
    }, //
};

/// The maximum number of push or next operations which may exist on a single repeater.
pub const MAX_QUEUE_LENGTH: usize = 1024;

/// Returns an instance of `RepeaterOverflowError` with `message`.
///
/// An error subclass which is thrown when there are too many pending push or
/// next operations on a single repeater. The constructor defines `name` as an
/// own property that is not enumerable, so the error prints as
/// `RepeaterOverflowError` and carries no enumerable property.
#[must_use]
pub fn repeater_overflow_error(message: &str) -> Value {
    Value::from(ErrorObject::Plain {
        name: ErrorName::Other(JsString::from("RepeaterOverflowError")),
        message: JsString::from(message),
    })
}

/// The executor `(push, stop) => ...` a repeater runs on its first `next`.
///
/// What it returns becomes the execution, which the class resolves through
/// the resolve function, so a promise costs the jobs of a thenable, and a
/// throw rejects the execution.
pub type Executor = Box<dyn FnOnce(&Agent, PushHandle, StopHandle) -> Completion>;

/// The states of a repeater, which are ordered, and a repeater may only advance to higher states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    /// The initial state of the repeater.
    Initial,
    /// Repeaters advance to this state the first time the next method is called on the repeater.
    Started,
    /// Repeaters advance to this state when the stop function is called.
    Stopped,
    /// Repeaters advance to this state when there are no values left to be pulled from the repeater.
    Done,
    /// Repeaters advance to this state if an error is thrown into the repeater.
    Rejected,
}

/// A push the repeater holds until a `next` takes it, `{resolve, value}` of `r.pushes`.
struct PendingPush {
    /// The resolve function of the promise `push` handed out.
    resolve: ResolvingFunctions,
    /// The promise of the pushed value, `valueP` of the reference.
    value: Promise,
}

/// A `next` the repeater holds until a push answers it, `{resolve, value}` of `r.nexts`.
struct PendingNext {
    /// The resolve function of the promise `next` returned.
    resolve: ResolvingFunctions,
    /// The value `next` was called with.
    value: Value,
}

/// The record of one repeater, the object `records` maps an instance to.
///
/// The functions `onnext` and `onstop` are the `NOOP` of the reference while
/// they are `None`, and a resolve function they hold stays in place after it
/// is called, as the reference keeps it, which is harmless, because a second
/// call of a resolve function does nothing.
struct Record {
    /// The executor, until the first `next` runs it.
    executor: Option<Executor>,
    /// The error the repeater ends with, `undefined` while it has none.
    err: Value,
    /// The state.
    state: State,
    /// The pushes no `next` took yet.
    pushes: VecDeque<PendingPush>,
    /// The calls of `next` no push answered yet.
    nexts: VecDeque<PendingNext>,
    /// The promise every later push chains on, `undefined` before the first push.
    pending: Option<Promise>,
    /// The promise of the executor, `undefined` before the first `next`.
    execution: Option<Promise>,
    /// The function that resolves the promise of the last push.
    onnext: Option<ResolvingFunctions>,
    /// The function that resolves the promise of `stop`.
    onstop: Option<ResolvingFunctions>,
}

/// The record as the class, the push function and the stop function share it.
// An `unsafe` cell would skip each borrow flag.
type Shared = Rc<RefCell<Record>>;

/// An instance of the class `Repeater`.
///
/// The instance is an async iterator and its own async iterable, and
/// [`ObjectLike`] answers its three methods as functions bound to it, which is
/// what a consumer reads off the prototype.
pub struct Repeater {
    /// The record.
    record: Shared,
}

/// The push function the executor receives, `push.bind(null, r)`.
#[derive(Clone)]
pub struct PushHandle {
    /// The record.
    record: Shared,
}

/// The object `push` returns, whose `then` and `catch` clear the floating flag.
///
/// The reference answers a plain object with `then`, `catch` and `finally` in
/// place of `nextP`, so that a call of `then` or `catch` on it is seen and the
/// promise counts as handled. A push into a stopped repeater answers
/// `Promise.resolve(undefined)` itself, which has no flag.
#[derive(Clone)]
pub struct PushResult {
    /// The promise of the push, `nextP` of the reference.
    next: Promise,
    /// The floating flag, `None` for the promise of a stopped repeater.
    floating: Option<Rc<Cell<bool>>>,
}

/// The stop function the executor receives, callable and thenable.
///
/// `stop.bind(null, r)` whose `then`, `catch` and `finally` are those of the
/// promise that settles when the repeater stops.
#[derive(Clone)]
pub struct StopHandle {
    /// The record.
    record: Shared,
    /// The promise of `stop` itself, `stopP` of the reference.
    stopped: Promise,
}

/// Returns `NOOP` as the handler of a `then`, which answers `undefined`.
fn noop() -> Reaction {
    Box::new(|_, _| Completion::Normal(Value::Undefined))
}

/// Marks a promise passed in as handled.
///
/// A promise is the one thenable of the value model, so it is the one value
/// that gets the two handlers, each of which costs its job when the promise
/// settles.
fn swallow(agent: &Agent, value: &Value) {
    if let Value::Promise(promise) = value {
        let _ = promise.then(agent, Some(noop()), Some(noop()));
    }
}

/// Returns `Promise.resolve(r.execution)`.
fn resolved_execution(agent: &Agent, execution: Option<Promise>) -> Promise {
    execution.unwrap_or_else(|| Promise::resolve(agent, Value::Undefined))
}

/// Mimics the behavior of async generators where the final iteration is consumed.
///
/// The answer settles with what the execution settled with, or rejects with
/// the error the record held, which the call clears. The execution of the
/// record becomes a promise that fulfils with `undefined` either way, so a
/// later call answers `undefined`.
fn consume_execution(agent: &Agent, r: &Shared) -> Promise {
    let (err, execution) = {
        let record = r.borrow();
        (record.err.clone(), record.execution.clone())
    };
    let execution = resolved_execution(agent, execution).then(
        agent,
        Some(Box::new(move |_, value| {
            if err.is_nullish() {
                Completion::Normal(value)
            } else {
                Completion::Throw(err)
            }
        })),
        None,
    );
    let consumed = execution.then(agent, Some(noop()), Some(noop()));
    let pending = {
        let mut record = r.borrow_mut();
        record.err = Value::Undefined;
        record.execution = Some(consumed);
        record.pending.clone()
    };
    match pending {
        None => execution,
        Some(pending) => pending.then(
            agent,
            Some(Box::new(move |_, _| {
                Completion::Normal(Value::Promise(execution))
            })),
            None,
        ),
    }
}

/// Builds an iteration from `value`, whose promise is unwrapped, so that an iteration
/// never has its value property set to a promise.
///
/// Whether the iteration is done is read when it is created, and a repeater
/// that was rejected in the meantime answers the consumed execution as the
/// final iteration.
fn create_iteration(agent: &Agent, r: &Shared, value: Value) -> Promise {
    let done = r.borrow().state >= State::Done;
    let record = Rc::clone(r);
    Promise::resolve(agent, value).then(
        agent,
        Some(Box::new(move |agent, value| {
            let rejected = record.borrow().state >= State::Rejected;
            if !done && rejected {
                let iteration = consume_execution(agent, &record).then(
                    agent,
                    Some(Box::new(|_, value| {
                        Completion::Normal(iter_result(value, true))
                    })),
                    None,
                );
                return Completion::Normal(Value::Promise(iteration));
            }
            Completion::Normal(iter_result(value, done))
        })),
        None,
    )
}

/// Advances the state to `Stopped`.
///
/// The function bound and passed to the executor as its stop argument. The
/// first error passed in is kept. A repeater with no queued push finishes at
/// once, and the queued pushes of any other are resolved and stay queued, so
/// `next` drains them first.
fn stop(agent: &Agent, r: &Shared, err: Value) {
    let (onnext, onstop) = {
        let mut record = r.borrow_mut();
        if record.state >= State::Stopped {
            return;
        }
        record.state = State::Stopped;
        (record.onnext.clone(), record.onstop.clone())
    };
    if let Some(onnext) = onnext {
        onnext.resolve(agent, Value::Undefined);
    }
    if let Some(onstop) = onstop {
        onstop.resolve(agent, Value::Undefined);
    }
    let pushes: Vec<ResolvingFunctions> = {
        let mut record = r.borrow_mut();
        if record.err.is_nullish() {
            record.err = err;
        }
        record
            .pushes
            .iter()
            .map(|push| push.resolve.clone())
            .collect()
    };
    if pushes.is_empty() {
        finish(agent, r);
        return;
    }
    for push in pushes {
        push.resolve(agent, Value::Undefined);
    }
}

/// Advances the state to `Done`, which clears all pending values and ends iteration immediately.
///
/// Stopping a repeater lets `next` drain the queued pushes, while finishing
/// one answers every pending `next` with the consumed execution as a done
/// iteration and drops the queues.
fn finish(agent: &Agent, r: &Shared) {
    let state = r.borrow().state;
    if state >= State::Done {
        return;
    }
    if state < State::Stopped {
        stop(agent, r, Value::Undefined);
    }
    let nexts = {
        let mut record = r.borrow_mut();
        record.state = State::Done;
        core::mem::take(&mut record.nexts)
    };
    for next in nexts {
        let pending = r.borrow().pending.clone();
        let execution = match pending {
            None => consume_execution(agent, r),
            Some(pending) => {
                let record = Rc::clone(r);
                pending.then(
                    agent,
                    Some(Box::new(move |agent, _| {
                        Completion::Normal(Value::Promise(consume_execution(agent, &record)))
                    })),
                    None,
                )
            }
        };
        let iteration = create_iteration(agent, r, Value::Promise(execution));
        next.resolve.resolve(agent, Value::Promise(iteration));
    }
    let mut record = r.borrow_mut();
    record.pushes.clear();
    record.nexts.clear();
}

/// Advances the state to `Rejected`, when a promise passed to push rejects or a push call is unhandled.
fn reject(agent: &Agent, r: &Shared) {
    let state = r.borrow().state;
    if state >= State::Rejected {
        return;
    }
    if state < State::Done {
        finish(agent, r);
    }
    r.borrow_mut().state = State::Rejected;
}

/// Returns the handler that keeps `err` and rejects the repeater, the `catch` of a pushed value.
fn reject_with_value(r: &Shared) -> Reaction {
    let record = Rc::clone(r);
    Box::new(move |agent, err| {
        {
            let mut record = record.borrow_mut();
            if record.state < State::Stopped {
                record.err = err;
            }
        }
        reject(agent, &record);
        Completion::Normal(Value::Undefined)
    })
}

/// Returns the handler that keeps `err` and rejects the repeater, the `catch` of `r.pending`.
fn reject_with_pending(r: &Shared) -> Reaction {
    let record = Rc::clone(r);
    Box::new(move |agent, err| {
        record.borrow_mut().err = err;
        reject(agent, &record);
        Completion::Normal(Value::Undefined)
    })
}

impl PushHandle {
    /// Performs `push(value)`, whose `Err` is the synchronous throw of the overflow.
    ///
    /// The value goes to the first pending `next` when there is one, and waits
    /// in the queue of pushes otherwise. The answer settles when a `next`
    /// takes the value, rejects when a value thrown into the repeater reaches
    /// this push, and counts as floating until its `then` or its `catch` is
    /// called, so that such a rejection of a push no one handles rejects the
    /// repeater.
    ///
    /// # Errors
    ///
    /// Returns the `RepeaterOverflowError` when [`MAX_QUEUE_LENGTH`] pushes
    /// are queued already.
    pub fn push(&self, agent: &Agent, value: Value) -> Result<PushResult, Value> {
        let r = &self.record;
        swallow(agent, &value);
        let (queued, state, pending) = {
            let record = r.borrow();
            (record.pushes.len(), record.state, record.pending.clone())
        };
        if queued >= MAX_QUEUE_LENGTH {
            return Err(repeater_overflow_error(&format!(
                "No more than {MAX_QUEUE_LENGTH} pending calls to push are allowed on a single repeater."
            )));
        }
        if state >= State::Stopped {
            return Ok(PushResult {
                next: Promise::resolve(agent, Value::Undefined),
                floating: None,
            });
        }
        let value_p = match pending {
            None => Promise::resolve(agent, value),
            Some(pending) => pending.then(
                agent,
                Some(Box::new(move |_, _| Completion::Normal(value))),
                None,
            ),
        };
        let value_p = value_p.then(agent, None, Some(reject_with_value(r)));
        let next_p = next_promise(agent, r, &value_p);
        let floating = Rc::new(Cell::new(true));
        let flag = Rc::clone(&floating);
        let unhandled = next_p.then(
            agent,
            None,
            Some(Box::new(move |_, err| {
                if flag.get() {
                    Completion::Throw(err)
                } else {
                    Completion::Normal(Value::Undefined)
                }
            })),
        );
        let pending = value_p
            .then(
                agent,
                Some(Box::new(move |_, _| {
                    Completion::Normal(Value::Promise(unhandled))
                })),
                None,
            )
            .then(agent, None, Some(reject_with_pending(r)));
        r.borrow_mut().pending = Some(pending);
        Ok(PushResult {
            next: next_p,
            floating: Some(floating),
        })
    }
}

/// Hands `value_p` to the first pending `next` or queues it, and returns `nextP`.
///
/// With a pending `next` the push settles when the next `next` arrives, or at
/// once with the value of the `next` after it, and without one the push waits
/// in the queue.
fn next_promise(agent: &Agent, r: &Shared, value_p: &Promise) -> Promise {
    let next = r.borrow_mut().nexts.pop_front();
    let Some(next) = next else {
        let (next_p, resolve) = Promise::with_resolvers(agent);
        r.borrow_mut().pushes.push_back(PendingPush {
            resolve,
            value: value_p.clone(),
        });
        return next_p;
    };
    let iteration = create_iteration(agent, r, Value::Promise(value_p.clone()));
    next.resolve.resolve(agent, Value::Promise(iteration));
    let following = r.borrow().nexts.front().map(|next| next.value.clone());
    if let Some(value) = following {
        return Promise::resolve(agent, value);
    }
    let (next_p, resolve) = Promise::with_resolvers(agent);
    r.borrow_mut().onnext = Some(resolve);
    next_p
}

impl PushResult {
    /// Performs `then(onfulfilled, onrejected)`, which clears the floating flag.
    #[must_use = "the derived promise is the only way to observe a throw of a handler"]
    pub fn then(
        &self,
        agent: &Agent,
        on_fulfilled: Option<Reaction>,
        on_rejected: Option<Reaction>,
    ) -> Promise {
        if let Some(floating) = &self.floating {
            floating.set(false);
        }
        self.next.then(agent, on_fulfilled, on_rejected)
    }

    /// Performs `catch(onrejected)`, which clears the floating flag.
    #[must_use = "the derived promise is the only way to observe a throw of a handler"]
    pub fn catch(&self, agent: &Agent, on_rejected: Reaction) -> Promise {
        self.then(agent, None, Some(on_rejected))
    }

    /// Returns the promise `await` waits on, `PromiseResolve(%Promise%, next)`.
    ///
    /// The object of a push is a thenable and no promise, so `await` makes a
    /// promise and resolves it with the object, which enqueues one
    /// `PromiseResolveThenableJob`, and that job calls `then` and clears the
    /// floating flag. The answer of a stopped repeater is a promise and comes
    /// back as it is.
    #[must_use]
    pub fn awaited(&self, agent: &Agent) -> Promise {
        if self.floating.is_none() {
            return self.next.clone();
        }
        let this = self.clone();
        resolve_thenable(agent, move |agent, on_fulfilled, on_rejected| {
            this.then(agent, Some(on_fulfilled), Some(on_rejected))
        })
    }
}

/// Returns a promise resolved with a thenable whose `then` is `then`.
///
/// Translation of the Promise Resolve Functions of ECMA-262 section 27.2.1.3.2
/// for a thenable that is no promise: the promise stays pending, and one
/// `PromiseResolveThenableJob` calls `then` with fresh resolving functions.
fn resolve_thenable(
    agent: &Agent,
    then: impl FnOnce(&Agent, Reaction, Reaction) -> Promise + 'static,
) -> Promise {
    let (promise, resolving_functions) = Promise::with_resolvers(agent);
    agent.enqueue_job(Job::new(move |agent| {
        let resolve = resolving_functions.clone();
        let reject = resolving_functions;
        let _derived = then(
            agent,
            Box::new(move |agent, value| {
                resolve.resolve(agent, value);
                Completion::Normal(Value::Undefined)
            }),
            Box::new(move |agent, reason| {
                reject.reject(agent, reason);
                Completion::Normal(Value::Undefined)
            }),
        );
    }));
    promise
}

impl StopHandle {
    /// Performs `stop()` and `stop(error)`.
    ///
    /// An error makes the repeater end with that error once its queued pushes
    /// are drained, unless it holds an error already.
    pub fn stop(&self, agent: &Agent, error: Option<Value>) {
        stop(agent, &self.record, error.unwrap_or(Value::Undefined));
    }

    /// Returns the promise of `stop` itself, which settles when the repeater stops.
    #[must_use]
    pub fn stopped(&self) -> Promise {
        self.stopped.clone()
    }

    /// Returns the promise `await stop` waits on.
    ///
    /// The stop function is a thenable and no promise, so `await` resolves
    /// a fresh promise with it through one `PromiseResolveThenableJob`, whose
    /// call of `then` reaches the promise of [`StopHandle::stopped`], as
    /// [`PushResult::awaited`] does for a push.
    #[must_use]
    pub fn awaited(&self, agent: &Agent) -> Promise {
        let stopped = self.stopped.clone();
        resolve_thenable(agent, move |agent, on_fulfilled, on_rejected| {
            stopped.then(agent, Some(on_fulfilled), Some(on_rejected))
        })
    }
}

/// Creates the stop callable promise which is passed to the executor.
fn create_stop(agent: &Agent, r: &Shared) -> StopHandle {
    let (stopped, onstop) = Promise::with_resolvers(agent);
    r.borrow_mut().onstop = Some(onstop);
    StopHandle {
        record: Rc::clone(r),
        stopped,
    }
}

/// Calls the executor passed into the constructor, the first time the next method is called.
///
/// Advances the state to `Started`. The executor runs inside the constructor
/// of the promise of the execution, so its throw rejects that promise, and its
/// answer is resolved through the resolve function. A rejected execution stops
/// the repeater. The record is not borrowed while the executor runs, because
/// the executor pushes and stops synchronously.
fn execute(agent: &Agent, r: &Shared) {
    let executor = {
        let mut record = r.borrow_mut();
        if record.state >= State::Started {
            return;
        }
        record.state = State::Started;
        record.executor.take()
    };
    let push = PushHandle {
        record: Rc::clone(r),
    };
    let stop_handle = create_stop(agent, r);
    let execution = Promise::new(agent, move |agent, resolving_functions| {
        let Some(executor) = executor else {
            return Completion::Normal(Value::Undefined);
        };
        match executor(agent, push, stop_handle) {
            Completion::Normal(value) => {
                resolving_functions.resolve(agent, value);
                Completion::Normal(Value::Undefined)
            }
            Completion::Throw(error) => Completion::Throw(error),
        }
    });
    r.borrow_mut().execution = Some(execution.clone());
    let record = Rc::clone(r);
    let _ = execution.then(
        agent,
        None,
        Some(Box::new(move |agent, _| {
            stop(agent, &record, Value::Undefined);
            Completion::Normal(Value::Undefined)
        })),
    );
}

impl Repeater {
    /// Creates `new Repeater(executor)` with no buffer.
    ///
    /// The executor runs on the first call of [`Repeater::next`].
    #[must_use]
    pub fn new(executor: Executor) -> Rc<Self> {
        Rc::new(Self {
            record: Rc::new(RefCell::new(Record {
                executor: Some(executor),
                err: Value::Undefined,
                state: State::Initial,
                pushes: VecDeque::new(),
                nexts: VecDeque::new(),
                pending: None,
                execution: None,
                onnext: None,
                onstop: None,
            })),
        })
    }

    /// Performs `next(value)` and returns its promise, or the throw of the overflow.
    ///
    /// The first call runs the executor. A queued push answers at once, a
    /// stopped repeater with no queued push finishes and answers the consumed
    /// execution, and any other call waits in the queue of `next` calls.
    #[must_use]
    pub fn next(self: &Rc<Self>, agent: &Agent, value: Value) -> Completion {
        let r = &self.record;
        swallow(agent, &value);
        let (queued, state) = {
            let record = r.borrow();
            (record.nexts.len(), record.state)
        };
        if queued >= MAX_QUEUE_LENGTH {
            return Completion::Throw(repeater_overflow_error(&format!(
                "No more than {MAX_QUEUE_LENGTH} pending calls to next are allowed on a single repeater."
            )));
        }
        if state <= State::Initial {
            execute(agent, r);
        }
        let onnext = r.borrow().onnext.clone();
        if let Some(onnext) = onnext {
            onnext.resolve(agent, value.clone());
        }
        let push = r.borrow_mut().pushes.pop_front();
        if let Some(push) = push {
            r.borrow_mut().onnext = Some(push.resolve);
            let iteration = create_iteration(agent, r, Value::Promise(push.value));
            return Completion::Normal(Value::Promise(iteration));
        }
        if r.borrow().state >= State::Stopped {
            finish(agent, r);
            let execution = consume_execution(agent, r);
            let iteration = create_iteration(agent, r, Value::Promise(execution));
            return Completion::Normal(Value::Promise(iteration));
        }
        let (promise, resolve) = Promise::with_resolvers(agent);
        r.borrow_mut()
            .nexts
            .push_back(PendingNext { resolve, value });
        Completion::Normal(Value::Promise(promise))
    }

    /// Performs `return(value)`, which finishes the repeater and answers `value` as done.
    ///
    /// The execution is overridden, because return always answers the value
    /// passed in.
    #[must_use]
    pub fn r#return(self: &Rc<Self>, agent: &Agent, value: Value) -> Completion {
        let r = &self.record;
        swallow(agent, &value);
        finish(agent, r);
        let execution = r.borrow().execution.clone();
        let overridden = resolved_execution(agent, execution).then(
            agent,
            Some(Box::new(move |_, _| Completion::Normal(value))),
            None,
        );
        r.borrow_mut().execution = Some(overridden);
        let consumed = consume_execution(agent, r);
        Completion::Normal(Value::Promise(create_iteration(
            agent,
            r,
            Value::Promise(consumed),
        )))
    }

    /// Performs `throw(error)`, which hands the error to the repeater.
    ///
    /// A repeater that has not started or has stopped finishes and answers the
    /// error as its final iteration, unless it holds an error already, which
    /// is more informative for the caller. A running repeater gets the error
    /// as the value of a `next`, a rejected promise that reaches the push the
    /// executor waits on.
    #[must_use]
    pub fn throw(self: &Rc<Self>, agent: &Agent, error: Value) -> Completion {
        let r = &self.record;
        let state = r.borrow().state;
        if state <= State::Initial || state >= State::Stopped {
            finish(agent, r);
            {
                let mut record = r.borrow_mut();
                if record.err.is_nullish() {
                    record.err = error;
                }
            }
            let consumed = consume_execution(agent, r);
            return Completion::Normal(Value::Promise(create_iteration(
                agent,
                r,
                Value::Promise(consumed),
            )));
        }
        let rejected = Promise::reject(agent, error);
        self.next(agent, Value::Promise(rejected))
    }
}

/// Returns the first argument of a call, `undefined` when there is none.
fn first_argument(args: &[Value]) -> Value {
    args.first().cloned().unwrap_or(Value::Undefined)
}

impl ObjectLike for Repeater {
    /// Answers `next`, `return` and `throw` as functions bound to the instance.
    ///
    /// The three are methods of the prototype, which a property read of the
    /// instance reaches, and a method a consumer reads off the instance is
    /// called with the instance as `this`, which the binding stands for.
    fn get(self: Rc<Self>, key: &str) -> Value {
        let function = match key {
            "next" => Function::new("next", move |agent, args| {
                self.next(agent, first_argument(args))
            }),
            "return" => Function::new("return", move |agent, args| {
                self.r#return(agent, first_argument(args))
            }),
            "throw" => Function::new("throw", move |agent, args| {
                self.throw(agent, first_argument(args))
            }),
            _ => return Value::Undefined,
        };
        Value::Function(function)
    }

    /// Lists no key, because the instance keeps its record outside of its properties.
    fn own_keys(&self) -> Vec<String> {
        Vec::new()
    }

    fn constructor_name(&self) -> String {
        String::from("Repeater")
    }

    /// Answers the method `[Symbol.asyncIterator]() { return this }` at `repeater.js:538-540`.
    fn async_iterator(self: Rc<Self>) -> Option<Function> {
        Some(Function::new("[Symbol.asyncIterator]", move |_, _| {
            Completion::Normal(Value::Object(Rc::clone(&self) as Rc<dyn ObjectLike>))
        }))
    }
}
