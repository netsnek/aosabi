// SPDX-License-Identifier: MIT
/*
 * aosabi-execute subscribe: the subscription entry of the executor.
 *
 * Derived from @graphql-tools/executor 1.5.1, cjs/execution/execute.js, MIT.
 * Derived from graphql-js 16.12.0, src/execution/subscribe.ts, MIT.
 */

//! The subscription entry.
//!
//! [`subscribe`] is the branch `normalizedExecutor` takes for a subscription
//! operation. It resolves the source event stream of the one root field
//! through its `subscribe` resolver and maps every event of that stream to
//! the execution of the operation with the event as the root value, which is
//! the `MapSourceToResponseEvent` algorithm of the GraphQL specification.
//! Every function keeps the name it has in `execute.js`, in snake case.
//!
//! | Reference | Here |
//! | --- | --- |
//! | `buildPerEventExecutionContext`, `:248-255` | `build_per_event_execution_context` |
//! | `subscribe`, `:848-870` | [`subscribe`] |
//! | `ensureAsyncIterable`, `:909-916` | `ensure_async_iterable`, the async generator of its one result |
//! | `mapSourceToResponse` and `wrapError`, `:917-939` | private functions of the same names |
//! | `createSourceEventStreamImpl`, `:940-951` | a private function of the same name |
//! | `executeSubscription`, `:952-997` | a private function of the same name |
//! | `assertEventStream`, `:998-1021` | a private function of the same name |
//!
//! # What is left out
//!
//! | Reference | Why |
//! | --- | --- |
//! | `isIncrementalResults` and `flattenIncrementalResults`, `:871-908`, and the incremental branch of `ensureAsyncIterable` | no directive named `defer` or `stream` reaches a schema of Aosabi, as in [`execute`](mod@crate::execute) |
//! | `signal?.throwIfAborted()` and the `onSignalAbort` wrapper of `assertEventStream`, `:999` and `:1007-1019` | the server hands the executor no signal, as in [`execute`](mod@crate::execute) |
//! | `schemaCoordinateInErrors`, the fourth argument of `locatedError` | Yoga never asks for it |
//!
//! The module names `core` and `alloc` and no item of `std`.

use alloc::{
    boxed::Box,
    collections::VecDeque,
    format,
    rc::Rc,
    string::String,
    sync::Arc,
    vec,
    vec::Vec, //
};
use core::cell::{
    Cell,
    RefCell, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    OriginalError,
    PathSegment,
    ast::{
        AstNode,
        FieldNode,
        OperationDefinitionNode, //
    }, //
};
use aosabi_type::{
    Field,
    NamedType, //
};

use crate::{
    arena::Arena,
    collect_fields::{
        FieldNodeCache,
        collect_fields, //
    },
    errors::{
        create_graphql_error,
        into_graphql_error,
        located_error,
        thrown_definition_error, //
    },
    execute::{
        ExecutionArgs,
        ExecutionContext,
        FieldNodes,
        FieldResolve,
        FieldResolverFn,
        ResolveInfo,
        arguments_object,
        build_execution_context,
        build_resolve_info,
        execute_impl,
        get_field_def,
        to_boolean, //
    },
    flatten_async_iterable::flatten_async_iterable,
    handle_maybe_promise::{
        MaybePromise,
        handle_maybe_promise, //
    },
    map_async_iterator::{
        OnValue,
        map_async_iterator, //
    },
    path::{
        Path,
        add_path,
        path_to_array, //
    },
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
        PlainObject,
        Value,
        cross_inspect,
        is_async_iterable,
        iter_result,
        to_js_string, //
    },
    values::get_argument_values, //
};

