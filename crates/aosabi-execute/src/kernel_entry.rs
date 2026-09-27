// SPDX-License-Identifier: MIT
/*
 * aosabi-execute kernel_entry: the general executor behind the entry traits of the kernel.
 *
 * No analogue in the reference. The traits are
 * crates/aosabi-kernel/src/execute_closed.rs.
 */

//! The request pipeline and the general executor.
//!
//! [`run_request`] is the one pipeline of the crate. It parses the document,
//! validates it with the specified rules unless told not to, runs
//! [`normalized_executor`] on a fresh agent and drives the agent until the
//! result has settled. A syntax error and a validation error answer as a
//! result with errors and no `data` key, which is the body Yoga answers them
//! with (docs/byte-equality.md section 8.2).
//!
//! The pipeline splits where the agent is driven. [`prepare_request`] runs
//! everything before that point and answers a [`Prepared`], and
//! [`finish_request`] reads the settled state into the [`Executed`] of the
//! request. [`run_request`] drives the agent between the two on the calling
//! thread, and a host that runs an event loop awaits the [`Drive`] of the
//! agent there instead, so a timer on the host clock waits without blocking
//! the thread.
//!
//! [`run_request`] keeps its contract of one settled result, so it answers a
//! subscription operation with one error and never runs it. The subscription
//! half is [`run_subscription`], which answers the single result of an early
//! error or a failed source stream, or the [`SubscriptionStream`] of many
//! results, which the agent of the request yields one by one as it is
//! polled.
//!
//! [`GeneralExecutor`] is the first implementation of the entry below HTTP,
//! [`Execute`], whose stream is a [`GeneralStream`], one result for a query
//! and a mutation and the results of a subscription, and [`JsonEncoder`] the
//! first encoder of a result, [`Encode`]. The kernel builds without an
//! allocator, so the sink that grows, [`VecSink`], sits here.
//!
//! The module names `core` and `alloc` and no item of `std`.
//!
//! [`Drive`]: crate::promise::Drive

use alloc::{
    rc::Rc,
    string::String,
    sync::Arc,
    vec,
    vec::Vec, //
};
use core::{
    fmt,
    future::Future,
    pin::Pin,
    task::{
        Context,
        Poll, //
    }, //
};

use aosabi_json::JsValue;
use aosabi_kernel::execute_closed::{
    Encode,
    Execute,
    Operation,
    ResultStream,
    Sink,
    SinkFull, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        DocumentNode,
        OperationTypeNode, //
    },
    parser::{
        ParseOptions,
        parse, //
    }, //
};
use aosabi_type::{
    ObjMap,
    Schema, //
};
use aosabi_validate::{
    ValidateOptions,
    validate, //
};

use crate::{
    errors::{
        into_graphql_error,
        thrown_definition_error, //
    },
    execute::{
        ExecutionArgs,
        LeafSerializer,
        SharedVariables, //
    },
    normalized_executor::{
        normalized_executor,
        operation_type, //
    },
    promise::{
        Agent,
        AgentOptions,
        Drive,
        Promise,
        PromiseState,
        Stalled,
        TimerMode,
        TraceEvent, //
    },
    result::ExecutionResult,
    value::{
        Completion,
        ContextValue,
        ErrorObject,
        Value,
        get_async_iterator,
        get_method,
        is_async_iterable,
        iter_result_parts,
        to_js_string, //
    }, //
};

/// The function that makes the root value of one request.
///
/// A root value is `Rc` based and belongs to one request, and an executor is
/// shared between threads, so the executor holds the factory and no value.
pub type RootFactory = Arc<dyn Fn() -> Value + Send + Sync>;

/// The general executor of one schema, which runs any document against it.
pub struct GeneralExecutor {
    /// The schema every request runs against.
    pub schema: Arc<Schema>,
    /// The factory of the root value.
    pub root_value: RootFactory,
    /// The serializers that see the executor value, by scalar name.
    pub leaf_serializers: Option<Arc<ObjMap<LeafSerializer>>>,
}

/// One request as a carrier hands it over.
#[derive(Clone, Debug)]
pub struct Request<'a> {
    /// The text of the document.
    pub document: &'a str,
    /// The raw variable values.
    pub variables: Option<JsValue>,
    /// The name of the operation to run.
    pub operation_name: Option<&'a str>,
    /// Whether the specified rules run before the executor.
    pub validate: bool,
}

