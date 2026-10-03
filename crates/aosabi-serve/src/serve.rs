// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-serve: the boot the generated entry runs, and the serving of one
 * stream.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/bundler/emit-server-glue.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `serve_with`, `serve_app_with` and `serve_stream`.
//!
//! The server entry of a TypeScript app loads the configuration, calling it when it
//! is a factory, runs the pass of the strategy `first`, mounts the GraphQL
//! handler with a pathless `use`, and runs the pass of the strategy `last`, in
//! which `useNodeServer` binds the port. [`serve_with`] does the same over the
//! configuration of a function of its own, `aosabi.config.rs`, and
//! [`serve_app_with`] over the configuration the app of the entry carries, the
//! plugins and the options of the chain of the builder of the facade, which it
//! evaluates under the abort hook of a configuration. An entry may answer a
//! `Result`, and its error ends the boot with the exit and the message of a
//! configuration that throws at load. Under [`serve_with`] the configuration of
//! the function is the one the boot runs, and a configuration the app carries
//! beside it is left out, so a project that keeps `aosabi.config.rs` boots as it
//! booted before the builder. Both run the seven steps of the old page of the
//! plan, with the same exit codes:
//!
//! 1. **The arguments.** No argument serves, `--print-schema` and
//!    `--requests PATH` are the harness modes of [`modes`], and
//!    any other argument prints the usage and exits 64.
//! 2. **The build step.** Under `AOSABI_BUILD=1` the step of
//!    [`build_step`](crate::build_step) runs and the process exits.
//! 3. **The boot.** One runtime runs the configuration under the abort hook,
//!    appends `use_node_server()` when no plugin of the list is named
//!    `node-server`, because a fresh service lists no carrier and the server
//!    card brings Network with HTTP(TCP), and runs `boot` of the core with the
//!    clock of the runtime as the host clock of every agent and `spawn_local` as
//!    its spawner, which drains the agent of every operation after its answer,
//!    so a timer a resolver set fires as node fires it. The randomness of
//!    `/dev/urandom`, the clock of the runtime, which a setup awaits through
//!    `aosabi_core::clock::delay`, and the capabilities the card hands over, the
//!    driver of the network capability among them, are installed before the
//!    first pass, and the keyed hashes of the thread take their key from that
//!    randomness first. A plugin whose setup throws and a cycle of the
//!    dependencies print the two lines of node's report that carry the
//!    reference's text, `Error: <message>` and, for an error with a cause,
//!    `  [cause]: <cause line>`, to standard error and exit 1, as node exits on
//!    the rejected top level await of the generated entry. The source excerpt,
//!    the stack frames and the version line of node are V8's inspection and have
//!    no counterpart. A schema that does not build prints its message and exits
//!    70, and a TLS configuration that does not read prints its message and
//!    exits 1, as the configuration abort does.
//! 4. **The serving plugin** binds the port in its setup, or the boot binds it
//!    itself in the development variant.
//! 5. **The drivers.** Every driver the facade hands in answers a listener
//!    when its switch is on, HTTP(Unix Socket) under `AOSABI_UNIX_SOCKET=1`
//!    among them, and a driver that fails to start prints its error and exits
//!    70. A driver is a driver of the server card and never an entry of
//!    `config.plugins`, so the plugin array stays the reference's.
//! 6. **The boot report**, when `AOSABI_BOOT_REPORT` names a file.
//! 7. **Serving.** Every listener runs as a local task until the process
//!    ends. A boot that bound no listener ends, as node ends when nothing
//!    keeps its loop alive, and there is no graceful shutdown, since the
//!    reference has none.
//!
//! Its plugins are gated by the role of the process, so `use_node_server` binds
//! nothing there, and a listener a plugin of the role binds in its setup runs as
//! in the server entry.
//!
//! [`serve_stream`] serves one stream through the Node connection of the fork
//! with `NodeConfig::NODE` and the timer of the runtime, over the one stream
//! type [`Stream`], a TCP or a Unix socket or a socket inside TLS, so the
//! connection is built once for every listener.

use core::{
    cell::RefCell,
    time::Duration, //
};
use std::{
    env,
    ffi::OsString,
    hash::{
        BuildHasher,
        RandomState, //
    },
    io,
    process::ExitCode,
    rc::Rc, //
};