/// Implements the "Subscribe" algorithm described in the GraphQL specification.
///
/// Returns the error result, the async iterable of results, or a promise of
/// either. If the arguments do not make a legal execution context, a result
/// with only errors is returned, and each of its errors gains
/// `extensions.http.status` 400 and the code `BAD_USER_INPUT`. If the source
/// stream could not be created due to faulty subscription resolver logic or
/// underlying systems, the answer is a result with only errors as well.
/// Otherwise the answer is an async iterator that yields one result per event
/// of the source stream, or a promise of it when the subscribe resolver
/// answers a promise.
///
/// # Errors
///
/// Returns what `buildExecutionContext` throws for an invalid schema, as
/// [`execute`](crate::execute()) does.
// Out of line, so an executor that runs no subscription compiles none of its path.
#[inline(never)]
pub fn subscribe(agent: &Agent, args: ExecutionArgs) -> Result<MaybePromise, Value> {
    // If a valid execution context cannot be created due to incorrect arguments,
    // a "Response" with only errors is returned.
    let exe_context = match build_execution_context(args)? {
        Ok(exe_context) => Rc::new(exe_context),
        // Return early errors if execution context failed.
        Err(errors) => {
            let errors: Vec<Value> = errors.into_iter().map(with_bad_user_input).collect();
            let entries = vec![(String::from("errors"), Value::from(errors))];
            return Ok(Value::from(PlainObject::from_entries(entries)));
        }
    };
    let result_or_stream = create_source_event_stream_impl(agent, &exe_context);
    let Value::Promise(result_or_stream) = result_or_stream else {
        return Ok(map_source_to_response(
            agent,
            &exe_context,
            result_or_stream,
        ));
    };
    let mapped = result_or_stream.then(
        agent,
        Some(Box::new(move |agent, resolved_result_or_stream| {
            Completion::Normal(map_source_to_response(
                agent,
                &exe_context,
                resolved_result_or_stream,
            ))
        })),
        None,
    );
    Ok(Value::Promise(mapped))
}

/// Returns an early error as a value, with `http.status` 400 and the code `BAD_USER_INPUT`.
///
/// `extensions ||= {}`, then `http ||= {}`, then `http.status = 400`, then
/// `code = 'BAD_USER_INPUT'`, each on the insertion ordered extensions, so a
/// key that exists keeps its place and a new one is appended. The extensions
/// of a `GraphQLError` are an object always, so the first step keeps them.
/// The early errors carry the code `OPERATION_RESOLUTION_FAILURE` or no
/// extension at all and never an `http` member, so a truthy `http` that is no
/// object, on which the assignment of the status would throw in strict mode,
/// has no reading, and it keeps its value here.
fn with_bad_user_input(mut error: GraphQLError) -> Value {
    let extensions = &mut error.extensions;
    let http = match extensions.get("http") {
        Some(JsValue::Object(http)) => Some(http.clone()),
        Some(http) if to_boolean(&Value::from_js(http)) => None,
        _ => Some(JsObject::new()),
    };
    if let Some(mut http) = http {
        http.insert("status", JsValue::Number(400.0));
        extensions.insert("http", JsValue::Object(http));
    }
    extensions.insert("code", JsValue::from("BAD_USER_INPUT"));
    Value::from(ErrorObject::GraphQL(error))
}

