// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the boot of an app, the loader and the plugin order.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/pylon-handler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `execute_config` and `boot`.
//!
//! One pass of the boot installs the base pipeline and mounts the child tree
//! of the target, applies `config.logger` and builds the plugin list. The list
//! opens with the viewer when `viewer` is `true`, divergence D48. The error
//! logger, the in context plugin and the configured plugins follow. The
//! landing plugin comes next when `landingPage` is `true`, divergence D47, and
//! the introspection refusal when `graphiql` is `false`. The pass filters the
//! list by the strategy of the pass, writing `'first'` onto a plugin that
//! states none, and by the run role, loads the survivors in the order of their
//! dependencies with each setup awaited in turn, writes the built list back
//! onto `config.plugins`, which is quirk `Q01_DUPLICATE_PLUGIN_WRITEBACK`, and
//! stores the configuration on the target. The loader labels a failed setup
//! with the name of the plugin or its index in the sorted list and keeps the
//! cause, and pushes a plugin's middleware after its own setup returned.
//!
//! The built in plugins of the first slot and of the landing page live in the
//! crates `aosabi-viewer` and `aosabi-landing`, which depend on this crate,
//! so a pass takes their constructors as [`Builtins`], which the facade fills.
//!
//! [`boot`] is the sequence the generated server entry runs: the first pass,
//! the GraphQL handler mounted with a pathless `use`, and the pass of the
//! strategy `last`. On a Unix host it installs the randomness of
//! `/dev/urandom` before the first pass, unless the serving layer installed a
//! provider before it.
//!
//! Each pass records on the target what the boot report writes: the array of
//! the pass with the strategy written back, the sorted order, the plugins the
//! loader awaited a setup of and those whose middleware it pushed, and the
//! array written back after the last pass. A plugin of the records is labelled
//! by its identity, its index in `config.plugins` as the boot found it, or for
//! an always on plugin its role and the pass that created it, because a name
//! may be shared or absent and the write back of `Q01` repeats the always on
//! plugins.

use core::fmt;
use std::rc::Rc;

use graphql_over_http::use_disable_introspection;

use crate::{
    app::{
        application::{
            App,
            BootedApp,
            Listen, //
        },
        env_alias,
        handler::{
            HandlerOptions,
            Mounted,
            handler_with_server, //
        },
        plugin_order::topo_sort_plugins,
        route_error::PluginError,
    },
    create_schema::CreateSchemaError,
    logger::configure_logger,
    plugin::{
        AppConfig,
        Plugin,
        PluginList,
        PluginRole,
        PluginSlot,
        Role,
        Strategy,
        from_http, //
    },
    plugins::{
        graphql_error_logger::use_graphql_error_logger,
        in_context::use_in_context, //
    },
};

/// The constructors of the built in plugins whose crates depend on this one.
#[derive(Clone, Copy)]
pub struct Builtins {
    /// A plugin of the host that stands before every plugin of a pass, `None`
    /// for an app of Rust. The module `core.wasm` of the TypeScript option
    /// puts there the plugin that reads the thrown errors of the host at the
    /// end of an execution, so the logger of the errors reads them as a
    /// TypeScript service reads them. It takes no label in the records of the
    /// boot.
    pub first: Option<fn() -> Rc<dyn Plugin>>,
    /// `useViewer()`, the first slot of the list.
    pub viewer: fn() -> Rc<dyn Plugin>,
    /// `useUnhandledRoute()`, the landing page.
    pub unhandled_route: fn() -> Rc<dyn Plugin>,
}

/// The options of a pass, `{pluginsStrategy}`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExecuteConfigArgs {
    /// The strategy of the pass, `'first'` when absent.
    pub plugins_strategy: Option<Strategy>,
}

/// An always on plugin, the role its label in the records of the boot names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    /// `useViewer()`, the first slot.
    Viewer,
    /// `useGraphqlErrorLogger()`.
    GraphqlErrorLogger,
    /// `useInContext()`.
    InContext,
    /// `useUnhandledRoute()`, the landing page.
    Landing,
    /// `useDisableIntrospection()`.
    DisableIntrospection,
}

impl Builtin {
    /// Returns the role as the boot report writes it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::GraphqlErrorLogger => "graphql-error-logger",
            Self::InContext => "in-context",
            Self::Landing => "landing",
            Self::DisableIntrospection => "disable-introspection",
        }
    }
}

/// The label of a plugin in the records of the boot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginLabel {
    /// A plugin of `config.plugins` as the boot found it, by its index there.
    Declared(usize),
    /// An always on plugin, by its role and the pass, counted from 1, that
    /// created it.
    Builtin(Builtin, usize),
}

/// One plugin of a record, its label, its name and its members.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginRecord {
    /// The label.
    pub label: PluginLabel,
    /// The name, `None` for a plugin without one.
    pub name: Option<String>,
    /// The members, [`Plugin::members`].
    pub members: Vec<&'static str>,
}

