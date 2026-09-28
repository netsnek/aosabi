// SPDX-License-Identifier: MIT
/*
 * graphql-over-http readable_stream: the part of WHATWG Streams the server sent
 * events processor writes into.
 *
 * No file of the reference is translated here, where the stream is the
 * ReadableStream of the platform. The model follows
 * https://streams.spec.whatwg.org/ sections 4.2, 4.5 and 4.9, a default
 * controller over a count strategy of high water mark 1.
 */

//! The readable stream of the server sent events processor.
//!
//! [`ReadableStream`] is a stream with a default controller over the count
//! strategy of high water mark 1, the strategy of `new ReadableStream(source)`
//! with no second argument. Its steps are the ones of the specification:
//! `start` runs in the constructor, `pull` runs while the stream wants a chunk,
//! with the flags `pulling` and `pullAgain`, an `enqueue` hands its chunk
//! straight to a read that waits, `close` closes once the queue is drained,
//! `error` drops the queue, and a cancel closes the stream and runs the
//! `cancel` of the source.
//!
//! Two sides share the state of a stream. The source runs on the agent of the
//! request, and the serve layer reads the chunks through the [`StreamBody`] the
//! stream becomes. A read changes the state at once as a read of the reference
//! does, and where the specification calls `pull` from the read, the read marks
//! the pull and wakes the driver, which runs it as a job of the agent. The
//! agent is therefore driven by the task of its driver and not by the reads, so
//! a timer of the source fires while no read waits, and the ping of the
//! processor can see a queue of one chunk and stop for good as the reference's
//! does.
//!
//! The driver runs no request context around `pull`. The reference calls `next`
//! of the stream only in the pull of its readable stream, outside the async
//! context of the request, so a repeater executor that starts on that call sees
//! no context, and a job of the driver runs where no context is in place.
//!
//! A call of `enqueue` or `close` in a state the specification refuses throws a
//! `TypeError` there and does nothing here. The processor makes such a call
//! only after a cancel, inside a reaction whose throw would error a stream that
//! is closed already, which changes nothing.

use core::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    task::{
        Context,
        Poll,
        Waker, //
    },
};
use std::{collections::VecDeque, rc::Rc};

use aosabi_execute::{
    Agent, Promise, Value,
    promise::{
        Job,
        ResolvingFunctions, //
    },
    value::{
        Completion,
        to_js_string, //
    },
};
use aosabi_http::{
    LocalBoxFuture, Stream,
    fetch::body::{
        BodyError,
        Chunk, //
    },
};
use aosabi_lang::js_string::to_string_lossy;

/// The high water mark of the count strategy, the default of a `ReadableStream`.
const HIGH_WATER_MARK: i64 = 1;

/// The source of a stream, `UnderlyingSource` with its three methods.
///
/// Each method runs on the agent of the stream, with the controller of the
/// stream. The stream calls no method again while one runs.
pub trait UnderlyingSource {
    /// Starts the source, `start(controller)`.
    fn start(&mut self, agent: &Agent, controller: &ReadableStreamController);

    /// Asks the source for chunks, `pull(controller)`, and answers what the call answers.
    ///
    /// A promise holds the stream's next pull until it settles, and a throw
    /// or a rejection errors the stream.
    fn pull(&mut self, agent: &Agent, controller: &ReadableStreamController) -> Completion;

    /// Tells the source that the reader cancelled the stream, `cancel(reason)`.
    fn cancel(&mut self, agent: &Agent, reason: Value) -> Completion;
}

/// The state of a stream, `[[state]]` with `[[storedError]]`.
enum Phase {
    /// `"readable"`.
    Readable,
    /// `"closed"`.
    Closed,
    /// `"errored"`, with the stored error and whether the reader has seen it.
    Errored(Value, bool),
}

