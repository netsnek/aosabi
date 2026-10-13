// SPDX-License-Identifier: MIT
/*
 * aosabi-js boot: the boot of the app the host hands in.
 *
 * No analogue in the reference.
 */

//! The boot of the app the host hands in.
//!
//! The boot record is the SDL, as `.aosabi/schema.mjs` of the build holds it, then
//! the JSON text of the IR of the same build, then the type resolvers of the
//! form of the canary among those the glue hands `handler()`, a count and
//! each the name of its union or interface, a count of its members and each
//! the name of the member, a count and the names of the fields its check
//! tests with `in`, none for a build of the reference, then one byte for `graphiql`
//! and one for `landingPage`, each 0 when the configuration leaves it out, 1
//! for `true` and 2 for `false`, then one byte for the job mode, 0 for the
//! faithful mode and 1 for the fast mode, one byte for the read mode, 0 for
//! one `js_get` per read and 1 for the projection, and one byte for the
//! platform of the host, 1 for Bun and 0 for any other. Then come the
//! variables of the process of the host, a count and each name and value,
//! which the engine takes as the variables of its process, since the module
//! reads no environment of its own, then the option `logger`, a byte 0 when
//! the configuration leaves it out, 1 for `true`, 2 for `false` and 3 for an
//! object, whose value record follows, then the options of the last
//! `Sentry.init` of `aosabi/sentry/node` the app made before the boot, a byte
//! 0 for none or 1 and their JSON, and last the plugins of the configuration
//! in their order, a count and each a byte 0, the name of a marker, the JSON
//! of its options and a count of the functions of its options, each its key
//! and its handle in the table of the boot, 0 for a function the engine runs
//! none of, or a byte 1 and a plugin of the app.
//!
//! A plugin of the app is the record of [`crate::plugin::read`], the keys the
//! host reports of its object: its name, its strategy, its roles, its
//! dependencies, its hook `onExecute` and the other hooks of Envelop and Yoga
//! it holds, which fail the boot. Its middleware and the routes its setup
//! registers run in the hono of the host, `aosabi/pylon`, as do the routes and
//! the middleware of the app, the base pipeline and the children of the
//! app. So the app the engine boots holds no route of the app and installs no
//! base pipeline,
//! [`App::hosted_pipeline`]: its first middleware puts the context of the
//! dispatch and the logger of the request the host opened in place, [`seed`],
//! and then come the routes the plugins of the engine register in their
//! setups, the viewer, the landing page and the login of Zitadel, each of
//! which the host hands a request where its own routing reaches it.
//!
//! The boot runs the two passes of [`aosabi_core::boot`] with the GraphQL layer
//! built between them, the server of `graphql-over-http` over the schema of the
//! app, which the function `answer` of that crate answers with, mounted on `*`
//! where the boot of a Rust app mounts its handler, and keeps that server
//! beside the app, [`Booted`]. The host routes a request of the GraphQL
//! endpoint in its own hono, so the engine hands it to that server with no
//! router, [`crate::endpoint`], and the router and the chain of the port of
//! hono run for the routes of the plugins of the engine and its not found
//! handler alone.
//!
//! A boot with no handle of a graphql object, [`ABSENT`], boots the logger
//! alone, before the glue of the app runs, so a logger call of the app at the
//! load of a module or in the setup of a plugin writes through the engine at
//! once. Its record is the byte of the platform of the
//! host and the variables of its process. The app then boots over the same
//! instance when its host boots it, which `worker()` of a Worker does at its
//! call and every other host at the first request of the app, and where the
//! passes of `executeConfig` of the wrapper configured the logger, the boot
//! keeps the root logger those passes made and
//! the line of each request they set, so the lazy sink of the last pass writes
//! the records of the requests.
//!
//! The SDL and the IR become the schema record of
//! [`aosabi_core::boot::canary_schema_record`], which holds the IR to the SDL
//! and makes the type resolver of every union and interface from the IR, so a
//! value of the app with no `__typename` resolves, and whose texts
//! live as long as the instance, since the module boots once per instance. A
//! type resolver of the form of the canary that the host read off the glue
//! of a build of the canary takes the place of the rule of its type, so that
//! union or interface resolves by the rule of the canary over every property
//! name of a value, as that build resolves it.
//!
//! Where the host hands the schema record a build wrote in the place of the
//! IR, the boot reads that record through [`crate::prebuilt`] in place of the
//! IR and the parse of the SDL, with the same type resolvers of the canary,
//! and the schema build makes the types of the app from it.
//!
//! The app comes as one handle of the table of the boot: the copy the host made
//! of the graphql object the glue of the build hands to `handler()`, read
//! as the service reads it when it starts. The boot reads its roots `Query`,
//! `Mutation` and `Subscription` through `js_get`, each a copy of the entries
//! of a root of the app or absent, and each root factory answers a [`JsObject`]
//! over the handle of its root, so the walk of `resolvers_to_graphql_resolvers`
//! and `wrap_resolver` reads the app as it reads a Rust app, and the read of a
//! member of a root at each call of its resolver runs no getter of the app.

