// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the application, compose and the base pipeline.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/app/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! `App`.
//!
//! [`App::new`] stores the run time roots of the fragment, [`App::gate`] wraps
//! each of them in an async function that awaits the gate first, and
//! [`App::compose`] records each child and merges its roots one level deep, a
//! child's member taking the place of the parent's member of the same name and
//! keeping its position, `{...into[kind], ...map}`. The fragment type is a tag
//! for the build, and [`IntoApp::into_app`] erases it, so the boot, a setup and
//! `execute_config` all see one type.
//!
//! The base pipeline is registered on `*` once, at the boot and not in the
//! constructor: the coding layer, the async context bind, the access logger under
//! `except(['/__aosabi/*'], ...)`, divergence
//! D54, the onion of the plugin middleware and the error handler.
//! [`App::realize`] then mounts each child at its base path or `/`, depth first,
//! once, so the pipeline stands before every route of the tree. A route an app
//! registers on the served root before the boot runs behind the pipeline as well,
//! since the boot moves it behind the pipeline in the order of the router,
//! divergence D75. The TypeScript option runs it before the pipeline.
//!
//! The native builder of the facade, `aosabi::App`, which the index of a plain
//! crate builds, registers the parts of its roots as values of any type, and
//! [`App::merge_parts`] merges them into the roots of the engine app it holds,
//! the parts of one root in the order of their registration. The app keeps every
//! part once and lends it to each request, so the root object of a part is the
//! object of its struct over the one value, which the glue of the crate makes,
//! and a part needs no `Default` and no `Clone`. A builder that sets a gate wraps
//! the merged parts in it through [`App::gate_roots`], which it names from its
//! call of `.gate(..)` alone, so an app that sets no gate links no wrapper. The
//! builder hands the app the configuration of its chain, the plugins and the
//! options of `aosabi.config.ts`, through [`App::carry_config`], and the serving
//! entry of such an app takes it back through [`App::take_config`] before the
//! boot, which stores the configuration it boots with as for any app.
//!
//! [`App::middleware`] is `app.use(path, middleware)` of Hono under the name a
//! Rust author writes, beside `App::r#use`, so the setup of a plugin, which
//! receives this app, registers a middleware with no raw identifier.
//!
//! The dispatch of the plugin middleware hands Hono the value of its first
//! middleware, and a middleware of the value form that returns nothing loses what
//! its continuation answered. When the chain answers nothing and nothing set the
//! answer of the context, the layer above sets the context itself as the answer.
//! The coding layer throws a `TypeError` where it reads the headers of that
//! answer, the error handler answers it, setting that answer throws a second
//! `TypeError`, and Hono's dispatch hands that one to the handler again, whose
//! 500 is the answer.
//!
//! The coding layer stands first, as `compress()`, and reads
//! the answer once the rest has run. It codes the answer only under the
//! feature `base-compress`, which `aosabi-js` turns on for the module and the
//! native library of the TypeScript option, so the base pipeline of a Rust
//! service codes no answer and an app codes the routes it chooses with
//! `compress()`, divergence D72 of `docs/byte-equality.md` section 3.
//!
//! A method that registers a route bounds its closure by `Fn` directly, so rustc
//! gives the closure its parameter types. The gate takes a closure that answers
//! `()` or `Result<()>`, and [`App::gate_async`] takes an asynchronous one,
//! because Rust cannot admit a conversion from every future beside the conversion
//! from `()`.

use core::{
    any::Any,
    cell::{
        Cell,
        RefCell, //
    },
    future::Future,
    marker::PhantomData,
    panic::Location, //
};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use std::time::{
    SystemTime,
    UNIX_EPOCH, //
};
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    kernel_entry::RootFactory,
    promise::{
        Agent,
        Promise, //
    },
    value::{
        Completion,
        Function,
        PlainObject,
        Value, //
    }, //
};
use aosabi_hooks::LocalBoxFuture;
use aosabi_http::{
    Arg,
    Context,
    Env,
    ExecutionCtx,
    MiddlewareFn,
    Request,
    Response,
    RouteError,
    RouteRecord,
    WebApp,
    compress,
    except, //
};
use aosabi_json::{
    JsValue,
    json, //
};

use crate::{
    app::{
        entry::{
            Children,
            Fragment,
            IntoApp,
            NoGraphql, //
        },
        execute_config::BootRecord,
        request_id::request_id,
    },
    context::with_context,
    glue::{
        Error,
        Thrown, //
    },
    logger::{
        access_log_enabled,
        get_logger,
        get_root_logger,
        normalize_error,
        run_with_logger, //
    },
    plugin::{
        AppConfig,
        PluginMiddleware, //
    },
    resolvers::Resolvers,
    result::Result, //
};

/// The continuation of a middleware of the core, `aosabi_http::Next<Thrown>`.
pub type Next = aosabi_http::Next<Thrown>;

/// The capability gate of an app, called before every resolver of its roots.
///
/// The gate is shared by the root objects of every request, which a service
/// may hand between threads, so it is `Send` and `Sync`.
pub type Gate = Arc<dyn Fn() -> GateCall + Send + Sync>;

