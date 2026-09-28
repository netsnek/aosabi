// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the trait of a plugin and the payloads of its hooks.
 */

//! The trait of a plugin and the payloads of its hooks.
//!
//! A plugin of the reference is an object whose members are its hooks, and a
//! hook it lacks is absent. [`HookPlugin`] has a method per hook with a body
//! that does nothing, and a hook with no body behaves as an absent one,
//! because the orchestrator reaches a hook only to call it and a default body
//! changes nothing the orchestrator reads. A hook receives its payload, whose
//! methods are the functions the reference passes in it, `setSchema`,
//! `setParseFn` and the others, and a hook that returns a done hook returns
//! it as a boxed closure.
//!
//! A thrown value is the executor's [`Value`], the value `aosabi-core` wraps
//! in its `Thrown`.
//!
//! A done hook sees a single result or a stream of them, the `isAsyncIterable`
//! fork of the reference, which [`ExecutionOutcome`] is. A stream is the async
//! iterable the executor answers for a subscription, held with the agent its
//! jobs run on, and a done hook that wants to see each of its results answers
//! the hook `on_next` in its [`OnExecuteDoneHookResult`].

use core::{
    any::Any,
    future::Future,
    pin::Pin, //
};
use std::{
    rc::Rc,
    sync::Arc, //
};

#[cfg(feature = "host-hooks")]
use aosabi_execute::Promise;
use aosabi_execute::{
    Agent,
    ExecutionArgs,
    ExecutionResult,
    Value,
    execute::SharedDocument, //
};
use aosabi_lang::{
    GraphQLError,
    ast::DocumentNode, //
};
use aosabi_type::Schema;
use aosabi_validate::ValidationRule;

use crate::context::HookContext;

/// A future that is not `Send`, boxed and pinned.
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// The parse function of the engine, `parse(source)`.
pub type ParseFn = Rc<dyn Fn(&str) -> Result<Arc<DocumentNode>, Value>>;

/// The validate function of the engine, `validate(schema, document, rules)`.
pub type ValidateFn = Rc<
    dyn Fn(&Schema, &DocumentNode, Option<&[ValidationRule]>) -> Result<Vec<GraphQLError>, Value>,
>;

/// The execute or subscribe function of the engine, which answers the result or a promise of it on the agent.
pub type ExecuteFn = Rc<dyn Fn(&Agent, ExecutionArgs) -> Result<Value, Value>>;

/// A handler of a failure of the context building, which may replace the error.
pub type ContextErrorHandler = Rc<dyn Fn(&mut OnContextError<'_>)>;

/// A single result or a stream of results, the `isAsyncIterable` fork of every done hook.
pub enum ExecutionOutcome {
    /// A single result.
    Single(ExecutionResult),
    /// A stream of results, which a subscription answers.
    Stream(ResultStream),
}

/// A stream of results, the async iterable a subscription answers, with the agent its jobs run on.
///
/// The reference hands the async iterable itself from layer to layer, and
/// each layer wraps it in an iterator of its own. Every value of the
/// iterable belongs to the agent of its request, so the stream keeps that
/// agent beside it, and whoever reads the stream runs its jobs there. The
/// trait `ResultStream` of `aosabi-kernel` is another item, the stream of a
/// carrier below HTTP.
pub struct ResultStream {
    /// The agent of the request, which runs every job of the stream.
    agent: Agent,
    /// The async iterable, which answers its iterator through `[Symbol.asyncIterator]()`.
    iterable: Value,
}

impl ResultStream {
    /// Returns the stream of `iterable` on `agent`.
    #[must_use]
    pub fn new(agent: Agent, iterable: Value) -> Self {
        Self { agent, iterable }
    }

    /// Returns the agent of the request.
    #[must_use]
    pub fn agent(&self) -> &Agent {
        &self.agent
    }

    /// Returns the async iterable.
    #[must_use]
    pub fn iterable(&self) -> &Value {
        &self.iterable
    }

    /// Replaces the async iterable with the iterable `wrap` makes of it, as a layer of the reference reassigns `result`.
    pub fn wrap(&mut self, wrap: impl FnOnce(&Agent, Value) -> Value) {
        let iterable = core::mem::replace(&mut self.iterable, Value::Undefined);
        let agent = &self.agent;
        self.iterable = agent.enter(|| wrap(agent, iterable));
    }

    /// Returns the agent and the async iterable.
    #[must_use]
    pub fn into_parts(self) -> (Agent, Value) {
        (self.agent, self.iterable)
    }
}

impl ExecutionOutcome {
    /// Returns the single result, and `None` for a stream.
    #[must_use]
    pub fn single(&self) -> Option<&ExecutionResult> {
        match self {
            Self::Single(result) => Some(result),
            Self::Stream(_) => None,
        }
    }
}

impl core::fmt::Debug for ExecutionOutcome {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Single(result) => f.debug_tuple("Single").field(result).finish(),
            Self::Stream(_) => f.debug_tuple("Stream").finish_non_exhaustive(),
        }
    }
}