#![forbid(unsafe_code)]

use core::{
    cell::{
        Cell,
        RefCell, //
    },
    future::Future,
    pin::pin,
    task::{
        Context,
        Poll,
        Waker, //
    },
};
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi::{
    AppConfig,
    BUILTINS,
    ExecutionOutcome,
    IntoApp as _, //
};
use aosabi_core::{
    App,
    BootError,
    BootedApp,
    Builtins,
    ExecuteConfigArgs,
    Fragment,
    HandlerOptions,
    IntoPlugin as _,
    PluginList,
    PluginSlot,
    Resolvers,
    Strategy,
    app::{
        build_http_options,
        build_schema, //
    },
    boot::CanaryResolver,
    cache_budget,
    context::with_context,
    descriptor::NoRoots,
    execute_config,
    get_env::set_process_var,
    logger::{
        LevelSpec,
        LogFormat,
        LogLevel,
        Logger,
        LoggerConfig,
        LoggerOption,
        access_log_enabled,
        parse_level_spec,
        run_with_logger,
        set_access_log, //
    },
    plugin::{
        PluginObject,
        object::__wrap,
        on_execute_done, //
    },
    random::{
        RandomError,
        Randomness,
        install_randomness, //
    }, //
};
use aosabi_execute::{
    kernel_entry::RootFactory,
    value::Value, //
};
use aosabi_hooks::{
    plugin::OnSubscribeHooks,
    utils::handle_stream_or_single_execution_result, //
};
use aosabi_json::JsValue;
use aosabi_lang::js_string::to_string_lossy;
use graphql_over_http::HttpServer;

use crate::{
    abi::import,
    host::{
        self,
        Reader, //
    },
    plugin,
    request::{
        self,
        BOOT, //
    },
    value::{
        self,
        JsObject, //
    },
};

/// The handle of a root the app leaves out.
const ABSENT: u32 = u32::MAX;

/// The roots of the graphql object of an app, in the order of [`Resolvers`].
const ROOT_NAMES: [&str; 3] = ["Query", "Mutation", "Subscription"];

/// The polls the boot may take, a bound no boot reaches, since the setup of
/// the plugins the markers name waits on nothing.
const POLLS: usize = 1_000_000;

/// The booted app and the GraphQL layer it answers its endpoint with.
pub struct Booted {
    /// The app of the engine, the routes the plugins of the engine register
    /// and its not found handler.
    pub app: BootedApp,
    /// The server of `graphql-over-http` over the schema of the app, which
    /// the function `answer` of that crate answers the GraphQL endpoint with.
    pub server: Rc<HttpServer>,
}

