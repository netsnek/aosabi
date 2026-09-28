// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the orchestrator, which runs the hooks of every plugin in
 * their order.
 */

//! The orchestrator.
//!
//! The orchestrator calls `on_plugin_init` of every plugin once, in the order
//! of the list, and puts a plugin that a hook adds right after the plugin that
//! added it, so the added plugin's own `on_plugin_init` runs next. A request
//! then runs the before hooks of parse, validate, context building, execute
//! and subscribe in the order of the list and the done hooks in the order they
//! were returned. A parse error reaches every done hook of the parse and is
//! thrown after them, and a failure of the context building passes every
//! context error handler. Nothing sorts, which is ordering A of
//! `docs/plugins.md` section 5.1.
//!
//! Parse and validate answer at once, since no hook awaits inside them, and
//! the context building answers a future, so a hook that awaits fits there.
//! Execute and subscribe run the engine on the agent the caller passes, drive
//! the agent until the result has settled, and then run every done hook over
//! the result.
//!
//! A subscription answers a stream when the engine settles to an async
//! iterable. The stream is mapped through the hooks `on_next` the done hooks
//! answer, wrapped for their hooks `on_end` and wrapped for the hooks
//! `on_subscribe_error`, in that order. Yoga's server registers a hook
//! `onSubscribe` in every configuration, so this path is the only one a
//! subscription takes. Yoga's own hook `onSubscribeError`, which rethrows an
//! abort error past the mask, finds none to rethrow, because the server passes
//! no abort signal, and is not ported.

use core::cell::{
    Cell,
    RefCell, //
};
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    Agent,
    ErrorObject,
    ExecutionArgs,
    ExecutionResult,
    PromiseState,
    Value,
    map_async_iterator,
    map_async_iterator::OnValue,
    value::{
        Completion,
        is_async_iterable, //
    }, //
};
use aosabi_lang::{
    GraphQLError,
    ast::DocumentNode, //
};
use aosabi_type::Schema;
use aosabi_validate::ValidationRule;

#[cfg(feature = "host-hooks")]
use crate::plugin::Awaited;
use crate::{
    context::HookContext,
    document_string_map::{
        set_document_string,
        set_document_string_of_source, //
    },
    plugin::{
        ContextErrorHandler,
        ExecuteFn,
        ExecutionOutcome,
        HookPlugin,
        LocalBoxFuture,
        OnContextBuilding,
        OnContextError,
        OnEndHook,
        OnEnveloped,
        OnExecute,
        OnExecuteDone,
        OnExecuteDoneHook,
        OnNextHook,
        OnParse,
        OnParseDone,
        OnPluginInit,
        OnSchemaChange,
        OnSubscribe,
        OnSubscribeErrorHook,
        OnValidate,
        OnValidateDone,
        ParseFn,
        ResultStream,
        ValidateFn, //
    },
    utils::{
        OnError,
        error_async_iterator,
        final_async_iterator,
        outcome_value, //
    }, //
};

/// Returns the error of a missing engine function, `throwEngineFunctionError(name)`.
///
/// The text is the reference's.
fn engine_function_error(name: &str) -> Value {
    Value::from(ErrorObject::error(format!(
        "No `{name}` function found! Register it using \"useEngine\" plugin."
    )))
}

/// The orchestrator of the hooks of every plugin.
pub struct Orchestrator {
    /// The plugins after every splice of `on_plugin_init`.
    plugins: Vec<Rc<dyn HookPlugin>>,
    /// The current schema.
    // An `unsafe` cell would skip the flag.
    schema: RefCell<Option<Arc<Schema>>>,
    /// The handlers of a failure of the context building.
    context_error_handlers: Vec<ContextErrorHandler>,
    /// The parse function a parse runs while no hook sets one, made once.
    default_parse: ParseFn,
    /// The validate function a validation runs while no hook sets one, made once.
    default_validate: ValidateFn,
    /// The emptied rule list of the last validation, whose room the next one reuses.
    spare_rules: Cell<Vec<ValidationRule>>,
}

