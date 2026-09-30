// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the plugin contract.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! `Plugin`.
//!
//! An Aosabi plugin is the plugin type of Yoga with eight members of its own,
//! `name`, `roles`, `strategy`, `dependsOn`, `options`, `middleware`, `setup`
//! and `build`. [`Plugin`] is one trait over the three halves,
//! `Plugin: HttpPlugin`, whose envelop, adapter and Yoga hooks have bodies
//! that do nothing, so a plugin writes the members it has as a plugin object
//! of the reference names them. `setup` takes the app mutably and answers a
//! future, because the reference awaits a setup that may be asynchronous. A
//! plugin is `'static` and neither `Send` nor `Sync`, held in an `Rc` whose
//! address is its identity for the sort of `plugin_order`.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `object` | none, the object literal form of a plugin | [`PluginObject`], [`SetupFn`], [`BuildFn`], [`IntoSetupResult`], [`on_execute_done`], [`IntoDoneResult`] and the helpers `plugin!` names |
//! | `middleware` | `core/index.ts:118`, `app/index.ts:334-343` | [`PluginMiddleware`], [`ValueMiddlewareFn`] and [`IntoMiddlewareValue`] |
//! | `config` | `core/index.ts:125-160` | [`AppConfig`], [`PluginList`] and [`IntoConfig`] |

pub mod config;
pub mod middleware;
pub mod object;

use core::{
    cell::Cell,
    panic::Location, //
};
use std::rc::Rc;

use aosabi_execute::Value;
use aosabi_hooks::{
    HookPlugin,
    LocalBoxFuture,
    plugin::{
        ContextBuildingHook,
        OnContextBuilding,
        OnEnveloped,
        OnExecute,
        OnExecuteHooks,
        OnParse,
        OnParseDoneHook,
        OnPluginInit,
        OnSchemaChange,
        OnSubscribe,
        OnSubscribeHooks,
        OnValidate,
        OnValidateDoneHook, //
    }, //
};
use aosabi_json::JsValue;
use graphql_over_http::{
    HttpPlugin,
    ServerAdapterPlugin,
    plugin::{
        OnExecutionResult,
        OnParams,
        OnRequestParse,
        OnRequestParseDoneHook,
        OnResultProcess,
        OnYogaInit, //
    },
    server_adapter::{
        OnRequest,
        OnResponse, //
    }, //
};

pub use self::{
    config::{
        AppConfig,
        IntoConfig,
        PluginList, //
    },
    middleware::{
        IntoMiddlewareValue,
        PluginMiddleware,
        ValueMiddlewareFn, //
    },
    object::{
        BuildFn,
        IntoDoneResult,
        IntoSetupResult,
        PluginObject,
        SetupFn,
        on_execute_done, //
    },
};
use crate::{
    app::application::App,
    result::Result, //
};

/// The member names of a plugin object in their canonical order.
///
/// The members of its own come in the order of their declaration at
/// `core/index.ts:93-122`, then the hooks of envelop, of the server adapter
/// and of Yoga in the order of `docs/plugins.md` sections 3.1 to 3.3, which is
/// the order the reference half of the boot report lists them in. The boot
/// writes a strategy onto every plugin, so `strategy` is no member and the
/// boot report carries it as a field of its own.
pub const MEMBER_ORDER: [&str; 24] = [
    "name",
    "roles",
    "dependsOn",
    "options",
    "middleware",
    "setup",
    "build",
    "onPluginInit",
    "onEnveloped",
    "onSchemaChange",
    "onParse",
    "onValidate",
    "onContextBuilding",
    "onExecute",
    "onSubscribe",
    "onRequest",
    "onResponse",
    "onDispose",
    "instrumentation",
    "onYogaInit",
    "onRequestParse",
    "onParams",
    "onExecutionResult",
    "onResultProcess",
];

/// The coarse phase of a plugin relative to the mount of the GraphQL
/// handler, `strategy` of `core/index.ts:105-107`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// `'first'`, before the handler.
    First,
    /// `'last'`, after the handler.
    Last,
}

/// A run role a plugin takes part in, `PluginRole` of `core/index.ts:85`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginRole {
    /// `'web'`, the process that serves HTTP.
    Web,
    /// `'worker'`, the process that consumes queues.
    Worker,
}

/// The run role of the process, `core/index.ts:84`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// `'web'`, the default.
    Web,
    /// `'worker'`.
    Worker,
    /// `'all'`, both sides in one process.
    All,
}

/// The context every `build` hook is handed, `BuildContext` of `core/index.ts:67-73`.
#[derive(Clone, Debug, Default)]
pub struct BuildContext {
    /// `'build'` or `'dev'`.
    pub mode: String,
    /// The root of the project.
    pub root: String,
    /// The source directory.
    pub src_dir: String,
    /// The output directory.
    pub out_dir: String,
    /// The schema text an earlier stage wrote, `out.sdl`.
    pub out_sdl: Option<String>,
    /// The client directory an earlier stage wrote, `out.clientDir`.
    pub out_client_dir: Option<String>,
}

