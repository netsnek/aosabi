// SPDX-License-Identifier: MIT
/*
 * aosabi-execute flatten_async_iterable: the flattening of an async iterable
 * of async iterables into one.
 *
 * Derived from @graphql-tools/executor 1.5.1, cjs/execution/flattenAsyncIterable.js, MIT.
 */

//! The flattening of an async iterable of async iterables.
//!
//! The subscription entry maps every event of the source stream to an async
//! iterable of results and hands the stream of those iterables to
//! [`flatten_async_iterable`], whose answer yields every result of each in
//! turn. The reference writes the three methods by hand rather than as an
//! async generator, because calling `return` on a generator would not
//! interrupt its `for await`.
//!
//! The three methods are async functions. Each is [`Agent::spawn`] over a
//! Rust future that awaits a [`Promise`] where the reference writes `await`,
//! so a call runs to its first await at once and every await costs the job it
//! costs in the reference. The member `[Symbol.asyncDispose]` is left out,
//! because nothing disposes of a stream.
//!
//! The module names `core` and `alloc` and no item of `std`, and its state
//! is shared through `Rc`.

use alloc::{
    boxed::Box,
    format,
    rc::Rc,
    vec, //
};
use core::cell::RefCell;

use crate::{
    promise::{
        Agent,
        Promise,
        ResolvingFunctions, //
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
        iter_result,
        iter_result_parts, //
    }, //
};

/// What the three methods of a flattened iterator share.
struct Flatten {
    /// The iterator of the outer iterable, `topIterator` of the reference.
    top_iterator: Value,
    /// What the call of `[Symbol.asyncIterator]` of the outer iterable threw.
    failure: Option<Value>,
    /// The variables the methods assign.
    // An `unsafe` cell would skip the flag.
    state: RefCell<FlattenState>,
}

/// The `let` bindings of `flattenAsyncIterable` that the methods assign.
#[derive(Default)]
struct FlattenState {
    /// The iterator of the inner iterable being drained, `currentNestedIterator`.
    current: Option<Value>,
    /// The promise a concurrent `next` waits on while one call fetches the
    /// next inner iterator, `waitForCurrentNestedIterator`.
    wait: Option<Promise>,
    /// Whether the flattened iterator has ended, `done`.
    done: bool,
}

