// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the GraphQL handler of an app, over Yoga.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/pylon-handler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `handler`.
//!
//! The handler builds the schema from the type definitions, the root resolvers
//! of the app through `resolversToGraphQLResolvers`, the build side resolver
//! map merged one level deep with the build side winning, and the bound
//! scalars. It builds Yoga with the options in the order
//! `graphqlEndpoint: '/graphql'`, `maskedErrors: false` when `NODE_ENV` is
//! `development`, the whole configuration spread, `landingPage: false`,
//! `graphiql` as a function of the playground, named Aosabi's by D52, only
//! when `config.graphiql === true`, the named divergence D48 of
//! `docs/byte-equality.md` section 3, and the schema. The spread carries
//! `config.plugins`, which the first pass of the boot wrote back, into the
//! plugins of Yoga, which places them after its result processors, so the
//! hooks of the error logger and of the in context plugin run because of it.
//! The key `logger` reaches Yoga through the spread and Yoga reads `logging`,
//! so it is ignored there.
//!
//! The hot swap seam of development (source lines 353-361) has no
//! reading, because a Rust server rebuilds its binary for a new schema.
//!
//! Every operation runs on an agent of its own, and a timer a resolver sets
//! with `setTimeout` fires on the event loop of node after the answer is
//! written. A host that hands in a [`Spawner`] gets that order: the middleware
//! collects the agent of every operation Yoga runs for the request, and after
//! the answer each of them is drained in a task of its own until it is idle. A
//! stream runs on its agent through the driver its body hands out, and the
//! drains of the request then run after that driver ends, so no two tasks
//! drive one agent. Without a spawner, as in an in process run, the agents end
//! with their answer.

use core::{
    cell::RefCell,
    fmt,
    future::Future,
    pin::Pin,
    task::{
        Context as TaskContext,
        Poll, //
    }, //
};
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    Agent,
    execute::TypeResolverFn, //
    promise::{
        AgentOptions,
        TimerMode,
        agent::HostClock, //
    },
};
use aosabi_http::{
    Body,
    BodyStream,
    Context,
    LocalBoxFuture,
    MiddlewareFn,
    Response,
    Stream,
    fetch::body::Chunk, //
};
use graphql_over_http::{
    AgentSetup,
    CacheBudget,
    Graphiql,
    GraphiqlOptions,
    HttpOptions,
    HttpPlugin,
    HttpServer,
    MaskedErrors,
    ServerContext,
    answer, //
};

use crate::{
    app::{
        application::{
            App,
            Next, //
        },
        env_alias,
    },
    context::ambient_hooks,
    create_schema::{
        CreateSchemaError,
        ResolverMap,
        create_schema, //
    },
    glue::{
        Thrown,
        resolve_type::{
            ResolveTypeRule,
            type_resolver, //
        },
    },
    resolvers::{
        Resolvers,
        resolvers_to_graphql_resolvers, //
    },
    scalars::leaf_serializers,
    service::CoreState, //
};

/// The endpoint the handler sets first, `graphqlEndpoint: '/graphql'`.
const GRAPHQL_ENDPOINT: &str = "/graphql";

/// The title of the playground with Aosabi's name, D52.
const PLAYGROUND_TITLE: &str = "Aosabi Playground";

/// The query of the playground, `defaultQuery` with Aosabi's name, D52.
const PLAYGROUND_QUERY: &str = "# Welcome to the Aosabi Playground!";

/// The options of [`handler`], the handler options with the type
/// definitions and the build side type resolvers the build wrote.
pub struct HandlerOptions {
    /// The schema text, `typeDefs`.
    pub type_defs: &'static str,
    /// The type resolver of every union and interface, the build side
    /// `resolvers`, which the build writes from the model of `resolvers.js`.
    pub resolve_types: &'static [ResolveTypeRule],
    /// The run time roots, `graphql`.
    pub graphql: Resolvers,
    /// The clock of the host, `None` for the virtual wheel of an in process run.
    pub host_clock: Option<Rc<dyn HostClock>>,
    /// The spawner of the host, `None` for an in process run, whose agents
    /// end with their answer.
    pub spawner: Option<Spawner>,
    /// Aosabi only: the budget of the request cache of the card the build
    /// serves, [`crate::cache_budget::TARGET`] on a served build, and Yoga's
    /// 1,024 entries with no bound in bytes for `None`.
    pub parser_and_validation_cache_budget: Option<CacheBudget>,
}