thread_local! {
    /// The booted app.
    static APP: RefCell<Option<Rc<Booted>>> = const { RefCell::new(None) };
    /// The handles of the roots `Query`, `Mutation` and `Subscription`.
    static ROOTS: Cell<[u32; 3]> = const { Cell::new([ABSENT; 3]) };
    /// Whether a pass of `executeConfig` of the host configured the logger.
    static CONFIGURED: Cell<bool> = const { Cell::new(false) };
    /// The variables of the process of the host the boot took, in their order.
    static VARS: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
    /// The environment of a request over [`VARS`], made once after they change.
    static ENV: RefCell<Option<Rc<dyn core::any::Any>>> = const { RefCell::new(None) };
}

/// Returns the environment of a request of the app, `c.env`: the variables of
/// the process of the host the boot took, with the string bindings of a Worker
/// among them, so a layer of the engine that reads the environment of the
/// context reads what `env(c)` of the adapter of hono answers on that host, the
/// process on Bun and the bindings on a Worker, such as `AUTH_KEY` of the
/// Zitadel layer.
pub fn env() -> aosabi_http::Env {
    let bindings = ENV.with_borrow_mut(|cached| {
        Rc::clone(cached.get_or_insert_with(|| {
            let vars: Rc<dyn core::any::Any> =
                Rc::new(aosabi_http::Vars(VARS.with_borrow(Clone::clone)));
            vars
        }))
    });
    aosabi_http::Env::Bindings(bindings)
}

/// The fragment of the app of the host, whose roots are the handles the boot
/// read from its graphql object.
#[derive(Clone, Copy)]
struct HostFragment;

impl Fragment for HostFragment {
    type Roots = NoRoots;

    fn resolvers(self) -> Resolvers {
        let [query, mutation, subscription] = ROOTS.get();
        Resolvers {
            query: factory(query),
            mutation: factory(mutation),
            subscription: factory(subscription),
        }
    }
}

/// Returns the factory of the root `handle`, or `None` for an absent root.
fn factory(handle: u32) -> Option<RootFactory> {
    if handle == ABSENT {
        return None;
    }
    let factory: RootFactory =
        Arc::new(move || Value::Object(Rc::new(JsObject::new(BOOT, handle))));
    Some(factory)
}

/// Returns the booted app, once the host has booted it.
pub fn app() -> Option<Rc<Booted>> {
    APP.with_borrow(Clone::clone)
}

/// Boots the app from the boot record in the first `len` bytes of the input
/// buffer and the handle `graphql` of its graphql object, or the logger alone
/// where `graphql` is [`ABSENT`], and returns 0 or the address of the message
/// of the failure.
pub fn boot(len: u32, graphql: u32) -> usize {
    let record = host::input(len);
    if graphql == ABSENT {
        return match prime(&record) {
            Ok(()) => 0,
            Err(message) => host::output(message.as_bytes()),
        };
    }
    let roots = ROOT_NAMES.map(|name| value::boot_member(graphql, name).unwrap_or(ABSENT));
    match request::with_current(BOOT, || boot_from(&record, roots)) {
        Ok(app) => {
            APP.with_borrow_mut(|slot| *slot = Some(Rc::new(app)));
            0
        }
        Err(message) => host::output(message.as_bytes()),
    }
}

/// Boots the logger alone from the record `record`: the byte of the platform
/// of the host and the variables of its process, the writer of the console
/// and the randomness of the host.
fn prime(record: &[u8]) -> Result<(), &'static str> {
    let mut reader = Reader::new(record);
    aosabi_http::fetch::response::set_listed_text_type(reader.byte().ok_or(BROKEN)? != BUN);
    for _ in 0..reader.count().ok_or(BROKEN)? {
        let name = reader.text().ok_or(BROKEN)?;
        take_process_var(name, reader.text().ok_or(BROKEN)?);
    }
    crate::log::install_writer();
    install_randomness(Box::new(HostRandom));
    crate::outbound::install();
    Ok(())
}