/// What a call of a gate answers, at once or as a future.
pub enum GateCall {
    /// The gate answered at once, `undefined` or a throw.
    Done(Result<()>),
    /// The gate answered a promise, which the wrapper awaits.
    Pending(LocalBoxFuture<'static, Result<()>>),
}

/// What a synchronous gate may answer, `()`, `Result<()>` or the `Result`
/// of the error of the native layer.
pub trait IntoGate {
    /// Returns the answer of the gate.
    fn into_gate(self) -> GateCall;
}

impl IntoGate for () {
    fn into_gate(self) -> GateCall {
        GateCall::Done(Ok(()))
    }
}

impl IntoGate for Result<()> {
    fn into_gate(self) -> GateCall {
        GateCall::Done(self)
    }
}

/// The gate of an app of the native layer, which answers the `Result` of the
/// prelude of the facade, whose error throws as the thrown value of the engine,
/// since the facade takes `Thrown` off its root of on.
impl IntoGate for core::result::Result<(), Error> {
    fn into_gate(self) -> GateCall {
        GateCall::Done(self.map_err(Thrown::from))
    }
}

/// The fragment tag of a composed app, which only the build reads.
pub struct Composed<P, C>(PhantomData<fn() -> (P, C)>);

/// A root of the schema, which a part of the native builder registers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Root {
    /// `Query`, which `.query(part)` registers to.
    Query,
    /// `Mutation`, which `.mutation(part)` registers to.
    Mutation,
    /// `Subscription`, which `.subscription(part)` registers to.
    Subscription,
}

/// One part of a root that the native builder registered, the value the app
/// keeps once and lends to each request.
pub struct RootPart {
    /// The root the part belongs to.
    pub root: Root,
    /// The value of the part, a value of any type the builder kept.
    pub value: Rc<dyn Any>,
    /// The name of the type of the part, which a refusal names.
    pub type_name: &'static str,
    /// Where the part was registered.
    pub registered_at: &'static Location<'static>,
}

/// The glue of a plain crate that makes the root object of a part, the
/// object of the struct of its type over the value the app keeps, or `None`
/// for a value of no type the crate registers.
///
/// The builder of the source path writes it into the generated module of the
/// crate, which hands it to [`IntoApp::into_app_with`].
pub type PartObjects = fn(&Rc<dyn Any>) -> Option<Value>;

thread_local! {
    /// The root objects of the registered parts of the apps of this thread,
    /// which the factory of a root reads by its position.
    ///
    /// The factory of a root is shared between threads by its type, while a part
    /// is a value of any type, so the factory holds the position alone and every
    /// served crate answers on the one thread that booted it.
    static PART_OBJECTS: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

/// Returns the factory of the root object `object` of one part, which every
/// request of the thread reads.
///
/// A thread that kept no such object, which no served crate starts, reads
/// `undefined`.
fn part_factory(object: Value) -> RootFactory {
    let position = PART_OBJECTS.with_borrow_mut(|objects| {
        objects.push(object);
        objects.len() - 1
    });
    Arc::new(move || {
        PART_OBJECTS
            .with_borrow(|objects| objects.get(position).cloned().unwrap_or(Value::Undefined))
    })
}

/// A listener a serving plugin binds in its setup, which the boot runs over
/// the booted app after the last pass.
///
/// `aosabi-serve` implements it for HTTP(TCP) and `aosabi-carrier-unix` for
/// HTTP(Unix Socket), so every listener answers through the same
/// [`BootedApp::fetch`].
pub trait Listen: 'static {
    /// Runs the listener until it ends.
    fn run(self: Box<Self>, app: Rc<BootedApp>) -> LocalBoxFuture<'static, ()>;
}

/// Where an app of the tree calls `.gate(...)`, which the boot report lists.
#[derive(Clone, Debug)]
pub struct GateRecord {
    /// The identity of the app, `name`, when it has one.
    pub app: Option<String>,
    /// Where the gate was set.
    pub registered_at: &'static core::panic::Location<'static>,
}

/// The app after both passes of the boot, the realized web app with the
/// base pipeline and the GraphQL middleware.
pub struct BootedApp {
    /// The realized web app.
    web: WebApp<Thrown>,
    /// The configuration the boot wrote back.
    config: Option<AppConfig>,
    /// The records of the passes of the boot.
    boot: BootRecord,
    /// The gates of the tree.
    gates: Vec<GateRecord>,
}

impl BootedApp {
    /// Returns the booted app of `app`.
    #[must_use]
    pub fn new(app: App) -> (Self, Vec<Box<dyn Listen>>) {
        let gates = app.gates();
        let AppInner {
            web,
            config,
            listeners,
            boot,
            ..
        } = *app.inner;
        (
            Self {
                web,
                config,
                boot,
                gates,
            },
            listeners,
        )
    }

    /// Returns the records of the passes of the boot, which the boot report writes.
    #[must_use]
    pub fn boot_record(&self) -> &BootRecord {
        &self.boot
    }