/// Returns the context of one event, which shares the context it copies and has its own errors.
///
/// The spread `{...exeContext}` copies references, so the schema, the
/// fragments, the operation, the variable values and the resolvers are
/// shared, the payload becomes the root value, and the error list is fresh.
/// The memo of `collectSubfields` is fresh in the reference, because
/// `memoize3` keys it by the identity of the context, which the spread makes
/// new. The field plan of the document, which holds what that memo holds and
/// is a function of the document and the schema alone, is shared, and the
/// nodes the execution keeps for itself, the arena of its answer and the
/// promised fields of its objects start empty. The arena is made by
/// [`Arena::new`], as the context of every execution makes it, so it takes
/// the vectors its thread keeps between executions, emptied.
pub(crate) fn build_per_event_execution_context(
    exe_context: &Rc<ExecutionContext>,
    payload: Value,
) -> Rc<ExecutionContext> {
    Rc::new(ExecutionContext {
        schema: Arc::clone(&exe_context.schema),
        fragments: Rc::clone(&exe_context.fragments),
        root_value: payload,
        context_value: Rc::clone(&exe_context.context_value),
        operation: Rc::clone(&exe_context.operation),
        variable_values: Rc::clone(&exe_context.variable_values),
        field_resolver: Arc::clone(&exe_context.field_resolver),
        reads_members: exe_context.reads_members,
        type_resolver: Arc::clone(&exe_context.type_resolver),
        subscribe_field_resolver: Arc::clone(&exe_context.subscribe_field_resolver),
        leaf_serializers: exe_context.leaf_serializers.clone(),
        leaf_rules: exe_context.leaf_rules,
        errors: RefCell::new(Vec::new()),
        plan: Rc::clone(&exe_context.plan),
        local: RefCell::default(),
        operation_key: exe_context.operation_key,
        arena: RefCell::new(Arena::new()),
        pending: RefCell::default(),
    })
}

/// Maps every event of the source stream to the response of executing the operation on it.
///
/// A value that is no async iterable is a result with only errors and comes
/// back as it is. For each payload yielded from a subscription, the operation
/// runs over the normal executor with `payload` as the root value, which is
/// the `ExecuteSubscriptionEvent` algorithm, and a rejection of the source
/// becomes a `GraphQLError` that blames the operation.
fn map_source_to_response(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    result_or_stream: Value,
) -> Value {
    if !is_async_iterable(&result_or_stream) {
        return result_or_stream;
    }
    let per_event = Rc::clone(exe_context);
    let on_next: OnValue = Rc::new(move |agent, payload| {
        let exe_context = Rc::clone(&per_event);
        handle_maybe_promise(
            agent,
            move |agent| {
                let event_context = build_per_event_execution_context(&exe_context, payload);
                execute_impl(agent, &event_context).into()
            },
            Box::new(|_, result| Completion::Normal(ensure_async_iterable(result))),
            None,
        )
    });
    let operation = Rc::clone(&exe_context.operation);
    let on_error: OnValue =
        Rc::new(move |_, error| Completion::Throw(wrap_rejection(&error, &operation)));
    let mapped = map_async_iterator(agent, result_or_stream, on_next, Some(on_error), None);
    flatten_async_iterable(agent, &mapped)
}

/// Returns what the error handler of `mapSourceToResponse` throws for `error`.
///
/// An `AggregateError` becomes a new one whose errors are each wrapped, with
/// the message it had, and any other error is wrapped itself.
fn wrap_rejection(error: &Value, operation: &OperationDefinitionNode) -> Value {
    let Value::Error(aggregate) = error else {
        return wrap_error(error, operation).unwrap_or_else(|thrown| thrown);
    };
    let ErrorObject::Aggregate { message, errors } = aggregate.as_ref() else {
        return wrap_error(error, operation).unwrap_or_else(|thrown| thrown);
    };
    let mut wrapped = Vec::with_capacity(errors.len());
    for error in errors {
        match wrap_error(error, operation) {
            Ok(error) => wrapped.push(error),
            Err(thrown) => return thrown,
        }
    }
    Value::from(ErrorObject::Aggregate {
        message: message.clone(),
        errors: wrapped,
    })
}