/// The spawner of a host, which runs a future as a task of its own on the thread of the request.
///
/// The serve layer hands its `spawn_local` in beside its clock.
pub type Spawner = Rc<dyn Fn(LocalBoxFuture<'static, ()>)>;

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The agents of the request whose answer the middleware polls, which
    /// the setup of every agent of an operation joins.
    static REQUEST_AGENTS: RefCell<Option<Rc<RefCell<Vec<Agent>>>>> = const { RefCell::new(None) };
}

/// Builds the executable schema, `buildSchema(typeDefs, graphql, resolvers)`.
///
/// # Errors
///
/// Returns what the root conversion and the schema build throw.
pub fn build_schema(
    options: &HandlerOptions,
) -> Result<(Arc<aosabi_type::Schema>, Arc<CoreState>), CreateSchemaError> {
    let core = CoreState::new(leaf_serializers(options.type_defs));
    let graphql = resolvers_to_graphql_resolvers(&core, &options.graphql).map_err(|thrown| {
        let message = match &thrown {
            aosabi_execute::value::Value::Error(error) => error.message(),
            other => aosabi_execute::value::to_js_string(other),
        };
        CreateSchemaError::Resolvers(message)
    })?;
    let mut build_side = ResolverMap::default();
    for rule in options.resolve_types {
        let resolver: TypeResolverFn = type_resolver(rule);
        build_side
            .resolve_types
            .insert(String::from(rule.type_name), resolver);
    }
    let schema = create_schema(options.type_defs, graphql, build_side)?;
    Ok((schema, core))
}

/// Returns the options of Yoga for the schema of `target`, `buildYoga(schema)`.
///
/// Aosabi only, the options take `cache_budget`, the budget of the request
/// cache of the card, which changes what the cache keeps and no answer, and
/// the validation and the bound on nesting of the build settings of
/// [`crate::single_pass`], which change how a document is validated and no
/// answer.
#[must_use]
pub fn build_http_options(
    schema: Arc<aosabi_type::Schema>,
    core: &Arc<CoreState>,
    target: &App,
    host_clock: Option<Rc<dyn HostClock>>,
    cache_budget: Option<CacheBudget>,
) -> HttpOptions {
    let config = target.config().cloned().unwrap_or_default();
    let mut options = HttpOptions {
        graphql_endpoint: Some(String::from(GRAPHQL_ENDPOINT)),
        ..HttpOptions::default()
    };
    // Dev: surface the real error on the GraphQL response and the server log
    // instead of Yoga's default masking.
    if env_alias::var("NODE_ENV").as_deref() == Some("development") {
        options.masked_errors = MaskedErrors::Off;
    }
    // The configuration spread, `...config`.
    if let Some(masked_errors) = config.masked_errors.clone() {
        options.masked_errors = masked_errors;
    }
    if let Some(endpoint) = config.graphql_endpoint.clone() {
        options.graphql_endpoint = Some(endpoint);
    }
    if let Some(endpoint) = config.health_check_endpoint.clone() {
        options.health_check_endpoint = Some(endpoint);
    }
    if let Some(multipart) = config.multipart {
        options.multipart = Some(multipart);
    }
    if let Some(plugins) = &config.plugins {
        options.plugins = plugins
            .slots()
            .iter()
            .map(|slot| {
                let plugin: Rc<dyn HttpPlugin> = Rc::clone(&slot.plugin) as Rc<dyn HttpPlugin>;
                plugin
            })
            .collect();
    }
    // The page of `GraphiQL` only on `graphiql: true`, the named divergence D48.
    options.graphiql = if config.graphiql == Some(true) {
        Graphiql::Factory(Rc::new(|_request, _context| {
            Some(GraphiqlOptions {
                should_persist_headers: Some(true),
                title: Some(String::from(PLAYGROUND_TITLE)),
                default_query: Some(String::from(PLAYGROUND_QUERY)),
            })
        }))
    } else {
        Graphiql::Off
    };
    options.schema = Some(schema);
    options.agent = agent_setup(core, host_clock);
    options.parser_and_validation_cache_budget = cache_budget;
    options.single_pass = Some(crate::single_pass::SINGLE_PASS);
    options.single_pass_max_depth = Some(crate::single_pass::TARGET);
    options
}