/// One entry of an array of plugins, with the keys the configuration reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotRecord {
    /// The plugin.
    pub plugin: PluginRecord,
    /// The strategy as the entry holds it.
    pub strategy: Option<Strategy>,
    /// The run roles, `None` for a plugin without `roles`.
    pub roles: Option<Vec<PluginRole>>,
    /// The dependencies, `None` for a plugin without `dependsOn`.
    pub depends_on: Option<Vec<String>>,
}

/// What one pass of the boot did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassRecord {
    /// The strategy of the pass.
    pub strategy: Strategy,
    /// The run role of the process.
    pub role: Role,
    /// The array of the pass with the strategy written back.
    pub array: Vec<SlotRecord>,
    /// The survivors of the filter in the order of their dependencies.
    pub sorted: Vec<PluginRecord>,
    /// The sorted plugins that carry a setup, the setups the loader awaited.
    pub loaded: Vec<PluginRecord>,
    /// The sorted plugins whose middleware the loader pushed, in order.
    pub middleware: Vec<PluginRecord>,
}

/// The records of the passes of a boot.
#[derive(Debug, Default)]
pub struct BootRecord {
    /// One record per pass that completed, in order.
    pub passes: Vec<PassRecord>,
    /// The array the last pass wrote back onto `config.plugins`.
    pub after_boot: Vec<SlotRecord>,
    /// The label of every plugin a pass met, by identity.
    labels: Vec<(*const (), PluginLabel)>,
    /// The GraphQL server the boot mounted, which
    /// [`BootedApp::graphql_server`] hands the road of `GraphTP`, none before
    /// the mount.
    graphql: Option<Mounted>,
    /// The middleware the loader pushed onto the plugin chain of the app, a
    /// pass of the boot and every call of [`load_plugins_middleware`] alike,
    /// which [`BootedApp::plugin_middleware`] hands the road of `GraphTP`.
    plugin_middleware: usize,
}

impl BootRecord {
    /// Returns the label of `slot`, when a pass met its plugin.
    #[must_use]
    pub fn label_of(&self, slot: &PluginSlot) -> Option<PluginLabel> {
        self.label(slot.identity())
    }

    /// Returns the label of the plugin of identity `identity`.
    fn label(&self, identity: *const ()) -> Option<PluginLabel> {
        self.labels
            .iter()
            .find(|(known, _)| *known == identity)
            .map(|(_, label)| *label)
    }

    /// Labels the plugin of identity `identity` with `label`, unless a pass labelled it already.
    fn assign(&mut self, identity: *const (), label: PluginLabel) {
        if self.label(identity).is_none() {
            self.labels.push((identity, label));
        }
    }

    /// Returns the record of `plugin` of identity `identity`.
    fn plugin(&self, identity: *const (), plugin: &dyn Plugin) -> PluginRecord {
        PluginRecord {
            // The pass labels every plugin of its array before it sorts, so
            // the index no array holds never shows.
            label: self
                .label(identity)
                .unwrap_or(PluginLabel::Declared(usize::MAX)),
            name: plugin.name().map(String::from),
            members: plugin.members(),
        }
    }

    /// Returns the record of the entry `slot`.
    fn slot(&self, slot: &PluginSlot) -> SlotRecord {
        let plugin = self.plugin(slot.identity(), slot.plugin.as_ref());
        let depends_on = plugin
            .members
            .contains(&"dependsOn")
            .then(|| slot.plugin.depends_on().to_vec());
        SlotRecord {
            plugin,
            strategy: slot.strategy.get(),
            roles: slot.plugin.roles().map(<[PluginRole]>::to_vec),
            depends_on,
        }
    }
}

/// What the loader did in one pass, the order it sorted and the plugins it pushed.
struct Loaded {
    /// The sorted plugins.
    sorted: Vec<Rc<dyn Plugin>>,
    /// The sorted plugins whose middleware was pushed.
    middleware: Vec<Rc<dyn Plugin>>,
}

/// Returns the run role of the process, `currentRole()`.
///
/// `worker` and `all` as they read, and `web` for any other value, with the
/// role read from `AOSABI_ROLE`, the one the engine reads.
#[must_use]
pub fn current_role() -> Role {
    match env_alias::var("AOSABI_ROLE").as_deref() {
        Some("worker") => Role::Worker,
        Some("all") => Role::All,
        _ => Role::Web,
    }
}

/// Returns whether `plugin` runs under `role`, `roleAllows(role, plugin)`.
///
/// `all` runs every plugin, a plugin with no roles runs everywhere, and
/// otherwise the plugin lists the role.
#[must_use]
pub fn role_allows(role: Role, plugin: &dyn Plugin) -> bool {
    let Some(roles) = plugin.roles() else {
        return true;
    };
    match role {
        Role::All => true,
        Role::Web => roles.contains(&crate::plugin::PluginRole::Web),
        Role::Worker => roles.contains(&crate::plugin::PluginRole::Worker),
    }
}