/// Why a request has no result.
#[derive(Debug)]
pub enum RunFailure {
    /// The entry threw the value, or the promise it returned rejected with it.
    Thrown(Value),
    /// The agent ran out of jobs and timers before the result settled.
    Stalled,
}

/// What [`run_request`] answers, the outcome of the request and the trace of its agent.
#[derive(Debug)]
pub struct Executed {
    /// The result of the request, or why it has none.
    pub outcome: Result<ExecutionResult, RunFailure>,
    /// The trace events of the agent, which is empty unless the options ask for a trace.
    pub trace: Vec<TraceEvent>,
}

/// Returns the result a list of request errors answers with, `{ errors }` without `data`.
fn errors_only(errors: Vec<GraphQLError>) -> Executed {
    Executed {
        outcome: Ok(ExecutionResult::from_errors(errors)),
        trace: Vec::new(),
    }
}

/// Returns the answer of a document the specified rules refuse, and `None` for a valid one.
///
/// A throw of the validation, which an invalid schema causes, is the throw
/// of the request.
fn refuse_invalid(schema: &Schema, document: &DocumentNode) -> Option<Executed> {
    match validate(schema, document, None, ValidateOptions::default(), None) {
        Ok(validation_errors) if validation_errors.is_empty() => None,
        Ok(validation_errors) => Some(errors_only(validation_errors)),
        Err(error) => Some(Executed {
            outcome: Err(RunFailure::Thrown(thrown_definition_error(error))),
            trace: Vec::new(),
        }),
    }
}

/// A request prepared up to the point where its agent is driven.
pub enum Prepared {
    /// The request has its answer before any agent runs: a syntax error, the
    /// errors of the validation, or the throw of a schema the validation
    /// refuses.
    Answered(Executed),
    /// The executor ran on `agent`, and `result` settles when the agent is driven.
    Driving {
        /// The agent of the request.
        agent: Agent,
        /// The promise of the result.
        result: Promise,
    },
    /// The executor returned or threw on `agent` without a promise.
    Settled {
        /// The agent of the request.
        agent: Agent,
        /// The result as a settled promise would hold it, fulfilled with the
        /// value the executor returned or rejected with the value it threw.
        state: PromiseState,
    },
}

/// The text `run_request` answers a subscription operation with.
///
/// A text of Aosabi and of no reference, because the reference has no entry
/// that answers one settled result for a subscription.
const SUBSCRIPTION_ANSWERS_A_STREAM: &str =
    "A subscription answers a stream, which run_subscription returns.";

/// Parses the document and validates it unless told not to.
///
/// The `Err` is the answer of a request that ends here: a syntax error, the
/// errors of the validation, or the throw of a schema the validation
/// refuses.
// The error is the whole answer of a request. Each of the three callers takes
// it apart once, and the error path runs at most once per request, so a box
// would add an allocation and save no move that matters. The lint depends on
// the target, since the error reaches its threshold of 128 bytes on a 64-bit
// target and stays under it in the build of the crate for wasm32, so it is
// allowed, as the parser of aosabi-lang allows its error.
#[allow(clippy::result_large_err)]
fn check_request(
    executor: &GeneralExecutor,
    request: &Request<'_>,
) -> Result<DocumentNode, Executed> {
    let document = match parse(request.document, ParseOptions::default()) {
        Ok(document) => document,
        Err(syntax_error) => return Err(errors_only(vec![syntax_error])),
    };
    if request.validate
        && let Some(refused) = refuse_invalid(&executor.schema, &document)
    {
        return Err(refused);
    }
    Ok(document)
}

/// Returns whether the operation `request` names in `document` is a subscription.
fn is_subscription(document: &DocumentNode, request: &Request<'_>) -> bool {
    operation_type(document, request.operation_name) == Some(OperationTypeNode::Subscription)
}

