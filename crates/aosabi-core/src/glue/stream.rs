// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue stream: the stream a subscription field of the native
 * layer answers, the task that builds one, and the completion of a method
 * that answers one.
 *
 * No analogue in the reference, whose subscription resolver answers an async
 * iterable, the Repeater of @repeaterjs/repeater. The trait has the shape of
 * the trait Stream of futures-core 0.3, which the project writes itself,
 * since the standard library has no stream trait.
 */

//! The stream of a subscription field of the native layer.
//!
//! A subscription method of the native layer answers `impl Stream<Item = T>`,
//! which the builder reads from the plain source as a TypeScript build reads a
//! `Repeater<T>`. The trait [`Stream`] has the shape of
//! `futures_core::Stream`, `poll_next` over a pinned self and `size_hint`,
//! and the project writes it, since the standard library has no stream trait
//! and the dependency rule admits no crate for one.
//!
//! | TypeScript | Native layer |
//! | --- | --- |
//! | `ticks: (): Repeater<Tick> => new Repeater(async (push, stop) => {..})` | `pub fn ticks(&self) -> impl Stream<Item = Tick> { stream::task(\|push, stop\| async move {..}) }` |
//! | `changes: () => pubSub.subscribe('changed')` | `pub fn changes(&self) -> impl Stream<Item = Change> { PUB_SUB.subscribe(CHANGED, ()) }` |
//! | a resolver that throws before it answers its repeater | `pub fn ticks(&self) -> Result<impl Stream<Item = Tick>>` that answers `Err` |
//!
//! [`task`] is `new Repeater(executor)` under the name of the native layer,
//! [`Repeater::new`] itself with the error of the native layer, so a task is
//! the repeater of the engine and its frames are the frames of the engine.
//! The executor is a closure that answers a future,
//! `|push, stop| async move {..}`, the form of the plan's row, or an async
//! closure, `async move |push, stop| {..}`, and rustc infers `push` and
//! `stop` from the bound of [`task`] in both forms. The executor runs on the
//! first `next`, as the executor of the reference does, and `push` and
//! `stop` are the handles of the repeater, with the methods the module
//! `repeater` names. A stream the engine makes, a task and the subscription
//! of a pub sub, answers its repeater to the engine.
//!
//! Any other type that implements [`Stream`] is a stream of the application,
//! which the engine drives: the repeater of the field polls the stream on its
//! agent from the first `next` on, pushes each item and awaits the push
//! before it polls again, as the pump of a pub sub awaits each push, stops
//! when the stream ends, and stops with the value thrown into it when a push
//! answers one. A stream that is pending when its consumer returns is
//! dropped when it next answers or with its agent.
//!
//! The glue calls a method that answers a stream when the field subscribes,
//! as `subscribe` of graphql-js calls the resolver, so a `Result` that
//! answers `Err` throws at the subscription and the body of the method reads
//! the context of the request that subscribes. The stream the method answers
//! may borrow the object of the method, so [`complete_stream`] polls one
//! future that owns a handle of the object, calls the method, keeps the
//! stream and waits for the first `next`, once with a waker that does nothing
//! when the field subscribes, and from the first `next` on on the agent of
//! the stream.

use core::{
    future::{
        Future,
        IntoFuture,
        poll_fn, //
    },
    pin::{
        Pin,
        pin, //
    },
    task::{
        Context,
        Poll,
        Waker, //
    }, //
};
use std::{
    cell::RefCell,
    rc::Rc, //
};

use aosabi_execute::{
    promise::{
        Agent,
        LocalFuture,
        Promise,
        PromiseState, //
    },
    repeater::{
        PushHandle,
        Repeater as ExecutorRepeater,
        StopHandle, //
    },
    value::{
        Completion,
        Value, //
    }, //
};

use crate::{
    descriptor::Repeater,
    glue::{
        completion::IntoThrown,
        into_value::IntoValue,
        thrown::Error, //
    },
    repeater::{
        Push,
        Stop, //
    },
};