    /// Returns the gates of the tree, the root first and each child after its parent.
    #[must_use]
    pub fn gates(&self) -> &[GateRecord] {
        &self.gates
    }

    /// Answers `request`, `app.fetch(request, env, executionCtx)`.
    ///
    /// # Errors
    ///
    /// The future answers the value the chain threw when it is no instance of
    /// `Error` or the error handler threw, which the adapter answers with 500.
    pub fn fetch(
        &self,
        request: Rc<Request>,
        env: Env,
        execution_ctx: Option<ExecutionCtx>,
    ) -> LocalBoxFuture<'static, core::result::Result<Response, Thrown>> {
        self.web.fetch(request, env, execution_ctx)
    }

    /// Returns the configuration the boot wrote back, `app.config`.
    #[must_use]
    pub fn config(&self) -> Option<&AppConfig> {
        self.config.as_ref()
    }

    /// Returns the registered routes in order, `app.routes`.
    #[must_use]
    pub fn routes(&self) -> Vec<RouteRecord> {
        self.web.routes()
    }
}

/// The state of an app that the fragment type does not name.
struct AppInner {
    /// The web app.
    web: WebApp<Thrown>,
    /// The run time roots, `graphql`.
    graphql: Resolvers,
    /// The composed children, `children`.
    children: Vec<App>,
    /// The identity of the app, `name`.
    name: Option<String>,
    /// The gate, `gate`.
    gate: Option<Gate>,
    /// Where the gate was set.
    gate_at: Option<&'static core::panic::Location<'static>>,
    /// Where `compose` mounts the routes of the app, `routePrefix`.
    route_prefix: Option<String>,
    /// The plugin middleware chain, `pluginsMiddleware`.
    plugins_middleware: Rc<RefCell<Vec<PluginMiddleware>>>,
    /// Whether the base pipeline is installed.
    base_pipeline_installed: bool,
    /// Whether the child tree is mounted.
    realized: bool,
    /// The configuration the boot stored, `config`.
    config: Option<AppConfig>,
    /// The listeners the serving plugins bound.
    listeners: Vec<Box<dyn Listen>>,
    /// The records of the passes of the boot.
    boot: BootRecord,
}

/// The application.
///
/// `G` is the fragment type the build reads, [`NoGraphql`] for an app with
/// no `graphql` and for the erased app every setup takes.
pub struct App<G = NoGraphql> {
    /// The state.
    inner: Box<AppInner>,
    /// The fragment type.
    fragment: PhantomData<fn() -> G>,
}

impl Default for App<NoGraphql> {
    fn default() -> Self {
        Self::from_inner(AppInner {
            web: WebApp::new(),
            graphql: Resolvers::default(),
            children: Vec::new(),
            name: None,
            gate: None,
            gate_at: None,
            route_prefix: None,
            plugins_middleware: Rc::new(RefCell::new(Vec::new())),
            base_pipeline_installed: false,
            realized: false,
            config: None,
            listeners: Vec::new(),
            boot: BootRecord::default(),
        })
    }
}

impl<G: Fragment> App<G> {
    /// Creates the app of the fragment `graphql`.
    ///
    /// The run time roots of the fragment merge into the empty roots of the app.
    /// The base pipeline is not installed here.
    #[must_use]
    pub fn new(graphql: G) -> Self {
        let mut app = App::default();
        merge_fragment(&mut app.inner.graphql, &graphql.resolvers());
        app.retag()
    }
}

impl<G> App<G> {
    /// Returns the app over `inner`.
    fn from_inner(inner: AppInner) -> Self {
        Self {
            inner: Box::new(inner),
            fragment: PhantomData,
        }
    }

    /// Returns the app with another fragment type.
    fn retag<H>(self) -> App<H> {
        App {
            inner: self.inner,
            fragment: PhantomData,
        }
    }

    /// Returns the app with its fragment type erased.
    pub(crate) fn erase(self) -> App {
        self.retag()
    }

    /// Sets where `compose` mounts the routes of the app, `basePath`.
    #[must_use]
    pub fn base_path(mut self, path: &str) -> Self {
        self.inner.route_prefix = Some(String::from(path));
        self
    }

    /// Leaves the base pipeline and the mount of the child tree to the host,
    /// so the boot installs neither, and installs the error handler of the
    /// base pipeline alone.
    ///
    /// The TypeScript option runs the base pipeline and the routes of an
    /// app in the real hono of its host, which hands the
    /// engine the requests of `/graphql` and of the routes of the plugins of
    /// the engine behind that pipeline, so the app of its engine holds the
    /// GraphQL handler and those routes alone, and a second pipeline would
    /// compress and log twice. An error a route of a plugin of the engine
    /// throws never reaches the error handler of the host, so the app of the
    /// engine answers it as the error handler of the base pipeline does.
    #[must_use]
    pub fn hosted_pipeline(mut self) -> Self {
        self.inner.base_pipeline_installed = true;
        self.inner.realized = true;
        self.install_error_handler();
        self
    }

