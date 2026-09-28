// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the helpers over a single result and a stream of results.
 */

//! The helpers over a single result and a stream of results.
//!
//! A done hook reads a single result or wraps a stream of them, and
//! [`handle_stream_or_single_execution_result`] is that fork. The orchestrator
//! wraps the stream of a subscription in the wrappers of this module,
//! [`final_async_iterator`] for the hooks `on_end` and
//! [`error_async_iterator`] for the hooks `on_subscribe_error`, and maps it
//! through `mapAsyncIterator` of `aosabi-execute`, which the reference
//! re-exports.
//!
//! The two wrappers are one iterator over the source that watches one thing of
//! it, the end of the stream or a rejection of `next`, as the two functions of
//! the reference differ in that alone. The iterator reads
//! `source[Symbol.asyncIterator]()` at the first call of a method and not when
//! it is made, as the reference's `ensureIterator` does. Where the reference
//! calls `.then` or `.catch` on what a method of the source answered and that
//! answer is a value, the value becomes `Promise.resolve(value)` first, the
//! reading `mapAsyncIterator` of `aosabi-execute` takes, and a fake promise of
//! the reference is the value itself on the synchronous path. The members
//! `[Symbol.asyncDispose]` of the reference are not ported, because nothing
//! Yoga runs calls them.

use core::cell::{
    Cell,
    RefCell, //
};
use std::rc::Rc;

use aosabi_execute::{
    Agent,
    ExecutionArgs,
    ExecutionResult,
    Value,
    promise::{
        Promise,
        Reaction, //
    },
    value::{
        Completion,
        ErrorObject,
        Function,
        IteratorObject,
        ObjectLike,
        get_async_iterator,
        get_method,
        is_async_iterable as is_async_iterable_value,
        iter_result,
        iter_result_parts, //
    }, //
};

use crate::plugin::{
    ExecuteFn,
    ExecutionOutcome,
    OnExecuteDone,
    OnExecuteDoneHookResult, //
};

/// The callback of `finalAsyncIterator`, `onFinal`.
pub type OnFinal = Rc<dyn Fn()>;

/// The callback of `errorAsyncIterator`, `onError`, whose throw rejects the call of `next`.
pub type OnError = Rc<dyn Fn(&Agent, Value) -> Completion>;

/// Returns whether `source` names `__schema`, `isIntrospectionOperationString(source)`.
///
/// The test of the reference.
#[must_use]
pub fn is_introspection_operation_string(source: &str) -> bool {
    source.contains("__schema")
}

/// Returns the subscribe function of `subscribe_fn`, `makeSubscribe(subscribeFn)`.
///
/// The reference accepts the arguments of a subscription as one object or as
/// the eight positional arguments of graphql-js, `getSubscribeArgs`. A Rust
/// caller passes the one [`ExecutionArgs`], so the positional form has no
/// reading and the answer calls `subscribe_fn` as it is.
#[must_use]
pub fn make_subscribe(
    subscribe_fn: impl Fn(&Agent, ExecutionArgs) -> Result<Value, Value> + 'static,
) -> ExecuteFn {
    Rc::new(subscribe_fn)
}

/// Returns whether `outcome` is a stream of results, `isAsyncIterable(result)`.
///
/// The test of the reference over the fork of [`ExecutionOutcome`].
#[must_use]
pub fn is_async_iterable(outcome: &ExecutionOutcome) -> bool {
    matches!(outcome, ExecutionOutcome::Stream(_))
}

/// Runs `handle` on the result of `payload`, `handleStreamOrSingleExecutionResult(payload, fn)`.
///
/// A single result is handed to `handle`, which may change it in place, and
/// the answer is `None`. A stream is answered with the hook `on_next` that
/// hands each of its results to `handle`, which the orchestrator runs through
/// `mapAsyncIterator`.
pub fn handle_stream_or_single_execution_result(
    payload: &mut OnExecuteDone<'_>,
    handle: impl Fn(&mut ExecutionResult) + 'static,
) -> Option<OnExecuteDoneHookResult> {
    match payload.result {
        ExecutionOutcome::Single(result) => {
            handle(result);
            None
        }
        ExecutionOutcome::Stream(_) => Some(OnExecuteDoneHookResult {
            on_next: Some(Box::new(move |payload| {
                if let ExecutionOutcome::Single(result) = payload.result {
                    handle(result);
                }
            })),
            on_end: None,
        }),
    }
}