/// A plugin, the hooks it hangs on the steps of a request.
///
/// Every hook has a body that does nothing, so a plugin writes the hooks it
/// has, as a plugin object of the reference names its members.
pub trait HookPlugin: 'static {
    /// The hook `onPluginInit`, called once as the orchestrator is made.
    fn on_plugin_init(&self, _p: &mut OnPluginInit<'_>) {}

    /// The hook `onEnveloped`, called when the enveloped functions of a request are made.
    ///
    /// # Errors
    ///
    /// Returns the value the hook throws.
    fn on_enveloped(&self, _p: &mut OnEnveloped<'_>) -> Result<(), Value> {
        Ok(())
    }

    /// The hook `onSchemaChange`, called when another plugin replaces the schema.
    fn on_schema_change(&self, _p: &mut OnSchemaChange<'_>) {}

    /// The hook `onContextBuilding`, which may extend the context now or after a future.
    fn on_context_building(&self, _p: &mut OnContextBuilding<'_>) -> Option<ContextBuildingHook> {
        None
    }

    /// The hook `onParse`, which may answer the done hook of the parse.
    ///
    /// # Errors
    ///
    /// Returns the value the hook throws.
    fn on_parse(&self, _p: &mut OnParse<'_>) -> Result<Option<OnParseDoneHook>, Value> {
        Ok(None)
    }

    /// The hook `onValidate`, which may answer the done hook of the validation.
    fn on_validate(&self, _p: &mut OnValidate<'_>) -> Option<OnValidateDoneHook> {
        None
    }

    /// The hook `onExecute`, which may answer the hooks of the execution's end.
    ///
    /// # Errors
    ///
    /// Returns the value the hook throws.
    fn on_execute(&self, _p: &mut OnExecute<'_>) -> Result<Option<OnExecuteHooks>, Value> {
        Ok(None)
    }

    /// The hook `onSubscribe`, which may answer the hooks of the subscription's result and error.
    ///
    /// # Errors
    ///
    /// Returns the value the hook throws.
    fn on_subscribe(&self, _p: &mut OnSubscribe<'_>) -> Result<Option<OnSubscribeHooks>, Value> {
        Ok(None)
    }
}

/// The payload of `onPluginInit`.
pub struct OnPluginInit<'a> {
    /// The plugins as the orchestrator holds them at the call.
    pub(crate) plugins: &'a [Rc<dyn HookPlugin>],
    /// The plugins to splice in after the plugin whose hook runs.
    pub(crate) added: Vec<Rc<dyn HookPlugin>>,
    /// The schema the hook sets.
    pub(crate) schema: Option<Arc<Schema>>,
    /// The context error handlers the hook registers.
    pub(crate) context_error_handlers: Vec<ContextErrorHandler>,
}

