// SPDX-License-Identifier: MIT
/*
 * aosabi-core repeater: the constructor of the typed Repeater<T> and its
 * push and stop arguments.
 *
 * The types Push and Stop and the constructor of the class, over the repeater
 * of aosabi-execute.
 *
 * Derived from @repeaterjs/repeater 3.0.6, cjs/repeater.d.ts, MIT.
 */

//! The constructor of the typed `Repeater<T>`.
//!
//! A subscription resolver of a TypeScript service answers `new Repeater<T>(async (push,
//! stop) => {...})`, and the Rust form is
//! `Repeater::new(async move |push, stop| {...})`. The executor is an async
//! closure, which runs on the agent of the stream as an async function runs,
//! synchronously up to its first await, on the first call of `next`.
//!
//! The reference passes `push` and `stop` as callables, and `stop` is a
//! callable and a promise at once, which a Rust value cannot be on stable.
//! [`Push`] and [`Stop`] are therefore handles with methods:
//!
//! | TypeScript | Rust |
//! | --- | --- |
//! | `await push(value)` | `push.push(value).await?;` |
//! | `stop()` | `stop.stop();` |
//! | `stop(error)` | `stop.stop_with(error);` |
//! | `await stop` | `stop.await;` |
//! | `throw error` inside the executor | `Err(error)?` |
//!
//! A push happens when [`Push::push`] is called, and the await of its answer
//! is what clears the floating flag of the reference, so a push whose answer
//! is never awaited stays floating as an unawaited push of the reference
//! does. Both handles reach their agent through `Agent::current`, which is
//! the agent of the stream inside the executor and inside every timer and
//! reaction of it.

use alloc::rc::Rc;
use core::{
    future::{
        Future,
        IntoFuture, //
    },
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll, //
    }, //
};

use aosabi_execute::{
    promise::{
        Agent,
        PromiseFuture,
        tick, //
    },
    repeater::{
        PushHandle,
        PushResult,
        Repeater as ExecutorRepeater,
        StopHandle, //
    },
    value::{
        Completion,
        ObjectLike,
        Value, //
    }, //
};

use crate::{
    descriptor::Repeater,
    glue::{
        IntoThrown,
        Thrown,
        into_value::IntoValue, //
    }, //
};

impl<T: IntoValue + 'static> Repeater<T> {
    /// Creates `new Repeater<T>(executor)`.
    ///
    /// The executor runs on the first call of `next`, inside an async
    /// function spawned on the agent of the stream, so it runs synchronously
    /// up to its first await, and its `Err` rejects the execution as a throw
    /// of the executor does, which stops the repeater. The future of the
    /// executor owns only its `'static` captures, so it spawns with no named
    /// type of the closure.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use aosabi::{Repeater, json, json::JsObject};
    ///
    /// fn ticks(count: f64) -> Repeater<JsObject> {
    ///     Repeater::new(async move |push, stop| {
    ///         let mut seq = 1.0;
    ///         while seq <= count {
    ///             push.push(json!({ seq })).await?;
    ///             seq += 1.0;
    ///         }
    ///         stop.stop();
    ///         Ok(())
    ///     })
    /// }
    /// ```
    pub fn new<F>(executor: F) -> Self
    where
        F: AsyncFnOnce(Push<T>, Stop) -> Result<(), Thrown> + 'static,
    {
        let inner = ExecutorRepeater::new(Box::new(move |agent, push, stop| {
            let push = Push {
                handle: push,
                marker: PhantomData,
            };
            let stop = Stop { handle: stop };
            let body = async move {
                match executor(push, stop).await {
                    Ok(()) => Completion::Normal(Value::Undefined),
                    Err(thrown) => Completion::Throw(thrown.0),
                }
            };
            Completion::Normal(Value::Promise(agent.spawn(Box::pin(body))))
        }));
        Self::from_inner(inner)
    }
}

/// The value a resolver answers is the instance of the class `Repeater`.
impl<T> IntoValue for Repeater<T> {
    fn into_value(self) -> Value {
        let object: Rc<dyn ObjectLike> = self.inner;
        Value::Object(object)
    }
}

/// The `push` argument, `Push<T, TNext>` of the pinned typings with `TNext` unused.
///
/// The handle is typed, so a `Repeater<Tick>` refuses a pushed value of
/// another type at compile time.
pub struct Push<T> {
    /// The push function of the executor's repeater.
    handle: PushHandle,
    /// The type of the values pushed.
    marker: PhantomData<fn(T)>,
}

