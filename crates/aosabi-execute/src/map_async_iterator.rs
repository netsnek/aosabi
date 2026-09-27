// SPDX-License-Identifier: MIT
/*
 * aosabi-execute map_async_iterator: the async iterator that maps the values
 * of another through a callback.
 *
 * The one mapAsyncIterator, which envelop and @graphql-tools/utils re-export.
 *
 * Derived from whatwg-node 1.3.2, cjs/index.js, MIT.
 */

//! The `mapAsyncIterator` of `@whatwg-node/promise-helpers`.
//!
//! The executor maps the source stream of a subscription to its responses
//! through this helper, envelop maps the stream through the `onNext` handlers
//! of its plugins, and Yoga maps it to masked errors, all through the one
//! function of `@whatwg-node/promise-helpers` 1.3.2. It answers an
//! [`IteratorObject`] whose `next` reads the next result of the source and
//! maps its value through `onNext`, and whose `return` and `throw` pass
//! through to the source.
//!
//! The reference answers a fake promise in two places, a value or an error
//! with a `then` that runs at once. A fake promise is the value itself here,
//! on the synchronous path of [󰀁](crate::handle_maybe_promise()), and a fake
//! promise that rejects is a throw on that path. Where the reference calls
//! `.then` on the answer of a method of the source and that answer is a value,
//! the value becomes `Promise.resolve(value)` first, which costs jobs the
//! reference does not run and changes no frame, because each frame of one
//! stream waits for the one before it.
//!
//! The module names `core` and `alloc` and no item of `std`, and its state
//! is shared through `Rc`.

use alloc::{
    boxed::Box,
    rc::Rc, //
};
use core::cell::RefCell;

use crate::{
    execute::to_boolean,
    handle_maybe_promise::handle_maybe_promise,
    promise::{
        Agent,
        Promise,
        Reaction, //
    },
    value::{
        Completion,
        ErrorObject,
        Function,
        IteratorObject,
        ObjectLike,
        Value,
        get_async_iterator,
        get_method,
        is_async_iterable,
        iter_result,
        iter_result_parts, //
    }, //
};

/// A callback over a value, `onNext` and `onError` of the reference.
pub type OnValue = Rc<dyn Fn(&Agent, Value) -> Completion>;

/// A callback with no argument, `onEnd` of the reference.
pub type OnEnd = Rc<dyn Fn(&Agent) -> Completion>;

/// What the three methods of a mapped iterator share.
// An `unsafe` cell would skip each borrow flag.
struct Mapped {
    /// The source `iterator` after the call of its `[Symbol.asyncIterator]`.
    iterator: Value,
    /// What that call threw, which every method throws.
    failure: Option<Value>,
    /// The method `return` the source had when it was mapped, `$return` of the reference.
    r#return: Option<Function>,
    /// The callback `onNext`.
    on_next: OnValue,
    /// The callback `onError`, which defines `mapReject`.
    on_error: Option<OnValue>,
    /// The callback `onEnd`, which defines `onEndWithValue`.
    on_end: Option<OnEnd>,
    /// What the first `mapReject` answered, `onErrorResult` of the reference.
    on_error_result: RefCell<Option<Value>>,
    /// What the first `onEndWithValue` answered, `onEndWithValueResult` of the reference.
    on_end_result: RefCell<Option<Value>>,
}