impl Orchestrator {
    /// Returns the plugins after every splice.
    #[must_use]
    pub fn plugins(&self) -> &[Rc<dyn HookPlugin>] {
        &self.plugins
    }

    /// Returns the current schema, `getCurrentSchema()`.
    #[must_use]
    pub fn current_schema(&self) -> Option<Arc<Schema>> {
        self.schema.borrow().clone()
    }

    /// Replaces the schema and tells every plugin but the one at `ignore`, `replaceSchema(schema, ignore)`.
    ///
    /// A schema that is the current one already changes nothing and tells no
    /// plugin.
    fn replace_schema(&self, schema: &Arc<Schema>, ignore: Option<usize>) {
        if self
            .schema
            .borrow()
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, schema))
        {
            return;
        }
        *self.schema.borrow_mut() = Some(Arc::clone(schema));
        self.announce_schema(schema, ignore);
    }

    /// Hands `schema` to `on_schema_change` of every plugin but the one at `ignore`.
    ///
    /// A plugin that replaces the schema in its hook announces the replacement
    /// to every other plugin at once, and the plugins after it here still
    /// receive `schema`, where the loops of the reference hand them the
    /// replacement. No plugin of the workspace replaces the schema in that
    /// hook.
    fn announce_schema(&self, schema: &Arc<Schema>, ignore: Option<usize>) {
        for (i, plugin) in self.plugins.iter().enumerate() {
            if Some(i) == ignore {
                continue;
            }
            let mut payload = OnSchemaChange {
                schema,
                replaced: None,
            };
            plugin.on_schema_change(&mut payload);
            if let Some(replaced) = payload.replaced {
                self.replace_schema(&replaced, Some(i));
            }
        }
    }

    /// Runs `on_enveloped` of every plugin over the initial context, `init(initialContext)`.
    ///
    /// # Errors
    ///
    /// Returns the value a hook throws.
    pub fn init(&self, context: &HookContext) -> Result<(), Value> {
        for (i, plugin) in self.plugins.iter().enumerate() {
            let mut payload = OnEnveloped {
                context,
                schema: None,
            };
            plugin.on_enveloped(&mut payload)?;
            if let Some(schema) = payload.schema {
                self.replace_schema(&schema, Some(i));
            }
        }
        Ok(())
    }

    /// Parses `source` through the parse hooks, the parse function `parse(context)` answers.
    ///
    /// The hooks run in order, a document or an error a hook sets skips the
    /// parse function, and every done hook sees the result before an error is
    /// thrown.
    ///
    /// # Errors
    ///
    /// Returns the value a hook throws, or the parse error after every done
    /// hook of the parse saw it.
    pub fn parse(&self, context: &HookContext, source: &str) -> Result<Arc<DocumentNode>, Value> {
        let mut parse_fn: ParseFn = Rc::clone(&self.default_parse);
        let mut parsed: Option<Result<Arc<DocumentNode>, Value>> = None;
        let mut of_source = None;
        let mut done_hooks = Vec::new();
        for plugin in &self.plugins {
            let mut payload = OnParse {
                context,
                source,
                parse_fn: &mut parse_fn,
                parsed: &mut parsed,
                of_source: &mut of_source,
            };
            done_hooks.extend(plugin.on_parse(&mut payload)?);
        }
        let mut result = match parsed {
            Some(result) => result,
            None => parse_fn(source),
        };
        for done in done_hooks {
            done(&mut OnParseDone {
                context,
                result: &mut result,
            });
        }
        let document = result?;
        // A hook that compared the body of the document's source with the
        // source marked the document, and no hook replaced it since. The mark
        // holds the document it names, so no other document took its address.
        if of_source.is_some_and(|marked| Arc::ptr_eq(&marked, &document)) {
            set_document_string_of_source(&document);
        } else {
            set_document_string(&document, source);
        }
        Ok(document)
    }

    /// Validates `document` against `schema` through the validate hooks.
    ///
    /// `rules` are the rules the caller names, `None` for the rules of the
    /// validate function. Errors a hook sets skip the validate function, and
    /// every done hook sees the errors after it.
    ///
    /// # Errors
    ///
    /// Returns the value the validate function throws, a schema it refuses
    /// among them.
    pub fn validate(
        &self,
        context: &HookContext,
        schema: &Arc<Schema>,
        document: &DocumentNode,
        rules: Option<Vec<ValidationRule>>,
    ) -> Result<Vec<GraphQLError>, Value> {
        let mut rules = rules;
        let mut validate_fn: ValidateFn = Rc::clone(&self.default_validate);
        let mut spare = self.spare_rules.take();
        let mut result: Option<Vec<GraphQLError>> = None;
        let mut done_hooks = Vec::new();
        for plugin in &self.plugins {
            let mut payload = OnValidate {
                context,
                schema,
                document,
                rules: &mut rules,
                validate_fn: &mut validate_fn,
                result: &mut result,
                spare: &mut spare,
            };
            done_hooks.extend(plugin.on_validate(&mut payload));
        }
        let result = match result {
            Some(result) => Ok(result),
            None => validate_fn(schema, document, rules.as_deref()),
        };
        if let Some(mut used) = rules {
            used.clear();
            spare = used;
        }
        self.spare_rules.set(spare);
        let mut result = result?;
        let valid = result.is_empty();
        for done in done_hooks {
            done(&mut OnValidateDone {
                valid,
                result: &mut result,
                context,
            });
        }
        Ok(result)
    }

    /// Builds the context of the request through the context hooks, `contextFactory(initialContext)()`.
    ///
    /// The hooks run in order, each awaited before the next, a hook that
    /// breaks the building stops it and skips the done hooks, and a failure
    /// passes every context error handler before it is thrown.
    pub fn context_factory(
        self: &Rc<Self>,
        context: HookContext,
    ) -> LocalBoxFuture<'static, Result<HookContext, Value>> {
        let orchestrator = Rc::clone(self);
        Box::pin(async move {
            let mut done_hooks = Vec::new();
            for plugin in &orchestrator.plugins {
                let mut payload = OnContextBuilding {
                    context: &context,
                    broken: false,
                };
                let hook = plugin.on_context_building(&mut payload).unwrap_or_default();
                let broken = payload.broken;
                if let Some(wait) = hook.wait
                    && let Err(error) = wait.await
                {
                    return Err(orchestrator.context_error(&context, error));
                }
                if broken {
                    return Ok(context);
                }
                done_hooks.extend(hook.done);
            }
            for done in done_hooks {
                done(&context);
            }
            Ok(context)
        })
    }

    /// Passes `error` through every context error handler and answers the error they leave.
    fn context_error(&self, context: &HookContext, error: Value) -> Value {
        let mut error = error;
        for handler in &self.context_error_handlers {
            handler(&mut OnContextError {
                context,
                error: &mut error,
            });
        }
        error
    }

    /// Runs the execute hooks, the execute function on `agent` and the done hooks.
    ///
    /// The context value of `args` is the context the hooks extend, a hook
    /// that sets the result stops the hooks after it and the execute function,
    /// and the future drives `agent` until the result has settled. The done
    /// hooks then run as `run_done_hooks` runs them. No executor of Aosabi
    /// answers a stream here, since it answers a stream for a subscription
    /// alone.
    pub fn execute(
        self: &Rc<Self>,
        agent: &Agent,
        args: ExecutionArgs,
    ) -> LocalBoxFuture<'static, Result<ExecutionOutcome, Value>> {
        let orchestrator = Rc::clone(self);
        let agent = agent.clone();
        Box::pin(async move {
            let mut args = args;
            let context = context_of(&args);
            let mut execute_fn: ExecuteFn =
                Rc::new(|_: &Agent, _: ExecutionArgs| Err(engine_function_error("execute")));
            let mut result = None;
            let mut done_hooks = Vec::new();
            for plugin in &orchestrator.plugins {
                let mut payload = OnExecute {
                    args: &mut args,
                    context: &context,
                    execute_fn: &mut execute_fn,
                    result: &mut result,
                };
                if let Some(hooks) = plugin.on_execute(&mut payload)? {
                    done_hooks.extend(hooks.on_execute_done);
                }
                if result.is_some() {
                    break;
                }
            }
            let outcome = match result {
                Some(outcome) => outcome,
                None => run_engine(&agent, &execute_fn, clone_args(&args, &context)).await?,
            };
            run_done_hooks(&agent, args, outcome, done_hooks, Vec::new()).await
        })
    }

    /// Runs the subscribe hooks, the subscribe function on `agent` and the result hooks.
    ///
    /// The hooks run as those of [`Orchestrator::execute`] do, and the result
    /// hooks then run as `run_done_hooks` runs them, with the hooks
    /// `on_subscribe_error` over a failure of the stream.
    pub fn subscribe(
        self: &Rc<Self>,
        agent: &Agent,
        args: ExecutionArgs,
    ) -> LocalBoxFuture<'static, Result<ExecutionOutcome, Value>> {
        let orchestrator = Rc::clone(self);
        let agent = agent.clone();
        Box::pin(async move {
            let mut args = args;
            let context = context_of(&args);
            let mut subscribe_fn: ExecuteFn =
                Rc::new(|_: &Agent, _: ExecutionArgs| Err(engine_function_error("subscribe")));
            let mut result = None;
            let mut done_hooks = Vec::new();
            let mut error_hooks = Vec::new();
            for plugin in &orchestrator.plugins {
                let mut payload = OnSubscribe {
                    args: &mut args,
                    context: &context,
                    subscribe_fn: &mut subscribe_fn,
                    result: &mut result,
                };
                if let Some(hooks) = plugin.on_subscribe(&mut payload)? {
                    done_hooks.extend(hooks.on_subscribe_result);
                    error_hooks.extend(hooks.on_subscribe_error);
                }
                if result.is_some() {
                    break;
                }
            }
            let outcome = match result {
                Some(outcome) => outcome,
                None => run_engine(&agent, &subscribe_fn, clone_args(&args, &context)).await?,
            };
            run_done_hooks(&agent, args, outcome, done_hooks, error_hooks).await
        })
    }
}