/// Takes the variable `name` of the process of the host with `value` as a
/// variable of the process of the engine, which the core reads through
/// `getEnv()` and its alias table and Yoga for the interval of the ping of a
/// stream.
fn take_process_var(name: &str, value: &str) {
    set_process_var(name, value);
    graphql_over_http::host_env::set_process_var(name, value);
    VARS.with_borrow_mut(
        |vars| match vars.iter_mut().find(|(held, _)| held == name) {
            Some(entry) => entry.1 = String::from(value),
            None => vars.push((String::from(name), String::from(value))),
        },
    );
    ENV.with_borrow_mut(|cached| *cached = None);
}

/// Notes that a pass of `executeConfig` of the host configured the logger.
pub fn note_configured() {
    CONFIGURED.set(true);
}

/// Boots the app of `record` over `roots`.
fn boot_from(record: &[u8], roots: [u32; 3]) -> Result<Booted, String> {
    let mut reader = Reader::new(record);
    let schema = reader.text().ok_or("the boot record holds no schema")?;
    let ir = reader.text().ok_or("the boot record holds no IR")?;
    let canary = canary_resolvers(&mut reader).ok_or(BROKEN)?;
    let schema = crate::prebuilt::schema_record_of(schema, ir, &canary)?;
    let graphiql = flag(
        reader
            .byte()
            .ok_or("the boot record holds no graphiql byte")?,
    );
    let landing_page = flag(
        reader
            .byte()
            .ok_or("the boot record holds no landing byte")?,
    );
    request::set_fast(reader.byte().ok_or(BROKEN)? == 1);
    value::set_project(reader.byte().ok_or(BROKEN)? == 1);
    aosabi_http::fetch::response::set_listed_text_type(reader.byte().ok_or(BROKEN)? != BUN);
    for _ in 0..reader.count().ok_or(BROKEN)? {
        let name = reader.text().ok_or(BROKEN)?;
        take_process_var(name, reader.text().ok_or(BROKEN)?);
    }
    crate::log::install_writer();
    install_randomness(Box::new(HostRandom));
    crate::outbound::install();
    let logger = match reader.byte().ok_or(BROKEN)? {
        0 => None,
        1 => Some(LoggerOption::Enabled(true)),
        2 => Some(LoggerOption::Enabled(false)),
        _ => Some(LoggerOption::Config(logger_config(
            &value::decode(&mut reader, BOOT).ok_or(BROKEN)?.to_js(),
        ))),
    };
    if reader.byte().ok_or(BROKEN)? == 1 {
        crate::sentry::init(&host::read_json(&mut reader).ok_or(BROKEN)?);
    }
    let count = reader.count().ok_or(BROKEN)?;
    let mut slots = Vec::with_capacity(count.min(16));
    let mut identity = false;
    for _ in 0..count {
        if reader.byte().ok_or(BROKEN)? == 0 {
            let name = reader.text().ok_or(BROKEN)?;
            let options = host::read_json(&mut reader).ok_or(BROKEN)?;
            let functions = marker_functions(&mut reader).ok_or(BROKEN)?;
            slots.extend(marker(name, &options, &functions)?);
        } else {
            let read = plugin::read(&mut reader).ok_or(BROKEN)??;
            identity |= read.identity;
            slots.push(PluginSlot::new(read.plugin));
        }
    }
    value::set_identity_map(identity);
    // A pass of the host made the root logger and set the line of each
    // request, which the boot keeps: `false` turns the line off and keeps the
    // root, and the line comes back as the host set it after the boot.
    let configured = CONFIGURED.get();
    let access = access_log_enabled();
    let config = AppConfig {
        graphiql,
        landing_page,
        logger: if configured {
            Some(LoggerOption::Enabled(false))
        } else {
            logger
        },
        plugins: Some(PluginList::new(slots)),
        ..AppConfig::default()
    };
    let options = HandlerOptions {
        type_defs: schema.type_defs,
        resolve_types: schema.resolve_types,
        graphql: Resolvers::default(),
        host_clock: Some(Rc::new(crate::clock::HostTimers)),
        spawner: None,
        parser_and_validation_cache_budget: Some(cache_budget::TARGET),
    };
    ROOTS.set(roots);
    #[cfg(feature = "execute")]
    crate::execute::set_service(service(&schema).map_err(|error| error.to_string())?);
    let mut app = App::new(HostFragment).hosted_pipeline().into_app();
    app.use_boxed("*", seed());
    let booted = run(boot_engine(app, config, options));
    if configured {
        set_access_log(access);
    }
    match booted {
        Some(Ok(booted)) => Ok(booted),
        Some(Err(error)) => Err(error.to_string()),
        None => Err(String::from("the boot did not end")),
    }
}