impl OnPluginInit<'_> {
    /// Returns the plugins, `plugins`.
    #[must_use]
    pub fn plugins(&self) -> &[Rc<dyn HookPlugin>] {
        self.plugins
    }

    /// Adds `plugin` right after the plugin whose hook runs, `addPlugin(plugin)`.
    pub fn add_plugin(&mut self, plugin: Rc<dyn HookPlugin>) {
        self.added.push(plugin);
    }

    /// Sets the schema, `setSchema(schema)`.
    pub fn set_schema(&mut self, schema: Arc<Schema>) {
        self.schema = Some(schema);
    }

    /// Registers a handler of a failure of the context building, `registerContextErrorHandler(handler)`.
    pub fn register_context_error_handler(&mut self, handler: ContextErrorHandler) {
        self.context_error_handlers.push(handler);
    }
}

/// The payload of `onEnveloped`.
pub struct OnEnveloped<'a> {
    /// The initial context of the request.
    pub context: &'a HookContext,
    /// The schema the hook sets.
    pub(crate) schema: Option<Arc<Schema>>,
}

impl OnEnveloped<'_> {
    /// Sets `key` to `value` on the context, `extendContext({ key: value })`.
    pub fn extend_context(&mut self, key: &str, value: Rc<dyn Any>) {
        self.context.extend(key, value);
    }

    /// Sets the schema, `setSchema(schema)`.
    pub fn set_schema(&mut self, schema: Arc<Schema>) {
        self.schema = Some(schema);
    }
}

/// The payload of `onSchemaChange`.
pub struct OnSchemaChange<'a> {
    /// The new schema.
    pub schema: &'a Arc<Schema>,
    /// The schema the hook replaces it with.
    pub(crate) replaced: Option<Arc<Schema>>,
}

impl OnSchemaChange<'_> {
    /// Replaces the schema, `replaceSchema(schema)`.
    pub fn replace_schema(&mut self, schema: Arc<Schema>) {
        self.replaced = Some(schema);
    }
}

/// The payload of `onContextBuilding`.
pub struct OnContextBuilding<'a> {
    /// The context being built.
    pub context: &'a HookContext,
    /// Whether the hook broke the building.
    pub(crate) broken: bool,
}

impl OnContextBuilding<'_> {
    /// Sets `key` to `value` on the context, `extendContext({ key: value })`.
    pub fn extend_context(&mut self, key: &str, value: Rc<dyn Any>) {
        self.context.extend(key, value);
    }

    /// Stops the building after this hook, `breakContextBuilding()`.
    pub fn break_context_building(&mut self) {
        self.broken = true;
    }
}

/// What `onContextBuilding` answers: a future to await before the next hook, and a done hook.
#[derive(Default)]
pub struct ContextBuildingHook {
    /// The future the building awaits before the next hook.
    pub wait: Option<LocalBoxFuture<'static, Result<(), Value>>>,
    /// The hook that runs after every hook of the building.
    pub done: Option<ContextBuildingDoneHook>,
}

/// The payload of a context error handler.
pub struct OnContextError<'a> {
    /// The context being built.
    pub context: &'a HookContext,
    /// The error the building failed with.
    pub error: &'a mut Value,
}

impl OnContextError<'_> {
    /// Replaces the error, `setError(error)`.
    pub fn set_error(&mut self, error: Value) {
        *self.error = error;
    }
}

/// The payload of `onParse`.
pub struct OnParse<'a> {
    /// The context of the request.
    pub context: &'a HookContext,
    /// The source of the document, `params.source`.
    pub source: &'a str,
    /// The parse function the parse runs.
    pub(crate) parse_fn: &'a mut ParseFn,
    /// The document or the error a hook set.
    pub(crate) parsed: &'a mut Option<Result<Arc<DocumentNode>, Value>>,
    /// The document a hook set as one parsed from exactly `source`, held until the parse ends.
    pub(crate) of_source: &'a mut Option<SharedDocument>,
}