/// The state the stream, its controller, its body and its driver share.
// An `unsafe` cell would skip each borrow flag.
#[expect(
    clippy::struct_excessive_bools,
    reason = "the internal slots of the controller of the specification are flags"
)]
struct StreamState {
    /// `[[state]]`.
    phase: Phase,
    /// `[[queue]]`, each chunk of size 1.
    queue: VecDeque<Vec<u8>>,
    /// `[[started]]`.
    started: bool,
    /// `[[closeRequested]]`.
    close_requested: bool,
    /// `[[pulling]]`.
    pulling: bool,
    /// `[[pullAgain]]`.
    pull_again: bool,
    /// The source, `None` while one of its methods runs and once the algorithms are cleared.
    source: Option<Box<dyn UnderlyingSource>>,
    /// Whether the algorithms are cleared, `ReadableStreamDefaultControllerClearAlgorithms`.
    cleared: bool,
    /// The waker of the read that waits, the one read request a reader leaves.
    read_request: Option<Waker>,
    /// The chunk that fulfilled the read request, which the next poll of the body answers.
    handed: Option<Vec<u8>>,
    /// Whether a read set `[[pulling]]` and the driver has yet to call the source.
    pull_pending: bool,
    /// Whether the body was dropped while the stream was readable.
    cancel_requested: bool,
    /// The waker of the task of the driver.
    driver_waker: Option<Waker>,
}

impl StreamState {
    /// Returns whether the stream is closed or errored.
    fn is_settled(&self) -> bool {
        !matches!(self.phase, Phase::Readable)
    }

    /// Returns whether a chunk or a close may still come, `ReadableStreamDefaultControllerCanCloseOrEnqueue`.
    fn can_close_or_enqueue(&self) -> bool {
        !self.close_requested && matches!(self.phase, Phase::Readable)
    }

    /// Returns `ReadableStreamDefaultControllerGetDesiredSize`, `None` for an errored stream.
    fn desired_size(&self) -> Option<i64> {
        match self.phase {
            Phase::Errored(..) => None,
            Phase::Closed => Some(0),
            Phase::Readable => {
                let queued = i64::try_from(self.queue.len()).unwrap_or(i64::MAX);
                Some(HIGH_WATER_MARK.saturating_sub(queued))
            }
        }
    }

    /// Returns `ReadableStreamDefaultControllerShouldCallPull`.
    fn should_call_pull(&self) -> bool {
        if !self.can_close_or_enqueue() || !self.started {
            return false;
        }
        if self.read_request.is_some() {
            return true;
        }
        self.desired_size().is_some_and(|size| size > 0)
    }

    /// Performs the checks of `ReadableStreamDefaultControllerCallPullIfNeeded` and returns whether the source is to be pulled now.
    ///
    /// A pull that runs already sets `[[pullAgain]]`, and a pull that is due
    /// sets `[[pulling]]` before the caller calls the source.
    fn call_pull_if_needed(&mut self) -> bool {
        if !self.should_call_pull() {
            return false;
        }
        if self.pulling {
            self.pull_again = true;
            return false;
        }
        self.pulling = true;
        true
    }

    /// Drops the source, `ReadableStreamDefaultControllerClearAlgorithms`.
    fn clear_algorithms(&mut self) {
        self.cleared = true;
        self.source = None;
    }

    /// Closes the stream, `ReadableStreamClose`, and ends the read that waits.
    fn close(&mut self) {
        self.phase = Phase::Closed;
        if let Some(read) = self.read_request.take() {
            read.wake();
        }
    }

    /// Errors the stream with `error`, `ReadableStreamDefaultControllerError`.
    fn error(&mut self, error: Value) {
        if !matches!(self.phase, Phase::Readable) {
            return;
        }
        self.queue.clear();
        self.clear_algorithms();
        self.phase = Phase::Errored(error, false);
        if let Some(read) = self.read_request.take() {
            read.wake();
        }
    }

    /// Wakes the driver, which runs what a read or a drop of the body left for it.
    fn wake_driver(&self) {
        if let Some(driver) = &self.driver_waker {
            driver.wake_by_ref();
        }
    }