/// Loads the plugins of a pass into `target`, `loadPluginsMiddleware(plugins, target)`.
///
/// # Errors
///
/// Returns the cycle of the dependencies and the first setup that throws,
/// labelled as the reference labels it.
pub async fn load_plugins_middleware(
    plugins: &[PluginSlot],
    target: &mut App,
) -> Result<(), PluginError> {
    load(plugins, target).await.map(|_| ())
}

/// Loads the plugins of a pass into `target` and answers what the loader did.
async fn load(plugins: &[PluginSlot], target: &mut App) -> Result<Loaded, PluginError> {
    let sorted = topo_sort_plugins(plugins)?;
    let mut pushed = Vec::new();
    for (index, plugin) in sorted.0.iter().enumerate() {
        // Isolate and attribute setup failures.
        if let Err(cause) = plugin.setup(target).await {
            return Err(PluginError::of(plugin.name(), index, cause));
        }
        if let Some(middleware) = plugin.middleware() {
            target.push_plugin_middleware(middleware);
            target.boot_record_mut().plugin_middleware += 1;
            pushed.push(Rc::clone(plugin));
        }
    }
    Ok(Loaded {
        sorted: sorted.0,
        middleware: pushed,
    })
}

/// Runs one pass of the boot over `target`, `executeConfig(config, args, target)`.
///
/// # Errors
///
/// Returns what [`load_plugins_middleware`] returns.
pub async fn execute_config(
    config: &mut AppConfig,
    args: ExecuteConfigArgs,
    target: &mut App,
    builtins: &Builtins,
) -> Result<(), PluginError> {
    // Boot the served root in order, both idempotent: the base pipeline
    // first, then the composed child tree.
    target.install_base_pipeline();
    target.realize();

    configure_logger(config.logger.as_ref());

    let pass = target.boot_record_mut().passes.len() + 1;
    let mut created: Vec<(PluginSlot, Builtin)> = vec![
        (
            PluginSlot::new(use_graphql_error_logger()),
            Builtin::GraphqlErrorLogger,
        ),
        // Always on: `@inContext` is part of the schema.
        (PluginSlot::new(use_in_context()), Builtin::InContext),
    ];
    // The viewer opens the list on `viewer: true` alone, the named divergence
    // D48.
    if config.viewer == Some(true) {
        created.insert(0, (PluginSlot::new((builtins.viewer)()), Builtin::Viewer));
    }
    let mut plugins: Vec<PluginSlot> = builtins
        .first
        .map(|first| PluginSlot::new(first()))
        .into_iter()
        .collect();
    plugins.extend(created.iter().map(|(slot, _)| slot.clone()));
    if let Some(configured) = &config.plugins {
        plugins.extend(configured.slots().iter().cloned());
    }

    // The landing page only on `landingPage: true`, the named divergence D47.
    if config.landing_page == Some(true) {
        let slot = PluginSlot::new((builtins.unhandled_route)());
        plugins.push(slot.clone());
        created.push((slot, Builtin::Landing));
    }

    if config.graphiql == Some(false) {
        // `useDisableIntrospection() as Plugin`, which states no member beyond those of Yoga.
        let slot = PluginSlot::new(from_http(use_disable_introspection()));
        plugins.push(slot.clone());
        created.push((slot, Builtin::DisableIntrospection));
    }
    label_pass(target.boot_record_mut(), config, &created, pass);

    let plugins_strategy = args.plugins_strategy.unwrap_or(Strategy::First);

    // The run role gates which plugins load.
    let role = current_role();

    let survivors: Vec<PluginSlot> = plugins
        .iter()
        .filter(|slot| {
            if slot.strategy.get().is_none() {
                slot.strategy.set(Some(Strategy::First));
            }
            if !role_allows(role, slot.plugin.as_ref()) {
                return false;
            }
            slot.strategy.get() == Some(plugins_strategy)
        })
        .cloned()
        .collect();
    let loaded = load(&survivors, target).await?;

    let record = target.boot_record_mut();
    let done = record_pass(record, plugins_strategy, role, &plugins, &loaded);
    record.after_boot.clone_from(&done.array);
    record.passes.push(done);

    config.plugins = Some(PluginList::new(plugins));

    target.set_config(config.clone());
    Ok(())
}

/// Labels the plugins pass `pass` meets: each plugin of `config.plugins` no
/// pass labelled by its index there, and each always on plugin of `created`
/// by its role and the pass.
fn label_pass(
    record: &mut BootRecord,
    config: &AppConfig,
    created: &[(PluginSlot, Builtin)],
    pass: usize,
) {
    if let Some(configured) = &config.plugins {
        for (index, slot) in configured.slots().iter().enumerate() {
            record.assign(slot.identity(), PluginLabel::Declared(index));
        }
    }
    for (slot, builtin) in created {
        record.assign(slot.identity(), PluginLabel::Builtin(*builtin, pass));
    }
}