/// Runs [`normalized_executor`] over `document` on a fresh agent and returns the agent with
/// what the executor answered or threw.
///
/// `setup` runs on the agent first. The root value is made and the executor
/// entered inside [`Agent::enter`], so a resolver that runs synchronously
/// finds its agent.
fn enter_executor(
    executor: &GeneralExecutor,
    document: DocumentNode,
    request: &Request<'_>,
    context_value: ContextValue,
    options: AgentOptions,
    setup: &dyn Fn(&Agent),
) -> (Agent, Result<Value, Value>) {
    let agent = Agent::new(options);
    setup(&agent);
    let settled = agent.enter(|| {
        let args = ExecutionArgs {
            schema: Arc::clone(&executor.schema),
            document: Arc::new(document),
            root_value: (executor.root_value)(),
            context_value,
            variable_values: request
                .variables
                .clone()
                .map(|values| -> SharedVariables { Rc::new(values) }),
            operation_name: request.operation_name.map(String::from),
            field_resolver: None,
            type_resolver: None,
            subscribe_field_resolver: None,
            leaf_serializers: executor.leaf_serializers.clone(),
        };
        normalized_executor(&agent, args)
    });
    (agent, settled)
}

/// Parses, validates unless told not to, and runs the executor on a fresh agent.
///
/// `context_value` is the context every resolver receives and `options` the
/// options of the agent. `setup` runs on the agent before the executor does,
/// which is where a layer above sets its hooks and its host clock. The root
/// value is made and the executor entered inside [`Agent::enter`], so a
/// resolver that runs synchronously finds its agent. The request stops where
/// the agent has to be driven, and [`finish_request`] reads its end. A
/// subscription operation is answered with one error and never runs, because
/// its answer is a stream, which [`run_subscription`] returns.
#[must_use]
pub fn prepare_request(
    executor: &GeneralExecutor,
    request: &Request<'_>,
    context_value: ContextValue,
    options: AgentOptions,
    setup: &dyn Fn(&Agent),
) -> Prepared {
    match check_request(executor, request) {
        Ok(document) => {
            prepare_document(executor, document, request, context_value, options, setup)
        }
        Err(answered) => Prepared::Answered(answered),
    }
}

/// Runs the executor over a checked `document`, the half of [`prepare_request`] after the parse.
fn prepare_document(
    executor: &GeneralExecutor,
    document: DocumentNode,
    request: &Request<'_>,
    context_value: ContextValue,
    options: AgentOptions,
    setup: &dyn Fn(&Agent),
) -> Prepared {
    if is_subscription(&document, request) {
        let refusal = GraphQLError::new(
            SUBSCRIPTION_ANSWERS_A_STREAM,
            GraphQLErrorOptions::default(),
        );
        return Prepared::Answered(errors_only(vec![refusal]));
    }
    let (agent, settled) =
        enter_executor(executor, document, request, context_value, options, setup);
    match settled {
        Ok(Value::Promise(result)) => Prepared::Driving { agent, result },
        Ok(result) => Prepared::Settled {
            agent,
            state: PromiseState::Fulfilled(result),
        },
        Err(error) => Prepared::Settled {
            agent,
            state: PromiseState::Rejected(error),
        },
    }
}

/// Reads the end of a request whose agent `driven` answered.
///
/// `driven` is what [`Agent::run`] or the [`Drive`]
/// of the agent answered for the promise of [`Prepared::Driving`], or the
/// state of [`Prepared::Settled`] in `Ok`. A fulfilled result is read into an
/// [`ExecutionResult`], a rejection is the throw of the request, and a
/// pending result or a stall is [`RunFailure::Stalled`]. The trace of the
/// agent is taken with it.
#[must_use]
pub fn finish_request(agent: &Agent, driven: Result<PromiseState, Stalled>) -> Executed {
    let outcome = match driven {
        Ok(PromiseState::Fulfilled(result)) => Ok(ExecutionResult::from_value(&result)),
        Ok(PromiseState::Rejected(error)) => Err(RunFailure::Thrown(error)),
        Ok(PromiseState::Pending) | Err(Stalled) => Err(RunFailure::Stalled),
    };
    Executed {
        outcome,
        trace: agent.take_trace(),
    }
}