/// Returns `createGraphQLError(error.message, {originalError: error, nodes: [operation]})`.
///
/// The message is the `message` property of the error, which is empty for a
/// value that has none, and the extensions are those of the original error
/// when it carries an object there. The read of `message` off `undefined` or
/// `null` throws the `TypeError` V8 writes, which is the `Err`. A thrown
/// value that is no error is kept as the coerced error of the executor, which
/// carries what a later layer reads of an original error, its name and its
/// message.
fn wrap_error(error: &Value, operation: &OperationDefinitionNode) -> Result<Value, Value> {
    let message = message_of(error)?;
    let is_graphql_error =
        matches!(error, Value::Error(error) if matches!(error.as_ref(), ErrorObject::GraphQL(_)));
    let object_extensions = match error {
        Value::Object(object) => match Rc::clone(object).get("extensions").to_js() {
            JsValue::Object(extensions) => Some(extensions),
            _ => None,
        },
        _ => None,
    };
    let mut thrown = into_graphql_error(error.clone());
    let (original_error, extensions) = if is_graphql_error {
        (OriginalError::from(thrown), None)
    } else {
        let extensions = core::mem::take(&mut thrown.extensions);
        let original_error = match thrown.original_error.take() {
            Some(original_error) => original_error,
            None => OriginalError::from(thrown),
        };
        (
            original_error,
            Some(object_extensions.unwrap_or(extensions)),
        )
    };
    let wrapped = create_graphql_error(
        message,
        GraphQLErrorOptions {
            nodes: Some(vec![AstNode::from(operation.clone())]),
            original_error: Some(original_error),
            extensions,
            ..GraphQLErrorOptions::default()
        },
    );
    Ok(Value::from(ErrorObject::GraphQL(wrapped)))
}

/// Returns `error.message` as the message the constructor of an error makes of it.
fn message_of(error: &Value) -> Result<JsString, Value> {
    let message = match error {
        Value::Undefined => {
            return Err(Value::from(ErrorObject::type_error(
                "Cannot read properties of undefined (reading 'message')",
            )));
        }
        Value::Null => {
            return Err(Value::from(ErrorObject::type_error(
                "Cannot read properties of null (reading 'message')",
            )));
        }
        Value::Error(error) => return Ok(error.message()),
        Value::Object(object) => Rc::clone(object).get("message"),
        _ => Value::Undefined,
    };
    match message {
        Value::Undefined => Ok(JsString::from("")),
        message => Ok(to_js_string(&message)),
    }
}

/// Resolves the source event stream, and answers the errors of a failure as a result.
///
/// A throw of `executeSubscription` and a rejection of its promise both
/// become `{ errors: [error] }`, a result with no `data` key.
fn create_source_event_stream_impl(agent: &Agent, exe_context: &Rc<ExecutionContext>) -> Value {
    match execute_subscription(agent, exe_context) {
        Ok(Value::Promise(event_stream)) => Value::Promise(event_stream.then(
            agent,
            None,
            Some(Box::new(|_, error| Completion::Normal(errors_only(error)))),
        )),
        Ok(event_stream) => event_stream,
        Err(error) => errors_only(error),
    }
}

/// Returns `{ errors: [error] }`.
fn errors_only(error: Value) -> Value {
    let entries = vec![(String::from("errors"), Value::from(vec![error]))];
    Value::from(PlainObject::from_entries(entries))
}

/// Returns the subscribe resolver of `field_def`, `fieldDef.subscribe ?? exeContext.subscribeFieldResolver`.
///
/// The slot of the type layer is read as a [`FieldResolve`], as
/// [`execute`](mod@crate::execute) reads the slot `resolve`, and a slot that
/// holds no such function reads as none.
fn subscribe_fn_of(exe_context: &ExecutionContext, field_def: &Field) -> FieldResolverFn {
    let subscribe = field_def
        .subscribe
        .as_ref()
        .and_then(|handle| handle.downcast_ref::<FieldResolve>());
    match subscribe {
        Some(subscribe) => Arc::clone(&subscribe.0),
        None => Arc::clone(&exe_context.subscribe_field_resolver),
    }
}

/// Returns the `GraphQLError` that blames `nodes`, as a thrown value.
fn graphql_error(message: String, nodes: Vec<AstNode>) -> Value {
    let error = create_graphql_error(
        message,
        GraphQLErrorOptions {
            nodes: Some(nodes),
            ..GraphQLErrorOptions::default()
        },
    );
    Value::from(ErrorObject::GraphQL(error))
}

