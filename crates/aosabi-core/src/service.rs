// SPDX-License-Identifier: MIT
/*
 * aosabi-core service: the service an application builds and a request runs
 * through.
 *
 * No analogue in the TypeScript runtime, where the application is an instance
 * and a request enters through HTTP.
 */

//! The service.
//!
//! A [`Service`] is what `handler` builds below HTTP: the roots of an
//! application run through the resolver wrapping, the seven bound scalars and
//! the schema both enter. [`Service::execute`] runs one request through the one
//! pipeline of the executor crate, inside a fresh [`Context`], the way a TypeScript
//! service runs every request inside `asyncContext.run`. [`Service::subscribe`] runs a
//! subscription the same way, inside a fresh context that carries the
//! [`CarrierPolicy`] of the carrier that called, and answers its single result
//! or its stream.
//!
//! A service is `Send` and `Sync` and is shared between requests. Everything
//! of one request, its agent, its context and its values, lives on the
//! thread that runs it and never enters the service.

use core::{
    any::Any,
    cell::RefCell,
    future::Future as _,
    pin::Pin,
    task::{
        self,
        Poll,
        Waker, //
    }, //
};
use std::{
    rc::Rc,
    sync::{
        Arc,
        Mutex,
        PoisonError, //
    }, //
};

use aosabi_execute::{
    execute::LeafSerializer,
    kernel_entry::{
        Executed,
        GeneralExecutor,
        Request,
        RootFactory,
        RunFailure,
        Subscribed,
        SubscriptionStream,
        run_request,
        run_subscription, //
    },
    normalized_executor::operation_type,
    promise::{
        Agent,
        AgentOptions, //
    },
    result::ExecutionResult,
    value::{
        ObjectLike,
        Value,
        to_js_string, //
    }, //
};
use aosabi_json::{
    JsString,
    JsValue,
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_kernel::execute_closed::{
    Execute,
    Operation,
    ResultStream, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::OperationTypeNode,
    parser::{
        ParseOptions,
        parse, //
    }, //
};
use aosabi_type::{
    ObjMap,
    Schema, //
};

use crate::{
    carrier::{
        CarrierPolicy,
        Revision,
        set_carrier_policy,
        take_revision, //
    },
    context::{
        Context,
        ambient_hooks,
        run_with_context,
        service_context, //
    },
    create_schema::{
        CreateSchemaError,
        ResolverMap,
        create_schema, //
    },
    in_context::apply_in_context,
    resolvers::{
        Resolvers,
        resolvers_to_graphql_resolvers, //
    },
    scalars::leaf_serializers, //
};

/// The one error a persisted operation answers until the document table exists.
const PERSISTED_DOCUMENTS_PENDING: &str = "Persisted documents land at M12.";

/// The unique fields cache, by the name of a possible type.
type UniqueFields = IndexMap<String, Vec<String>, KeyHasherBuilder>;

/// What lives as long as the service.
///
/// A type lives as long as its schema, so the port keys the cache by the type
/// name and keeps it with the service. The leaf serializers are the side table
/// of the bound scalars, which the executor asks before the hook of the type
/// layer.
pub struct CoreState {
    /// The unique fields cache.
    unique_fields: Mutex<UniqueFields>,
    /// The serializers of the bound scalars over the value of the executor.
    leaf_serializers: Arc<ObjMap<LeafSerializer>>,
}

impl CoreState {
    /// Creates the state of a service whose bound scalars serialize through
    /// `leaf_serializers`.
    #[must_use]
    pub fn new(leaf_serializers: ObjMap<LeafSerializer>) -> Arc<Self> {
        Arc::new(Self {
            unique_fields: Mutex::new(UniqueFields::default()),
            leaf_serializers: Arc::new(leaf_serializers),
        })
    }

    /// Returns the table the executor takes as `ExecutionArgs::leaf_serializers`.
    #[must_use]
    pub const fn leaf_serializers(&self) -> &Arc<ObjMap<LeafSerializer>> {
        &self.leaf_serializers
    }

    /// Returns `uniqueFieldsCache.get(type)?.[0]`.
    ///
    /// A cached empty list answers `None`, as a missing entry does.
    pub(crate) fn unique_field(&self, type_name: &str) -> Option<String> {
        let cache = self.lock_unique_fields();
        cache.get(type_name)?.first().cloned()
    }

    /// Performs `uniqueFieldsCache.set(type, fields)`.
    pub(crate) fn set_unique_fields(&self, type_name: &str, fields: Vec<String>) {
        self.lock_unique_fields()
            .insert(type_name.to_owned(), fields);
    }

    /// Locks the cache. A poisoned lock still holds a whole map, because no
    /// writer panics between two steps of one write, so the map is used as
    /// it stands.
    fn lock_unique_fields(&self) -> std::sync::MutexGuard<'_, UniqueFields> {
        self.unique_fields
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// A built service: a schema, its executor and the state the wrapping keeps.
pub struct Service {
    /// The SDL text the schema was built from.
    type_defs: &'static str,
    /// The executor over the built schema.
    executor: GeneralExecutor,
    /// The state that lives as long as the service.
    core: Arc<CoreState>,
}

impl Service {
    /// Starts a service over the SDL text `type_defs`.
    #[must_use]
    pub fn builder(type_defs: &'static str) -> ServiceBuilder {
        ServiceBuilder {
            type_defs,
            roots: Resolvers::default(),
            resolvers: ResolverMap::default(),
        }
    }

    /// Returns the schema of the service.
    #[must_use]
    pub const fn schema(&self) -> &Arc<Schema> {
        &self.executor.schema
    }

    /// Returns the SDL text the service was built from.
    #[must_use]
    pub const fn type_defs(&self) -> &'static str {
        self.type_defs
    }

    /// Returns the state that lives as long as the service.
    #[must_use]
    pub const fn core(&self) -> &Arc<CoreState> {
        &self.core
    }

    /// Runs one request to its end.
    ///
    /// The request runs through `run_request` of the executor crate on a
    /// fresh agent, inside a fresh [`Context`] that is also
    /// the context value of the executor. The agent carries the ambient hooks of
    /// [`crate::context`], so the context and the resolve info follow an
    /// `async fn` across its awaits. The `@inContext` directive of a query or
    /// a mutation is read into the context before the executor runs, which is
    /// what the plugin `useInContext` does in `onExecute`. A subscription
    /// operation answers the one error of `run_request`, because its answer is
    /// a stream, which [`Service::subscribe`] returns.
    #[must_use]
    pub fn execute(&self, request: &Request<'_>, options: AgentOptions) -> Executed {
        let context = service_context();
        if !is_subscription(request) {
            apply_in_context(&context, request);
        }
        let context_value: Rc<dyn Any> = Rc::new(context.clone());
        run_with_context(context, || {
            run_request(
                &self.executor,
                request,
                context_value,
                options,
                &setup_agent,
            )
        })
    }

    /// Runs one request as [`Service::execute`] does, then runs its agent until it is idle.
    ///
    /// A timer a resolver sets fires after the answer, as the event loop of node
    /// fires it, so the publish of a detached timer reaches the streams of the
    /// request stream before its next line is read. On the virtual clock the
    /// timers fire at once in the order of their deadlines. The drain stops
    /// where only a foreign waker could move the agent on, because nothing on
    /// the calling thread wakes it. The answer and its trace are taken before
    /// the drain, so the drain changes neither.
    pub(crate) fn execute_drained(&self, request: &Request<'_>, options: AgentOptions) -> Executed {
        let context = service_context();
        if !is_subscription(request) {
            apply_in_context(&context, request);
        }
        let context_value: Rc<dyn Any> = Rc::new(context.clone());
        let kept = RefCell::new(None);
        let keep = |agent: &Agent| {
            setup_agent(agent);
            *kept.borrow_mut() = Some(agent.clone());
        };
        run_with_context(context, || {
            let executed = run_request(&self.executor, request, context_value, options, &keep);
            if let Some(agent) = kept.borrow_mut().take() {
                let mut idle = agent.drain_until_idle();
                let _ = Pin::new(&mut idle).poll(&mut task::Context::from_waker(Waker::noop()));
            }
            executed
        })
    }

    /// Subscribes one request on a fresh agent inside a fresh context that carries `policy`.
    ///
    /// The request runs through `run_subscription` of the executor crate,
    /// which answers the single result of an early error or a failed source
    /// stream, or the stream of results, whose agent runs on every poll of
    /// it. The context is the one of a synthetic `POST /graphql`, as for
    /// [`Service::execute`], and the subscription binding reads the policy off
    /// it before the subscribe resolver runs. The `@inContext` directive is
    /// not applied, because the reference applies it in `onExecute` alone,
    /// which a subscription never reaches. An operation that is no
    /// subscription runs to its one result without the directive.
    ///
    /// # Errors
    ///
    /// Returns [`RunFailure`] as [`Service::execute`] does: the throw of the
    /// entry or the rejection of its promise, and a stall before the answer
    /// settled.
    pub fn subscribe(
        &self,
        request: &Request<'_>,
        options: AgentOptions,
        policy: CarrierPolicy,
    ) -> Result<Subscribed, RunFailure> {
        let context = service_context();
        set_carrier_policy(&context, policy);
        self.subscribe_in(&context, request, options)
    }

    /// Subscribes one request inside `context`, the body of [`Service::subscribe`].
    fn subscribe_in(
        &self,
        context: &Context,
        request: &Request<'_>,
        options: AgentOptions,
    ) -> Result<Subscribed, RunFailure> {
        let context_value: Rc<dyn Any> = Rc::new(context.clone());
        run_with_context(context.clone(), || {
            run_subscription(
                &self.executor,
                request,
                context_value,
                options,
                &setup_agent,
            )
        })
    }
}

/// Sets the ambient hooks of [`crate::context`] on the agent of a request.
fn setup_agent(agent: &Agent) {
    let (capture, restore) = ambient_hooks();
    agent.set_ambient_hooks(capture, restore);
}

/// Returns whether the operation `request` names is a subscription.
///
/// A subscription operation is written with the keyword `subscription`, so a
/// text without it is answered with no parse. A text with it is parsed once
/// more, and a document that does not parse is no subscription, because it
/// reaches no resolver.
fn is_subscription(request: &Request<'_>) -> bool {
    if !request.document.contains("subscription") {
        return false;
    }
    let options = ParseOptions {
        no_location: true,
        ..ParseOptions::default()
    };
    parse(request.document, options).is_ok_and(|document| {
        operation_type(&document, request.operation_name) == Some(OperationTypeNode::Subscription)
    })
}

// A service is shared between the threads that serve requests, so the claim
// is checked where the type is defined and not where a server first needs it.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Service>();
};