    /// Sets the identity of the app, `name`.
    #[must_use]
    pub fn name(mut self, name: &str) -> Self {
        self.inner.name = Some(String::from(name));
        self
    }

    /// Sets the gate of the app, `gate`, a closure that answers `()` or
    /// `Result<()>`.
    ///
    /// Every root entry the app holds becomes
    /// `async (...args) => { await gate(); return fn(...args) }`. The reference
    /// wraps the entries of its constructor, so a gate is set before `compose`
    /// merges the children, which keep their own gates. The line of the call is
    /// recorded for the boot report, where the authoring check reads it as the
    /// construct kind `gate`.
    #[must_use]
    #[track_caller]
    pub fn gate<F, R>(mut self, gate: F) -> Self
    where
        F: Fn() -> R + Send + Sync + 'static,
        R: IntoGate,
    {
        let gate: Gate = Arc::new(move || gate().into_gate());
        self.inner.graphql = gate_resolvers(&self.inner.graphql, &gate);
        self.inner.gate = Some(gate);
        self.inner.gate_at = Some(core::panic::Location::caller());
        self
    }

    /// Sets an asynchronous gate, `gate: async () => ...`.
    ///
    /// The line of the call is recorded as the line of [`App::gate`] is.
    #[must_use]
    #[track_caller]
    pub fn gate_async<F, R>(mut self, gate: F) -> Self
    where
        F: AsyncFn() -> R + Send + Sync + 'static,
        R: IntoGate + 'static,
    {
        let gate = Arc::new(gate);
        let gate: Gate = Arc::new(move || {
            let gate = Arc::clone(&gate);
            GateCall::Pending(Box::pin(async move {
                match gate().await.into_gate() {
                    GateCall::Done(result) => result,
                    GateCall::Pending(future) => future.await,
                }
            }))
        });
        self.inner.graphql = gate_resolvers(&self.inner.graphql, &gate);
        self.inner.gate = Some(gate);
        self.inner.gate_at = Some(core::panic::Location::caller());
        self
    }

    /// Merges the registered `parts` into the roots of the app, the parts of
    /// one root in the order of their registration, and answers the app.
    ///
    /// The one additive method the native builder of the facade implements
    /// `App` through. `objects` is the glue of the crate that makes the root
    /// object of a part, `None` where the app is booted with no glue, and a
    /// root of several parts spreads their objects in order, as
    /// [`App::compose`] merges the roots of its children.
    ///
    /// # Panics
    ///
    /// Panics with the type and the place of the first part the glue makes no
    /// root object of, and of any part where no glue is given, since a part
    /// whose members the executor cannot read would answer `undefined` for
    /// every field. The generated packages of a plain crate always hand the
    /// glue of its generated copy.
    #[must_use]
    pub fn merge_parts(mut self, parts: Vec<RootPart>, objects: Option<PartObjects>) -> Self {
        for part in parts {
            let object = objects.and_then(|objects| objects(&part.value));
            let Some(object) = object else {
                panic!(
                    "the part `{}` registered at {}:{} has no glue, which the generated copy of a plain crate writes: build and serve the app with cargo aosabi",
                    part.type_name,
                    part.registered_at.file(),
                    part.registered_at.line()
                );
            };
            let factory = Some(part_factory(object));
            let roots = match part.root {
                Root::Query => Resolvers {
                    query: factory,
                    ..Resolvers::default()
                },
                Root::Mutation => Resolvers {
                    mutation: factory,
                    ..Resolvers::default()
                },
                Root::Subscription => Resolvers {
                    subscription: factory,
                    ..Resolvers::default()
                },
            };
            merge_fragment(&mut self.inner.graphql, &roots);
        }
        self
    }

    /// Wraps every root entry the app holds in the gate the app holds, as
    /// [`App::gate`] wraps the entries the app holds at its call, and
    /// answers the app.
    ///
    /// The builder of the facade sets the gate where its chain names it and hands
    /// its parts over after its last call, so it wraps them here after
    /// [`App::merge_parts`] and before it composes its children, whose entries
    /// keep their own gates, as the reference wraps the entries of its
    /// constructor before `compose` merges the children. The builder names this
    /// method from its call of `.gate(..)` alone, so an app that sets no gate
    /// links no wrapper.
    #[must_use]
    pub fn gate_roots(mut self) -> Self {
        if let Some(gate) = &self.inner.gate {
            self.inner.graphql = gate_resolvers(&self.inner.graphql, gate);
        }
        self
    }

    /// Sets the configuration the app carries, the plugins and the options of
    /// the chain of the native builder, which the serving entry boots it with.
    #[must_use]
    pub fn carry_config(mut self, config: AppConfig) -> Self {
        self.inner.config = Some(config);
        self
    }

    /// Takes the configuration the app carries, leaving none, so the boot
    /// stores the configuration it boots with as for an app that carries
    /// none.
    pub fn take_config(&mut self) -> Option<AppConfig> {
        self.inner.config.take()
    }