/// Runs `done_hooks` over `outcome` in their order and wraps a stream for the hooks they answer.
///
/// Each done hook may change the result. A stream is then mapped through the
/// hooks `on_next` the done hooks answer, wrapped for their hooks `on_end`,
/// and wrapped for `error_hooks`, the hooks `on_subscribe_error` of a
/// subscription, whose error the call of `next` rejects with once every hook
/// saw it. Under the feature `host-hooks` a done hook that throws rejects the
/// run, and no done hook after it runs, as both loops of the reference throw
/// out. The run also awaits a promise a done hook answers, as the loop of an
/// execution does and the one of a subscription does not.
#[cfg_attr(
    not(feature = "host-hooks"),
    expect(
        clippy::unused_async,
        reason = "only a done hook of a host answers a promise the run awaits"
    )
)]
async fn run_done_hooks(
    agent: &Agent,
    args: ExecutionArgs,
    outcome: ExecutionOutcome,
    done_hooks: Vec<OnExecuteDoneHook>,
    error_hooks: Vec<OnSubscribeErrorHook>,
) -> Result<ExecutionOutcome, Value> {
    let args = Rc::new(args);
    let mut outcome = outcome;
    let mut on_next = Vec::new();
    let mut on_end = Vec::new();
    for done in done_hooks {
        let mut payload = OnExecuteDone::new(&args, &mut outcome, agent);
        if let Some(answer) = done(&mut payload) {
            on_next.extend(answer.on_next);
            on_end.extend(answer.on_end);
        }
        #[cfg(feature = "host-hooks")]
        {
            let OnExecuteDone {
                thrown, awaited, ..
            } = payload;
            hook_ended(agent, thrown, awaited, &mut outcome).await?;
        }
    }
    if let ExecutionOutcome::Stream(stream) = &mut outcome {
        map_stream(stream, &args, on_next, on_end);
        if !error_hooks.is_empty() {
            let on_error = subscribe_error(error_hooks);
            stream.wrap(|_, iterable| error_async_iterator(iterable, on_error));
        }
    }
    Ok(outcome)
}