use aosabi_core::{
    App,
    AppConfig,
    BootError,
    BootedApp,
    Builtins,
    ExecuteConfigArgs,
    HandlerOptions,
    IntoApp as _,
    IntoConfig,
    IntoEntry,
    Listen,
    PluginError,
    PluginList,
    PluginSlot,
    Resolvers,
    Role,
    Strategy,
    boot,
    cache_budget,
    clock::install_clock,
    current_role,
    execute::promise::agent::HostClock,
    execute_config,
    glue::resolve_type::ResolveTypeRule,
    net::{
        Fetch,
        install_fetch, //
    },
    random::{
        self,
        DevUrandom,
        install_randomness, //
    }, //
};
use hyper::server::conn::node1::{
    Builder,
    NodeConfig, //
};

use crate::{
    build_step::{
        BUILD_ENV,
        config_abort,
        load_app,
        load_config,
        run_app_build_step,
        run_build_step, //
    },
    listener::TcpListen,
    modes::{
        self,
        EX_SOFTWARE,
        EX_USAGE,
        USAGE, //
    },
    node_handler::NodeHandler,
    protocols::{
        Stream,
        TlsConfig, //
    },
    runtime::{
        LocalBoxFuture,
        Reactor,
        Runtime,
        hyper_rt::RuntimeTimer,
        spawn_local, //
    },
    use_node_server::{
        NodeServerOptions,
        PLUGIN_NAME,
        port_rule,
        use_node_server, //
    },
};

/// What the build step of an application wrote, which the server binary
/// embeds.
#[derive(Clone, Copy, Debug)]
pub struct BuildOutput {
    /// The schema text, `schema.graphql`.
    pub schema: &'static str,
    /// The type resolution of every union and interface, which the build
    /// writes beside the schema.
    pub resolve_types: &'static [ResolveTypeRule],
    /// The package name of the application crate, which names the socket of
    /// HTTP(Unix Socket).
    pub name: &'static str,
}

/// A serving driver the facade hands in, which answers a listener when its
/// switch is on.
pub type Driver = fn(&BuildOutput) -> io::Result<Option<Box<dyn Listen>>>;

/// A driver of the network capability the facade hands in, which answers the
/// driver of the runtime thread.
pub type FetchDriver = fn() -> io::Result<Rc<dyn Fetch>>;

/// The capabilities the facade hands the boot, each installed before the first pass.
///
/// A service asks its context for a capability rather than assuming it, so the
/// card decides what it hands over and the boot installs it on the runtime
/// thread before any setup runs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    /// The driver of the network capability, `None` for a card that hands
    /// none, where an outbound call answers `FetchError::NoNetwork`.
    pub fetch: Option<FetchDriver>,
}

/// The status node exits with on an uncaught error, which the rejected top level
/// await of a generated entry is.
const EXIT_UNCAUGHT: u8 = 1;

/// The period of the timer that holds a worker open, `1 << 30` milliseconds,
/// the delay of the referenced interval of `.aosabi/worker.mjs`.
const WORKER_HOLD: Duration = Duration::from_millis(1 << 30);

thread_local! {
    /// The TLS configuration the boot read, which the serving plugin binds its
    /// listener with.
    static BOOT_TLS: RefCell<Option<Rc<TlsConfig>>> = const { RefCell::new(None) };
}

/// Returns the TLS configuration the boot read, `None` for a plain port.
#[must_use]
pub fn boot_tls() -> Option<Rc<TlsConfig>> {
    BOOT_TLS.with(|tls| tls.borrow().clone())
}

/// Boots the app of `entry` with the configuration of `config` and serves it,
/// the boot the generated entry of a project with `aosabi.config.rs` runs.
///
/// Under the run role `worker` the boot is the one of the worker entry, and
/// `capabilities` are installed before the first pass under every role.
#[must_use]
pub fn serve_with<E: IntoEntry, C: IntoConfig>(
    entry: fn() -> E,
    config: fn() -> C,
    build: BuildOutput,
    builtins: Builtins,
    drivers: &'static [Driver],
    capabilities: Capabilities,
) -> ExitCode {
    let parts = Parts {
        builtins,
        drivers,
        capabilities,
    };
    run(Split(entry, config), &build, parts, None)
}