/// Returns the agent of an operation: the ambient hooks of the context, the
/// clock of the host when there is one, the serializers of the bound scalars,
/// and the join of the agents of the request the middleware collects.
fn agent_setup(core: &Arc<CoreState>, host_clock: Option<Rc<dyn HostClock>>) -> AgentSetup {
    let timers = if host_clock.is_some() {
        TimerMode::Host
    } else {
        TimerMode::Virtual
    };
    AgentSetup {
        options: AgentOptions {
            timers,
            trace: false,
        },
        setup: Some(Rc::new(move |agent| {
            let (capture, restore) = ambient_hooks();
            agent.set_ambient_hooks(capture, restore);
            if let Some(clock) = &host_clock {
                agent.set_host_clock(Rc::clone(clock));
            }
            join_request(agent);
        })),
        field_resolver: None,
        type_resolver: None,
        subscribe_field_resolver: None,
        leaf_serializers: Some(Arc::clone(core.leaf_serializers())),
    }
}

/// Returns the GraphQL middleware of `target`, `handler(options, target)`.
///
/// # Errors
///
/// Returns what the schema build throws.
pub fn handler(
    options: HandlerOptions,
    target: &App,
) -> Result<impl MiddlewareFn<Thrown>, CreateSchemaError> {
    handler_with_server(options, target).map(|(middleware, _)| middleware)
}

/// Returns the GraphQL middleware of `target` and the mount of the server it
/// answers with, which the boot keeps for the road of `GraphTP`.
///
/// # Errors
///
/// Returns what the schema build throws.
pub(crate) fn handler_with_server(
    options: HandlerOptions,
    target: &App,
) -> Result<(impl MiddlewareFn<Thrown>, Option<Mounted>), CreateSchemaError> {
    let (schema, core) = build_schema(&options)?;
    let spawner = options.spawner;
    let server = HttpServer::new(build_http_options(
        schema,
        &core,
        target,
        options.host_clock,
        options.parser_and_validation_cache_budget,
    ));
    let mounted = Some(Mounted {
        server: Rc::clone(&server),
        spawner: spawner.clone(),
    });
    let middleware = move |c: Context, next: Next| {
        let server = Rc::clone(&server);
        let spawner = spawner.clone();
        async move {
            let server_context = ServerContext {
                env: c.env(),
                execution_ctx: c.execution_ctx(),
            };
            let fetch = answer(&server, c.req().raw(), server_context.into_context());
            let Some(spawner) = spawner else {
                let response = fetch.await.map_err(Thrown)?;
                if response.status() == 404 {
                    return next.run(c).await;
                }
                return c.new_response(response.body(), &response);
            };
            let detached = Detached {
                spawner,
                agents: Rc::default(),
            };
            let response = Collecting {
                agents: Rc::clone(&detached.agents),
                future: Box::pin(fetch),
            }
            .await
            .map_err(Thrown)?;
            if response.status() == 404 {
                return next.run(c).await;
            }
            let body = detached.after(response.body());
            c.new_response(body, &response)
        }
    };
    Ok((middleware, mounted))
}

/// The GraphQL server the middleware answers with and the spawner of its
/// host, which the road of `GraphTP` answers with.
#[derive(Clone)]
pub(crate) struct Mounted {
    /// The server the middleware answers with.
    server: Rc<HttpServer>,
    /// The spawner of the host, `None` for an in process run.
    spawner: Option<Spawner>,
}

// The road of GraphTP is the one caller of both methods, and a build without
// the feature homa holds no road.
#[cfg_attr(not(feature = "homa"), expect(dead_code))]
impl Mounted {
    /// Returns the server the middleware answers with.
    pub(crate) fn server(&self) -> &Rc<HttpServer> {
        &self.server
    }

    /// Runs `answer` with the agents of its operations collected and, with a
    /// spawner, drained once it is answered, as the middleware runs the answer
    /// of the server, and returns its response with the body to hand out in
    /// place of the response's own, whose driver the drains follow for a
    /// stream.
    ///
    /// # Errors
    ///
    /// Returns the error of `answer`, after which the drains run as well.
    pub(crate) async fn collect<F, E>(&self, answer: F) -> Result<(Response, Body), E>
    where
        F: Future<Output = Result<Response, E>>,
    {
        let Some(spawner) = self.spawner.clone() else {
            let response = answer.await?;
            let body = response.body();
            return Ok((response, body));
        };
        let detached = Detached {
            spawner,
            agents: Rc::default(),
        };
        let response = Collecting {
            agents: Rc::clone(&detached.agents),
            future: Box::pin(answer),
        }
        .await?;
        let body = detached.after(response.body());
        Ok((response, body))
    }
}