impl OnParse<'_> {
    /// Replaces the parse function, `setParseFn(fn)`.
    pub fn set_parse_fn(&mut self, parse_fn: ParseFn) {
        *self.parse_fn = parse_fn;
    }

    /// Sets the document and skips the parse, `setParsedDocument(document)`.
    pub fn set_parsed_document(&mut self, document: Arc<DocumentNode>) {
        *self.parsed = Some(Ok(document));
    }

    /// Sets a document whose own source holds exactly the bytes of `source`, `setParsedDocument(document)`.
    ///
    /// The hook found the body of the document's source equal to `source`, as
    /// the request cache does on a hit, so the orchestrator keeps that body as
    /// the text of the document and compares the two no second time. The mark
    /// holds the document until the parse ends, so no other document takes its
    /// address meanwhile, and a document that a later hook or a done hook sets
    /// in its place meets the compare again.
    pub fn set_parsed_document_of_source(&mut self, document: SharedDocument) {
        *self.of_source = Some(SharedDocument::clone(&document));
        *self.parsed = Some(Ok(document));
    }

    /// Sets `key` to `value` on the context, `extendContext({ key: value })`.
    pub fn extend_context(&mut self, key: &str, value: Rc<dyn Any>) {
        self.context.extend(key, value);
    }
}

/// The done hook of `onParse`.
pub type OnParseDoneHook = Box<dyn FnOnce(&mut OnParseDone<'_>)>;

/// The payload of the done hook of `onParse`.
pub struct OnParseDone<'a> {
    /// The context of the request.
    pub context: &'a HookContext,
    /// The document, or the error the parse threw.
    pub result: &'a mut Result<Arc<DocumentNode>, Value>,
}

impl OnParseDone<'_> {
    /// Replaces the result, `replaceParseResult(result)`.
    pub fn replace_parse_result(&mut self, result: Result<Arc<DocumentNode>, Value>) {
        *self.result = result;
    }
}

/// The payload of `onValidate`.
pub struct OnValidate<'a> {
    /// The context of the request.
    pub context: &'a HookContext,
    /// The schema, `params.schema`.
    pub schema: &'a Arc<Schema>,
    /// The document, `params.documentAST`.
    pub document: &'a DocumentNode,
    /// The rules, `params.rules`, `None` for the rules of the validate function.
    pub(crate) rules: &'a mut Option<Vec<ValidationRule>>,
    /// The validate function the validation runs.
    pub(crate) validate_fn: &'a mut ValidateFn,
    /// The errors a hook set.
    pub(crate) result: &'a mut Option<Vec<GraphQLError>>,
    /// The empty list whose room the rules take when a hook adds the first rule.
    pub(crate) spare: &'a mut Vec<ValidationRule>,
}

impl OnValidate<'_> {
    /// Returns the rules the validation runs, `params.rules`.
    #[must_use]
    pub fn rules(&self) -> Option<&[ValidationRule]> {
        self.rules.as_deref()
    }

    /// Appends `rule` to the rules, `addValidationRule(rule)`.
    pub fn add_validation_rule(&mut self, rule: ValidationRule) {
        self.rules
            .get_or_insert_with(|| core::mem::take(self.spare))
            .push(rule);
    }

    /// Replaces the validate function, `setValidationFn(fn)`.
    pub fn set_validation_fn(&mut self, validate_fn: ValidateFn) {
        *self.validate_fn = validate_fn;
    }

    /// Sets the errors and skips the validation, `setResult(errors)`.
    pub fn set_result(&mut self, errors: Vec<GraphQLError>) {
        *self.result = Some(errors);
    }

    /// Sets `key` to `value` on the context, `extendContext({ key: value })`.
    pub fn extend_context(&mut self, key: &str, value: Rc<dyn Any>) {
        self.context.extend(key, value);
    }
}

/// The done hook of `onValidate`.
pub type OnValidateDoneHook = Box<dyn FnOnce(&mut OnValidateDone<'_>)>;

/// The payload of the done hook of `onValidate`.
pub struct OnValidateDone<'a> {
    /// Whether the document is valid.
    pub valid: bool,
    /// The errors of the validation, which the hook may change.
    pub result: &'a mut Vec<GraphQLError>,
    /// The context of the request.
    pub context: &'a HookContext,
}