impl<T: IntoValue> Push<T> {
    /// Performs `push(value)` now and returns its answer, which completes when the value is taken.
    ///
    /// The value goes to the first pending `next` or waits in the queue of the
    /// repeater. The answer completes with `Ok` once a `next` takes the value
    /// or the repeater stops, and with the thrown value when a value thrown
    /// into the repeater reaches this push. The answer of the 1025th queued
    /// push is the `RepeaterOverflowError`, and a push outside an agent
    /// answers the error an await outside an agent rejects with, as `sleep`
    /// does there.
    pub fn push(&self, value: T) -> PushFuture {
        let Some(agent) = Agent::current() else {
            return PushFuture(PushState::Awaiting(tick()));
        };
        match self.handle.push(&agent, value.into_value()) {
            Ok(result) => PushFuture(PushState::Pushed(result)),
            Err(overflow) => PushFuture(PushState::Thrown(Some(Thrown(overflow)))),
        }
    }
}

/// The answer of [`Push::push`], which completes when the pushed value is taken.
#[must_use = "a push that is not awaited stays floating, and its rejection rejects the repeater"]
pub struct PushFuture(PushState);

/// Where the answer of a push stands.
enum PushState {
    /// The push was made and its answer is not awaited yet.
    Pushed(PushResult),
    /// The answer is awaited.
    Awaiting(PromiseFuture),
    /// The push threw synchronously, and the value is handed out on the first poll.
    Thrown(Option<Thrown>),
}

impl Future for PushFuture {
    type Output = Result<(), Thrown>;

    /// Awaits the answer of the push, `await push(value)`.
    ///
    /// The first poll calls `then` of the answer through the
    /// `PromiseResolveThenableJob` of the await, which clears the floating
    /// flag, and every later poll waits on the promise of the await.
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut self.get_mut().0;
        loop {
            match this {
                PushState::Pushed(result) => {
                    let awaited = match Agent::current() {
                        Some(agent) => result.awaited(&agent).into_future(),
                        None => tick(),
                    };
                    *this = PushState::Awaiting(awaited);
                }
                PushState::Awaiting(awaited) => {
                    return Pin::new(awaited)
                        .poll(context)
                        .map(|settled| settled.map(|_| ()).map_err(Thrown));
                }
                PushState::Thrown(thrown) => {
                    let thrown = thrown.take().unwrap_or(Thrown(Value::Undefined));
                    return Poll::Ready(Err(thrown));
                }
            }
        }
    }
}

/// The `stop` argument, callable with and without an error and thenable.
///
/// A callable promise. Each clone is the same stop function, so a body that
/// stops after it awaited writes `stop.clone().await`.
#[derive(Clone)]
pub struct Stop {
    /// The stop function of the executor's repeater.
    handle: StopHandle,
}

impl Stop {
    /// Performs `stop()`.
    ///
    /// The repeater ends once its queued pushes are taken. Outside an agent
    /// the call does nothing, because a settlement needs the job queue of one.
    pub fn stop(&self) {
        if let Some(agent) = Agent::current() {
            self.handle.stop(&agent, None);
        }
    }

    /// Performs `stop(error)`.
    ///
    /// The repeater ends with `error` once its queued pushes are taken,
    /// unless it holds an error already. Outside an agent the call does
    /// nothing.
    pub fn stop_with(&self, error: impl IntoThrown) {
        if let Some(agent) = Agent::current() {
            self.handle.stop(&agent, Some(error.into_thrown()));
        }
    }
}

/// Performs `await stop`, which completes when the repeater stops.
impl IntoFuture for Stop {
    type Output = ();
    type IntoFuture = StopFuture;

    fn into_future(self) -> StopFuture {
        StopFuture(StopState::Stop(self.handle))
    }
}

/// The future of `await stop`.
pub struct StopFuture(StopState);

/// Where the await of a stop function stands.
enum StopState {
    /// The stop function is not awaited yet.
    Stop(StopHandle),
    /// The promise of the await.
    Awaiting(PromiseFuture),
}

impl Future for StopFuture {
    type Output = ();

    /// Awaits the stop function as a thenable, which completes when the repeater stops.
    ///
    /// The promise of a stop function only fulfils, so the output is `()`,
    /// and an await outside an agent completes at the first poll.
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        let this = &mut self.get_mut().0;
        loop {
            match this {
                StopState::Stop(handle) => {
                    let awaited = match Agent::current() {
                        Some(agent) => handle.awaited(&agent).into_future(),
                        None => tick(),
                    };
                    *this = StopState::Awaiting(awaited);
                }
                StopState::Awaiting(awaited) => {
                    return Pin::new(awaited).poll(context).map(|_| ());
                }
            }
        }
    }
}