/// Boots `app` with `config` and answers it with its GraphQL layer.
///
/// The steps of [`aosabi_core::boot`], the translation of
/// `emit-server-glue.ts:146-158`: the pass of the strategy `first`, the
/// server of the GraphQL layer over the schema of the roots of the app and
/// the configuration that pass wrote back, built as `handler()` builds it and
/// mounted on `*` as its step, [`crate::endpoint::middleware`], and the pass
/// of the strategy `last`. The server is kept beside the app as well, so a
/// request of the GraphQL endpoint reaches it with no router,
/// [`crate::endpoint::answer`]. The randomness of the host is in place
/// before, [`HostRandom`], and the listeners of the serving plugins stay
/// unbound, since the host serves.
async fn boot_engine(
    mut app: App,
    mut config: AppConfig,
    options: HandlerOptions,
) -> Result<Booted, BootError> {
    execute_config(
        &mut config,
        ExecuteConfigArgs::default(),
        &mut app,
        &HOST_BUILTINS,
    )
    .await
    .map_err(BootError::Plugin)?;
    let options = HandlerOptions {
        graphql: app.graphql().clone(),
        ..options
    };
    let (schema, core) = build_schema(&options).map_err(BootError::Schema)?;
    let server = HttpServer::new(build_http_options(
        schema,
        &core,
        &app,
        options.host_clock,
        options.parser_and_validation_cache_budget,
    ));
    app.use_boxed("*", crate::endpoint::middleware(Rc::clone(&server)));
    let last = ExecuteConfigArgs {
        plugins_strategy: Some(Strategy::Last),
    };
    execute_config(&mut config, last, &mut app, &HOST_BUILTINS)
        .await
        .map_err(BootError::Plugin)?;
    let (app, _listeners) = BootedApp::new(app);
    Ok(Booted { app, server })
}

/// Returns the service of the app over the schema record `schema`, the
/// schema, the roots and the type resolvers the handler of Yoga runs over,
/// which the executor of a test build runs a request on below HTTP.
#[cfg(feature = "execute")]
fn service(
    schema: &aosabi_core::boot::SchemaRecord,
) -> Result<aosabi_core::Service, aosabi_core::CreateSchemaError> {
    let mut build_side = aosabi_core::ResolverMap::default();
    for rule in schema.resolve_types {
        build_side.resolve_types.insert(
            String::from(rule.type_name),
            aosabi_core::glue::resolve_type::type_resolver(rule),
        );
    }
    aosabi_core::Service::builder(schema.type_defs)
        .roots(HostFragment.resolvers())
        .resolvers(build_side)
        .build()
}

/// Reads the type resolvers of the form of the canary at the cursor of
/// `reader`, or `None` for a broken record.
fn canary_resolvers(reader: &mut Reader<'_>) -> Option<Vec<CanaryResolver>> {
    let count = reader.count()?;
    let mut resolvers = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let type_name = String::from(reader.text()?);
        let mut members = Vec::new();
        for _ in 0..reader.count()? {
            let member = String::from(reader.text()?);
            let mut fields = Vec::new();
            for _ in 0..reader.count()? {
                fields.push(String::from(reader.text()?));
            }
            members.push((member, fields));
        }
        resolvers.push(CanaryResolver { type_name, members });
    }
    Some(resolvers)
}