/// Ends the call of a done hook of a host, whose payload left the value
/// `thrown` it threw and the promise `awaited` it answered.
///
/// A throw rejects the execution with its value. A promise is awaited on
/// `agent` before the next hook runs, and its fulfilment runs what follows it
/// over `outcome`, while its rejection rejects the execution with its reason,
/// as `iterateAsync` of the reference awaits a done hook that answers a
/// promise and rejects with its rejection. A promise that never settles while
/// the agent idles ends the execution as a stalled request ends it.
#[cfg(feature = "host-hooks")]
async fn hook_ended(
    agent: &Agent,
    thrown: Option<Value>,
    awaited: Option<Awaited>,
    outcome: &mut ExecutionOutcome,
) -> Result<(), Value> {
    if let Some(thrown) = thrown {
        return Err(thrown);
    }
    let Some(Awaited { promise, then }) = awaited else {
        return Ok(());
    };
    match agent.drive(&promise).await {
        Ok(PromiseState::Fulfilled(_)) => {
            then(outcome);
            Ok(())
        }
        Ok(PromiseState::Rejected(error)) => Err(error),
        Ok(PromiseState::Pending) | Err(_) => Err(Value::from(ErrorObject::error(
            "The request stalled before a done hook settled.",
        ))),
    }
}