    /// Answers one poll of the reader, the read and its `[[PullSteps]]`.
    fn read(&mut self, cx: &Context<'_>) -> Poll<Option<Chunk>> {
        if let Some(chunk) = self.handed.take() {
            return Poll::Ready(Some(Ok(chunk)));
        }
        match &mut self.phase {
            Phase::Errored(error, seen) => {
                if *seen {
                    return Poll::Ready(None);
                }
                *seen = true;
                let message = to_string_lossy(&to_js_string(error)).into_owned();
                return Poll::Ready(Some(Err(BodyError::Failed(message))));
            }
            Phase::Closed => return Poll::Ready(None),
            Phase::Readable => {}
        }
        if let Some(read) = &mut self.read_request {
            // The read that waits is polled again before a chunk came.
            read.clone_from(cx.waker());
            return Poll::Pending;
        }
        if let Some(chunk) = self.queue.pop_front() {
            if self.close_requested && self.queue.is_empty() {
                self.clear_algorithms();
                self.close();
                self.wake_driver();
            } else if self.call_pull_if_needed() {
                self.pull_pending = true;
                self.wake_driver();
            }
            return Poll::Ready(Some(Ok(chunk)));
        }
        self.read_request = Some(cx.waker().clone());
        if self.call_pull_if_needed() {
            self.pull_pending = true;
            self.wake_driver();
        }
        Poll::Pending
    }
}

/// The shared state of one stream.
type Shared = Rc<RefCell<StreamState>>;

/// The controller of a stream, `ReadableStreamDefaultController`.
///
/// A clone is the same controller.
#[derive(Clone)]
pub struct ReadableStreamController {
    /// The state of the stream.
    state: Shared,
}

impl ReadableStreamController {
    /// Adds `chunk` to the stream, `controller.enqueue(chunk)`.
    ///
    /// A read that waits takes the chunk at once, and the queue takes it
    /// otherwise. The pull that follows runs on the agent that runs the
    /// caller.
    pub fn enqueue(&self, chunk: Vec<u8>) {
        let pull = {
            let mut state = self.state.borrow_mut();
            if !state.can_close_or_enqueue() {
                return;
            }
            match state.read_request.take() {
                Some(read) => {
                    state.handed = Some(chunk);
                    read.wake();
                }
                None => state.queue.push_back(chunk),
            }
            state.call_pull_if_needed()
        };
        if pull {
            pull_now(&self.state);
        }
    }

    /// Closes the stream once its queue is drained, `controller.close()`.
    pub fn close(&self) {
        let mut state = self.state.borrow_mut();
        if !state.can_close_or_enqueue() {
            return;
        }
        state.close_requested = true;
        if state.queue.is_empty() {
            state.clear_algorithms();
            state.close();
        }
    }

    /// Errors the stream with `error`, `controller.error(error)`.
    pub fn error(&self, error: Value) {
        self.state.borrow_mut().error(error);
    }

    /// Returns the high water mark minus the queued chunks, `controller.desiredSize`.
    ///
    /// The answer is 0 for a closed stream and `None`, the `null` of the
    /// reference, once the stream errored.
    #[must_use]
    pub fn desired_size(&self) -> Option<i64> {
        self.state.borrow().desired_size()
    }
}

/// Calls the source from the agent that runs the caller, or leaves the call to the driver outside of every agent.
fn pull_now(state: &Shared) {
    if let Some(agent) = Agent::current() {
        run_pull(&agent, state);
        return;
    }
    let mut state = state.borrow_mut();
    state.pull_pending = true;
    state.wake_driver();
}

/// Calls `pull` of the source and handles the promise it answers, the second half of `ReadableStreamDefaultControllerCallPullIfNeeded`.
fn run_pull(agent: &Agent, state: &Shared) {
    let Some(mut source) = state.borrow_mut().source.take() else {
        return;
    };
    let controller = ReadableStreamController {
        state: Rc::clone(state),
    };
    let completion = source.pull(agent, &controller);
    restore_source(state, source);
    let fulfilled = Rc::clone(state);
    let rejected = Rc::clone(state);
    let _derived = promise_of(agent, completion).then(
        agent,
        Some(Box::new(move |agent, _| {
            let pull = {
                let mut state = fulfilled.borrow_mut();
                state.pulling = false;
                if state.pull_again {
                    state.pull_again = false;
                    state.call_pull_if_needed()
                } else {
                    false
                }
            };
            if pull {
                run_pull(agent, &fulfilled);
            }
            Completion::Normal(Value::Undefined)
        })),
        Some(Box::new(move |_, error| {
            rejected.borrow_mut().error(error);
            Completion::Normal(Value::Undefined)
        })),
    );
}