/// Boots the app of `entry` with the configuration the app carries and
/// serves it, the boot the generated entry of a project without
/// `aosabi.config.rs` runs.
///
/// The entry runs under the abort hook of a configuration, so an entry that
/// answers an error or panics prints the abort message with its cause and
/// exits 1, under every role and in the build step. An app that carries no
/// configuration boots with the default one, as under [`serve_with`] with
/// `AppConfig::default`.
#[must_use]
pub fn serve_app_with<E: IntoEntry>(
    entry: fn() -> E,
    build: BuildOutput,
    builtins: Builtins,
    drivers: &'static [Driver],
    capabilities: Capabilities,
) -> ExitCode {
    let parts = Parts {
        builtins,
        drivers,
        capabilities,
    };
    run(Carried(entry), &build, parts, None)
}

/// [`serve_with`] over `reactor`, which the probe of design B' calls to serve a
/// twin through the poll reactor on Linux.
#[doc(hidden)]
#[must_use]
pub fn serve_with_reactor<E: IntoEntry, C: IntoConfig>(
    entry: fn() -> E,
    config: fn() -> C,
    build: BuildOutput,
    builtins: Builtins,
    drivers: &'static [Driver],
    capabilities: Capabilities,
    reactor: Box<dyn Reactor>,
) -> ExitCode {
    let parts = Parts {
        builtins,
        drivers,
        capabilities,
    };
    run(Split(entry, config), &build, parts, Some(reactor))
}

/// Where a boot takes its app and the configuration it boots with from.
///
/// [`Split`] is the source of [`serve_with`], an entry and a configuration
/// of a function of its own, and [`Carried`] the source of
/// [`serve_app_with`], an entry whose app carries its configuration. Each is
/// a type of its own, so the boot is monomorphised once per source and a
/// binary links the loading of the source its main names alone: the main of
/// `aosabi::serve` links no [`load_app`] and no [`run_app_build_step`], and
/// the main of `aosabi::serve_app` no [`load_config`] and no
/// [`run_build_step`].
trait Source: Copy {
    /// What the entry answers.
    type Entry: IntoEntry;

    /// Returns the entry, which the mode `--requests` answers through.
    fn entry(self) -> fn() -> Self::Entry;

    /// Runs the build step over the configuration of the source.
    fn build_step(self, runtime: &Runtime) -> ExitCode;

    /// Loads the app and the configuration the boot runs with.
    ///
    /// # Errors
    ///
    /// Answers the exit of a configuration that throws at load for an entry
    /// that answers an error, after the abort message with the text of that
    /// error.
    async fn load(self) -> Result<(App, AppConfig), ExitCode>;
}

/// The app of an entry and the configuration of a function of its own,
/// `aosabi::serve` under the hidden feature `mirror` of the facade.
struct Split<E, C>(fn() -> E, fn() -> C);

impl<E, C> Clone for Split<E, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E, C> Copy for Split<E, C> {}

impl<E: IntoEntry, C: IntoConfig> Source for Split<E, C> {
    type Entry = E;

    fn entry(self) -> fn() -> E {
        self.0
    }

    fn build_step(self, runtime: &Runtime) -> ExitCode {
        run_build_step(runtime, self.1)
    }

    /// The configuration of the function loads first under the abort hook
    /// and the app of the entry after it, as the generated entry of the
    /// reference imports the configuration first, and a configuration the
    /// app carries beside it is left out.
    async fn load(self) -> Result<(App, AppConfig), ExitCode> {
        let Self(entry, config) = self;
        let config = load_config(config).await;
        let mut app = match entry().into_entry() {
            Ok(app) => app.into_app(),
            Err(cause) => return Err(config_abort(&cause)),
        };
        let _carried = app.take_config();
        Ok((app, config))
    }
}

/// The app of an entry with the configuration it carries,
/// `aosabi::serve_app`.
struct Carried<E>(fn() -> E);

impl<E> Clone for Carried<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E> Copy for Carried<E> {}

impl<E: IntoEntry> Source for Carried<E> {
    type Entry = E;

    fn entry(self) -> fn() -> E {
        self.0
    }

    fn build_step(self, runtime: &Runtime) -> ExitCode {
        run_app_build_step(runtime, self.0)
    }

    /// The entry runs under the abort hook, and the app answers with the
    /// configuration it carries.
    async fn load(self) -> Result<(App, AppConfig), ExitCode> {
        load_app(self.0)
    }
}

/// What the card hands the boot beside the app and its configuration.
struct Parts {
    /// The constructors of the built in plugins outside the core.
    builtins: Builtins,
    /// The serving drivers of the card.
    drivers: &'static [Driver],
    /// The capabilities of the card.
    capabilities: Capabilities,
}