/// The builder of a [`Service`].
pub struct ServiceBuilder {
    /// The SDL text.
    type_defs: &'static str,
    /// The roots of the application.
    roots: Resolvers,
    /// The build side resolver map.
    resolvers: ResolverMap,
}

impl ServiceBuilder {
    /// Sets the `Query` root, of which each thread takes one clone at the first call of a field of
    /// the root and reads its members from that clone for every later call.
    #[must_use]
    pub fn query<T: ObjectLike + Clone + Send + Sync + 'static>(mut self, root: T) -> Self {
        self.roots.query = Some(root_factory(root));
        self
    }

    /// Sets the `Mutation` root, of which each thread takes one clone at the first call of a field
    /// of the root and reads its members from that clone for every later call.
    #[must_use]
    pub fn mutation<T: ObjectLike + Clone + Send + Sync + 'static>(mut self, root: T) -> Self {
        self.roots.mutation = Some(root_factory(root));
        self
    }

    /// Sets all three roots as factories, which is how a caller hands in a
    /// root that is no Rust type of its own.
    #[must_use]
    pub fn roots(mut self, resolvers: Resolvers) -> Self {
        self.roots = resolvers;
        self
    }

    /// Sets the build side resolver map, which wins over the roots on a conflict.
    #[must_use]
    pub fn resolvers(mut self, resolvers: ResolverMap) -> Self {
        self.resolvers = resolvers;
        self
    }

    /// Builds the service.
    ///
    /// # Errors
    ///
    /// Returns [`CreateSchemaError::Resolvers`] with the message
    /// `At least one 'Query' resolver must be provided.` and its example text
    /// for roots without a `Query` member, and what [`create_schema`] returns.
    pub fn build(self) -> Result<Service, CreateSchemaError> {
        let core = CoreState::new(leaf_serializers(self.type_defs));
        let graphql = resolvers_to_graphql_resolvers(&core, &self.roots)
            .map_err(|error| CreateSchemaError::Resolvers(thrown_message(&error)))?;
        let schema = create_schema(self.type_defs, graphql, self.resolvers)?;
        let executor = GeneralExecutor {
            schema,
            // Yoga hands the executor no root value, and a root resolver of
            // a service never reads its source.
            root_value: Arc::new(|| Value::Undefined),
            leaf_serializers: Some(Arc::clone(core.leaf_serializers())),
        };
        Ok(Service {
            type_defs: self.type_defs,
            executor,
            core,
        })
    }
}