/// Parses, validates unless told not to, and runs one request to its end on a fresh agent.
///
/// This is [`prepare_request`], the agent run on the calling thread until
/// the result has settled, and [`finish_request`]. `context_value`,
/// `options` and `setup` are those of [`prepare_request`]. A subscription
/// operation answers the one error `A subscription answers a stream, which
/// run_subscription returns.` without running, a text of Aosabi and of no
/// reference.
#[must_use]
pub fn run_request(
    executor: &GeneralExecutor,
    request: &Request<'_>,
    context_value: ContextValue,
    options: AgentOptions,
    setup: &dyn Fn(&Agent),
) -> Executed {
    run_prepared(prepare_request(
        executor,
        request,
        context_value,
        options,
        setup,
    ))
}

/// Runs a prepared request to its end on the calling thread.
fn run_prepared(prepared: Prepared) -> Executed {
    match prepared {
        Prepared::Answered(executed) => executed,
        Prepared::Driving { agent, result } => {
            let driven = agent.run(&result);
            finish_request(&agent, driven)
        }
        Prepared::Settled { agent, state } => finish_request(&agent, Ok(state)),
    }
}

/// What [`run_subscription`] answers, one result or a stream of results.
#[derive(Debug)]
pub enum Subscribed {
    /// A single result, which an early error or a failed source stream answers.
    One(ExecutionResult),
    /// A stream of results.
    Stream(SubscriptionStream),
}

/// Parses, validates and subscribes on a fresh agent, and runs the agent until the
/// subscription has answered its single result or its stream.
///
/// `context_value`, `options` and `setup` are those of [`prepare_request`].
/// The agent runs on the calling thread until the promise of the subscribe
/// resolver has settled, as [`run_request`] runs it, and it is then kept by
/// the stream, which drives it on every poll. An answer that is an async
/// iterable becomes the stream over its iterator, and any other answer is the
/// single result it holds. An operation that is no subscription runs as
/// [`run_request`] runs it and answers its one result.
///
/// # Errors
///
/// Returns [`RunFailure`] as [`run_request`] does: the throw of the entry or
/// the rejection of its promise, and a stall before the answer settled.
pub fn run_subscription(
    executor: &GeneralExecutor,
    request: &Request<'_>,
    context_value: ContextValue,
    options: AgentOptions,
    setup: &dyn Fn(&Agent),
) -> Result<Subscribed, RunFailure> {
    match check_request(executor, request) {
        Ok(document) => {
            subscribe_document(executor, document, request, context_value, options, setup)
        }
        Err(answered) => answered.outcome.map(Subscribed::One),
    }
}

/// Subscribes over a checked `document`, the half of [`run_subscription`] after the parse.
fn subscribe_document(
    executor: &GeneralExecutor,
    document: DocumentNode,
    request: &Request<'_>,
    context_value: ContextValue,
    options: AgentOptions,
    setup: &dyn Fn(&Agent),
) -> Result<Subscribed, RunFailure> {
    let (agent, settled) =
        enter_executor(executor, document, request, context_value, options, setup);
    let answer = match settled {
        Ok(Value::Promise(answer)) => match agent.run(&answer) {
            Ok(PromiseState::Fulfilled(answer)) => answer,
            Ok(PromiseState::Rejected(error)) => return Err(RunFailure::Thrown(error)),
            Ok(PromiseState::Pending) | Err(Stalled) => return Err(RunFailure::Stalled),
        },
        Ok(answer) => answer,
        Err(error) => return Err(RunFailure::Thrown(error)),
    };
    if !is_async_iterable(&answer) {
        return Ok(Subscribed::One(ExecutionResult::from_value(&answer)));
    }
    let iterator = agent.enter(|| get_async_iterator(&agent, &answer));
    match iterator {
        Completion::Normal(iterator) => Ok(Subscribed::Stream(SubscriptionStream {
            agent,
            iterator,
            pending: None,
            done: false,
        })),
        Completion::Throw(error) => Err(RunFailure::Thrown(error)),
    }
}

