// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi: the builder of the app of the native index of a plain crate.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/app/index.ts and
 * packages/pylon/src/core/index.ts, Apache-2.0. Changed by the Aosabi authors.
 */

//! The builder of an app.
//!
//! The index of a plain crate builds its app as one chain, `App::new()`, then
//! `.query(Query)`, `.mutation(Mutation)` and `.subscription(..)` for the
//! parts of its roots.
//!
//! A part of a root is a value of any type with no lifetime. The builder
//! keeps it once and the app lends it to each request as `&self`, so a part
//! needs no `Default` and no `Clone`. The builder reads nothing of the type:
//! the builder of the source path reads the members of the type from the
//! source of the crate, and the glue it writes into the generated copy of the
//! crate makes the root object of the part, which the app takes through
//! [`IntoApp::into_app_with`]. Every call takes and answers the builder by
//! value and forwards under `#[track_caller]` where the engine records a
//! place, so a part, a route, a gate and a plugin keep the line of their
//! call.
//!
//! The rest of the chain mirrors the index of a TypeScript app, each call
//! under the name of its TypeScript counterpart with Rust casing:
//!
//! | Calls | The TypeScript counterpart |
//! | --- | --- |
//! | `.base_path(..)`, `.name(..)`, `.gate(..)`, `.gate_async(..)` | the options of the app, the gate wrapping the entries of the parts of this app and of no composed one |
//! | `.get`, `.post`, `.put`, `.delete`, `.patch`, `.options`, `.all`, `.on`, `.route`, `.not_found`, `.on_error` | the routes and the handlers of Hono |
//! | `.middleware(path, mw)` | `app.use(path, mw)` of Hono, named `middleware` because `use` is a keyword of Rust |
//! | `.compose(child)` | `compose(...children)`, one child per call, the parent's roots first and the children's in the order of the calls |
//! | `.plugin(..)` | one element of `plugins` of `aosabi.config.ts`, in the order of the calls |
//! | `.graphiql(..)`, `.landing_page(..)`, `.logger(..)`, `.masked_errors(..)`, `.graphql_endpoint(..)`, `.health_check_endpoint(..)`, `.multipart(..)` | the other keys of the configuration that Aosabi ports |
//! | `.viewer(..)` | the key `viewer` of Aosabi |
//!
//! The plugins and the keys make the configuration the app carries, which
//! `aosabi::serve_app` serves, so a project of the native index needs no
//! `aosabi.config.rs`. The boot report names a plugin of the chain by its
//! name and by the file and the line of its call, which
//! `core::panic::Location::caller()` gives under `#[track_caller]`.
//!
//! # Examples
//!
//! ```ignore
//! use aosabi::prelude::*;
//!
//! pub struct Query;
//!
//! impl Query {
//!     pub fn hello(&self) -> String {
//!         String::from("Hello, world!")
//!     }
//! }
//!
//! pub fn app() -> impl IntoApp {
//!     App::new()
//!         .query(Query)
//!         .get("/ping", |c| async move { c.text("pong") })
//!         .plugin(aosabi::use_node_server())
//! }
//! ```

use core::{
    any::{
        Any,
        type_name, //
    },
    future::Future,
    panic::Location, //
};
use std::rc::Rc;

use aosabi_core::{
    AppConfig,
    Context,
    IntoApp,
    IntoGate,
    IntoPlugin,
    LoggerOption,
    Next,
    PluginList,
    PluginSlot,
    Result,
    Thrown,
    app::application::{
        PartObjects,
        Root,
        RootPart, //
    },
    descriptor::{
        AppRoots,
        NoRoots, //
    },
    plugin::config::MaskedErrors, //
};
use aosabi_http::Response;

/// A composed app, which answers the engine's app with the glue of the
/// crate where the parent answers its own with it.
type Child = Box<dyn FnOnce(Option<PartObjects>) -> aosabi_core::App>;