/// The watch handle a `build` hook answers, `BuildController` of `core/index.ts:55-60`.
pub trait BuildController {
    /// Rebuilds, `rebuild()`.
    fn rebuild(&self) -> LocalBoxFuture<'_, Result<()>>;
    /// Stops watching, `dispose()`.
    fn dispose(&self) -> LocalBoxFuture<'_, Result<()>>;
    /// Aborts a rebuild a newer change superseded, `cancel?()`.
    fn cancel(&self) -> Option<LocalBoxFuture<'_, Result<()>>> {
        None
    }
}

/// An Aosabi plugin.
///
/// Every member has a body that stands for an absent member, and the setup of
/// a plugin without one is a future that is ready at once, which the loader
/// awaits as the reference awaits `undefined` of a missing setup.
pub trait Plugin: HttpPlugin {
    /// The identity used for the order of the dependencies and the label of a
    /// failure, `name`.
    fn name(&self) -> Option<&str> {
        None
    }

    /// The run roles the plugin takes part in, `roles`, every role when absent.
    fn roles(&self) -> Option<&[PluginRole]> {
        None
    }

    /// The phase of the plugin, `strategy`.
    fn strategy(&self) -> Option<Strategy> {
        None
    }

    /// The names of the plugins this one loads after, `dependsOn`.
    fn depends_on(&self) -> &[String] {
        &[]
    }

    /// The options of the plugin, which tooling reads, `options`.
    fn options(&self) -> Option<&JsValue> {
        None
    }

    /// The middleware the loader pushes after the setup, `middleware`.
    fn middleware(&self) -> Option<PluginMiddleware> {
        None
    }

    /// The setup the loader awaits with the app, `setup(app)`.
    fn setup<'a>(&'a self, _app: &'a mut App) -> LocalBoxFuture<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }

    /// The one build side hook, `build(ctx)`, `None` for a plugin without one.
    fn build<'a>(
        &'a self,
        _ctx: &'a BuildContext,
    ) -> Option<LocalBoxFuture<'a, Result<Box<dyn BuildController>>>> {
        None
    }

    /// The names of the members the plugin object of the reference carries,
    /// in the order of [`MEMBER_ORDER`].
    ///
    /// A member is a key the object holds, `key in plugin`, whatever its
    /// value, so a name that is the empty string is a member here while the
    /// sort counts the plugin as unnamed. The boot report and `inspect` read
    /// the list, and a plugin that implements the trait by hand names its
    /// members in the crate that defines it.
    fn members(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

/// Where a plugin of the list is registered, which the boot report and the
/// authoring check read.
///
/// A configuration of the derive path lists its plugins in `plugins!`, whose
/// element gives its text and its line. The builder of an app registers a
/// plugin by a call of `.plugin(..)` under `#[track_caller]`, whose place
/// `core::panic::Location::caller()` gives with the standard library and no
/// macro, so the boot report names that plugin by its name and by the file and
/// the line of its call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginSource {
    /// An element of `plugins!`.
    Element {
        /// The text of the element.
        text: &'static str,
        /// The source line of the element, counted from 1.
        line: u32,
    },
    /// A call of `.plugin(..)` of the builder of an app, at its place.
    Call(&'static Location<'static>),
}

/// One entry of `config.plugins`, the plugin and the strategy the filter
/// writes back.
///
/// The reference writes `'first'` onto a plugin object that states no
/// strategy and writes the built array back onto the configuration (quirk
/// `Q01_DUPLICATE_PLUGIN_WRITEBACK`). The strategy lives in a cell a clone of
/// the slot shares, so the write of one pass is read by the next as the
/// write on the object is.
#[derive(Clone)]
pub struct PluginSlot {
    /// The plugin.
    pub plugin: Rc<dyn Plugin>,
    /// The strategy, the plugin's own until the filter writes one.
    pub strategy: Rc<Cell<Option<Strategy>>>,
    /// Where the plugin is registered, the element of `plugins!` or the call
    /// of `.plugin(..)`, `None` for a plugin the boot adds.
    pub source: Option<PluginSource>,
}

impl PluginSlot {
    /// Returns the slot of `plugin`, with the strategy the plugin states.
    #[must_use]
    pub fn new(plugin: Rc<dyn Plugin>) -> Self {
        let strategy = plugin.strategy();
        Self {
            plugin,
            strategy: Rc::new(Cell::new(strategy)),
            source: None,
        }
    }

    /// Returns the slot of `plugin` at the element `text` on `line` of
    /// `plugins!`.
    #[must_use]
    pub fn of(plugin: impl IntoPlugin, text: &'static str, line: u32) -> Self {
        let mut slot = Self::new(plugin.into_plugin());
        slot.source = Some(PluginSource::Element { text, line });
        slot
    }