/// A stream of values that arrive one after the other, the async iterable a
/// subscription field answers.
///
/// The trait has the shape of `futures_core::Stream`. A stream of an
/// application implements [`Stream::poll_next`], and the engine drives it as
/// the module documentation says.
pub trait Stream {
    /// The type of the values the stream yields.
    type Item;

    /// Answers the next value, `Ready(None)` once the stream ends, or
    /// `Pending` with the waker of `cx` kept to wake the task when a value
    /// arrives.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;

    /// Returns the bounds on the number of values left, `(0, None)` when
    /// they are not known, as `futures_core::Stream::size_hint` does.
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }

    /// Returns the value of the repeater of a stream the engine makes, or
    /// the stream itself for a stream of an application.
    ///
    /// The glue calls it, and a stream of an application keeps the default.
    #[doc(hidden)]
    fn into_engine(self) -> Result<Value, Self>
    where
        Self: Sized,
    {
        Err(self)
    }
}

/// The repeater of the engine, which a task and a pub sub make, as a stream.
///
/// The engine reads it through its repeater. Its values are values of the
/// engine, which no Rust value answers, so polled from Rust it ends with no
/// item.
impl<T> Stream for Repeater<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<T>> {
        Poll::Ready(None)
    }

    fn into_engine(self) -> Result<Value, Self> {
        Ok(self.into_value())
    }
}

/// Returns the stream the executor `executor` pushes, `new
/// Repeater(executor)` of the reference.
///
/// The executor takes the `push` and the `stop` of the stream and answers a
/// future, `|push, stop| async move {..}`, and rustc infers both from the
/// bound. A closure that reads a value of the method takes `move` as well,
/// `move |push, stop| async move {..}`, since the executor owns what it
/// captures, and an async closure, `async move |push, stop| {..}`, is an
/// executor too. The executor runs on the first `next`, inside an async
/// function spawned on the agent of the stream, and its future answers the
/// `Result` of the native layer, so `?` throws any error into the stream,
/// which stops it. It is [`Repeater::new`], whose executor answers the
/// thrown value of the derive path.
///
/// # Examples
///
/// ```ignore
/// use aosabi::prelude::*;
///
/// pub struct Subscription;
///
/// impl Subscription {
///     pub fn ticks(&self, count: u32) -> impl Stream<Item = json::Map> {
///         stream::task(move |push, stop| async move {
///             for seq in 1..=count {
///                 push.push(json::Map::from([("seq", seq)])).await?;
///             }
///             stop.stop();
///             Ok(())
///         })
///     }
/// }
/// ```
pub fn task<T, F, Fut>(executor: F) -> Repeater<T>
where
    T: IntoValue + 'static,
    F: FnOnce(Push<T>, Stop) -> Fut + 'static,
    Fut: Future<Output = Result<(), Error>> + 'static,
{
    Repeater::new(async move |push, stop| executor(push, stop).await.map_err(Into::into))
}

/// What a subscription method of the native layer answers: a stream, or a
/// `Result` of one whose error throws when the field subscribes.
#[diagnostic::on_unimplemented(
    message = "a subscription method cannot answer `{Self}`",
    note = "a subscription method answers `impl Stream<Item = T>` or a `Result` of one"
)]
pub trait IntoStream {
    /// The stream.
    type Stream: Stream;

    /// Returns the stream, or the value the subscription throws.
    ///
    /// # Errors
    ///
    /// Returns the thrown value of a `Result` that answers `Err`.
    fn into_stream(self) -> Result<Self::Stream, Value>;
}

impl<S: Stream> IntoStream for S {
    type Stream = S;

    fn into_stream(self) -> Result<S, Value> {
        Ok(self)
    }
}

impl<S: Stream, E: IntoThrown> IntoStream for Result<S, E> {
    type Stream = S;

    fn into_stream(self) -> Result<S, Value> {
        self.map_err(IntoThrown::into_thrown)
    }
}

/// The push and the stop of the repeater of a stream of an application,
/// which its first `next` hands over.
#[derive(Default)]
struct Gate {
    /// The handles, `None` until the first `next`.
    handles: RefCell<Option<(PushHandle, StopHandle)>>,
}