/// Puts `source` back after one of its methods ran, unless the method cleared the algorithms.
fn restore_source(state: &Shared, source: Box<dyn UnderlyingSource>) {
    let mut state = state.borrow_mut();
    if !state.cleared {
        state.source = Some(source);
    }
}

/// Returns the promise of a completion, `PromiseResolve` of a value or a promise rejected with a throw.
fn promise_of(agent: &Agent, completion: Completion) -> Promise {
    match completion {
        Completion::Normal(value) => Promise::resolve(agent, value),
        Completion::Throw(error) => Promise::reject(agent, error),
    }
}

/// Runs `ReadableStreamCancel(stream, reason)` on the agent of the stream.
///
/// The stream closes first, then the queue is reset and the source is told,
/// and the algorithms are cleared, so the promise the source answers runs to
/// its end in the drain of the driver.
fn readable_stream_cancel(agent: &Agent, state: &Shared, reason: Value) {
    let source = {
        let mut state = state.borrow_mut();
        if state.is_settled() {
            return;
        }
        state.close();
        state.queue.clear();
        let source = state.source.take();
        state.clear_algorithms();
        source
    };
    if let Some(mut source) = source {
        let _ = promise_of(agent, source.cancel(agent, reason));
    }
}

/// A readable stream, `new ReadableStream(source)`.
pub struct ReadableStream {
    /// The state of the stream.
    state: Shared,
    /// The driver, which the body hands out.
    driver: Driver,
}

impl ReadableStream {
    /// Creates the stream of `source` on `agent` and runs `start`, `new ReadableStream(source)`.
    #[must_use]
    pub fn new(agent: &Agent, source: Box<dyn UnderlyingSource>) -> Self {
        let state = Rc::new(RefCell::new(StreamState {
            phase: Phase::Readable,
            queue: VecDeque::new(),
            started: false,
            close_requested: false,
            pulling: false,
            pull_again: false,
            source: None,
            cleared: false,
            read_request: None,
            handed: None,
            pull_pending: false,
            cancel_requested: false,
            driver_waker: None,
        }));
        let controller = ReadableStreamController {
            state: Rc::clone(&state),
        };
        let mut source = source;
        agent.enter(|| source.start(agent, &controller));
        restore_source(&state, source);
        let started = Rc::clone(&state);
        let rejected = Rc::clone(&state);
        let _derived = Promise::resolve(agent, Value::Undefined).then(
            agent,
            Some(Box::new(move |agent, _| {
                let pull = {
                    let mut state = started.borrow_mut();
                    state.started = true;
                    state.call_pull_if_needed()
                };
                if pull {
                    run_pull(agent, &started);
                }
                Completion::Normal(Value::Undefined)
            })),
            Some(Box::new(move |_, error| {
                rejected.borrow_mut().error(error);
                Completion::Normal(Value::Undefined)
            })),
        );
        let (ended, resolve_ended) = Promise::with_resolvers(agent);
        let driver = Driver {
            agent: agent.clone(),
            state: Rc::clone(&state),
            ended,
            resolve_ended: Some(resolve_ended),
            cancel_enqueued: false,
            draining: false,
        };
        Self { state, driver }
    }

    /// Returns the reader side, which the serve layer writes and whose drop cancels the stream.
    #[must_use]
    pub fn into_body(self) -> StreamBody {
        StreamBody {
            state: self.state,
            driver: Some(self.driver),
        }
    }
}