/// Returns the result of a stream as a value of the agent, `{ errors?, data?, extensions? }`.
///
/// A result of a stream reaches the hooks `on_next` as an [`ExecutionResult`],
/// as a single result reaches `on_execute_done`, and goes back into the stream
/// as the object [`ExecutionResult::into_value`] makes, with the keys in the
/// order of `buildResponse`, whose data stays the answer of the arena of its
/// execution. A stream answered by a hook is its iterable.
#[must_use]
pub(crate) fn outcome_value(outcome: ExecutionOutcome) -> Value {
    match outcome {
        ExecutionOutcome::Single(result) => result.into_value(),
        ExecutionOutcome::Stream(stream) => stream.into_parts().1,
    }
}

/// Returns whether `value` is truthy, ECMA-262 `ToBoolean`.
///
/// `undefined`, `null`, `false`, `+0`, `-0`, `NaN` and the empty string are
/// false, and every other value is true.
fn to_boolean(value: &Value) -> bool {
    match value {
        Value::Undefined | Value::Null => false,
        Value::Bool(boolean) => *boolean,
        Value::Number(number) => *number != 0.0 && !number.is_nan(),
        Value::String(string) => !string.is_empty(),
        _ => true,
    }
}

/// Performs `answer.then(on_fulfilled, on_rejected)` on what a method of the source answered.
///
/// A promise takes the two handlers, a value becomes `Promise.resolve(value)`
/// first, and a throw passes through, since it leaves the method before the
/// call of `.then`.
fn then(
    agent: &Agent,
    answer: Completion,
    on_fulfilled: Option<Reaction>,
    on_rejected: Option<Reaction>,
) -> Completion {
    let promise = match answer {
        Completion::Normal(Value::Promise(promise)) => promise,
        Completion::Normal(value) => Promise::resolve(agent, value),
        Completion::Throw(error) => return Completion::Throw(error),
    };
    Completion::Normal(Value::Promise(promise.then(
        agent,
        on_fulfilled,
        on_rejected,
    )))
}

/// The source of an iterator of this module and its iterator once read, `source` and `iterator` of the reference.
// An `unsafe` cell would skip the borrow flag.
struct Source {
    /// The async iterable.
    source: Value,
    /// The iterator of `source[Symbol.asyncIterator]()`, once a method read it.
    iterator: RefCell<Option<Value>>,
}

impl Source {
    /// Returns the source of `source`, whose iterator no method has read yet.
    fn new(source: Value) -> Self {
        Self {
            source,
            iterator: RefCell::new(None),
        }
    }

    /// Returns the iterator, reading it at the first call, `ensureIterator()`.
    ///
    /// A throw of `[Symbol.asyncIterator]()` keeps no iterator, so the next
    /// call reads again. A source without the method throws the `TypeError` V8
    /// writes for the call of the reference. Every caller wraps an async
    /// iterable, so no answer reaches that throw.
    fn ensure_iterator(&self, agent: &Agent) -> Result<Value, Value> {
        if let Some(iterator) = self.iterator.borrow().as_ref() {
            return Ok(iterator.clone());
        }
        if !is_async_iterable_value(&self.source) {
            return Err(Value::from(ErrorObject::type_error(
                "source[Symbol.asyncIterator] is not a function",
            )));
        }
        let iterator = get_async_iterator(agent, &self.source).into_result()?;
        *self.iterator.borrow_mut() = Some(iterator.clone());
        Ok(iterator)
    }

    /// Calls `ensureIterator().next()` and answers its completion.
    ///
    /// An iterator without `next` throws the `TypeError` V8 writes for that call.
    fn call_next(&self, agent: &Agent) -> Completion {
        let iterator = match self.ensure_iterator(agent) {
            Ok(iterator) => iterator,
            Err(error) => return Completion::Throw(error),
        };
        match get_method(&iterator, "next") {
            Some(next) => next.call(agent, &[]),
            None => Completion::Throw(Value::from(ErrorObject::type_error(
                "ensureIterator(...).next is not a function",
            ))),
        }
    }

    /// Calls `ensureIterator()[name]?.()` and answers its completion, `None` without the method.
    ///
    /// The method is called with no argument, also for `throw`, as the
    /// reference calls it.
    fn call_optional(&self, agent: &Agent, name: &str) -> Result<Option<Value>, Value> {
        let iterator = self.ensure_iterator(agent)?;
        match get_method(&iterator, name) {
            Some(method) => method.call(agent, &[]).into_result().map(Some),
            None => Ok(None),
        }
    }