    /// Records `children` and merges their roots, `compose(...children)`.
    ///
    /// The routes of a child are mounted by [`App::realize`] at the boot.
    #[must_use]
    pub fn compose<C: Children>(mut self, children: C) -> App<Composed<G, C>> {
        for child in children.into_apps() {
            merge_fragment(&mut self.inner.graphql, &child.inner.graphql);
            self.inner.children.push(child);
        }
        self.retag()
    }

    /// Registers `handler` for `GET` requests of `path`, `app.get(path, handler)`.
    #[track_caller]
    pub fn get<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.get(path, handler);
        self
    }

    /// Registers `handler` for `POST` requests of `path`, `app.post(path, handler)`.
    #[track_caller]
    pub fn post<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.post(path, handler);
        self
    }

    /// Registers `handler` for `PUT` requests of `path`, `app.put(path, handler)`.
    #[track_caller]
    pub fn put<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.put(path, handler);
        self
    }

    /// Registers `handler` for `DELETE` requests of `path`, `app.delete(path, handler)`.
    #[track_caller]
    pub fn delete<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.delete(path, handler);
        self
    }

    /// Registers `handler` for `PATCH` requests of `path`, `app.patch(path, handler)`.
    #[track_caller]
    pub fn patch<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.patch(path, handler);
        self
    }

    /// Registers `handler` for `OPTIONS` requests of `path`, `app.options(path, handler)`.
    #[track_caller]
    pub fn options<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.options(path, handler);
        self
    }

    /// Registers `handler` for every method of `path`, `app.all(path, handler)`.
    #[track_caller]
    pub fn all<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.all(path, handler);
        self
    }

    /// Registers `handler` for `method` on `path`, `app.on(method, path, handler)`.
    #[track_caller]
    pub fn on<F, Fut>(&mut self, method: &str, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.on(method, path, handler);
        self
    }

    /// Registers `middleware` for every method of `path` and below it, `app.use(path, middleware)`.
    #[track_caller]
    pub fn r#use<F, Fut>(&mut self, path: &str, middleware: F) -> &mut Self
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.r#use(path, middleware);
        self
    }

    /// Registers `middleware` for every method of `path` and below it,
    /// `app.use(path, middleware)`, as `App::r#use` does, under a name
    /// that is no keyword of Rust.
    #[track_caller]
    pub fn middleware<F, Fut>(&mut self, path: &str, middleware: F) -> &mut Self
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.r#use(path, middleware);
        self
    }

    /// Registers a middleware held as a trait object, `app.use(path, middleware)`.
    #[track_caller]
    pub fn use_boxed(&mut self, path: &str, middleware: Rc<dyn MiddlewareFn<Thrown>>) -> &mut Self {
        self.inner.web.use_boxed(path, middleware);
        self
    }

    /// Mounts the routes `app` holds now under `path`, `app.route(path, app)`.
    pub fn route<A: IntoApp>(&mut self, path: &str, app: A) -> &mut Self {
        let app = app.into_app();
        self.inner.web.route(path, &app.inner.web);
        self
    }

    /// Sets the not found handler, `app.notFound(handler)`.
    pub fn not_found<F, Fut>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.not_found(handler);
        self
    }

    /// Sets the error handler, `app.onError(handler)`.
    pub fn on_error<F, Fut>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(Thrown, Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.inner.web.on_error(handler);
        self
    }

    /// Returns the configuration the boot stored, `app.config`.
    #[must_use]
    pub fn config(&self) -> Option<&AppConfig> {
        self.inner.config.as_ref()
    }

    /// Stores the configuration, `target.config = config`.
    pub(crate) fn set_config(&mut self, config: AppConfig) {
        self.inner.config = Some(config);
    }

    /// Returns the identity of the app, `name`.
    #[must_use]
    pub fn app_name(&self) -> Option<&str> {
        self.inner.name.as_deref()
    }

    /// Returns the gates of the tree, the app first and each child after its parent.
    #[must_use]
    pub fn gates(&self) -> Vec<GateRecord> {
        let mut gates = Vec::new();
        self.collect_gates(&mut gates);
        gates
    }

    /// Appends the gate of the app and those of its children to `gates`.
    fn collect_gates(&self, gates: &mut Vec<GateRecord>) {
        if let Some(registered_at) = self.inner.gate_at {
            gates.push(GateRecord {
                app: self.inner.name.clone(),
                registered_at,
            });
        }
        for child in &self.inner.children {
            child.collect_gates(gates);
        }
    }

    /// Returns the records of the passes of the boot for writing.
    pub(crate) fn boot_record_mut(&mut self) -> &mut BootRecord {
        &mut self.inner.boot
    }

    /// Returns the run time roots of the app, `graphql`.
    #[must_use]
    pub fn graphql(&self) -> &Resolvers {
        &self.inner.graphql
    }

    /// Returns the registered routes in order, `app.routes`.
    #[must_use]
    pub fn routes(&self) -> Vec<RouteRecord> {
        self.inner.web.routes()
    }

    /// Adds a listener a serving plugin bound in its setup, which the boot
    /// runs after the last pass.
    pub fn attach_listener(&mut self, listener: Box<dyn Listen>) {
        self.inner.listeners.push(listener);
    }

    /// Pushes `middleware` onto the plugin chain, `pluginsMiddleware.push(mw)`.
    pub(crate) fn push_plugin_middleware(&mut self, middleware: PluginMiddleware) {
        self.inner.plugins_middleware.borrow_mut().push(middleware);
    }

    /// Installs the base pipeline of the served root once, `installBasePipeline()`.
    ///
    /// The routes the app registered before it then run behind it, divergence
    /// D75.
    pub fn install_base_pipeline(&mut self) {
        let registered = self.inner.web.routes().len();
        if self.inner.base_pipeline_installed {
            return;
        }
        self.inner.base_pipeline_installed = true;

        let web = &mut self.inner.web;
        let coding: Option<Rc<dyn MiddlewareFn<Thrown>>> = cfg!(feature = "base-compress")
            .then(|| Rc::new(compress::<Thrown>()) as Rc<dyn MiddlewareFn<Thrown>>);
        web.r#use(
            "*",
            move |c: Context, next: Next| -> LocalBoxFuture<'static, Result<Response>> {
                // The coding layer reads `ctx.res.headers` once the rest has
                // run, which throws on a context that is its own answer, and
                // codes the answer under the feature `base-compress` alone.
                let rest = Next::new(move |c: Context| async move {
                    next.run(c.clone()).await?;
                    if is_own_answer(&c) {
                        return Err(Thrown::type_error(READS_GET));
                    }
                    Ok(())
                });
                match &coding {
                    Some(compress) => compress.call(c, rest),
                    None => Box::pin(rest.run(c)),
                }
            },
        );

        web.r#use("*", |c: Context, next: Next| {
            with_context(c.clone(), next.run(c))
        });

        // Structured request logger (rfcs/RUNTIME_LOGGER.md). Skips
        // /__aosabi/* (static assets) and the whole line when
        // `config.logger: false`.
        let access = except(&["/__aosabi/*"], |c: Context, next: Next| async move {
            let start = now_ms();
            let req_log = get_root_logger()
                .child(json!({
                    requestId: request_id(&c),
                    method: c.req().method(),
                    path: c.req().path(),
                }))
                .with_tag("http");
            run_with_logger(req_log, async move {
                let response = next.run(c.clone()).await?;
                if access_log_enabled() {
                    req_log.info(
                        "request",
                        json!({
                            status: access_status(&c),
                            durationMs: now_ms() - start,
                        }),
                    );
                }
                Ok(response)
            })
            .await
        });
        web.use_boxed("*", Rc::new(access));

        let middleware = Rc::clone(&self.inner.plugins_middleware);
        web.r#use("*", move |c: Context, next: Next| {
            // The default csrf of a Rust service, hono's check with its
            // defaults on a request that carries a cookie and that no csrf of
            // the app matches, runs in this entry ahead of the plugin
            // middleware, behind the access logger, which logs its 403, so
            // `app.routes` lists the entries of the TypeScript pipeline,
            // divergence D70. The TypeScript option has none.
            let middleware = Rc::clone(&middleware);
            let value = aosabi_http::middleware::csrf::behind_default_csrf(&c, move |c| {
                dispatch_plugins(middleware, 0, c, next)
            });
            async move {
                // The default body limit of a Rust service, hono's bodyLimit
                // with 1 MiB held to every read of a body that no body limit
                // of the app takes, runs in this entry around the rest of it,
                // behind the access logger, which logs its 413, so
                // `app.routes` lists the entries of the TypeScript pipeline,
                // divergence D76. The TypeScript option has none.
                let value =
                    aosabi_http::middleware::body_limit::behind_default_body_limit(&c, value);
                if let Some(response) = value.await? {
                    return Ok(response);
                }
                // The chain answered nothing, so Hono gets no value.
                if !c.finalized() {
                    mark_own_answer(&c);
                }
                Ok(c.res())
            }
        });

        self.install_error_handler();

        self.inner.web.put_behind(registered);
    }

    /// Installs the error handler of the base pipeline, the last step of
    /// `installBasePipeline()`.
    fn install_error_handler(&mut self) {
        // Map a thrown error's HTTP status for plain routes, auth free: it
        // reads a numeric `statusCode` convention alone.
        self.inner
            .web
            .on_error(|err: Thrown, c: Context| async move {
                let response = error_response(&err, &c)?;
                if take_own_answer(&c) {
                    // Compose sets the answer through the setter of `c.res`,
                    // which reads the headers of the answer before it, here the
                    // context itself.
                    return Err(Thrown::type_error(READS_ENTRIES));
                }
                Ok(response)
            });
    }

    /// Mounts the composed child tree once, depth first, `realize()`.
    ///
    /// Each child realizes its own children first and is mounted at its base path
    /// or `/`.
    pub fn realize(&mut self) {
        if self.inner.realized {
            return;
        }
        self.inner.realized = true;
        let inner = &mut *self.inner;
        for child in &mut inner.children {
            child.realize();
            let prefix = child
                .inner
                .route_prefix
                .clone()
                .unwrap_or_else(|| String::from("/"));
            inner.web.route(&prefix, &child.inner.web);
        }
    }
}