/// Returns `locatedError(error, fieldNodes, pathToArray(path))` as a thrown value.
fn located(error: Value, field_nodes: &FieldNodes, path: &Path) -> Value {
    let error = located_error(error, field_nodes, path_to_array(Some(path)));
    Value::from(ErrorObject::GraphQL(error))
}

/// Resolves the source event stream of the one root field of the operation.
///
/// The root fields are collected on the subscription type and the first of
/// them is resolved with its `subscribe` resolver, which is the
/// `ResolveFieldEventStream` algorithm of the GraphQL specification. A throw
/// of that resolver, of the argument coercion and of the assertion, and a
/// rejection of a promise the resolver answers, are located at the field. A
/// selection set with no field left after `@skip` and `@include` has no first
/// entry, and the destructuring of the reference throws the `TypeError` V8
/// writes for it.
fn execute_subscription(agent: &Agent, exe_context: &Rc<ExecutionContext>) -> Result<Value, Value> {
    let schema = &exe_context.schema;
    let operation = &exe_context.operation;
    let root_type = schema
        .get_subscription_type()
        .and_then(NamedType::as_object);
    let Some(root_type) = root_type else {
        return Err(graphql_error(
            String::from("Schema is not configured to execute subscription operation."),
            vec![AstNode::from(OperationDefinitionNode::clone(operation))],
        ));
    };
    // The source stream reads its root field once per subscription, so its
    // collection shares the field nodes through a cache of its own and adds
    // nothing to the plan, which every event reads.
    let root_fields = collect_fields(
        schema,
        &exe_context.fragments,
        &exe_context.variable_values,
        root_type,
        &operation.selection_set,
        &FieldNodeCache::new(),
    )
    .map_err(thrown_definition_error)?;
    let Some((response_name, field_nodes)) = root_fields.iter().next() else {
        return Err(Value::from(ErrorObject::type_error(
            "undefined is not iterable (cannot read property Symbol(Symbol.iterator))",
        )));
    };
    let Some(first_node) = field_nodes.first() else {
        return Err(Value::from(ErrorObject::type_error(
            "Cannot read properties of undefined (reading 'name')",
        )));
    };
    let field_name = first_node.name.value();
    let field_def =
        get_field_def(schema, root_type, first_node).map_err(thrown_definition_error)?;
    let Some(field_def) = field_def else {
        let nodes = field_nodes
            .iter()
            .map(|node| AstNode::from(FieldNode::clone(node)))
            .collect();
        return Err(graphql_error(
            format!("The subscription field \"{field_name}\" is not defined."),
            nodes,
        ));
    };
    let path = add_path(
        None,
        PathSegment::Key(response_name.clone()),
        Some(&root_type.name),
    );
    let info = Rc::new(build_resolve_info(
        exe_context,
        field_def,
        field_nodes,
        root_type,
        &path,
    ));
    resolve_field_event_stream(agent, exe_context, field_def, first_node, &info)
        .map_err(|error| located(error, field_nodes, &path))
}