/// Serves `io` through the Node connection of the fork for `app`, inside TLS
/// when `secure`.
#[must_use]
pub fn serve_stream(io: Stream, app: Rc<BootedApp>, secure: bool) -> LocalBoxFuture<'static, ()> {
    let mut builder = Builder::new(NodeConfig::NODE);
    if let Ok(timer) = RuntimeTimer::current() {
        builder.timer(timer);
    }
    let connection = builder.serve_connection(io, NodeHandler::new(app, secure));
    Box::pin(async move {
        // The end of a connection is its whole outcome, and Node logs none.
        let _ = connection.await;
    })
}

/// Runs the seven steps, or the boot of the worker entry under the role `worker`.
fn run<S: Source>(
    source: S,
    build: &BuildOutput,
    parts: Parts,
    reactor: Option<Box<dyn Reactor>>,
) -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => {}
        [flag] if flag == "--print-schema" => return modes::print_schema(build),
        [flag, path] if flag == "--requests" => {
            return modes::answer_requests(source.entry(), build, path);
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(EX_USAGE);
        }
    }
    let runtime = match reactor {
        Some(reactor) => Runtime::with_reactor(reactor),
        None => match Runtime::new() {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(EX_SOFTWARE);
            }
        },
    };
    if env::var_os(BUILD_ENV).is_some_and(|value| value == "1") {
        return source.build_step(&runtime);
    }
    install_randomness(Box::new(DevUrandom));
    seed_keys();
    install_clock(runtime.clock());
    if current_role() == Role::Worker {
        return runtime.block_on(boot_worker(source, parts));
    }
    if let Err(code) = read_tls() {
        return code;
    }
    let clock: Rc<dyn HostClock> = runtime.clock();
    runtime.block_on(boot_and_serve(source, build, parts, clock))
}

/// Keys the keyed hashes of the runtime thread from the randomness of the host.
///
/// The cache of parsed documents hashes the text of a request with the state
/// `aosabi_json::value::KeyState` takes when the cache is made, the key
/// [`aosabi_json::value::seed`] stores for the thread or the fixed key, so the
/// boot stores one before the first pass makes the cache. The key is sixteen
/// bytes of the randomness the boot installed, and where that source cannot be
/// read, two words the `RandomState` of the standard library hashes under the
/// key it draws from the operating system, so the thread never keeps the fixed
/// key. A thread keyed before keeps its key, which `seed` answers.
fn seed_keys() {
    let mut key = [0; 16];
    if random::fill(&mut key).is_err() {
        let state = RandomState::new();
        let (low, high) = key.split_at_mut(8);
        low.copy_from_slice(&state.hash_one(0_u8).to_le_bytes());
        high.copy_from_slice(&state.hash_one(1_u8).to_le_bytes());
    }
    // A key stored on this thread before stays the key.
    let _ = aosabi_json::value::seed(key);
}

/// Installs the capabilities `capabilities` names on the runtime thread.
///
/// A driver that fails to start prints its error, and the boot exits 70, as
/// it does for a serving driver.
fn install_capabilities(capabilities: Capabilities) -> Result<(), ExitCode> {
    if let Some(driver) = capabilities.fetch {
        match driver() {
            Ok(fetch) => install_fetch(fetch),
            Err(error) => {
                eprintln!("{error}");
                return Err(ExitCode::from(EX_SOFTWARE));
            }
        }
    }
    Ok(())
}

/// Prints the failure of a pass as node reports the rejected top level await
/// of the generated entry, and answers its status.
///
/// The two lines are the ones of node's report that carry the reference's text:
/// the loader and the sort throw a plain `Error`, so the first reads
/// `Error: <message>`, and the cause line follows for a setup that threw, while
/// a cycle carries no cause.
fn boot_failure(error: &PluginError) -> ExitCode {
    eprintln!("Error: {}", error.message);
    if let Some(cause) = error.cause_line() {
        eprintln!("  [cause]: {cause}");
    }
    ExitCode::from(EXIT_UNCAUGHT)
}