/// Maps `stream` through the hooks `on_next` and wraps it for the hooks `on_end`.
///
/// Each result of the stream passes the hooks `on_next` in their order, each
/// of which may replace it, as the payload of `on_execute_done` reads a single
/// result, and the hooks `on_end` run once, in their order, when the stream
/// ends. Under the feature `host-hooks` a hook `on_next` that throws ends the
/// mapping of that result with its value, as the mapping function the
/// reference hands `mapAsyncIterator` rejects. The mapping awaits no promise a
/// hook `on_next` answers, since the hooks of the engine answer at once and no
/// hook of a host runs there, an execution of the engine answering no stream of
/// results.
fn map_stream(
    stream: &mut ResultStream,
    args: &Rc<ExecutionArgs>,
    on_next: Vec<OnNextHook>,
    on_end: Vec<OnEndHook>,
) {
    if !on_next.is_empty() {
        let args = Rc::clone(args);
        let on_next = Rc::new(on_next);
        let map: OnValue = Rc::new(move |agent, value| {
            let mut result = ExecutionOutcome::Single(ExecutionResult::from_value(&value));
            for on_next in on_next.iter() {
                let mut payload = OnExecuteDone::new(&args, &mut result, agent);
                on_next(&mut payload);
                #[cfg(feature = "host-hooks")]
                if let Some(thrown) = payload.thrown {
                    return Completion::Throw(thrown);
                }
            }
            Completion::Normal(outcome_value(result))
        });
        stream.wrap(|agent, iterable| map_async_iterator(agent, iterable, map, None, None));
    }
    if !on_end.is_empty() {
        let on_final = Rc::new(move || {
            for on_end in &on_end {
                on_end();
            }
        });
        stream.wrap(|_, iterable| final_async_iterator(iterable, on_final));
    }
}

/// Returns the handler of a failed stream, which passes the error through every hook of `handlers` and throws it.
///
/// The handler the reference hands `errorAsyncIterator`.
fn subscribe_error(handlers: Vec<OnSubscribeErrorHook>) -> OnError {
    Rc::new(move |_, error| {
        let mut error = error;
        for handler in &handlers {
            handler(&mut error);
        }
        Completion::Throw(error)
    })
}