/// Runs the `try` block of `executeSubscription` at `execute.js:971-993`.
///
/// The arguments are coerced from `first_node`, the `fieldNodes[0]` the
/// caller has read, the subscribe resolver is called with the root value,
/// and its answer is asserted to be an event stream. A promise is asserted
/// once it fulfils, and its rejection is located in a second reaction, as
/// the two `then` calls of the reference do.
fn resolve_field_event_stream(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    field_def: &Field,
    first_node: &FieldNode,
    info: &Rc<ResolveInfo>,
) -> Result<Value, Value> {
    // Build a JS object of arguments from the field.arguments AST, using the
    // variables scope to fulfill any variable references.
    let args = get_argument_values(field_def, first_node, &exe_context.variable_values)
        .map_err(|error| Value::from(ErrorObject::GraphQL(error)))?;
    let args = arguments_object(args);
    // Call the `subscribe()` resolver or the default resolver to produce an
    // AsyncIterable yielding raw payloads.
    let resolve_fn = subscribe_fn_of(exe_context, field_def);
    let context_value = &exe_context.context_value;
    let result =
        resolve_fn(agent, &exe_context.root_value, &args, context_value, info).into_result()?;
    let Value::Promise(result) = result else {
        return assert_event_stream(result);
    };
    let asserted = result.then(
        agent,
        Some(Box::new(|_, result| assert_event_stream(result).into())),
        None,
    );
    let field_nodes = Rc::clone(&info.field_nodes);
    let path = info.path.clone();
    let located_rejection = asserted.then(
        agent,
        None,
        Some(Box::new(move |_, error| {
            Completion::Throw(located(error, &field_nodes, &path))
        })),
    );
    Ok(Value::Promise(located_rejection))
}

/// Asserts that the subscribe resolver answered an event stream.
///
/// An error is thrown as it is, and any other value that is no async iterable
/// throws a `GraphQLError` that prints it with the `inspect` of
/// `cross-inspect`, which `@graphql-tools/utils` re-exports.
fn assert_event_stream(result: Value) -> Result<Value, Value> {
    if result.is_error() {
        return Err(result);
    }
    // Assert field returned an event stream, otherwise yield an error.
    if !is_async_iterable(&result) {
        let message = format!(
            "Subscription field must return Async Iterable. Received: {}.",
            cross_inspect(&result)
        );
        let error = create_graphql_error(message, GraphQLErrorOptions::default());
        return Err(Value::from(ErrorObject::GraphQL(error)));
    }
    Ok(result)
}

/// The state of an async generator object, ECMA-262 section 27.6.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratorState {
    /// The body has not started.
    SuspendedStart,
    /// The body waits at its `yield`.
    SuspendedYield,
    /// The body runs or awaits.
    Executing,
    /// A `return` awaits its value after the body completed.
    AwaitingReturn,
    /// The body has completed.
    Completed,
}

/// The completion a request resumes the body with.
#[derive(Clone)]
enum Resumption {
    /// `next(value)`.
    Next,
    /// `return(value)`.
    Return(Value),
    /// `throw(error)`.
    Throw(Value),
}

/// One request of the queue of an async generator, `AsyncGeneratorRequest`.
struct GeneratorRequest {
    /// The completion of the request.
    completion: Resumption,
    /// The resolving functions of the promise the request answered.
    capability: ResolvingFunctions,
}

/// The async generator object `ensureAsyncIterable(result)` answers.
///
/// The `yield` awaits the result first. A `next` resumes the body, a `return`
/// or a `throw` at the `yield` ends it, and a request that arrives while the
/// body awaits waits in the queue, so each request settles in the job the
/// clauses give it.
// An `unsafe` cell would skip each borrow flag.
struct ResultGenerator {
    /// The argument of the generator function, until the body starts.
    result: RefCell<Option<Value>>,
    /// `[[AsyncGeneratorState]]`.
    state: Cell<GeneratorState>,
    /// `[[AsyncGeneratorQueue]]`.
    queue: RefCell<VecDeque<GeneratorRequest>>,
}