/// Returns the [`IteratorObject`] of `mapAsyncIterator(iterator, onNext, onError, onEnd)`.
///
/// An async iterable is replaced by the iterator of its
/// `[Symbol.asyncIterator]` method first, and the method `return` of the
/// source is read once, here, as the reference reads `$return`. A throw of
/// that call leaves the function in the reference, while the answer here is a
/// value, so the throw moves to every method of the answer. No object of the
/// model throws there, because [`IteratorObject`] answers itself.
pub fn map_async_iterator(
    agent: &Agent,
    iterator: Value,
    on_next: OnValue,
    on_error: Option<OnValue>,
    on_end: Option<OnEnd>,
) -> Value {
    let (iterator, failure) = if is_async_iterable(&iterator) {
        match get_async_iterator(agent, &iterator) {
            Completion::Normal(iterator) => (iterator, None),
            Completion::Throw(error) => (Value::Undefined, Some(error)),
        }
    } else {
        (iterator, None)
    };
    let r#return = get_method(&iterator, "return");
    let mapped = Rc::new(Mapped {
        iterator,
        failure,
        r#return,
        on_next,
        on_error,
        on_end,
        on_error_result: RefCell::new(None),
        on_end_result: RefCell::new(None),
    });
    let next = {
        let mapped = Rc::clone(&mapped);
        Function::new("next", move |agent, _| mapped.next(agent))
    };
    let r#return = {
        let mapped = Rc::clone(&mapped);
        Function::new("return", move |agent, _| mapped.r#return(agent))
    };
    let throw = Function::new("throw", move |agent, args| {
        let error = args.first().cloned().unwrap_or(Value::Undefined);
        mapped.throw(agent, error)
    });
    let object: Rc<dyn ObjectLike> =
        IteratorObject::new("mapAsyncIterator", next, Some(r#return), Some(throw));
    Value::Object(object)
}

/// Returns `{value, done: false}`.
#[must_use]
pub fn iterator_result(value: Value) -> Value {
    iter_result(value, false)
}

/// Returns `iteratorResult` as the handler of a `then`.
fn iterator_result_reaction() -> Reaction {
    Box::new(|_, value| Completion::Normal(iterator_result(value)))
}

/// Returns `rethrow`, the handler that throws `error` whatever it receives.
fn rethrow(error: Value) -> Reaction {
    Box::new(move |_, _| Completion::Throw(error))
}

/// Performs `answer.then(on_fulfilled, on_rejected)` on what a method of the source answered.
///
/// A promise takes the two handlers. Any other value stands for a fake
/// promise of the source and becomes `Promise.resolve(value)`, the reading
/// of the module doc for a consumer that calls `.then`. A throw passes
/// through and reaches neither handler, which is what the reference does
/// with a throw of the call itself, since that throw leaves the method
/// before its `.then`.
fn then(
    agent: &Agent,
    answer: Completion,
    on_fulfilled: Reaction,
    on_rejected: Option<Reaction>,
) -> Completion {
    let promise = match answer {
        Completion::Normal(Value::Promise(promise)) => promise,
        Completion::Normal(value) => Promise::resolve(agent, value),
        Completion::Throw(error) => return Completion::Throw(error),
    };
    Completion::Normal(Value::Promise(promise.then(
        agent,
        Some(on_fulfilled),
        on_rejected,
    )))
}

/// Reads a memo of `||=`, which counts only when it is truthy.
fn memo(cell: &RefCell<Option<Value>>) -> Option<Value> {
    cell.borrow().clone().filter(to_boolean)
}

impl Mapped {
    /// Performs `next()`, which maps the next result of the source.
    ///
    /// The method `next` of the source is read at each call, and a source
    /// without one throws the `TypeError` V8 writes for `iterator.next()`.
    fn next(self: &Rc<Self>, agent: &Agent) -> Completion {
        if let Some(error) = &self.failure {
            return Completion::Throw(error.clone());
        }
        let Some(next) = get_method(&self.iterator, "next") else {
            return Completion::Throw(Value::from(ErrorObject::type_error(
                "iterator.next is not a function",
            )));
        };
        let answer = next.call(agent, &[]);
        then(
            agent,
            answer,
            self.map_result_reaction(),
            self.map_reject_reaction(),
        )
    }

    /// Performs `return()`, which closes the source and ends the mapped iterator.
    ///
    /// Without a method `return` of the source the answer is
    /// `{value: undefined, done: true}`, the fake promise of the reference,
    /// and `onEnd` runs on it through `.then`.
    fn r#return(self: &Rc<Self>, agent: &Agent) -> Completion {
        if let Some(error) = &self.failure {
            return Completion::Throw(error.clone());
        }
        let answer = match &self.r#return {
            Some(r#return) => then(
                agent,
                r#return.call(agent, &[]),
                self.map_result_reaction(),
                self.map_reject_reaction(),
            ),
            None => Completion::Normal(iter_result(Value::Undefined, true)),
        };
        if self.on_end.is_none() {
            return answer;
        }
        let this = Rc::clone(self);
        let on_end_with_value: Reaction =
            Box::new(move |agent, value| this.on_end_with_value(agent, value));
        then(agent, answer, on_end_with_value, None)
    }

    /// Performs `throw(error)`, which hands `error` to the source.
    ///
    /// A source with a method `throw` gets the error and its answer is mapped.
    /// A source with a method `return` is closed and the error rethrown. Any
    /// other source leaves the fake promise that rejects, which is a throw on
    /// the synchronous path.
    ///
    /// When the source is itself a mapped iterator of that last kind, its
    /// throw passes through [`then`] here, so `onError` never runs, where the
    /// reference hands the error of the fake promise to `mapReject` at once.
    /// No consumer of the model calls `throw` on such a chain.
    fn throw(self: &Rc<Self>, agent: &Agent, error: Value) -> Completion {
        if let Some(failure) = &self.failure {
            return Completion::Throw(failure.clone());
        }
        if let Some(throw) = get_method(&self.iterator, "throw") {
            let answer = throw.call(agent, &[error]);
            return then(
                agent,
                answer,
                self.map_result_reaction(),
                self.map_reject_reaction(),
            );
        }
        if self.r#return.is_some() {
            return self.abrupt_close(agent, error);
        }
        Completion::Throw(error)
    }

    /// Performs `mapResult(result)`, which maps the value of a result that is not done.
    ///
    /// A result that is done passes through, or through `onEndWithValue` when
    /// there is an `onEnd`. The value of any other result is awaited when it
    /// is a promise and then handed to `onNext`, and a throw or a rejection of
    /// `onNext` closes the source through `abruptClose` when the source has a
    /// method `return`.
    fn map_result(self: &Rc<Self>, agent: &Agent, result: Value) -> Completion {
        let (value, done) = iter_result_parts(&result);
        if done {
            return match self.on_end {
                Some(_) => self.on_end_with_value(agent, result),
                None => Completion::Normal(result),
            };
        }
        let this = Rc::clone(self);
        handle_maybe_promise(
            agent,
            |_| Completion::Normal(value),
            Box::new(move |agent, value| {
                let on_next = Rc::clone(&this.on_next);
                handle_maybe_promise(
                    agent,
                    move |agent| on_next(agent, value),
                    iterator_result_reaction(),
                    this.abrupt_close_reaction(),
                )
            }),
            None,
        )
    }

    /// Performs `mapReject(error)`, which maps a rejection of the source through `onError` once.
    ///
    /// The first answer is kept as `onErrorResult` and every later rejection
    /// answers it again. A throw of `onError` with no `abruptClose` keeps
    /// nothing, because the assignment of `||=` never runs.
    fn map_reject(self: &Rc<Self>, agent: &Agent, on_error: &OnValue, error: Value) -> Completion {
        if let Some(result) = memo(&self.on_error_result) {
            return Completion::Normal(result);
        }
        let this = Rc::clone(self);
        let on_error = Rc::clone(on_error);
        let completion = handle_maybe_promise(
            agent,
            |_| Completion::Normal(error),
            Box::new(move |agent, error| {
                handle_maybe_promise(
                    agent,
                    move |agent| on_error(agent, error),
                    iterator_result_reaction(),
                    this.abrupt_close_reaction(),
                )
            }),
            None,
        );
        if let Completion::Normal(result) = &completion {
            *self.on_error_result.borrow_mut() = Some(result.clone());
        }
        completion
    }

    /// Performs `onEndWithValue(value)`, which runs `onEnd` once and answers `value`.
    ///
    /// The first answer is kept as `onEndWithValueResult`, so `onEnd` runs
    /// once however the iterator ends, and every later end answers the value
    /// of the first. A throw or a rejection of `onEnd` answers the value as
    /// well.
    fn on_end_with_value(self: &Rc<Self>, agent: &Agent, value: Value) -> Completion {
        if let Some(result) = memo(&self.on_end_result) {
            return Completion::Normal(result);
        }
        let Some(on_end) = self.on_end.clone() else {
            return Completion::Normal(value);
        };
        let kept = value.clone();
        let completion = handle_maybe_promise(
            agent,
            move |agent| on_end(agent),
            Box::new(move |_, _| Completion::Normal(value)),
            Some(Box::new(move |_, _| Completion::Normal(kept))),
        );
        if let Completion::Normal(result) = &completion {
            *self.on_end_result.borrow_mut() = Some(result.clone());
        }
        completion
    }

    /// Performs `abruptClose(error)`, which closes the source and then throws `error`.
    ///
    /// The answer of `$return` rejects with `error` once it settles, whichever
    /// way it settles.
    fn abrupt_close(&self, agent: &Agent, error: Value) -> Completion {
        let Some(r#return) = &self.r#return else {
            return Completion::Throw(error);
        };
        let answer = r#return.call(agent, &[]);
        then(agent, answer, rethrow(error.clone()), Some(rethrow(error)))
    }

    /// Returns `mapResult` as the handler of a `then`.
    fn map_result_reaction(self: &Rc<Self>) -> Reaction {
        let this = Rc::clone(self);
        Box::new(move |agent, result| this.map_result(agent, result))
    }

    /// Returns `mapReject` as the handler of a `then`, which exists when there is an `onError`.
    fn map_reject_reaction(self: &Rc<Self>) -> Option<Reaction> {
        let on_error = self.on_error.clone()?;
        let this = Rc::clone(self);
        Some(Box::new(move |agent, error| {
            this.map_reject(agent, &on_error, error)
        }))
    }

    /// Returns `abruptClose` as the handler of a `then`, which exists when the source has a
    /// method `return`.
    fn abrupt_close_reaction(self: &Rc<Self>) -> Option<Reaction> {
        self.r#return.as_ref()?;
        let this = Rc::clone(self);
        Some(Box::new(move |agent, error| {
            this.abrupt_close(agent, error)
        }))
    }
}