/// Boots the app as the worker entry `.aosabi/worker.mjs` does and holds the
/// process open.
///
/// The configuration loaded as the server entry loads it, the pass of the
/// strategy `first` and the pass of the strategy `last` with no handler mounted
/// between them, and the referenced interval. The role of the process is
/// `worker` already, which is how this boot is chosen, where the reference's
/// entry sets it itself.
async fn boot_worker<S: Source>(source: S, parts: Parts) -> ExitCode {
    if let Err(code) = install_capabilities(parts.capabilities) {
        return code;
    }
    let (mut app, mut config) = match source.load().await {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    ensure_node_server(&mut config);
    let last = ExecuteConfigArgs {
        plugins_strategy: Some(Strategy::Last),
    };
    for args in [ExecuteConfigArgs::default(), last] {
        if let Err(error) = execute_config(&mut config, args, &mut app, &parts.builtins).await {
            return boot_failure(&error);
        }
    }
    let (booted, listeners) = BootedApp::new(app);
    let app = Rc::new(booted);
    if let Err(error) = modes::write_boot_report(&app) {
        eprintln!("{error}");
        return ExitCode::from(EX_SOFTWARE);
    }
    for listener in listeners {
        spawn_local(listener.run(Rc::clone(&app)));
    }
    hold_open().await
}

/// Holds the runtime open with a timer of `1 << 30` milliseconds, set again
/// whenever it fires, the twin of `setInterval(() => {}, 1 << 30)`.
///
/// The future never ends, so the process ends on a signal alone.
async fn hold_open() -> ExitCode {
    let timer = match RuntimeTimer::current() {
        Ok(timer) => timer,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(EX_SOFTWARE);
        }
    };
    loop {
        timer.sleep_for(WORKER_HOLD).await;
    }
}

/// Reads the TLS configuration of the environment for the serving plugin.
#[cfg(feature = "tls")]
fn read_tls() -> Result<(), ExitCode> {
    match TlsConfig::from_env() {
        Ok(tls) => {
            BOOT_TLS.with(|slot| *slot.borrow_mut() = tls.map(Rc::new));
            Ok(())
        }
        Err(error) => {
            eprintln!("{error}");
            Err(ExitCode::from(1))
        }
    }
}

/// A build without the feature `tls` reads no TLS configuration.
#[cfg(not(feature = "tls"))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature is the one of the build with the feature tls"
)]
fn read_tls() -> Result<(), ExitCode> {
    Ok(())
}

/// Steps 3 to 7, inside the runtime.
async fn boot_and_serve<S: Source>(
    source: S,
    build: &BuildOutput,
    parts: Parts,
    clock: Rc<dyn HostClock>,
) -> ExitCode {
    if let Err(code) = install_capabilities(parts.capabilities) {
        return code;
    }
    let (app, mut config) = match source.load().await {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    ensure_node_server(&mut config);
    let options = HandlerOptions {
        type_defs: build.schema,
        resolve_types: build.resolve_types,
        graphql: Resolvers::default(),
        host_clock: Some(clock),
        spawner: Some(Rc::new(spawn_local)),
        parser_and_validation_cache_budget: Some(cache_budget::TARGET),
    };
    let booted = boot(app, config, options, &parts.builtins).await;
    let (booted, mut listeners) = match booted {
        Ok(booted) => booted,
        Err(BootError::Plugin(error)) => return boot_failure(&error),
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(EX_SOFTWARE);
        }
    };
    let app = Rc::new(booted);
    if modes::dev_serving() {
        match TcpListen::bind(port_rule(&NodeServerOptions::default()), boot_tls()) {
            Ok(listener) => listeners.push(Box::new(listener)),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(EX_SOFTWARE);
            }
        }
    }
    for driver in parts.drivers {
        match driver(build) {
            Ok(Some(listener)) => listeners.push(listener),
            Ok(None) => {}
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(EX_SOFTWARE);
            }
        }
    }
    if let Err(error) = modes::write_boot_report(&app) {
        eprintln!("{error}");
        return ExitCode::from(EX_SOFTWARE);
    }
    if listeners.is_empty() {
        return ExitCode::SUCCESS;
    }
    for listener in listeners {
        spawn_local(listener.run(Rc::clone(&app)));
    }
    core::future::pending::<()>().await;
    ExitCode::SUCCESS
}

/// Appends `use_node_server()` when no plugin of the list is named
/// `node-server`.
fn ensure_node_server(config: &mut AppConfig) {
    let named = config.plugins.as_ref().is_some_and(|list| {
        list.slots()
            .iter()
            .any(|slot| slot.plugin.name() == Some(PLUGIN_NAME))
    });
    if !named {
        config
            .plugins
            .get_or_insert_with(PluginList::default)
            .0
            .push(PluginSlot::new(use_node_server()));
    }
}