/// Returns the record of a pass of `strategy` under `role` over the array
/// `plugins`, whose survivors the loader handled as `loaded` says.
fn record_pass(
    record: &BootRecord,
    strategy: Strategy,
    role: Role,
    plugins: &[PluginSlot],
    loaded: &Loaded,
) -> PassRecord {
    let of =
        |plugin: &Rc<dyn Plugin>| record.plugin(Rc::as_ptr(plugin).cast::<()>(), plugin.as_ref());
    let sorted: Vec<PluginRecord> = loaded.sorted.iter().map(of).collect();
    let loaded_setups = sorted
        .iter()
        .filter(|plugin| plugin.members.contains(&"setup"))
        .cloned()
        .collect();
    PassRecord {
        strategy,
        role,
        array: plugins.iter().map(|slot| record.slot(slot)).collect(),
        sorted,
        loaded: loaded_setups,
        middleware: loaded.middleware.iter().map(of).collect(),
    }
}

/// What fails the boot, a plugin or the schema.
#[derive(Debug)]
pub enum BootError {
    /// A plugin failed during its setup, or the dependencies form a cycle.
    Plugin(PluginError),
    /// The schema of the handler does not build.
    Schema(CreateSchemaError),
}

impl fmt::Display for BootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plugin(error) => fmt::Display::fmt(error, f),
            Self::Schema(error) => write!(
                f,
                "{}",
                aosabi_lang::js_string::to_string_lossy(error.message())
            ),
        }
    }
}

impl core::error::Error for BootError {}

/// Boots `app` with `config` as the generated server entry does.
///
/// The pass of the strategy `first`, the GraphQL handler over the roots of the
/// app mounted on `*`, and the pass of the strategy `last`. The answer is the
/// booted app and the listeners the serving plugins bound, which the serve
/// layer runs.
///
/// # Errors
///
/// Returns the failure of a pass and of the schema build.
pub async fn boot(
    app: App,
    mut config: AppConfig,
    options: HandlerOptions,
    builtins: &Builtins,
) -> Result<(BootedApp, Vec<Box<dyn Listen>>), BootError> {
    install_host_randomness();
    let mut app = app;
    execute_config(
        &mut config,
        ExecuteConfigArgs::default(),
        &mut app,
        builtins,
    )
    .await
    .map_err(BootError::Plugin)?;
    let options = HandlerOptions {
        graphql: app.graphql().clone(),
        ..options
    };
    let (graphql, mounted) = handler_with_server(options, &app).map_err(BootError::Schema)?;
    app.boot_record_mut().graphql = mounted;
    app.use_boxed("*", Rc::new(graphql));
    let last = ExecuteConfigArgs {
        plugins_strategy: Some(Strategy::Last),
    };
    execute_config(&mut config, last, &mut app, builtins)
        .await
        .map_err(BootError::Plugin)?;
    Ok(BootedApp::new(app))
}

impl BootedApp {
    /// Returns the GraphQL server the boot mounted with the spawner of its
    /// host, none for an app no boot mounted one on.
    ///
    /// The road of `GraphTP` in `crate::graphtp::served` calls the hooks and
    /// the engine of this server with no request through [`BootedApp::fetch`].
    // The road of GraphTP is the one caller, and a build without the feature
    // homa holds no road.
    #[cfg_attr(not(feature = "homa"), expect(dead_code))]
    #[must_use]
    pub(crate) fn graphql_server(&self) -> Option<&Mounted> {
        self.boot_record().graphql.as_ref()
    }

    /// Returns how many middleware the loader pushed onto the plugin chain
    /// that the base pipeline dispatches on every request, whether a pass of
    /// the boot or a setup or other code through [`load_plugins_middleware`]
    /// pushed them.
    ///
    /// The road of `GraphTP` in `crate::graphtp::served` serves an app whose
    /// chain is empty, since a call of the road passes no chain.
    // The road of GraphTP is the one caller, and a build without the feature
    // homa holds no road.
    #[cfg_attr(not(feature = "homa"), expect(dead_code))]
    #[must_use]
    pub(crate) fn plugin_middleware(&self) -> usize {
        self.boot_record().plugin_middleware
    }
}

/// Installs `/dev/urandom` as the randomness of the process, unless a provider
/// is installed already.
#[cfg(unix)]
fn install_host_randomness() {
    crate::random::install_randomness(Box::new(crate::random::DevUrandom));
}

/// Installs nothing, since a host that is not Unix hands its own provider.
#[cfg(not(unix))]
fn install_host_randomness() {}