/// Returns the orchestrator of `plugins`, `createEnvelopOrchestrator({ plugins })`.
///
/// `on_plugin_init` of every plugin runs in order, a plugin it adds runs next,
/// and every plugin then sees the schema a hook set in its first
/// `on_schema_change`.
#[must_use]
pub fn create_hook_orchestrator(plugins: Vec<Rc<dyn HookPlugin>>) -> Rc<Orchestrator> {
    let mut plugins = plugins;
    let mut schema = None;
    let mut context_error_handlers = Vec::new();
    let mut i = 0;
    while i < plugins.len() {
        let plugin = Rc::clone(&plugins[i]);
        let mut payload = OnPluginInit {
            plugins: &plugins,
            added: Vec::new(),
            schema: None,
            context_error_handlers: Vec::new(),
        };
        plugin.on_plugin_init(&mut payload);
        let OnPluginInit {
            added,
            schema: set,
            context_error_handlers: handlers,
            ..
        } = payload;
        schema = set.or(schema);
        context_error_handlers.extend(handlers);
        i += 1;
        plugins.splice(i..i, added);
    }
    let orchestrator = Rc::new(Orchestrator {
        plugins,
        schema: RefCell::new(schema.clone()),
        context_error_handlers,
        default_parse: Rc::new(|_: &str| Err(engine_function_error("parse"))),
        default_validate: Rc::new(
            |_: &Schema, _: &DocumentNode, _: Option<&[ValidationRule]>| {
                Err(engine_function_error("validate"))
            },
        ),
        spare_rules: Cell::new(Vec::new()),
    });
    if let Some(schema) = schema {
        orchestrator.announce_schema(&schema, None);
    }
    orchestrator
}

/// Returns the context the context value of `args` is, or a new one, `args.contextValue || {}`.
fn context_of(args: &ExecutionArgs) -> HookContext {
    args.context_value
        .downcast_ref::<HookContext>()
        .cloned()
        .unwrap_or_default()
}

/// Returns a copy of `args` with the context value `context`, `{ ...args, contextValue: context }`.
///
/// The copy shares the variables of `args` by `Rc` and copies no value of
/// them, as the spread shares the object `variableValues`.
fn clone_args(args: &ExecutionArgs, context: &HookContext) -> ExecutionArgs {
    ExecutionArgs {
        schema: Arc::clone(&args.schema),
        document: Arc::clone(&args.document),
        root_value: args.root_value.clone(),
        context_value: Rc::new(context.clone()),
        variable_values: args.variable_values.clone(),
        operation_name: args.operation_name.clone(),
        field_resolver: args.field_resolver.clone(),
        type_resolver: args.type_resolver.clone(),
        subscribe_field_resolver: args.subscribe_field_resolver.clone(),
        leaf_serializers: args.leaf_serializers.clone(),
    }
}

/// Runs `engine` on `agent` inside the agent and drives the agent until the result has settled.
///
/// A result that is no promise is the result at once, and a promise is
/// awaited through the host loop of the agent. A promise that is still
/// pending when the agent has nothing left to run is the error of a stalled
/// request, a text of Aosabi and of no reference. A result that is async
/// iterable is a stream on `agent`, the `isAsyncIterable` fork of every
/// hook after it, and any other result is read as a single one.
async fn run_engine(
    agent: &Agent,
    engine: &ExecuteFn,
    args: ExecutionArgs,
) -> Result<ExecutionOutcome, Value> {
    let settled = agent.enter(|| engine(agent, args))?;
    let value = match settled {
        Value::Promise(promise) => match agent.drive(&promise).await {
            Ok(PromiseState::Fulfilled(value)) => value,
            Ok(PromiseState::Rejected(error)) => return Err(error),
            Ok(PromiseState::Pending) | Err(_) => {
                return Err(Value::from(ErrorObject::error(
                    "The request stalled before its result settled.",
                )));
            }
        },
        value => value,
    };
    if is_async_iterable(&value) {
        return Ok(ExecutionOutcome::Stream(ResultStream::new(
            agent.clone(),
            value,
        )));
    }
    Ok(ExecutionOutcome::Single(ExecutionResult::from_value(
        &value,
    )))
}