/// Returns the async generator object of `ensureAsyncIterable(result)`, which yields `result` once.
fn ensure_async_iterable(result: Value) -> Value {
    let generator = Rc::new(ResultGenerator {
        result: RefCell::new(Some(result)),
        state: Cell::new(GeneratorState::SuspendedStart),
        queue: RefCell::new(VecDeque::new()),
    });
    let next = {
        let generator = Rc::clone(&generator);
        Function::new("next", move |agent, _| generator.next(agent))
    };
    let r#return = {
        let generator = Rc::clone(&generator);
        Function::new("return", move |agent, args| {
            let value = args.first().cloned().unwrap_or(Value::Undefined);
            generator.r#return(agent, value)
        })
    };
    let throw = Function::new("throw", move |agent, args| {
        let error = args.first().cloned().unwrap_or(Value::Undefined);
        generator.throw(agent, error)
    });
    let object: Rc<dyn ObjectLike> =
        IteratorObject::new("ensureAsyncIterable", next, Some(r#return), Some(throw));
    Value::Object(object)
}

impl ResultGenerator {
    /// Performs `next()`, `AsyncGenerator.prototype.next` of ECMA-262 section 27.6.1.2.
    fn next(self: &Rc<Self>, agent: &Agent) -> Completion {
        let (promise, capability) = Promise::with_resolvers(agent);
        let state = self.state.get();
        if state == GeneratorState::Completed {
            capability.resolve(agent, iter_result(Value::Undefined, true));
            return Completion::Normal(Value::Promise(promise));
        }
        self.enqueue(Resumption::Next, capability);
        if matches!(
            state,
            GeneratorState::SuspendedStart | GeneratorState::SuspendedYield
        ) {
            self.resume(agent, Resumption::Next);
        }
        Completion::Normal(Value::Promise(promise))
    }

    /// Performs `return(value)`, `AsyncGenerator.prototype.return` of ECMA-262 section 27.6.1.3.
    fn r#return(self: &Rc<Self>, agent: &Agent, value: Value) -> Completion {
        let (promise, capability) = Promise::with_resolvers(agent);
        self.enqueue(Resumption::Return(value.clone()), capability);
        match self.state.get() {
            GeneratorState::SuspendedStart | GeneratorState::Completed => {
                self.state.set(GeneratorState::AwaitingReturn);
                self.await_return(agent);
            }
            GeneratorState::SuspendedYield => self.resume(agent, Resumption::Return(value)),
            GeneratorState::Executing | GeneratorState::AwaitingReturn => {}
        }
        Completion::Normal(Value::Promise(promise))
    }

    /// Performs `throw(error)`, `AsyncGenerator.prototype.throw` of ECMA-262 section 27.6.1.4.
    fn throw(self: &Rc<Self>, agent: &Agent, error: Value) -> Completion {
        let (promise, capability) = Promise::with_resolvers(agent);
        let mut state = self.state.get();
        if state == GeneratorState::SuspendedStart {
            state = GeneratorState::Completed;
            self.state.set(state);
        }
        if state == GeneratorState::Completed {
            capability.reject(agent, error);
            return Completion::Normal(Value::Promise(promise));
        }
        self.enqueue(Resumption::Throw(error.clone()), capability);
        if state == GeneratorState::SuspendedYield {
            self.resume(agent, Resumption::Throw(error));
        }
        Completion::Normal(Value::Promise(promise))
    }

    /// Adds a request to the queue, `AsyncGeneratorEnqueue`.
    fn enqueue(&self, completion: Resumption, capability: ResolvingFunctions) {
        self.queue.borrow_mut().push_back(GeneratorRequest {
            completion,
            capability,
        });
    }

    /// Resumes the body with `completion`, `AsyncGeneratorResume`.
    ///
    /// From the start the body awaits the result and yields it, and from
    /// the `yield` it ends.
    fn resume(self: &Rc<Self>, agent: &Agent, completion: Resumption) {
        let previous = self.state.replace(GeneratorState::Executing);
        if previous != GeneratorState::SuspendedStart {
            self.resume_after_yield(agent, completion);
            return;
        }
        let result = self.result.take().unwrap_or(Value::Undefined);
        let on_fulfilled = Rc::clone(self);
        let on_rejected = Rc::clone(self);
        let _ = Promise::resolve(agent, result).then(
            agent,
            Some(Box::new(move |agent, value| {
                on_fulfilled.yield_value(agent, value);
                Completion::Normal(Value::Undefined)
            })),
            Some(Box::new(move |agent, reason| {
                on_rejected.complete(agent, Err(reason));
                Completion::Normal(Value::Undefined)
            })),
        );
    }

    /// Yields `value`, `AsyncGeneratorYield`, and goes on at once when a request waits.
    fn yield_value(self: &Rc<Self>, agent: &Agent, value: Value) {
        self.complete_step(agent, Ok(value), false);
        let waiting = self
            .queue
            .borrow()
            .front()
            .map(|request| request.completion.clone());
        match waiting {
            Some(completion) => self.resume_after_yield(agent, completion),
            None => self.state.set(GeneratorState::SuspendedYield),
        }
    }

    /// Ends the body at its `yield` with `completion`, `AsyncGeneratorUnwrapYieldResumption`.
    ///
    /// A `next` lets the body run off its end, a `throw` throws at the
    /// `yield`, and a `return` awaits its value and returns it.
    fn resume_after_yield(self: &Rc<Self>, agent: &Agent, completion: Resumption) {
        let value = match completion {
            Resumption::Next => return self.complete(agent, Ok(Value::Undefined)),
            Resumption::Throw(error) => return self.complete(agent, Err(error)),
            Resumption::Return(value) => value,
        };
        let on_fulfilled = Rc::clone(self);
        let on_rejected = Rc::clone(self);
        let _ = Promise::resolve(agent, value).then(
            agent,
            Some(Box::new(move |agent, value| {
                on_fulfilled.complete(agent, Ok(value));
                Completion::Normal(Value::Undefined)
            })),
            Some(Box::new(move |agent, reason| {
                on_rejected.complete(agent, Err(reason));
                Completion::Normal(Value::Undefined)
            })),
        );
    }

    /// Completes the body with `result` and settles the requests that wait.
    fn complete(self: &Rc<Self>, agent: &Agent, result: Result<Value, Value>) {
        self.state.set(GeneratorState::Completed);
        self.complete_step(agent, result, true);
        self.drain_queue(agent);
    }

    /// Settles the first request with `result`, `AsyncGeneratorCompleteStep`.
    fn complete_step(&self, agent: &Agent, result: Result<Value, Value>, done: bool) {
        let request = self.queue.borrow_mut().pop_front();
        let Some(request) = request else {
            return;
        };
        match result {
            Ok(value) => request.capability.resolve(agent, iter_result(value, done)),
            Err(reason) => request.capability.reject(agent, reason),
        }
    }

    /// Settles the requests of a completed generator, `AsyncGeneratorDrainQueue`.
    fn drain_queue(self: &Rc<Self>, agent: &Agent) {
        loop {
            let first = self
                .queue
                .borrow()
                .front()
                .map(|request| request.completion.clone());
            match first {
                None => return,
                Some(Resumption::Return(_)) => {
                    self.state.set(GeneratorState::AwaitingReturn);
                    self.await_return(agent);
                    return;
                }
                Some(Resumption::Throw(error)) => self.complete_step(agent, Err(error), true),
                Some(Resumption::Next) => {
                    self.complete_step(agent, Ok(Value::Undefined), true);
                }
            }
        }
    }

    /// Awaits the value of the first request, a `return`, `AsyncGeneratorAwaitReturn`.
    fn await_return(self: &Rc<Self>, agent: &Agent) {
        let first = self
            .queue
            .borrow()
            .front()
            .map(|request| request.completion.clone());
        let Some(Resumption::Return(value)) = first else {
            return;
        };
        let on_fulfilled = Rc::clone(self);
        let on_rejected = Rc::clone(self);
        let _ = Promise::resolve(agent, value).then(
            agent,
            Some(Box::new(move |agent, value| {
                on_fulfilled.complete(agent, Ok(value));
                Completion::Normal(Value::Undefined)
            })),
            Some(Box::new(move |agent, reason| {
                on_rejected.complete(agent, Err(reason));
                Completion::Normal(Value::Undefined)
            })),
        );
    }
}