impl OnValidateDone<'_> {
    /// Replaces the errors, `setResult(errors)`.
    pub fn set_result(&mut self, errors: Vec<GraphQLError>) {
        *self.result = errors;
    }
}

/// The payload of `onExecute`.
pub struct OnExecute<'a> {
    /// The arguments of the execution.
    pub args: &'a mut ExecutionArgs,
    /// The context value of the execution.
    pub context: &'a HookContext,
    /// The execute function the execution runs.
    pub(crate) execute_fn: &'a mut ExecuteFn,
    /// The result a hook set, which stops the execution.
    pub(crate) result: &'a mut Option<ExecutionOutcome>,
}

impl OnExecute<'_> {
    /// Replaces the execute function, `setExecuteFn(fn)`.
    pub fn set_execute_fn(&mut self, execute_fn: ExecuteFn) {
        *self.execute_fn = execute_fn;
    }

    /// Sets the result and stops the execution and the hooks after this one, `setResultAndStopExecution(result)`.
    pub fn set_result_and_stop_execution(&mut self, result: ExecutionOutcome) {
        *self.result = Some(result);
    }

    /// Sets `key` to `value` on the context, `extendContext({ key: value })`.
    pub fn extend_context(&mut self, key: &str, value: Rc<dyn Any>) {
        self.context.extend(key, value);
    }
}

/// The hooks `onExecute` answers.
#[derive(Default)]
pub struct OnExecuteHooks {
    /// The hook `onExecuteDone`.
    pub on_execute_done: Option<OnExecuteDoneHook>,
}

/// The payload of `onExecuteDone`.
pub struct OnExecuteDone<'a> {
    /// The arguments of the execution.
    pub args: &'a ExecutionArgs,
    /// The result, which the hook may change.
    pub result: &'a mut ExecutionOutcome,
    /// The value the hook threw, which rejects the execution.
    #[cfg(feature = "host-hooks")]
    pub(crate) thrown: Option<Value>,
    /// The promise the hook answered and what follows its fulfilment, which
    /// the orchestrator awaits before the next hook runs.
    #[cfg(feature = "host-hooks")]
    pub(crate) awaited: Option<Awaited>,
    /// The agent the execution runs on.
    #[cfg(feature = "host-hooks")]
    pub(crate) agent: &'a Agent,
}

/// A promise a done hook of a host answered and what follows its
/// fulfilment, the result the hook leaves.
#[cfg(feature = "host-hooks")]
pub(crate) struct Awaited {
    /// The promise of the hook.
    pub(crate) promise: Promise,
    /// What follows the fulfilment of the promise, over the result.
    pub(crate) then: Box<dyn FnOnce(&mut ExecutionOutcome)>,
}

impl<'a> OnExecuteDone<'a> {
    /// Returns the payload of a done hook over the arguments `args` and the
    /// result `result` of an execution that runs on `agent`.
    pub(crate) fn new(
        args: &'a ExecutionArgs,
        result: &'a mut ExecutionOutcome,
        agent: &'a Agent,
    ) -> Self {
        #[cfg(not(feature = "host-hooks"))]
        let _ = agent;
        Self {
            args,
            result,
            #[cfg(feature = "host-hooks")]
            thrown: None,
            #[cfg(feature = "host-hooks")]
            awaited: None,
            #[cfg(feature = "host-hooks")]
            agent,
        }
    }

    /// Replaces the result, `setResult(result)`.
    pub fn set_result(&mut self, result: ExecutionOutcome) {
        *self.result = result;
    }

    /// Throws `value` out of the hook.
    ///
    /// The orchestrator rejects the execution with it once the hook returns and
    /// runs no done hook after it, as a done hook of Envelop that throws rejects
    /// the promise of the execution through `iterateAsync`. A result the hook
    /// set before it threw takes no effect.
    #[cfg(feature = "host-hooks")]
    pub fn throw(&mut self, value: Value) {
        self.thrown = Some(value);
    }