/// The reader side of a stream, the body of an answer.
///
/// Each poll is one read of the reader the serve layer holds, and the body
/// hands out its driver once, which the host runs as a task of its own beside
/// the reads. A host that never takes the driver never runs the source past
/// its `start`.
pub struct StreamBody {
    /// The state of the stream.
    state: Shared,
    /// The driver, until it is handed out.
    driver: Option<Driver>,
}

impl Stream for StreamBody {
    type Item = Chunk;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Chunk>> {
        self.state.borrow_mut().read(cx)
    }

    fn take_driver(self: Pin<&mut Self>) -> Option<LocalBoxFuture<'static, ()>> {
        let driver = self.get_mut().driver.take()?;
        Some(Box::pin(driver))
    }
}

impl Drop for StreamBody {
    /// Requests the cancel of a stream that is still readable and wakes the driver, which runs it.
    ///
    /// The serve layer drops the body when the client goes away or a write
    /// fails, which is where the writer of the Node adapter calls
    /// `reader.cancel(error)`.
    fn drop(&mut self) {
        let Ok(mut state) = self.state.try_borrow_mut() else {
            return;
        };
        if state.is_settled() {
            return;
        }
        state.cancel_requested = true;
        state.wake_driver();
    }
}

/// The future that drives the agent of a stream until the stream ends, and then until the agent is idle.
///
/// It runs the pulls the reads mark and the cancel the drop of the body
/// requests as jobs of the agent, and resolves the promise of its end once
/// the stream is closed or errored. The agent is driven by this future alone
/// while the stream lives.
struct Driver {
    /// The agent of the stream.
    agent: Agent,
    /// The state of the stream.
    state: Shared,
    /// The promise that settles when the stream ends.
    ended: Promise,
    /// Its resolving functions, until the stream ends.
    resolve_ended: Option<ResolvingFunctions>,
    /// Whether the cancel the body requested is a job of the agent.
    cancel_enqueued: bool,
    /// Whether the stream ended and the agent is drained until it is idle.
    draining: bool,
}

impl Driver {
    /// Takes over what the reads and the drop of the body left, and resolves the end of a stream that ended.
    fn serve(&mut self, cx: &Context<'_>) {
        let (pull, cancel, settled) = {
            let mut state = self.state.borrow_mut();
            match &mut state.driver_waker {
                Some(waker) => waker.clone_from(cx.waker()),
                None => state.driver_waker = Some(cx.waker().clone()),
            }
            let pull = core::mem::take(&mut state.pull_pending);
            let cancel = state.cancel_requested && !self.cancel_enqueued;
            (pull, cancel, state.is_settled())
        };
        if pull {
            let state = Rc::clone(&self.state);
            self.agent
                .enqueue_job(Job::new(move |agent| run_pull(agent, &state)));
        }
        if cancel {
            self.cancel_enqueued = true;
            let state = Rc::clone(&self.state);
            self.agent.enqueue_job(Job::new(move |agent| {
                readable_stream_cancel(agent, &state, Value::Undefined);
            }));
        }
        if settled && let Some(resolve) = self.resolve_ended.take() {
            resolve.resolve(&self.agent, Value::Undefined);
        }
    }
}

impl Future for Driver {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        loop {
            if this.draining {
                let mut idle = this.agent.drain_until_idle();
                if Pin::new(&mut idle).poll(cx).is_pending() {
                    return Poll::Pending;
                }
                this.state.borrow_mut().driver_waker = None;
                return Poll::Ready(());
            }
            this.serve(cx);
            let mut drive = this.agent.drive(&this.ended);
            match Pin::new(&mut drive).poll(cx) {
                Poll::Ready(Ok(_)) => this.draining = true,
                // The agent has nothing to run, or parks on a timer or a
                // foreign future. A stream that ended inside the poll goes
                // round once more to resolve its end.
                Poll::Ready(Err(_)) | Poll::Pending => {
                    let ended = this.state.borrow().is_settled() && this.resolve_ended.is_some();
                    if !ended {
                        return Poll::Pending;
                    }
                }
            }
        }
    }
}