/// The stream of results of a subscription, over the async iterator the executor answered.
///
/// Each poll drives the agent of the request until the pending call of
/// `next` settles. A result the iterator yields is the next item, a done
/// iteration ends the stream, and a rejected `next` answers one result with
/// the rejection as its one error and ends the stream, as a `for await` of
/// the reference ends at the throw. A call of `next` that waits on a timer or
/// a foreign waker leaves the poll `Pending` with the waker of its context
/// kept, and a call that nothing can settle any more stays `Pending`, as a
/// stream of the reference that has nothing left to run never yields.
///
/// A stream that is dropped before it ended is closed first, because a
/// carrier of [`Execute`] ends a stream by dropping it and has no other way
/// to reach `return`. The promises of a repeater hold its record in a cycle
/// of `Rc` until they settle, which `return` brings about, so a stream
/// dropped without it would keep its listener and its record alive.
pub struct SubscriptionStream {
    /// The agent of the request, which runs every job of the stream.
    agent: Agent,
    /// The async iterator of the stream.
    iterator: Value,
    /// The loop that runs the agent until the pending call of `next` settles.
    pending: Option<Drive>,
    /// Whether the stream has ended.
    done: bool,
}

impl SubscriptionStream {
    /// Returns the agent of the request.
    #[must_use]
    pub fn agent(&self) -> &Agent {
        &self.agent
    }

    /// Calls `return` on the iterator and drains the jobs it enqueues, which removes a listener.
    ///
    /// The stream ends, and a later poll answers `None`. A throw of the call
    /// and a rejection of its promise reach no consumer, as a client that
    /// goes away reads nothing more.
    pub fn close(&mut self) {
        self.done = true;
        self.pending = None;
        let agent = &self.agent;
        if let Some(r#return) = get_method(&self.iterator, "return") {
            let _ = agent.enter(|| r#return.call(agent, &[]));
        }
        agent.drain_jobs();
    }

    /// Calls `next` on the iterator and returns the loop that runs the agent until it settles.
    ///
    /// `Err` is the throw of the call, which the stream yields as the one
    /// error of its last result. An iterator without `next` throws the
    /// `TypeError` V8 writes for the call.
    fn call_next(&self) -> Result<Drive, Value> {
        let agent = &self.agent;
        let Some(next) = get_method(&self.iterator, "next") else {
            let error = ErrorObject::type_error("iterator.next is not a function");
            return Err(Value::from(error));
        };
        let answer = agent.enter(|| next.call(agent, &[])).into_result()?;
        Ok(agent.drive(&Promise::resolve(agent, answer)))
    }
}

impl Drop for SubscriptionStream {
    /// Closes a stream that has not ended, as [`SubscriptionStream::close`] does.
    fn drop(&mut self) {
        if !self.done {
            self.close();
        }
    }
}

impl fmt::Debug for SubscriptionStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubscriptionStream")
            .field("pending", &self.pending.is_some())
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

impl ResultStream for SubscriptionStream {
    type Item = ExecutionResult;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        let mut drive = match this.pending.take() {
            Some(drive) => drive,
            None => match this.call_next() {
                Ok(drive) => drive,
                Err(error) => {
                    this.done = true;
                    return Poll::Ready(Some(thrown_result(error)));
                }
            },
        };
        let state = match Pin::new(&mut drive).poll(cx) {
            Poll::Ready(Ok(state)) => state,
            Poll::Ready(Err(Stalled)) | Poll::Pending => {
                this.pending = Some(drive);
                return Poll::Pending;
            }
        };
        match state {
            PromiseState::Fulfilled(iteration) => {
                let (value, done) = iter_result_parts(&iteration);
                if done {
                    this.done = true;
                    return Poll::Ready(None);
                }
                Poll::Ready(Some(ExecutionResult::from_value(&value)))
            }
            PromiseState::Rejected(error) => {
                this.done = true;
                Poll::Ready(Some(thrown_result(error)))
            }
            PromiseState::Pending => {
                this.pending = Some(drive);
                Poll::Pending
            }
        }
    }
}

/// Returns the result with the one error `message`, without `data`.
fn one_error(message: impl Into<aosabi_json::JsString>) -> ExecutionResult {
    ExecutionResult::from_errors(vec![GraphQLError::new(
        message,
        GraphQLErrorOptions::default(),
    )])
}

/// Returns the result whose one error is the thrown `error`, with no `data` key.
///
/// A `GraphQLError` is the error as it is, and any other value is the error
/// the executor coerces it to, which carries its message.
fn thrown_result(error: Value) -> ExecutionResult {
    ExecutionResult::from_errors(vec![into_graphql_error(error)])
}