    /// Returns the slot of `plugin` that a call of `.plugin(..)` at `called`
    /// registers.
    #[must_use]
    pub fn called(plugin: impl IntoPlugin, called: &'static Location<'static>) -> Self {
        let mut slot = Self::new(plugin.into_plugin());
        slot.source = Some(PluginSource::Call(called));
        slot
    }

    /// Returns the address of the plugin, its identity.
    #[must_use]
    pub fn identity(&self) -> *const () {
        Rc::as_ptr(&self.plugin).cast::<()>()
    }
}

impl core::fmt::Debug for PluginSlot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PluginSlot")
            .field("name", &self.plugin.name())
            .field("strategy", &self.strategy.get())
            .field("source", &self.source)
            .finish()
    }
}

/// A plugin of Yoga as an Aosabi plugin with no member of its own, the cast
/// `plugin as Plugin` of the reference.
///
/// Its members are the hooks the plugin of Yoga names, since the cast adds
/// no key to the object.
struct HttpOnly(Rc<dyn HttpPlugin>);

impl HookPlugin for HttpOnly {
    fn on_plugin_init(&self, p: &mut OnPluginInit<'_>) {
        self.0.on_plugin_init(p);
    }

    fn on_enveloped(&self, p: &mut OnEnveloped<'_>) -> core::result::Result<(), Value> {
        self.0.on_enveloped(p)
    }

    fn on_schema_change(&self, p: &mut OnSchemaChange<'_>) {
        self.0.on_schema_change(p);
    }

    fn on_context_building(&self, p: &mut OnContextBuilding<'_>) -> Option<ContextBuildingHook> {
        self.0.on_context_building(p)
    }

    fn on_parse(
        &self,
        p: &mut OnParse<'_>,
    ) -> core::result::Result<Option<OnParseDoneHook>, Value> {
        self.0.on_parse(p)
    }

    fn on_validate(&self, p: &mut OnValidate<'_>) -> Option<OnValidateDoneHook> {
        self.0.on_validate(p)
    }

    fn on_execute(
        &self,
        p: &mut OnExecute<'_>,
    ) -> core::result::Result<Option<OnExecuteHooks>, Value> {
        self.0.on_execute(p)
    }

    fn on_subscribe(
        &self,
        p: &mut OnSubscribe<'_>,
    ) -> core::result::Result<Option<OnSubscribeHooks>, Value> {
        self.0.on_subscribe(p)
    }
}

impl ServerAdapterPlugin for HttpOnly {
    fn on_request(&self, p: &mut OnRequest<'_>) -> core::result::Result<(), Value> {
        self.0.on_request(p)
    }

    fn on_response(&self, p: &mut OnResponse<'_>) -> core::result::Result<(), Value> {
        self.0.on_response(p)
    }

    fn on_dispose(&self) {
        self.0.on_dispose();
    }
}

impl HttpPlugin for HttpOnly {
    fn on_yoga_init(&self, p: &mut OnYogaInit<'_>) {
        self.0.on_yoga_init(p);
    }

    fn on_request_parse(
        &self,
        p: &mut OnRequestParse<'_>,
    ) -> core::result::Result<Option<OnRequestParseDoneHook>, Value> {
        self.0.on_request_parse(p)
    }

    fn on_params(&self, p: &mut OnParams<'_>) -> core::result::Result<(), Value> {
        self.0.on_params(p)
    }

    fn on_execution_result(
        &self,
        p: &mut OnExecutionResult<'_>,
    ) -> core::result::Result<(), Value> {
        self.0.on_execution_result(p)
    }

    fn on_result_process(&self, p: &mut OnResultProcess<'_>) -> core::result::Result<(), Value> {
        self.0.on_result_process(p)
    }
}

impl Plugin for HttpOnly {
    /// Returns the hooks of the plugin of Yoga in the order of [`MEMBER_ORDER`].
    fn members(&self) -> Vec<&'static str> {
        let hooks = self.0.hooks();
        MEMBER_ORDER
            .into_iter()
            .filter(|member| hooks.contains(member))
            .collect()
    }
}

/// Returns the plugin of Yoga `plugin` as an Aosabi plugin with no member of
/// its own.
#[must_use]
pub fn from_http(plugin: Rc<dyn HttpPlugin>) -> Rc<dyn Plugin> {
    Rc::new(HttpOnly(plugin))
}

/// A value that is a plugin, a type of the plugin contract or a plugin held
/// already.
pub trait IntoPlugin {
    /// Returns the plugin behind a shared pointer.
    fn into_plugin(self) -> Rc<dyn Plugin>;
}

impl<P: Plugin> IntoPlugin for P {
    fn into_plugin(self) -> Rc<dyn Plugin> {
        Rc::new(self)
    }
}

impl IntoPlugin for Rc<dyn Plugin> {
    fn into_plugin(self) -> Rc<dyn Plugin> {
        self
    }
}