/// The builder of an app with the parts of its roots, its
/// routes, its composed apps and its configuration.
///
/// It holds the engine app every route and option forwards to, the parts and
/// the composed apps it has registered, in order, the configuration of its
/// chain, and the wrapper of its parts in its gate where a call sets one.
pub struct App {
    /// The engine app.
    engine: aosabi_core::App,
    /// The registered parts, in the order of their calls.
    parts: Vec<RootPart>,
    /// The composed apps, in the order of their calls.
    children: Vec<Child>,
    /// The configuration of the chain, `None` until a call sets a key.
    config: Option<AppConfig>,
    /// [`aosabi_core::App::gate_roots`], which only a call of `.gate(..)`
    /// or `.gate_async(..)` names, so an app with no gate links no wrapper.
    gate_roots: Option<fn(aosabi_core::App) -> aosabi_core::App>,
}

impl App {
    /// Creates the builder of an app with no root.
    #[must_use]
    #[track_caller]
    pub fn new() -> Self {
        Self {
            engine: aosabi_core::App::default(),
            parts: Vec::new(),
            children: Vec::new(),
            config: None,
            gate_roots: None,
        }
    }

    /// Registers `part` as a part of the root `Query`.
    ///
    /// The app keeps `part` once and lends it to each request, and its public
    /// fields and the public `&self` methods of its inherent blocks are the
    /// fields of the root, after those of the parts registered before it.
    #[must_use]
    #[track_caller]
    pub fn query<T: 'static>(self, part: T) -> Self {
        self.part(Root::Query, part, Location::caller())
    }

    /// Registers `part` as a part of the root `Mutation`, as
    /// [`App::query`] registers a part of `Query`.
    #[must_use]
    #[track_caller]
    pub fn mutation<T: 'static>(self, part: T) -> Self {
        self.part(Root::Mutation, part, Location::caller())
    }

    /// Registers `part` as a part of the root `Subscription`, as
    /// [`App::query`] registers a part of `Query`.
    #[must_use]
    #[track_caller]
    pub fn subscription<T: 'static>(self, part: T) -> Self {
        self.part(Root::Subscription, part, Location::caller())
    }

    /// Keeps `part` as a part of `root`, registered at `registered_at`.
    fn part<T: 'static>(
        mut self,
        root: Root,
        part: T,
        registered_at: &'static Location<'static>,
    ) -> Self {
        let value: Rc<dyn Any> = Rc::new(part);
        self.parts.push(RootPart {
            root,
            value,
            type_name: type_name::<T>(),
            registered_at,
        });
        self
    }

    /// Sets where `compose` mounts the routes of the app, `basePath`.
    #[must_use]
    pub fn base_path(mut self, path: &str) -> Self {
        self.engine = self.engine.base_path(path);
        self
    }

    /// Sets the identity of the app, `name`.
    #[must_use]
    pub fn name(mut self, name: &str) -> Self {
        self.engine = self.engine.name(name);
        self
    }

    /// Sets the gate of the app, `gate`, a closure that answers `()` or
    /// `Result<()>`.
    ///
    /// The gate wraps every entry of the parts of this app, wherever the call
    /// stands in the chain, and no entry of a composed app, which keeps its
    /// own gate. The line of the call is recorded for the boot report.
    #[must_use]
    #[track_caller]
    pub fn gate<F, R>(mut self, gate: F) -> Self
    where
        F: Fn() -> R + Send + Sync + 'static,
        R: IntoGate,
    {
        self.engine = self.engine.gate(gate);
        self.gate_roots = Some(aosabi_core::App::gate_roots);
        self
    }

    /// Sets an asynchronous gate, `gate: async () => ...`, as [`App::gate`]
    /// sets a gate.
    #[must_use]
    #[track_caller]
    pub fn gate_async<F, R>(mut self, gate: F) -> Self
    where
        F: AsyncFn() -> R + Send + Sync + 'static,
        R: IntoGate + 'static,
    {
        self.engine = self.engine.gate_async(gate);
        self.gate_roots = Some(aosabi_core::App::gate_roots);
        self
    }

    /// Registers `handler` for `GET` requests of `path`, `app.get(path, handler)`.
    #[must_use]
    #[track_caller]
    pub fn get<F, Fut>(mut self, path: &str, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.get(path, handler);
        self
    }

    /// Registers `handler` for `POST` requests of `path`, `app.post(path, handler)`.
    #[must_use]
    #[track_caller]
    pub fn post<F, Fut>(mut self, path: &str, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.post(path, handler);
        self
    }

    /// Registers `handler` for `PUT` requests of `path`, `app.put(path, handler)`.
    #[must_use]
    #[track_caller]
    pub fn put<F, Fut>(mut self, path: &str, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.put(path, handler);
        self
    }

    /// Registers `handler` for `DELETE` requests of `path`, `app.delete(path, handler)`.
    #[must_use]
    #[track_caller]
    pub fn delete<F, Fut>(mut self, path: &str, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.delete(path, handler);
        self
    }

    /// Registers `handler` for `PATCH` requests of `path`, `app.patch(path, handler)`.
    #[must_use]
    #[track_caller]
    pub fn patch<F, Fut>(mut self, path: &str, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.patch(path, handler);
        self
    }

    /// Registers `handler` for `OPTIONS` requests of `path`, `app.options(path, handler)`.
    #[must_use]
    #[track_caller]
    pub fn options<F, Fut>(mut self, path: &str, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.options(path, handler);
        self
    }

    /// Registers `handler` for every method of `path`, `app.all(path, handler)`.
    #[must_use]
    #[track_caller]
    pub fn all<F, Fut>(mut self, path: &str, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.all(path, handler);
        self
    }

    /// Registers `handler` for `method` on `path`, `app.on(method, path, handler)`.
    #[must_use]
    #[track_caller]
    pub fn on<F, Fut>(mut self, method: &str, path: &str, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.on(method, path, handler);
        self
    }

    /// Registers `middleware` for every method of `path` and below it,
    /// `app.use(path, middleware)`.
    #[must_use]
    #[track_caller]
    pub fn middleware<F, Fut>(mut self, path: &str, middleware: F) -> Self
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.middleware(path, middleware);
        self
    }

    /// Mounts the routes `app` holds now under `path`, `app.route(path, app)`.
    ///
    /// Hono mounts the routes of the app alone, so the parts, the composed
    /// apps and the configuration of `app` reach no root of this one.
    #[must_use]
    pub fn route(mut self, path: &str, app: App) -> Self {
        self.engine.route(path, app.engine);
        self
    }

    /// Sets the not found handler, `app.notFound(handler)`.
    #[must_use]
    pub fn not_found<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.not_found(handler);
        self
    }

    /// Sets the error handler, `app.onError(handler)`.
    #[must_use]
    pub fn on_error<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(Thrown, Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response>> + 'static,
    {
        self.engine.on_error(handler);
        self
    }

    /// Composes `child` into the app, `compose(child)`.
    ///
    /// The roots of the child merge after the roots of this app and of every
    /// child composed before it, and its routes are mounted at its base path,
    /// or at `/`, when the app boots. `compose(a, b)` of a TypeScript app
    /// is `.compose(a).compose(b)` here.
    #[must_use]
    pub fn compose<A: IntoApp + 'static>(mut self, child: A) -> Self {
        self.children.push(Box::new(move |objects| match objects {
            Some(objects) => child.into_app_with(objects),
            None => child.into_app(),
        }));
        self
    }

    /// Adds `plugin` to the plugins of the configuration, after those of the
    /// calls before it.
    ///
    /// The place of the call is recorded, so the boot report names the plugin
    /// by its name and by the file and the line of the call.
    #[must_use]
    #[track_caller]
    pub fn plugin(mut self, plugin: impl IntoPlugin) -> Self {
        let slot = PluginSlot::called(plugin, Location::caller());
        self.config()
            .plugins
            .get_or_insert_with(PluginList::default)
            .0
            .push(slot);
        self
    }

    /// Sets `graphiql`. The page of `GraphiQL` is served only when it is
    /// `true`, the named divergence D48, and `false` turns off introspection
    /// as well.
    #[must_use]
    pub fn graphiql(mut self, enabled: bool) -> Self {
        self.config().graphiql = Some(enabled);
        self
    }

    /// Sets `landingPage`. The landing page of an unhandled route is served
    /// only when it is `true`, the named divergence D47.
    #[must_use]
    pub fn landing_page(mut self, enabled: bool) -> Self {
        self.config().landing_page = Some(enabled);
        self
    }

    /// Sets `viewer`, the key of Aosabi that serves the viewer at `/viewer`
    /// only when it is `true`, the named divergence D48.
    #[must_use]
    pub fn viewer(mut self, enabled: bool) -> Self {
        self.config().viewer = Some(enabled);
        self
    }

    /// Sets `logger`, `false` for no access line or the object form.
    #[must_use]
    pub fn logger(mut self, logger: impl Into<LoggerOption>) -> Self {
        self.config().logger = Some(logger.into());
        self
    }

    /// Sets `maskedErrors` of Yoga.
    #[must_use]
    pub fn masked_errors(mut self, masked_errors: MaskedErrors) -> Self {
        self.config().masked_errors = Some(masked_errors);
        self
    }

    /// Sets `graphqlEndpoint` of Yoga.
    #[must_use]
    pub fn graphql_endpoint(mut self, endpoint: &str) -> Self {
        self.config().graphql_endpoint = Some(String::from(endpoint));
        self
    }

    /// Sets `healthCheckEndpoint` of Yoga.
    #[must_use]
    pub fn health_check_endpoint(mut self, endpoint: &str) -> Self {
        self.config().health_check_endpoint = Some(String::from(endpoint));
        self
    }

    /// Sets `multipart` of Yoga. The parser of uploads runs only when it is
    /// `true`, the named divergence D49.
    #[must_use]
    pub fn multipart(mut self, enabled: bool) -> Self {
        self.config().multipart = Some(enabled);
        self
    }

    /// Returns the configuration of the chain, made at the first key a call
    /// sets.
    fn config(&mut self) -> &mut AppConfig {
        self.config.get_or_insert_with(AppConfig::default)
    }

    /// Returns the engine app: the parts merged into its roots behind its
    /// gate, then each composed app with the same glue in the order of the
    /// calls, and the configuration of the chain carried.
    fn realize(self, objects: Option<PartObjects>) -> aosabi_core::App {
        let Self {
            engine,
            parts,
            children,
            config,
            gate_roots,
        } = self;
        let mut app = engine.merge_parts(parts, objects);
        if let Some(gate_roots) = gate_roots {
            app = gate_roots(app);
        }
        for child in children {
            app = app.compose((child(objects),)).into_app();
        }
        match config {
            Some(config) => app.carry_config(config),
            None => app,
        }
    }
}

impl Default for App {
    #[track_caller]
    fn default() -> Self {
        Self::new()
    }
}

/// The roots of the builder stand in no type: the builder of the source path
/// reads them from the source of the crate, its entry and the calls of its
/// chain, so the type names the roots of an app with no fragment.
impl AppRoots for App {
    type Roots = NoRoots;
}

impl IntoApp for App {
    /// Returns the engine app with the registered parts merged into its
    /// roots, with no glue to make their root objects.
    ///
    /// # Panics
    ///
    /// Panics where a part is registered, since with no glue the executor
    /// reads no member of it, which [`aosabi_core::App::merge_parts`]
    /// names.
    fn into_app(self) -> aosabi_core::App {
        self.realize(None)
    }

    fn into_app_with(self, objects: PartObjects) -> aosabi_core::App {
        self.realize(Some(objects))
    }
}