/// Returns the one result a request that has no result answers, with the error of its failure.
///
/// The `message` of a thrown error and `String(value)` of any other thrown
/// value, and the texts of a stall and of a persisted document, are texts of
/// Aosabi and of no reference.
fn result_of(outcome: Result<ExecutionResult, RunFailure>) -> ExecutionResult {
    match outcome {
        Ok(result) => result,
        Err(RunFailure::Thrown(Value::Error(error))) => one_error(error.message()),
        Err(RunFailure::Thrown(value)) => one_error(to_js_string(&value)),
        Err(RunFailure::Stalled) => one_error("The request stalled before its result settled."),
    }
}

impl Execute for GeneralExecutor {
    type Variables = JsValue;
    type Context = ContextValue;
    type Output = ExecutionResult;
    type Stream = GeneralStream;

    /// Runs `operation`, validated, on a virtual clock.
    ///
    /// A subscription runs through [`run_subscription`] and answers
    /// [`GeneralStream::Events`] when its answer is a stream, and any other
    /// operation runs through [`run_request`] and answers
    /// [`GeneralStream::One`]. A thrown value answers as a result with one
    /// error that carries its message, the `message` of an error and
    /// `String(value)` of any other value. Both texts are texts of Aosabi and
    /// of no reference.
    fn execute(
        &self,
        operation: Operation<'_>,
        variables: &JsValue,
        context: &ContextValue,
    ) -> GeneralStream {
        let Operation::Document {
            text,
            operation_name,
        } = operation
        else {
            return GeneralStream::one(one_error("Persisted documents land at M12."));
        };
        let request = Request {
            document: text,
            variables: (!variables.is_undefined()).then(|| variables.clone()),
            operation_name,
            validate: true,
        };
        let options = AgentOptions {
            timers: TimerMode::Virtual,
            trace: false,
        };
        let context_value = ContextValue::clone(context);
        let document = match check_request(self, &request) {
            Ok(document) => document,
            Err(answered) => return GeneralStream::one(result_of(answered.outcome)),
        };
        if !is_subscription(&document, &request) {
            let prepared =
                prepare_document(self, document, &request, context_value, options, &|_| {});
            return GeneralStream::one(result_of(run_prepared(prepared).outcome));
        }
        match subscribe_document(self, document, &request, context_value, options, &|_| {}) {
            Ok(Subscribed::Stream(stream)) => GeneralStream::Events(stream),
            Ok(Subscribed::One(result)) => GeneralStream::one(result),
            Err(failure) => GeneralStream::one(result_of(Err(failure))),
        }
    }
}

/// The result stream of a query or a mutation, which yields one result.
#[derive(Debug)]
pub struct OneResult(Option<ExecutionResult>);

impl ResultStream for OneResult {
    type Item = ExecutionResult;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.get_mut().0.take())
    }
}

/// What the general executor answers, one result or the stream of a subscription.
#[derive(Debug)]
pub enum GeneralStream {
    /// The one result of a query, a mutation, or a subscription that answers no stream.
    One(OneResult),
    /// The results of a subscription.
    Events(SubscriptionStream),
}

impl GeneralStream {
    /// Returns the stream of the one result `result`.
    fn one(result: ExecutionResult) -> Self {
        Self::One(OneResult(Some(result)))
    }
}

impl ResultStream for GeneralStream {
    type Item = ExecutionResult;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Self::One(stream) => Pin::new(stream).poll_next(cx),
            Self::Events(stream) => Pin::new(stream).poll_next(cx),
        }
    }
}

/// The encoder that writes a result as the bytes of `JSON.stringify`.
///
/// The data of a result of the executor is written from the arena of its
/// execution, the walk [`ExecutionResult::write_json`] makes, and no tree of
/// it is built.
#[derive(Clone, Copy, Debug, Default)]
pub struct JsonEncoder;

impl Encode<ExecutionResult> for JsonEncoder {
    fn encode(&self, result: &ExecutionResult, sink: &mut dyn Sink) -> Result<(), SinkFull> {
        let mut bytes = Vec::new();
        result.write_json(&mut bytes);
        sink.write(&bytes)
    }
}

/// A sink that grows, which is never full.
#[derive(Clone, Debug, Default)]
pub struct VecSink(pub Vec<u8>);

impl Sink for VecSink {
    fn write(&mut self, bytes: &[u8]) -> Result<(), SinkFull> {
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}