/// Given an async iterable of async iterables, flattens all yielded results into a single
/// async iterable.
///
/// The outer iterable is replaced by its iterator at once. A throw of that call
/// leaves the function in the reference, while the answer here is a value, so
/// the throw moves to every method of the answer, as
/// [󰀂](crate::map_async_iterator()) moves it. The one caller hands over the
/// answer of `mapAsyncIterator`, which answers itself, so no answer reaches
/// that throw.
#[must_use]
pub fn flatten_async_iterable(agent: &Agent, iterable: &Value) -> Value {
    let (top_iterator, failure) = match get_async_iterator(agent, iterable) {
        Completion::Normal(iterator) => (iterator, None),
        Completion::Throw(error) => (Value::Undefined, Some(error)),
    };
    let flatten = Rc::new(Flatten {
        top_iterator,
        failure,
        state: RefCell::new(FlattenState::default()),
    });
    let next = {
        let flatten = Rc::clone(&flatten);
        Function::new("next", move |agent, _| flatten.next(agent))
    };
    let r#return = {
        let flatten = Rc::clone(&flatten);
        Function::new("return", move |agent, _| flatten.r#return(agent))
    };
    let throw = Function::new("throw", move |agent, args| {
        let error = args.first().cloned().unwrap_or(Value::Undefined);
        flatten.throw(agent, error)
    });
    let object: Rc<dyn ObjectLike> =
        IteratorObject::new("flattenAsyncIterable", next, Some(r#return), Some(throw));
    Value::Object(object)
}

/// Returns the reason an await rejects with when no agent runs on the thread.
///
/// No analogue in the reference, where a job queue always exists. A method
/// of the flattened iterator runs inside a future its agent polls, so no
/// answer reaches it.
fn no_agent_error() -> Value {
    Value::from(ErrorObject::type_error(
        "A promise is awaited outside of an agent.",
    ))
}

/// Returns the agent that polls the running future.
fn current_agent() -> Result<Agent, Value> {
    Agent::current().ok_or_else(no_agent_error)
}

/// Performs `await value` on what a call answered, and passes a throw of the call through.
///
/// The agent is read before the await and is not kept across it, so the
/// future does not keep its agent alive.
async fn await_completion(completion: Completion) -> Result<Value, Value> {
    let value = completion.into_result()?;
    let promise = Promise::resolve(&current_agent()?, value);
    promise.await
}

/// Calls the method `name` of `value` with `args`, `value?.[name]?.(...args)`.
///
/// A nullish `value` or a missing method answers `undefined` without a call.
fn optional_call(agent: &Agent, value: Option<&Value>, name: &str, args: &[Value]) -> Completion {
    let method = value.and_then(|value| get_method(value, name));
    match method {
        Some(method) => method.call(agent, args),
        None => Completion::Normal(Value::Undefined),
    }
}

/// Returns whether `a` and `b` are the same object, which is `a === b` for two objects.
fn same_object(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}

impl Flatten {
    /// Performs `next()`, the async function at `flattenAsyncIterable.js:23-71`.
    fn next(self: &Rc<Self>, agent: &Agent) -> Completion {
        if let Some(error) = &self.failure {
            return Completion::Throw(error.clone());
        }
        let this = Rc::clone(self);
        let promise = agent.spawn(Box::pin(async move { this.next_body().await.into() }));
        Completion::Normal(Value::Promise(promise))
    }

    /// Runs the body of `next` and returns its promise, for `return await next()`.
    async fn await_next(self: &Rc<Self>) -> Result<Value, Value> {
        let promise = {
            let agent = current_agent()?;
            let this = Rc::clone(self);
            agent.spawn(Box::pin(async move { this.next_body().await.into() }))
        };
        promise.await
    }

    /// The body of `next`, whose `catch` ends the flattened iterator and rethrows.
    async fn next_body(self: &Rc<Self>) -> Result<Value, Value> {
        if self.state.borrow().done {
            return Ok(iter_result(Value::Undefined, true));
        }
        let result = self.next_try().await;
        if result.is_err() {
            self.state.borrow_mut().done = true;
        }
        result
    }

    /// The `try` block of `next`.
    ///
    /// Without a current inner iterator the call either waits for the call
    /// that fetches one or fetches it itself, and with one it pulls the inner
    /// iterator and moves on once that one is done. The inner iterator is
    /// dropped only when it is still the current one, because a concurrent
    /// call may have moved on already.
    async fn next_try(self: &Rc<Self>) -> Result<Value, Value> {
        let (current, wait) = {
            let state = self.state.borrow();
            (state.current.clone(), state.wait.clone())
        };
        if let Some(current) = current {
            let answer = call_next(&current, "currentNestedIterator")?;
            let nested_iterator_result = await_completion(answer).await?;
            let (_, done) = iter_result_parts(&nested_iterator_result);
            if !done {
                return Ok(nested_iterator_result);
            }
            {
                let mut state = self.state.borrow_mut();
                if state
                    .current
                    .as_ref()
                    .is_some_and(|value| same_object(value, &current))
                {
                    state.current = None;
                }
            }
            return self.await_next().await;
        }
        // Somebody else is getting it already.
        if let Some(wait) = wait {
            wait.await?;
            return self.await_next().await;
        }
        // Nobody else is getting it. We should!
        let resolve = self.start_wait()?;
        let answer = call_next(&self.top_iterator, "topIterator")?;
        let top_iterator_result = await_completion(answer).await?;
        let (value, done) = iter_result_parts(&top_iterator_result);
        if done {
            self.state.borrow_mut().done = true;
            return self.await_next().await;
        }
        {
            let agent = current_agent()?;
            let nested = get_async_iterator(&agent, &value).into_result()?;
            let mut state = self.state.borrow_mut();
            state.current = Some(nested);
            state.wait = None;
            drop(state);
            resolve.resolve(&agent, Value::Undefined);
        }
        self.await_next().await
    }

    /// Sets `waitForCurrentNestedIterator` to a new promise and returns its resolve function.
    fn start_wait(&self) -> Result<ResolvingFunctions, Value> {
        let (wait, resolve) = Promise::with_resolvers(&current_agent()?);
        self.state.borrow_mut().wait = Some(wait);
        Ok(resolve)
    }

    /// Performs `return()`, the async function at `flattenAsyncIterable.js:74-78`.
    ///
    /// The flattened iterator ends, and the current inner iterator and the
    /// outer one are both closed before the answer settles as done.
    fn r#return(self: &Rc<Self>, agent: &Agent) -> Completion {
        if let Some(error) = &self.failure {
            return Completion::Throw(error.clone());
        }
        let this = Rc::clone(self);
        let promise = agent.spawn(Box::pin(async move {
            let closed = this.close_both("return", &[]);
            match await_completion(closed).await {
                Ok(_) => Completion::Normal(iter_result(Value::Undefined, true)),
                Err(error) => Completion::Throw(error),
            }
        }));
        Completion::Normal(Value::Promise(promise))
    }

    /// Performs `throw(error)`, the async function at `flattenAsyncIterable.js:79-84`.
    ///
    /// The flattened iterator ends, the error is thrown into the current inner
    /// iterator and the outer one, and the answer rejects with it.
    fn throw(self: &Rc<Self>, agent: &Agent, error: Value) -> Completion {
        if let Some(failure) = &self.failure {
            return Completion::Throw(failure.clone());
        }
        let this = Rc::clone(self);
        let promise = agent.spawn(Box::pin(async move {
            let closed = this.close_both("throw", core::slice::from_ref(&error));
            match await_completion(closed).await {
                Ok(_) => Completion::Throw(error),
                Err(error) => Completion::Throw(error),
            }
        }));
        Completion::Normal(Value::Promise(promise))
    }

    /// Sets `done` and returns `Promise.all` over the method `name` of both iterators.
    ///
    /// This is `Promise.all([currentNestedIterator?.[name]?.(...args),
    /// topIterator[name]?.(...args)])`, whose two calls run in that order, and
    /// a throw of either call is the throw of the async function.
    fn close_both(&self, name: &str, args: &[Value]) -> Completion {
        let agent = match current_agent() {
            Ok(agent) => agent,
            Err(error) => return Completion::Throw(error),
        };
        let current = {
            let mut state = self.state.borrow_mut();
            state.done = true;
            state.current.clone()
        };
        let nested = match optional_call(&agent, current.as_ref(), name, args) {
            Completion::Normal(value) => value,
            Completion::Throw(error) => return Completion::Throw(error),
        };
        let top = match optional_call(&agent, Some(&self.top_iterator), name, args) {
            Completion::Normal(value) => value,
            Completion::Throw(error) => return Completion::Throw(error),
        };
        Completion::Normal(Value::Promise(Promise::all(&agent, vec![nested, top])))
    }
}

/// Calls `next()` on the iterator the binding `name` holds.
///
/// An iterator without the method throws the `TypeError` V8 writes for the
/// call, which names the binding, `topIterator.next is not a function` for
/// the outer one.
fn call_next(iterator: &Value, name: &str) -> Result<Completion, Value> {
    let Some(next) = get_method(iterator, "next") else {
        return Err(Value::from(ErrorObject::type_error(format!(
            "{name}.next is not a function"
        ))));
    };
    Ok(next.call(&current_agent()?, &[]))
}