/// Returns the option of the configuration a byte of the boot record names.
fn flag(byte: u8) -> Option<bool> {
    match byte {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// The built in plugins of the boot: the viewer and the landing page of the
/// facade, and before every plugin of a pass the plugin of [`finish_plugin`].
const HOST_BUILTINS: Builtins = Builtins {
    first: Some(finish_plugin),
    viewer: BUILTINS.viewer,
    unhandled_route: BUILTINS.unhandled_route,
};

/// Returns the plugin that makes the reads of the thrown errors of
/// the host at the end of the execution, [`value::finish`] over the errors of
/// a single result.
///
/// It stands before every plugin of the pass, the built in ones the boot adds
/// before the configuration among them, so its done hook runs before the done
/// hook of the logger of the errors, which then writes the record of an error
/// of the host from its reads of that error, a
/// `ServiceError` as a client error at `debug` with its code, before the done
/// hook of every plugin a marker names and before the mask of Yoga, which reads
/// the errors of the result. The reads of the logger
/// happen after the field of a request that could note them has
/// answered, so no answer shows the order. A subscription answers a stream of
/// results through the hooks of `onSubscribe`, and each result of the stream
/// takes the reads in its hook `onNext`, before the mask of Yoga reads it in
/// its own, as the reads happen when Yoga masks and writes each result
/// of the stream, and the error a stream fails with takes them in the hook
/// `onSubscribeError`, as [`value::finish_stream_error`] says.
fn finish_plugin() -> Rc<dyn aosabi_core::plugin::Plugin> {
    PluginObject {
        on_execute: __wrap::on_execute(|_payload| {
            on_execute_done(|done| {
                if let ExecutionOutcome::Single(result) = done.result {
                    value::finish(&mut result.errors);
                }
            })
        }),
        on_subscribe: __wrap::on_subscribe(|_payload| {
            Ok(Some(OnSubscribeHooks {
                on_subscribe_result: Some(Box::new(|done| {
                    handle_stream_or_single_execution_result(done, |result| {
                        value::finish(&mut result.errors);
                    })
                })),
                on_subscribe_error: Some(Box::new(value::finish_stream_error)),
            }))
        }),
        ..PluginObject::default()
    }
    .into_plugin()
}

/// Returns the plugin a marker of the wrapper names with its options, or
/// none for the serving plugin.
///
/// The markers are the plugins of the configuration that take data alone.
/// The serving plugin `useNodeServer` takes no slot, since the host serves,
/// as the Worker probe leaves its plugin list empty. The identity plugin of
/// `aosabi/auth/plugin` is a plugin of the app whose middleware runs the
/// provider of the app and whose hook `onExecute` is the map of the engine,
/// [`crate::plugin`]. The marker `zitadelLogin` of `aosabi/auth/zitadel` is
/// the plugin of the login routes of `aosabi-auth-zitadel` over its options,
/// [`crate::zitadel::login`], and the marker `useSentry` of the entry
/// `aosabi` the plugin of `aosabi-sentry` over its options and the functions
/// `functions` of its callback options, [`crate::sentry::plugin`].
// Out of line, so a boot compiles the plugins of the markers where the configuration names one.
#[inline(never)]
fn marker(
    name: &str,
    options: &JsValue,
    functions: &[(String, u32)],
) -> Result<Option<PluginSlot>, String> {
    match name {
        "zitadelLogin" => Ok(Some(crate::zitadel::login(options))),
        "useNodeServer" => Ok(None),
        "useSentry" => crate::sentry::plugin(options, functions).map(Some),
        other => Err(format!("the marker {other} has no plugin in aosabi-js")),
    }
}

/// Reads the functions of the options of a marker at the cursor of `reader`,
/// a count and each its key and its handle in the table of the boot.
fn marker_functions(reader: &mut Reader<'_>) -> Option<Vec<(String, u32)>> {
    let count = reader.count()?;
    let mut functions = Vec::with_capacity(count.min(16));
    for _ in 0..count {
        let key = String::from(reader.text()?);
        functions.push((key, reader.word()?));
    }
    Some(functions)
}

/// Polls `future` to its end with no waker, or answers `None` past the bound
/// of polls.
fn run<F: Future>(future: F) -> Option<F::Output> {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..POLLS {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return Some(output);
        }
    }
    None
}

/// The message of a boot record that ends early or holds a byte no field takes.
const BROKEN: &str = "the boot record is broken";

/// The byte of the platform of the host that is Bun, whose `Response` lists
/// no type of a string body among its header fields, where every other host
/// of the module, workerd and Node, lists it.
const BUN: u8 = 1;

/// Returns the first middleware of the app the engine boots, which runs the
/// dispatch of a request the host hands it with the context of the dispatch in
/// place, as the async context bind of the base pipeline puts the context
/// in place, and under the logger of the request the host opened it with, as
/// the access logger of that pipeline runs the rest of the chain under the
/// logger of the request, so the logger of the errors of an execution and
/// `getLogger()` of a resolver write through the logger of the request. The
/// context of the dispatch becomes the context of the request, which the calls
/// of `c` of the host read from then on.
fn seed() -> Rc<dyn aosabi_http::MiddlewareFn<aosabi_core::Thrown>> {
    Rc::new(|c: aosabi_http::Context, next: aosabi_core::Next| {
        let current = request::current();
        crate::context::replace_context(current, &c);
        let rest: aosabi_http::LocalBoxFuture<
            'static,
            Result<aosabi_http::Response, aosabi_core::Thrown>,
        > = match request::logger_of(current) {
            Some(raw) => Box::pin(run_with_logger(Logger::from_raw(raw), next.run(c.clone()))),
            None => Box::pin(next.run(c.clone())),
        };
        with_context(c, rest)
    })
}

/// Returns the object form of `config.logger` of the plain object `object`,
/// with no sink, since a function of the app crosses into no logger of the
/// engine.
// Out of line, so a boot compiles the object form of the option `logger` where the configuration
// holds one.
#[inline(never)]
pub(crate) fn logger_config(object: &JsValue) -> LoggerConfig {
    let JsValue::Object(object) = object else {
        return LoggerConfig::default();
    };
    let level = match object.get("level") {
        Some(JsValue::String(level)) => Some(parse_level_spec(&to_string_lossy(level))),
        Some(JsValue::Object(map)) => Some(LevelSpec::Map(
            map.iter()
                .filter_map(|(tag, level)| match level {
                    JsValue::String(level) => Some((
                        to_string_lossy(tag).into_owned(),
                        LogLevel::parse(&to_string_lossy(level))?,
                    )),
                    _ => None,
                })
                .collect(),
        )),
        _ => None,
    };
    let format = match object.get("format") {
        Some(JsValue::String(format)) => LogFormat::parse(&to_string_lossy(format)),
        _ => None,
    };
    let base = match object.get("base") {
        Some(JsValue::Object(base)) => Some(base.clone()),
        _ => None,
    };
    let redact = match object.get("redact") {
        Some(JsValue::Array(paths)) => Some(
            paths
                .iter()
                .filter_map(|path| match path {
                    JsValue::String(path) => Some(to_string_lossy(path).into_owned()),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    };
    let job_level = match object.get("job") {
        Some(JsValue::Object(job)) => match job.get("level") {
            Some(JsValue::String(level)) => LogLevel::parse(&to_string_lossy(level)),
            _ => None,
        },
        _ => None,
    };
    LoggerConfig {
        level,
        format,
        base,
        redact,
        sink: None,
        job_level,
    }
}

/// The randomness of the host, the bytes `crypto.getRandomValues` of the host
/// writes, which a request id of the engine takes in the place of
/// `crypto.randomUUID`, and which the host writes through the input
/// buffer as every record.
struct HostRandom;

impl Randomness for HostRandom {
    fn fill(&self, buf: &mut [u8]) -> Result<(), RandomError> {
        let len = import::host_random(host::length(buf.len()));
        let bytes = host::input(len);
        if bytes.len() != buf.len() {
            return Err(RandomError);
        }
        buf.copy_from_slice(&bytes);
        Ok(())
    }
}