/// The start of the stream of a subscription method, which the glue hands
/// the future that calls the method.
pub struct StreamStart {
    /// The gate of the repeater of the field.
    gate: Rc<Gate>,
}

impl StreamStart {
    /// Answers the completion of the subscription method that answered
    /// `answer`.
    ///
    /// An `Err` throws at once. A stream the engine makes answers its
    /// repeater at once. A stream of an application waits for the first
    /// `next` of the repeater [`complete_stream`] answers, and is then driven
    /// into it.
    pub async fn run<R>(self, answer: R) -> Completion
    where
        R: IntoStream,
        <R::Stream as Stream>::Item: IntoValue,
    {
        let stream = match answer.into_stream() {
            Ok(stream) => stream,
            Err(thrown) => return Completion::Throw(thrown),
        };
        let stream = match stream.into_engine() {
            Ok(repeater) => return Completion::Normal(repeater),
            Err(stream) => stream,
        };
        let (push, stop) = poll_fn(|_| match self.gate.handles.borrow_mut().take() {
            Some(handles) => Poll::Ready(handles),
            None => Poll::Pending,
        })
        .await;
        drive(stream, &push, &stop).await
    }
}

/// Returns the completion of a subscription method whose call `body` makes
/// with the start it is handed.
///
/// The future of `body` calls the method and answers through
/// [`StreamStart::run`]. It is polled once here, when the field subscribes,
/// with a waker that does nothing, so the method runs at the call of the
/// resolver. A future that completes answers its completion, the throw of an
/// `Err` or the repeater of a stream the engine makes. A future that waits
/// holds a stream of an application, and the completion is a repeater whose
/// executor hands the future its push and stop and spawns it on the agent of
/// the stream, on the first `next`.
pub fn complete_stream<B, F>(body: B) -> Completion
where
    B: FnOnce(StreamStart) -> F,
    F: Future<Output = Completion> + 'static,
{
    let gate = Rc::new(Gate::default());
    let mut future: LocalFuture = Box::pin(body(StreamStart {
        gate: Rc::clone(&gate),
    }));
    let mut context = Context::from_waker(Waker::noop());
    if let Poll::Ready(completion) = future.as_mut().poll(&mut context) {
        return completion;
    }
    let repeater = ExecutorRepeater::new(Box::new(move |agent, push, stop| {
        *gate.handles.borrow_mut() = Some((push, stop));
        Completion::Normal(Value::Promise(agent.spawn(future)))
    }));
    Completion::Normal(Value::Object(repeater))
}

/// Drives the stream of an application `stream` into the repeater of `push`
/// and `stop`.
///
/// Each item is pushed and its push awaited before the stream is polled
/// again, the end of the stream stops the repeater, and a push that answers
/// a thrown value stops the repeater with it, as the pump of a pub sub does.
/// The driver never keeps the agent across an await, since the agent keeps
/// the driver.
async fn drive<S>(stream: S, push: &PushHandle, stop: &StopHandle) -> Completion
where
    S: Stream,
    S::Item: IntoValue,
{
    let mut stream = pin!(stream);
    while matches!(stop.stopped().state(), PromiseState::Pending) {
        let Some(item) = poll_fn(|cx| stream.as_mut().poll_next(cx)).await else {
            if let Some(agent) = Agent::current() {
                stop.stop(&agent, None);
            }
            break;
        };
        match push_item(push, item.into_value()) {
            Some(Ok(pushed)) => {
                if let Err(error) = pushed.into_future().await {
                    if let Some(agent) = Agent::current() {
                        stop.stop(&agent, Some(error));
                    }
                    break;
                }
            }
            Some(Err(overflow)) => return Completion::Throw(overflow),
            None => break,
        }
    }
    Completion::Normal(Value::Undefined)
}

/// Pushes `value` and returns the promise of the await of its answer, or the
/// overflow it throws, `None` with no agent to run on.
fn push_item(push: &PushHandle, value: Value) -> Option<Result<Promise, Value>> {
    let agent = Agent::current()?;
    Some(
        push.push(&agent, value)
            .map(|pushed| pushed.awaited(&agent)),
    )
}