/// Returns the factory that answers a clone of `root` per call.
fn root_factory<T: ObjectLike + Clone + Send + Sync + 'static>(root: T) -> RootFactory {
    Arc::new(move || {
        let object: Rc<dyn ObjectLike> = Rc::new(root.clone());
        Value::Object(object)
    })
}

/// Returns the text a thrown value answers with as the one error of a result.
///
/// An error answers its `message` and any other value `String(value)`,
/// which is what the general executor of the executor crate answers.
fn thrown_message(thrown: &Value) -> JsString {
    match thrown {
        Value::Error(error) => error.message(),
        other => to_js_string(other),
    }
}

/// The result stream of a [`Service`], the one result of a request or the results of a subscription.
pub enum ServiceStream {
    /// The one result of a query, a mutation, or a subscription that answers no stream.
    One(Option<ExecutionResult>),
    /// The results of a subscription.
    Events {
        /// The stream of the executor crate, whose agent runs on every poll.
        stream: SubscriptionStream,
        /// The context of the request, where the pump of a hub fed stream notes its revisions.
        context: Context,
        /// The revision of the result yielded last, when a publish produced it.
        last: Option<Revision>,
    },
}

impl ServiceStream {
    /// Returns the revision `take_revision` answered for the result yielded last.
    ///
    /// A result a publish of the pub sub produced carries the revision of
    /// that publish, and any other result, the one of a query among them,
    /// carries none.
    #[must_use]
    pub fn last_revision(&self) -> Option<Revision> {
        match self {
            Self::One(_) => None,
            Self::Events { last, .. } => *last,
        }
    }
}