impl fmt::Debug for Mounted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mounted")
            .field("spawner", &self.spawner.is_some())
            .finish_non_exhaustive()
    }
}

/// Adds `agent` to the agents of the request whose answer the middleware polls.
///
/// Outside such a poll, in an in process run among others, nothing collects
/// the agent and it ends with its answer.
fn join_request(agent: &Agent) {
    let _ = REQUEST_AGENTS.try_with(|current| {
        if let Some(agents) = current.borrow().as_ref() {
            agents.borrow_mut().push(agent.clone());
        }
    });
}

/// The answer of Yoga, with the agents of its operations collected around every poll.
struct Collecting<F> {
    /// The agents of the request.
    agents: Rc<RefCell<Vec<Agent>>>,
    /// The answer of Yoga.
    future: Pin<Box<F>>,
}

impl<F: Future> Future for Collecting<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<F::Output> {
        let this = self.get_mut();
        let _scope = CollectScope::enter(Rc::clone(&this.agents));
        this.future.as_mut().poll(cx)
    }
}

/// The agents in place for one poll, which puts the ones before back when it goes out of scope.
///
/// The swap back sits in a destructor, so a panic that unwinds through a
/// poll leaves the cell of the thread as it found it.
struct CollectScope(Option<Rc<RefCell<Vec<Agent>>>>);

impl CollectScope {
    /// Puts `agents` in place and keeps the ones it replaced.
    fn enter(agents: Rc<RefCell<Vec<Agent>>>) -> Self {
        let previous = REQUEST_AGENTS
            .try_with(|current| current.replace(Some(agents)))
            .unwrap_or_default();
        Self(previous)
    }
}

impl Drop for CollectScope {
    fn drop(&mut self) {
        let previous = self.0.take();
        let _ = REQUEST_AGENTS.try_with(|current| current.replace(previous));
    }
}

/// The agents of a request and the spawner that drains them once the request is answered.
///
/// Each agent left when the value is dropped is drained until it is idle in
/// a task of its own, so a timer of one operation never waits for a timer of
/// another. The drop happens when the middleware returns, also with an
/// error, and when the request is dropped before its answer, in which case
/// the drain runs on what the operation left, as node runs a request whose
/// client went away.
struct Detached {
    /// The spawner of the host.
    spawner: Spawner,
    /// The agents of the operations of the request.
    agents: Rc<RefCell<Vec<Agent>>>,
}

impl Detached {
    /// Returns `body`, whose driver is followed by the drains of the request when it has one.
    ///
    /// A stream that hands out a driver runs its agent in that driver, so
    /// the drains wait for the driver to end. Any other body leaves the
    /// drains to the drop of this value.
    fn after(&self, body: Body) -> Body {
        let Body::Stream(stream) = body else {
            return body;
        };
        let Some(driver) = stream.take_driver() else {
            return Body::Stream(stream);
        };
        let detached = Detached {
            spawner: Rc::clone(&self.spawner),
            agents: Rc::new(RefCell::new(self.agents.take())),
        };
        let driver: LocalBoxFuture<'static, ()> = Box::pin(async move {
            driver.await;
            drop(detached);
        });
        Body::Stream(BodyStream::new(DrivenStream {
            stream,
            driver: Some(driver),
        }))
    }
}

impl Drop for Detached {
    fn drop(&mut self) {
        for agent in self.agents.take() {
            (self.spawner)(Box::pin(agent.drain_until_idle()));
        }
    }
}

/// A stream body whose driver is the one of the stream it reads, followed by the drains of the request.
struct DrivenStream {
    /// The stream whose chunks the body yields.
    stream: BodyStream,
    /// The driver, handed out once.
    driver: Option<LocalBoxFuture<'static, ()>>,
}

impl Stream for DrivenStream {
    type Item = Chunk;

    fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Chunk>> {
        self.stream.poll_chunk(cx)
    }

    fn take_driver(self: Pin<&mut Self>) -> Option<LocalBoxFuture<'static, ()>> {
        self.get_mut().driver.take()
    }
}