    /// Answers the promise `promise` from the hook, whose fulfilment runs
    /// `then` over the result.
    ///
    /// The orchestrator of an execution awaits the promise once the hook returns
    /// and before the next hook runs, then runs `then`, and a rejection rejects
    /// the execution with its reason and runs no done hook after it, as
    /// `iterateAsync` awaits a done hook of Envelop that answers a promise
    /// through `handleMaybePromise`.
    #[cfg(feature = "host-hooks")]
    pub fn wait(&mut self, promise: Promise, then: Box<dyn FnOnce(&mut ExecutionOutcome)>) {
        self.awaited = Some(Awaited { promise, then });
    }

    /// Returns the agent the execution runs on, which a hook of a host
    /// enters for its call, so a promise its answer holds belongs to it.
    #[cfg(feature = "host-hooks")]
    #[must_use]
    pub fn agent(&self) -> &Agent {
        self.agent
    }
}

/// The payload of `onSubscribe`.
pub struct OnSubscribe<'a> {
    /// The arguments of the subscription.
    pub args: &'a mut ExecutionArgs,
    /// The context value of the subscription.
    pub context: &'a HookContext,
    /// The subscribe function the subscription runs.
    pub(crate) subscribe_fn: &'a mut ExecuteFn,
    /// The result a hook set, which stops the subscription.
    pub(crate) result: &'a mut Option<ExecutionOutcome>,
}

impl OnSubscribe<'_> {
    /// Replaces the subscribe function, `setSubscribeFn(fn)`.
    pub fn set_subscribe_fn(&mut self, subscribe_fn: ExecuteFn) {
        *self.subscribe_fn = subscribe_fn;
    }

    /// Sets the result and stops the subscription and the hooks after this one, `setResultAndStopExecution(result)`.
    pub fn set_result_and_stop_execution(&mut self, result: ExecutionOutcome) {
        *self.result = Some(result);
    }

    /// Sets `key` to `value` on the context, `extendContext({ key: value })`.
    pub fn extend_context(&mut self, key: &str, value: Rc<dyn Any>) {
        self.context.extend(key, value);
    }
}

/// The hooks `onSubscribe` answers.
#[derive(Default)]
pub struct OnSubscribeHooks {
    /// The hook `onSubscribeResult`.
    pub on_subscribe_result: Option<OnExecuteDoneHook>,
    /// The hook `onSubscribeError`, which sees the error a stream fails with.
    pub on_subscribe_error: Option<OnSubscribeErrorHook>,
}

/// What `onExecuteDone` and `onSubscribeResult` may answer for a stream, `{ onNext, onEnd }`.
///
/// The type `OnExecuteDoneHookResult` of `@envelop/types` 5.2.1. The
/// orchestrator collects it from every done hook and applies it only when the
/// result is a stream.
#[derive(Default)]
pub struct OnExecuteDoneHookResult {
    /// The hook `onNext`, which sees each result of the stream and may replace it.
    pub on_next: Option<OnNextHook>,
    /// The hook `onEnd`, which runs once when the stream ends.
    pub on_end: Option<OnEndHook>,
}

/// The done hook of the context building.
pub type ContextBuildingDoneHook = Box<dyn FnOnce(&HookContext)>;

/// The hook `onExecuteDone`, and `onSubscribeResult`, which reads the same payload.
pub type OnExecuteDoneHook =
    Box<dyn FnOnce(&mut OnExecuteDone<'_>) -> Option<OnExecuteDoneHookResult>>;

/// The hook `onNext`, which reads each result of a stream as the payload of `onExecuteDone` reads a single one.
pub type OnNextHook = Box<dyn Fn(&mut OnExecuteDone<'_>)>;

/// The hook `onEnd`.
pub type OnEndHook = Box<dyn Fn()>;

/// The hook `onSubscribeError`, which may replace the error.
pub type OnSubscribeErrorHook = Box<dyn Fn(&mut Value)>;