/// The `TypeError` compress throws where it reads `ctx.res.headers.get(...)` of a
/// context that is its own answer.
const READS_GET: &str = "Cannot read properties of undefined (reading 'get')";

/// The `TypeError` the setter of `c.res` throws where it reads the headers of an
/// answer that is the context itself.
const READS_ENTRIES: &str = "Cannot read properties of undefined (reading 'entries')";

/// The mark of a context that is its own answer, `c.res = c`.
///
/// When the plugin middleware answers nothing and nothing set the answer of the
/// context, the layer above the dispatch of the loader sets the context itself as
/// the answer: the inner compose of `every` in the wrapper of the access logger,
/// whose continuation answers the context, or the async context layer, which
/// resolves with it where the wrapper skips the logger. Hono holds the empty
/// response of `c.res()` in its place here, and every layer that reads the answer
/// of the reference reads the mark first: the access line, compress and the error
/// handler.
struct OwnAnswer(Cell<bool>);

/// Marks `c` as its own answer.
fn mark_own_answer(c: &Context) {
    c.set_extension(Rc::new(OwnAnswer(Cell::new(true))));
}

/// Tells whether `c` is its own answer.
fn is_own_answer(c: &Context) -> bool {
    c.extension::<OwnAnswer>().is_some_and(|mark| mark.0.get())
}