    /// Performs `throw(error)` of both wrappers of the module.
    ///
    /// The answer of the method `throw` of the source when it is truthy, and
    /// the throw of `error` otherwise.
    fn throw(&self, agent: &Agent, error: Value) -> Completion {
        match self.call_optional(agent, "throw") {
            Ok(Some(promise)) if to_boolean(&promise) => Completion::Normal(promise),
            Ok(_) => Completion::Throw(error),
            Err(thrown) => Completion::Throw(thrown),
        }
    }

    /// Answers `promise || fakePromise({ done: true, value: undefined })`.
    fn or_done(promise: Option<Value>) -> Completion {
        match promise {
            Some(promise) if to_boolean(&promise) => Completion::Normal(promise),
            _ => Completion::Normal(iter_result(Value::Undefined, true)),
        }
    }
}

/// What a wrapper watches of its source, the one thing the two wrappers of the reference differ in.
enum Watch {
    /// The end of the stream, `finalAsyncIterator`, with whether the callback ran, `isDone`.
    End(Cell<bool>, OnFinal),
    /// A rejection of `next`, `errorAsyncIterator`.
    Rejection(OnError),
}

impl Watch {
    /// Runs the callback of the end once, where the watch is the end.
    fn end(&self) {
        if let Self::End(ended, on_final) = self
            && !ended.replace(true)
        {
            on_final();
        }
    }
}

/// Returns the iterator `name` over `source` that watches `watch`.
///
/// The method `next` calls `next` of the source and reads its answer as
/// `watch` asks, the method `return` calls `return` of the source, ends a
/// watch of the end, and answers a done result where the source answers none,
/// and the method `throw` calls `throw` of the source. The name reaches no
/// byte of an answer.
fn wrap(name: &'static str, source: Value, watch: Watch) -> Value {
    let state = Rc::new((Source::new(source), watch));
    let next = {
        let state = Rc::clone(&state);
        Function::new("next", move |agent, _| {
            let answer = state.0.call_next(agent);
            match &state.1 {
                Watch::End(..) => {
                    let state = Rc::clone(&state);
                    let on_fulfilled: Reaction = Box::new(move |_, result| {
                        if iter_result_parts(&result).1 {
                            state.1.end();
                        }
                        Completion::Normal(result)
                    });
                    then(agent, answer, Some(on_fulfilled), None)
                }
                Watch::Rejection(on_error) => {
                    let on_error = Rc::clone(on_error);
                    let on_rejected: Reaction =
                        Box::new(move |agent, error| match on_error(agent, error) {
                            Completion::Normal(_) => {
                                Completion::Normal(iter_result(Value::Undefined, true))
                            }
                            thrown @ Completion::Throw(_) => thrown,
                        });
                    then(agent, answer, None, Some(on_rejected))
                }
            }
        })
    };
    let r#return = {
        let state = Rc::clone(&state);
        Function::new("return", move |agent, _| {
            let promise = match state.0.call_optional(agent, "return") {
                Ok(promise) => promise,
                Err(error) => return Completion::Throw(error),
            };
            state.1.end();
            Source::or_done(promise)
        })
    };
    let throw = Function::new("throw", move |agent, args| {
        let error = args.first().cloned().unwrap_or(Value::Undefined);
        state.0.throw(agent, error)
    });
    let object: Rc<dyn ObjectLike> = IteratorObject::new(name, next, Some(r#return), Some(throw));
    Value::Object(object)
}

/// Returns the iterator over `source` that calls `on_final` once when it ends, `finalAsyncIterator(source, onFinal)`.
///
/// The call `on_final` runs after the first result that is done and at the
/// first call of `return`, whichever comes first, and never again.
#[must_use]
pub fn final_async_iterator(source: Value, on_final: OnFinal) -> Value {
    wrap(
        "finalAsyncIterator",
        source,
        Watch::End(Cell::new(false), on_final),
    )
}

/// Returns the iterator over `source` that hands a rejection of `next` to `on_error`, `errorAsyncIterator(source, onError)`.
///
/// A rejection of `next` of the source calls `on_error`, and the call of
/// `next` answers a done result when `on_error` returns and rejects with what
/// it throws.
#[must_use]
pub fn error_async_iterator(source: Value, on_error: OnError) -> Value {
    wrap("errorAsyncIterator", source, Watch::Rejection(on_error))
}