impl ResultStream for ServiceStream {
    type Item = ExecutionResult;

    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Self::One(result) => Poll::Ready(result.take()),
            Self::Events {
                stream,
                context,
                last,
            } => {
                let poll = Pin::new(stream).poll_next(cx);
                if let Poll::Ready(Some(_)) = &poll {
                    *last = take_revision(context);
                }
                poll
            }
        }
    }
}

impl Execute for Service {
    type Variables = JsValue;
    /// The policy of the carrier that called, which a carrier crate builds from its card.
    type Context = CarrierPolicy;
    type Output = ExecutionResult;
    type Stream = ServiceStream;

    /// Runs `operation` with validation on and the virtual timers.
    ///
    /// The context of the request is made inside and carries `policy`. A
    /// query and a mutation run through [`Service::execute`] and a
    /// subscription through the body of [`Service::subscribe`]. What the entry
    /// throws and a stalled run both answer as one error with no data, and a
    /// persisted operation answers the one error that names the milestone of
    /// the document table.
    fn execute(
        &self,
        operation: Operation<'_>,
        variables: &Self::Variables,
        policy: &Self::Context,
    ) -> Self::Stream {
        let Operation::Document {
            text,
            operation_name,
        } = operation
        else {
            return ServiceStream::One(Some(one_error(PERSISTED_DOCUMENTS_PENDING)));
        };
        let request = Request {
            document: text,
            variables: (!variables.is_undefined()).then(|| variables.clone()),
            operation_name,
            validate: true,
        };
        if !is_subscription(&request) {
            let executed = Service::execute(self, &request, AgentOptions::default());
            return ServiceStream::One(Some(result_of(executed.outcome)));
        }
        let context = service_context();
        set_carrier_policy(&context, *policy);
        match self.subscribe_in(&context, &request, AgentOptions::default()) {
            Ok(Subscribed::Stream(stream)) => ServiceStream::Events {
                stream,
                context,
                last: None,
            },
            Ok(Subscribed::One(result)) => ServiceStream::One(Some(result)),
            Err(failure) => ServiceStream::One(Some(result_of(Err(failure)))),
        }
    }
}

/// Returns the one result of a request, with the error of its failure when it has none.
fn result_of(outcome: Result<ExecutionResult, RunFailure>) -> ExecutionResult {
    match outcome {
        Ok(result) => result,
        Err(RunFailure::Thrown(thrown)) => one_error(thrown_message(&thrown)),
        Err(RunFailure::Stalled) => one_error(crate::request_stream::STALLED),
    }
}

/// Returns the result that carries one error with `message` and no data.
fn one_error(message: impl Into<JsString>) -> ExecutionResult {
    ExecutionResult::from_errors(vec![GraphQLError::new(
        message,
        GraphQLErrorOptions::default(),
    )])
}