/// Clears the mark of `c` and tells whether it was set.
///
/// The setter of `c.res` throws before it stores the new answer, so the
/// error handler answers the second throw with nothing marked any more.
fn take_own_answer(c: &Context) -> bool {
    c.extension::<OwnAnswer>()
        .is_some_and(|mark| mark.0.replace(false))
}

/// Returns the status the access line reads, `c.res.status`.
///
/// On a context that is its own answer the reference reads the method `status` of
/// the context, a function, which `JSON.stringify` leaves out of the JSON line
/// and the development line prints as its source. A record has no function value
/// here, so the status is absent, which the JSON line writes as the reference
/// does.
fn access_status(c: &Context) -> JsValue {
    if is_own_answer(c) {
        JsValue::Undefined
    } else {
        JsValue::Number(f64::from(c.res().status()))
    }
}

/// Answers `err` as the error handler of the base pipeline does.
///
/// The response of an `HTTPException`, a numeric `statusCode` as the status, and
/// otherwise the record `unhandled route error` and 500.
fn error_response(err: &Thrown, c: &Context) -> Result<Response> {
    if let Some(response) = err.http_exception_response() {
        return Ok(response);
    }
    if let Some(status) = err.status_code() {
        let body = JsValue::Object(json!({ error: err.message() }));
        return c.json_with(&body, Arg::Status(status_u16(status)));
    }
    get_logger().error(
        "unhandled route error",
        json!({ err: normalize_error(&err.0) }),
    );
    let body = JsValue::Object(json!({ error: "Internal Server Error" }));
    c.json_with(&body, Arg::Status(500))
}

/// Returns the status of a numeric `statusCode` as the status of a response.
///
/// hono hands the number to the `Response` constructor, which takes an
/// integer status, so a status outside the range of a status is left to the
/// constructor's refusal as 0.
fn status_u16(status: f64) -> u16 {
    if (0.0..=f64::from(u16::MAX)).contains(&status) {
        // The range test leaves a number the target holds.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the value lies in the range of the target"
        )]
        let status = status as u16;
        status
    } else {
        0
    }
}

/// Returns the milliseconds since the epoch, `Date.now()`.
///
/// A Worker reads no clock of the system from `std`, whose `SystemTime::now`
/// panics on `wasm32-unknown-unknown`, so a Worker build reads 0 until the glue
/// of the Worker hands the host's time, as the file of `graphql-over-http` does.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn now_ms() -> f64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    #[expect(
        clippy::cast_precision_loss,
        reason = "the milliseconds of a wall clock are far below 2^53"
    )]
    let ms = elapsed.as_millis() as f64;
    ms
}

/// Returns 0, the time of a Worker build, see the other variant.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn now_ms() -> f64 {
    0.0
}

/// Runs the plugin middleware `index` of `list` over `c` and answers its
/// value, the onion of `index.ts:334-343`.
///
/// Past the end of the list the rest of Hono's chain runs, whose value is the
/// response it leaves on the context. A middleware of the response form gets
/// a continuation that sets the value of the rest on the context when nothing
/// set an answer, so `next.run(c)` answers it, and a middleware of the value
/// form gets one that drops it, as `await next()` with nothing returned does.
/// The value of the dispatch is the value of its first middleware, which is
/// what the reference hands Hono.
fn dispatch_plugins(
    list: Rc<RefCell<Vec<PluginMiddleware>>>,
    index: usize,
    c: Context,
    next: Next,
) -> LocalBoxFuture<'static, Result<Option<Response>>> {
    let middleware = list.borrow().get(index).cloned();
    match middleware {
        None => Box::pin(async move { next.run(c).await.map(Some) }),
        Some(PluginMiddleware::Response(middleware)) => {
            let inner = Next::new(move |c: Context| async move {
                let value = dispatch_plugins(list, index + 1, c.clone(), next).await?;
                if let Some(response) = value
                    && !c.finalized()
                {
                    c.set_res(response);
                }
                Ok(())
            });
            let answer = middleware.call(c, inner);
            Box::pin(async move { answer.await.map(Some) })
        }
        Some(PluginMiddleware::Value(middleware)) => {
            let inner = Next::new(move |c: Context| async move {
                dispatch_plugins(list, index + 1, c, next).await.map(|_| ())
            });
            middleware.call(c, inner)
        }
    }
}

/// Merges the roots of `from` into `into`, `mergeFragment(into, from)`.
///
/// For each root the map of `from` spreads over the map of `into`, so a member of
/// both keeps the position of `into` and takes the value of `from`, and a root
/// `into` lacks is taken as it stands.
pub fn merge_fragment(into: &mut Resolvers, from: &Resolvers) {
    for (into_root, from_root) in [
        (&mut into.query, &from.query),
        (&mut into.mutation, &from.mutation),
        (&mut into.subscription, &from.subscription),
    ] {
        let Some(from_root) = from_root else {
            continue;
        };
        *into_root = Some(match into_root.take() {
            None => Arc::clone(from_root),
            Some(existing) => spread(&existing, from_root),
        });
    }
}

/// Returns the factory of `{...a, ...b}` over the root objects of two factories.
fn spread(a: &RootFactory, b: &RootFactory) -> RootFactory {
    let (a, b) = (Arc::clone(a), Arc::clone(b));
    Arc::new(move || {
        let mut entries: Vec<(String, Value)> = Vec::new();
        for object in [a(), b()] {
            let Value::Object(object) = object else {
                continue;
            };
            for key in object.own_keys() {
                let value = Rc::clone(&object).get(&key);
                match entries.iter_mut().find(|(known, _)| *known == key) {
                    Some(entry) => entry.1 = value,
                    None => entries.push((key, value)),
                }
            }
        }
        Value::Object(PlainObject::from_entries(entries))
    })
}

/// Returns the roots with every entry awaiting `gate` first, `gateResolvers(resolvers, gate)`.
///
/// Each entry becomes `async (...args) => { await gate(); return fn(...args) }`,
/// one async function whose first await is the gate, so a gated entry settles as
/// the reference's does.
#[must_use]
pub fn gate_resolvers(resolvers: &Resolvers, gate: &Gate) -> Resolvers {
    let wrap = |root: &Option<RootFactory>| {
        root.as_ref().map(|root| {
            let (root, gate) = (Arc::clone(root), Arc::clone(gate));
            let factory: RootFactory = Arc::new(move || {
                let Value::Object(object) = root() else {
                    return Value::Object(PlainObject::new());
                };
                let entries = object
                    .own_keys()
                    .into_iter()
                    .map(|key| {
                        let entry = Rc::clone(&object).get(&key);
                        let gated = gated_function(&key, entry, Arc::clone(&gate));
                        (key, Value::Function(gated))
                    })
                    .collect();
                Value::Object(PlainObject::from_entries(entries))
            });
            factory
        })
    };
    Resolvers {
        query: wrap(&resolvers.query),
        mutation: wrap(&resolvers.mutation),
        subscription: wrap(&resolvers.subscription),
    }
}

/// Returns the async function that awaits `gate` and then calls `entry`.
///
/// The gate runs in the body of the async function, so a gate that throws at once
/// rejects the promise of the call, as a throw in an async function does.
fn gated_function(name: &str, entry: Value, gate: Gate) -> Function {
    Function::new(name, move |agent: &Agent, args: &[Value]| {
        let args: Vec<Value> = args.to_vec();
        let entry = entry.clone();
        let gate = Arc::clone(&gate);
        let body = async move {
            let Some(agent) = Agent::current() else {
                return Completion::Throw(Thrown::error(OUTSIDE_AGENT).0);
            };
            let awaited = match gate() {
                GateCall::Done(Err(thrown)) => return Completion::Throw(thrown.0),
                GateCall::Done(Ok(())) => Promise::resolve(&agent, Value::Undefined),
                GateCall::Pending(future) => agent.spawn(Box::pin(async move {
                    match future.await {
                        Ok(()) => Completion::Normal(Value::Undefined),
                        Err(thrown) => Completion::Throw(thrown.0),
                    }
                })),
            };
            if let Err(thrown) = awaited.await {
                return Completion::Throw(thrown);
            }
            match entry {
                Value::Function(function) => function.call(&agent, &args),
                other => Completion::Throw(
                    Thrown::type_error(&format!(
                        "fn is not a function: {}",
                        aosabi_execute::value::inspect(&other)
                    ))
                    .0,
                ),
            }
        };
        Completion::Normal(Value::Promise(agent.spawn(Box::pin(body))))
    })
}

/// The throw of a gated entry polled outside an agent, which no executor
/// does.
const OUTSIDE_AGENT: &str = "a gated entry ran outside the agent of its request";
