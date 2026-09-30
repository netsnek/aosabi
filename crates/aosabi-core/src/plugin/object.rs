// SPDX-License-Identifier: MIT
/*
 * aosabi-core: a plugin written as an object literal.
 *
 * No analogue on the reference side as a file.
 */

//! `PluginObject` and the helpers `plugin!` names.
//!
//! A plugin of the reference is most often a factory that returns an object
//! literal. [`PluginObject`] is that literal: every member is a field that is
//! `None` for a member the literal leaves out, and a member it leaves out
//! delegates to the plugin in `inner` when the literal spreads one,
//! `{...other, middleware}`, because Rust has no object spread. The macro
//! `plugin!` writes such a literal and passes each closure through a helper of
//! the module [`__wrap`], which bounds the closure by `Fn` or by `AsyncFn`
//! directly, so rustc gives the closure its parameter types. The members of
//! the literal are its fields that are `Some` and the members of the plugin it
//! spreads, and [`on_execute_done`] writes the most common answer of
//! `onExecute`, a done hook alone.

use core::future::Future;
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
        OnExecuteDone,
        OnExecuteDoneHookResult,
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
use aosabi_http::Context;
use aosabi_json::{
    JsValue,
    into_json::IntoJson, //
};
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

use super::{
    BuildContext,
    BuildController,
    IntoMiddlewareValue,
    IntoPlugin,
    MEMBER_ORDER,
    Plugin,
    PluginMiddleware,
    PluginRole,
    Strategy, //
};
use crate::{
    app::application::{
        App,
        Next, //
    },
    result::Result, //
};

/// A setup member, `setup(app)`.
pub trait SetupFn: 'static {
    /// Runs the setup over `app`.
    fn call<'a>(&'a self, app: &'a mut App) -> LocalBoxFuture<'a, Result<()>>;
}

/// A build member, `build(ctx)`.
pub trait BuildFn: 'static {
    /// Runs the build hook over `ctx`.
    fn call<'a>(
        &'a self,
        ctx: &'a BuildContext,
    ) -> LocalBoxFuture<'a, Result<Box<dyn BuildController>>>;
}

/// What a setup body may give: `()` for `return;` as `docs/authoring.md`
/// section 7 writes it, or `Result<()>`.
pub trait IntoSetupResult {
    /// Returns the result of the setup.
    ///
    /// # Errors
    ///
    /// Returns what the setup threw.
    fn into_setup_result(self) -> Result<()>;
}

impl IntoSetupResult for () {
    fn into_setup_result(self) -> Result<()> {
        Ok(())
    }
}

impl IntoSetupResult for Result<()> {
    fn into_setup_result(self) -> Result<()> {
        self
    }
}

/// The hook `onPluginInit`.
pub type OnPluginInitFn = Box<dyn Fn(&mut OnPluginInit<'_>)>;
/// The hook `onEnveloped`.
pub type OnEnvelopedFn = Box<dyn Fn(&mut OnEnveloped<'_>) -> core::result::Result<(), Value>>;
/// The hook `onSchemaChange`.
pub type OnSchemaChangeFn = Box<dyn Fn(&mut OnSchemaChange<'_>)>;
/// The hook `onContextBuilding`.
pub type OnContextBuildingFn =
    Box<dyn Fn(&mut OnContextBuilding<'_>) -> Option<ContextBuildingHook>>;
/// The hook `onParse`.
pub type OnParseFn =
    Box<dyn Fn(&mut OnParse<'_>) -> core::result::Result<Option<OnParseDoneHook>, Value>>;
/// The hook `onValidate`.
pub type OnValidateFn = Box<dyn Fn(&mut OnValidate<'_>) -> Option<OnValidateDoneHook>>;
/// The hook `onExecute`.
pub type OnExecuteFn =
    Box<dyn Fn(&mut OnExecute<'_>) -> core::result::Result<Option<OnExecuteHooks>, Value>>;
/// The hook `onSubscribe`.
pub type OnSubscribeFn =
    Box<dyn Fn(&mut OnSubscribe<'_>) -> core::result::Result<Option<OnSubscribeHooks>, Value>>;
/// The hook `onRequest` of the server adapter.
pub type OnRequestFn = Box<dyn Fn(&mut OnRequest<'_>) -> core::result::Result<(), Value>>;
/// The hook `onResponse` of the server adapter.
pub type OnResponseFn = Box<dyn Fn(&mut OnResponse<'_>) -> core::result::Result<(), Value>>;
/// The hook `onDispose` of the server adapter.
pub type OnDisposeFn = Box<dyn Fn()>;
/// The hook `onYogaInit`.
pub type OnYogaInitFn = Box<dyn Fn(&mut OnYogaInit<'_>)>;
/// The hook `onRequestParse`.
pub type OnRequestParseFn = Box<
    dyn Fn(&mut OnRequestParse<'_>) -> core::result::Result<Option<OnRequestParseDoneHook>, Value>,
>;
/// The hook `onParams`.
pub type OnParamsFn = Box<dyn Fn(&mut OnParams<'_>) -> core::result::Result<(), Value>>;
/// The hook `onExecutionResult`.
pub type OnExecutionResultFn =
    Box<dyn Fn(&mut OnExecutionResult<'_>) -> core::result::Result<(), Value>>;
/// The hook `onResultProcess`.
pub type OnResultProcessFn =
    Box<dyn Fn(&mut OnResultProcess<'_>) -> core::result::Result<(), Value>>;

/// A plugin written as an object literal.
///
/// Every field is a member of the literal, `None` for a member it leaves out,
/// which then delegates to `inner` when the literal spreads a plugin.
#[derive(Default)]
pub struct PluginObject {
    /// `name`.
    pub name: Option<String>,
    /// `roles`.
    pub roles: Option<Vec<PluginRole>>,
    /// `strategy`.
    pub strategy: Option<Strategy>,
    /// `dependsOn`.
    pub depends_on: Option<Vec<String>>,
    /// `options`.
    pub options: Option<JsValue>,
    /// `middleware`.
    pub middleware: Option<PluginMiddleware>,
    /// `setup`.
    pub setup: Option<Box<dyn SetupFn>>,
    /// `build`.
    pub build: Option<Box<dyn BuildFn>>,
    /// `onPluginInit`.
    pub on_plugin_init: Option<OnPluginInitFn>,
    /// `onEnveloped`.
    pub on_enveloped: Option<OnEnvelopedFn>,
    /// `onSchemaChange`.
    pub on_schema_change: Option<OnSchemaChangeFn>,
    /// `onContextBuilding`.
    pub on_context_building: Option<OnContextBuildingFn>,
    /// `onParse`.
    pub on_parse: Option<OnParseFn>,
    /// `onValidate`.
    pub on_validate: Option<OnValidateFn>,
    /// `onExecute`.
    pub on_execute: Option<OnExecuteFn>,
    /// `onSubscribe`.
    pub on_subscribe: Option<OnSubscribeFn>,
    /// `onRequest`.
    pub on_request: Option<OnRequestFn>,
    /// `onResponse`.
    pub on_response: Option<OnResponseFn>,
    /// `onDispose`.
    pub on_dispose: Option<OnDisposeFn>,
    /// `onYogaInit`.
    pub on_yoga_init: Option<OnYogaInitFn>,
    /// `onRequestParse`.
    pub on_request_parse: Option<OnRequestParseFn>,
    /// `onParams`.
    pub on_params: Option<OnParamsFn>,
    /// `onExecutionResult`.
    pub on_execution_result: Option<OnExecutionResultFn>,
    /// `onResultProcess`.
    pub on_result_process: Option<OnResultProcessFn>,
    /// The plugin a literal spreads, `{...other, middleware}`: a member the
    /// literal does not write delegates to it.
    pub inner: Option<Rc<dyn Plugin>>,
}

impl HookPlugin for PluginObject {
    fn on_plugin_init(&self, p: &mut OnPluginInit<'_>) {
        match (&self.on_plugin_init, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_plugin_init(p),
            (None, None) => {}
        }
    }

    fn on_enveloped(&self, p: &mut OnEnveloped<'_>) -> core::result::Result<(), Value> {
        match (&self.on_enveloped, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_enveloped(p),
            (None, None) => Ok(()),
        }
    }

    fn on_schema_change(&self, p: &mut OnSchemaChange<'_>) {
        match (&self.on_schema_change, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_schema_change(p),
            (None, None) => {}
        }
    }

    fn on_context_building(&self, p: &mut OnContextBuilding<'_>) -> Option<ContextBuildingHook> {
        match (&self.on_context_building, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_context_building(p),
            (None, None) => None,
        }
    }

    fn on_parse(
        &self,
        p: &mut OnParse<'_>,
    ) -> core::result::Result<Option<OnParseDoneHook>, Value> {
        match (&self.on_parse, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_parse(p),
            (None, None) => Ok(None),
        }
    }

    fn on_validate(&self, p: &mut OnValidate<'_>) -> Option<OnValidateDoneHook> {
        match (&self.on_validate, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_validate(p),
            (None, None) => None,
        }
    }

    fn on_execute(
        &self,
        p: &mut OnExecute<'_>,
    ) -> core::result::Result<Option<OnExecuteHooks>, Value> {
        match (&self.on_execute, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_execute(p),
            (None, None) => Ok(None),
        }
    }

    fn on_subscribe(
        &self,
        p: &mut OnSubscribe<'_>,
    ) -> core::result::Result<Option<OnSubscribeHooks>, Value> {
        match (&self.on_subscribe, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_subscribe(p),
            (None, None) => Ok(None),
        }
    }
}

impl ServerAdapterPlugin for PluginObject {
    fn on_request(&self, p: &mut OnRequest<'_>) -> core::result::Result<(), Value> {
        match (&self.on_request, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_request(p),
            (None, None) => Ok(()),
        }
    }

    fn on_response(&self, p: &mut OnResponse<'_>) -> core::result::Result<(), Value> {
        match (&self.on_response, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_response(p),
            (None, None) => Ok(()),
        }
    }

    fn on_dispose(&self) {
        match (&self.on_dispose, &self.inner) {
            (Some(hook), _) => hook(),
            (None, Some(inner)) => inner.on_dispose(),
            (None, None) => {}
        }
    }
}

impl HttpPlugin for PluginObject {
    fn on_yoga_init(&self, p: &mut OnYogaInit<'_>) {
        match (&self.on_yoga_init, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_yoga_init(p),
            (None, None) => {}
        }
    }

    fn on_request_parse(
        &self,
        p: &mut OnRequestParse<'_>,
    ) -> core::result::Result<Option<OnRequestParseDoneHook>, Value> {
        match (&self.on_request_parse, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_request_parse(p),
            (None, None) => Ok(None),
        }
    }

    fn on_params(&self, p: &mut OnParams<'_>) -> core::result::Result<(), Value> {
        match (&self.on_params, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_params(p),
            (None, None) => Ok(()),
        }
    }

    fn on_execution_result(
        &self,
        p: &mut OnExecutionResult<'_>,
    ) -> core::result::Result<(), Value> {
        match (&self.on_execution_result, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_execution_result(p),
            (None, None) => Ok(()),
        }
    }

    fn on_result_process(&self, p: &mut OnResultProcess<'_>) -> core::result::Result<(), Value> {
        match (&self.on_result_process, &self.inner) {
            (Some(hook), _) => hook(p),
            (None, Some(inner)) => inner.on_result_process(p),
            (None, None) => Ok(()),
        }
    }
}

impl Plugin for PluginObject {
    fn name(&self) -> Option<&str> {
        match (&self.name, &self.inner) {
            (Some(name), _) => Some(name),
            (None, Some(inner)) => inner.name(),
            (None, None) => None,
        }
    }

    fn roles(&self) -> Option<&[PluginRole]> {
        match (&self.roles, &self.inner) {
            (Some(roles), _) => Some(roles),
            (None, Some(inner)) => inner.roles(),
            (None, None) => None,
        }
    }

    fn strategy(&self) -> Option<Strategy> {
        match (self.strategy, &self.inner) {
            (Some(strategy), _) => Some(strategy),
            (None, Some(inner)) => inner.strategy(),
            (None, None) => None,
        }
    }

    fn depends_on(&self) -> &[String] {
        match (&self.depends_on, &self.inner) {
            (Some(depends_on), _) => depends_on,
            (None, Some(inner)) => inner.depends_on(),
            (None, None) => &[],
        }
    }

    fn options(&self) -> Option<&JsValue> {
        match (&self.options, &self.inner) {
            (Some(options), _) => Some(options),
            (None, Some(inner)) => inner.options(),
            (None, None) => None,
        }
    }

    fn middleware(&self) -> Option<PluginMiddleware> {
        match (&self.middleware, &self.inner) {
            (Some(middleware), _) => Some(middleware.clone()),
            (None, Some(inner)) => inner.middleware(),
            (None, None) => None,
        }
    }

    fn setup<'a>(&'a self, app: &'a mut App) -> LocalBoxFuture<'a, Result<()>> {
        match (&self.setup, &self.inner) {
            (Some(setup), _) => setup.call(app),
            (None, Some(inner)) => inner.setup(app),
            (None, None) => Box::pin(async { Ok(()) }),
        }
    }

    fn build<'a>(
        &'a self,
        ctx: &'a BuildContext,
    ) -> Option<LocalBoxFuture<'a, Result<Box<dyn BuildController>>>> {
        match (&self.build, &self.inner) {
            (Some(build), _) => Some(build.call(ctx)),
            (None, Some(inner)) => inner.build(ctx),
            (None, None) => None,
        }
    }

    fn members(&self) -> Vec<&'static str> {
        let own = self.own_members();
        let spread = self
            .inner
            .as_ref()
            .map(|inner| inner.members())
            .unwrap_or_default();
        MEMBER_ORDER
            .iter()
            .zip(own)
            .filter(|(member, own)| *own || spread.contains(*member))
            .map(|(member, _)| *member)
            .collect()
    }
}

impl PluginObject {
    /// Returns whether the literal writes each member of [`MEMBER_ORDER`] itself.
    ///
    /// A field that is `Some` is a member whatever it holds, so a name that is
    /// the empty string is one, and `instrumentation` has no field, because
    /// that hook is not ported.
    fn own_members(&self) -> [bool; MEMBER_ORDER.len()] {
        [
            self.name.is_some(),
            self.roles.is_some(),
            self.depends_on.is_some(),
            self.options.is_some(),
            self.middleware.is_some(),
            self.setup.is_some(),
            self.build.is_some(),
            self.on_plugin_init.is_some(),
            self.on_enveloped.is_some(),
            self.on_schema_change.is_some(),
            self.on_parse.is_some(),
            self.on_validate.is_some(),
            self.on_context_building.is_some(),
            self.on_execute.is_some(),
            self.on_subscribe.is_some(),
            self.on_request.is_some(),
            self.on_response.is_some(),
            self.on_dispose.is_some(),
            false,
            self.on_yoga_init.is_some(),
            self.on_request_parse.is_some(),
            self.on_params.is_some(),
            self.on_execution_result.is_some(),
            self.on_result_process.is_some(),
        ]
    }
}

/// The builder of a plugin object, the native form of a plugin literal.
///
/// A plugin of the native layer is one chain of calls, each of which sets one
/// member of the literal:
/// `PluginObject::new("audit").setup(..).middleware(..)` is the form of
/// `{name: 'audit', setup, middleware}`, and `PluginObject::default()` begins
/// a plugin with no name. Each call takes and answers the object by value and
/// stores its member through the helper `plugin!` stores the key of the same
/// name through, so a closure takes its parameter types from the bound of the
/// call, and the members of the plugin are the calls of the chain, which the
/// boot report lists in the order of [`MEMBER_ORDER`] as it lists the keys of
/// a literal. A `setup`, a `build` and a `middleware` written as an `async`
/// closure take the calls `setup_async`, `build_async` and `middleware_async`,
/// the spread `...other` is `inner(other)`, and the roles and the strategy
/// take the enums [`PluginRole`] and [`Strategy`] where the macro reads the
/// strings of the reference.
///
/// A call shares its name with the member it sets and with the method of
/// [`Plugin`] that reads that member. Method resolution takes the call of the
/// builder for an object held by value, so a caller that holds one by value
/// reads a member through the trait, `Plugin::setup(&object, app)`, while an
/// object behind a reference or in an `Rc<dyn Plugin>` reads it as before.
impl PluginObject {
    /// Returns a plugin object named `name`, `{name}`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: __wrap::name(name),
            ..Self::default()
        }
    }

    /// Sets `roles`, the run roles the plugin takes part in.
    #[must_use]
    pub fn roles(mut self, roles: impl IntoIterator<Item = PluginRole>) -> Self {
        self.roles = __wrap::roles(roles);
        self
    }

    /// Sets `strategy`, the pass whose setup runs the plugin.
    #[must_use]
    pub fn strategy(mut self, strategy: Strategy) -> Self {
        self.strategy = __wrap::strategy(strategy);
        self
    }

    /// Sets `dependsOn`, the names of the plugins whose setup runs first.
    #[must_use]
    pub fn depends_on<S: Into<String>>(mut self, names: impl IntoIterator<Item = S>) -> Self {
        self.depends_on = __wrap::depends_on(names);
        self
    }

    /// Sets `options`, the options a plugin factory hands its object.
    #[must_use]
    pub fn options(mut self, options: impl IntoJson) -> Self {
        self.options = __wrap::options(options);
        self
    }

    /// Sets the plugin every member the chain leaves out delegates to, the
    /// spread `...other`.
    #[must_use]
    pub fn inner(mut self, plugin: impl IntoPlugin) -> Self {
        self.inner = __wrap::inner(plugin);
        self
    }

    /// Sets a synchronous `setup`, whose body gives `()` or `Result<()>`.
    #[must_use]
    pub fn setup<F, R>(mut self, f: F) -> Self
    where
        F: Fn(&mut App) -> R + 'static,
        R: IntoSetupResult + 'static,
    {
        self.setup = __wrap::setup(f);
        self
    }

    /// Sets an asynchronous `setup`, `setup: async (app) => {...}`.
    #[must_use]
    pub fn setup_async<F, R>(mut self, f: F) -> Self
    where
        F: AsyncFn(&mut App) -> R + 'static,
        R: IntoSetupResult + 'static,
    {
        self.setup = __wrap::setup_async(f);
        self
    }

    /// Sets a `build` that answers a future.
    #[must_use]
    pub fn build<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(&BuildContext) -> Fut + 'static,
        Fut: Future<Output = Result<Box<dyn BuildController>>> + 'static,
    {
        self.build = __wrap::build(f);
        self
    }

    /// Sets an asynchronous `build`, `build: async (ctx) => {...}`.
    #[must_use]
    pub fn build_async<F>(mut self, f: F) -> Self
    where
        F: AsyncFn(&BuildContext) -> Result<Box<dyn BuildController>> + 'static,
    {
        self.build = __wrap::build_async(f);
        self
    }

    /// Sets a `middleware` closure that answers a future, whose output picks
    /// the response form or the value form of the member as for `plugin!`.
    #[must_use]
    pub fn middleware<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future + 'static,
        Fut::Output: IntoMiddlewareValue,
    {
        self.middleware = __wrap::middleware(f);
        self
    }

    /// Sets an asynchronous `middleware`, `middleware: async (c, next) => {...}`.
    #[must_use]
    pub fn middleware_async<F, R>(mut self, f: F) -> Self
    where
        F: AsyncFn(Context, Next) -> R + 'static,
        R: IntoMiddlewareValue,
    {
        self.middleware = __wrap::middleware_async(f);
        self
    }

    /// Sets the hook `onPluginInit`.
    #[must_use]
    pub fn on_plugin_init<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnPluginInit<'_>) + 'static,
    {
        self.on_plugin_init = __wrap::on_plugin_init(f);
        self
    }

    /// Sets the hook `onEnveloped`.
    #[must_use]
    pub fn on_enveloped<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnEnveloped<'_>) -> core::result::Result<(), Value> + 'static,
    {
        self.on_enveloped = __wrap::on_enveloped(f);
        self
    }

    /// Sets the hook `onSchemaChange`.
    #[must_use]
    pub fn on_schema_change<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnSchemaChange<'_>) + 'static,
    {
        self.on_schema_change = __wrap::on_schema_change(f);
        self
    }

    /// Sets the hook `onContextBuilding`.
    #[must_use]
    pub fn on_context_building<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnContextBuilding<'_>) -> Option<ContextBuildingHook> + 'static,
    {
        self.on_context_building = __wrap::on_context_building(f);
        self
    }

    /// Sets the hook `onParse`.
    #[must_use]
    pub fn on_parse<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnParse<'_>) -> core::result::Result<Option<OnParseDoneHook>, Value> + 'static,
    {
        self.on_parse = __wrap::on_parse(f);
        self
    }

    /// Sets the hook `onValidate`.
    #[must_use]
    pub fn on_validate<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnValidate<'_>) -> Option<OnValidateDoneHook> + 'static,
    {
        self.on_validate = __wrap::on_validate(f);
        self
    }

    /// Sets the hook `onExecute`, whose closure answers its execute hooks, as
    /// `.on_execute(|_| on_execute_done(...))` writes it, or a `Result` of
    /// them.
    #[must_use]
    pub fn on_execute<F, R>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnExecute<'_>) -> R + 'static,
        R: __wrap::IntoExecuteHooks,
    {
        self.on_execute = __wrap::on_execute(f);
        self
    }

    /// Sets the hook `onSubscribe`.
    #[must_use]
    pub fn on_subscribe<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnSubscribe<'_>) -> core::result::Result<Option<OnSubscribeHooks>, Value>
            + 'static,
    {
        self.on_subscribe = __wrap::on_subscribe(f);
        self
    }

    /// Sets the hook `onRequest` of the server adapter.
    #[must_use]
    pub fn on_request<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnRequest<'_>) -> core::result::Result<(), Value> + 'static,
    {
        self.on_request = __wrap::on_request(f);
        self
    }

    /// Sets the hook `onResponse` of the server adapter.
    #[must_use]
    pub fn on_response<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnResponse<'_>) -> core::result::Result<(), Value> + 'static,
    {
        self.on_response = __wrap::on_response(f);
        self
    }

    /// Sets the hook `onDispose` of the server adapter.
    #[must_use]
    pub fn on_dispose<F>(mut self, f: F) -> Self
    where
        F: Fn() + 'static,
    {
        self.on_dispose = __wrap::on_dispose(f);
        self
    }

    /// Sets the hook `onYogaInit`.
    #[must_use]
    pub fn on_yoga_init<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnYogaInit<'_>) + 'static,
    {
        self.on_yoga_init = __wrap::on_yoga_init(f);
        self
    }

    /// Sets the hook `onRequestParse`.
    #[must_use]
    pub fn on_request_parse<F>(mut self, f: F) -> Self
    where
        F: Fn(
                &mut OnRequestParse<'_>,
            ) -> core::result::Result<Option<OnRequestParseDoneHook>, Value>
            + 'static,
    {
        self.on_request_parse = __wrap::on_request_parse(f);
        self
    }

    /// Sets the hook `onParams`.
    #[must_use]
    pub fn on_params<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnParams<'_>) -> core::result::Result<(), Value> + 'static,
    {
        self.on_params = __wrap::on_params(f);
        self
    }

    /// Sets the hook `onExecutionResult`.
    #[must_use]
    pub fn on_execution_result<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnExecutionResult<'_>) -> core::result::Result<(), Value> + 'static,
    {
        self.on_execution_result = __wrap::on_execution_result(f);
        self
    }

    /// Sets the hook `onResultProcess`.
    #[must_use]
    pub fn on_result_process<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut OnResultProcess<'_>) -> core::result::Result<(), Value> + 'static,
    {
        self.on_result_process = __wrap::on_result_process(f);
        self
    }
}

/// Returns the answer of `onExecute` with `done` as its done hook alone.
///
/// The twin of `return {onExecuteDone(payload) {...}}`, the shape every
/// remapping plugin answers, so a hook of `plugin!` writes
/// `on_execute: |_| on_execute_done(...)`, and a hook that also throws answers
/// `Ok(on_execute_done(...))`. The done hook answers `()`, which reads no
/// result of a stream, or what `handle_stream_or_single_execution_result`
/// answers, the hooks that read each result of a stream, as
/// `return handleStreamOrSingleExecutionResult(...)` does in the reference,
/// through [`IntoDoneResult`].
pub fn on_execute_done<F, R>(done: F) -> Option<OnExecuteHooks>
where
    F: FnOnce(&mut OnExecuteDone<'_>) -> R + 'static,
    R: IntoDoneResult,
{
    Some(OnExecuteHooks {
        on_execute_done: Some(Box::new(move |payload| done(payload).into_done_result())),
    })
}

/// What the done hook of [`on_execute_done`] answers.
///
/// `()` answers no hooks of a stream, and an
/// `Option<OnExecuteDoneHookResult>` answers itself, so a done hook returns
/// what the reference's `onExecuteDone` returns.
pub trait IntoDoneResult {
    /// Returns the hooks of a stream the done hook answers, if any.
    fn into_done_result(self) -> Option<OnExecuteDoneHookResult>;
}

impl IntoDoneResult for () {
    fn into_done_result(self) -> Option<OnExecuteDoneHookResult> {
        None
    }
}

impl IntoDoneResult for Option<OnExecuteDoneHookResult> {
    fn into_done_result(self) -> Option<OnExecuteDoneHookResult> {
        self
    }
}

/// The helpers the macro `plugin!` calls, each bounding its closure by `Fn`
/// or by `AsyncFn` directly, so rustc gives the closure its parameter types.
///
/// The module also names [`PluginRole`](crate::plugin::PluginRole) and
/// [`Strategy`](crate::plugin::Strategy) for the literal forms of `roles` and
/// `strategy`, which the macro reads at compile time into those enums.
#[doc(hidden)]
pub mod __wrap {
    use core::future::Future;
    use std::rc::Rc;

    use aosabi_execute::Value;
    use aosabi_hooks::{
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
    use aosabi_http::Context;
    use aosabi_json::{
        JsValue,
        into_json::IntoJson, //
    };
    use graphql_over_http::{
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
        },
    };

    use super::{
        AsyncBuild,
        AsyncSetup,
        BuildFn,
        IntoSetupResult,
        OnContextBuildingFn,
        OnDisposeFn,
        OnEnvelopedFn,
        OnExecuteFn,
        OnExecutionResultFn,
        OnParamsFn,
        OnParseFn,
        OnPluginInitFn,
        OnRequestFn,
        OnRequestParseFn,
        OnResponseFn,
        OnResultProcessFn,
        OnSchemaChangeFn,
        OnSubscribeFn,
        OnValidateFn,
        OnYogaInitFn,
        SetupFn,
        SyncBuild,
        SyncSetup, //
    };
    pub use crate::plugin::{
        PluginRole,
        Strategy, //
    };
    use crate::{
        app::application::{
            App,
            Next, //
        },
        plugin::{
            BuildContext,
            BuildController,
            IntoMiddlewareValue,
            IntoPlugin,
            Plugin,
            PluginMiddleware, //
        },
        result::Result,
    };

    /// What the hook `onExecute` may answer: its execute hooks, which is the
    /// returned object `{onExecuteDone(...) {...}}` of the reference and the
    /// answer of [`on_execute_done`](super::on_execute_done), or a `Result`
    /// of them for a hook that throws.
    pub trait IntoExecuteHooks {
        /// Returns the answer of the hook.
        ///
        /// # Errors
        ///
        /// Returns what the hook threw.
        fn into_execute_hooks(self) -> core::result::Result<Option<OnExecuteHooks>, Value>;
    }

    impl IntoExecuteHooks for Option<OnExecuteHooks> {
        fn into_execute_hooks(self) -> core::result::Result<Option<OnExecuteHooks>, Value> {
            Ok(self)
        }
    }

    impl IntoExecuteHooks for core::result::Result<Option<OnExecuteHooks>, Value> {
        fn into_execute_hooks(self) -> core::result::Result<Option<OnExecuteHooks>, Value> {
            self
        }
    }

    /// The member `name`.
    pub fn name(name: impl Into<String>) -> Option<String> {
        Some(name.into())
    }

    /// The member `roles`.
    pub fn roles(roles: impl IntoIterator<Item = PluginRole>) -> Option<Vec<PluginRole>> {
        Some(roles.into_iter().collect())
    }

    /// The member `strategy`.
    #[must_use]
    pub const fn strategy(strategy: Strategy) -> Option<Strategy> {
        Some(strategy)
    }

    /// The member `dependsOn`.
    pub fn depends_on<S: Into<String>>(names: impl IntoIterator<Item = S>) -> Option<Vec<String>> {
        Some(names.into_iter().map(Into::into).collect())
    }

    /// The member `options`.
    pub fn options(options: impl IntoJson) -> Option<JsValue> {
        Some(options.into_json())
    }

    /// The spread of a plugin, `...other`.
    pub fn inner(plugin: impl IntoPlugin) -> Option<Rc<dyn Plugin>> {
        Some(plugin.into_plugin())
    }

    /// A synchronous `setup`, whose body gives `()` or `Result<()>`.
    pub fn setup<F, R>(f: F) -> Option<Box<dyn SetupFn>>
    where
        F: Fn(&mut App) -> R + 'static,
        R: IntoSetupResult + 'static,
    {
        Some(Box::new(SyncSetup(f)))
    }

    /// An asynchronous `setup`, `setup: async |app| { ... }`.
    pub fn setup_async<F, R>(f: F) -> Option<Box<dyn SetupFn>>
    where
        F: AsyncFn(&mut App) -> R + 'static,
        R: IntoSetupResult + 'static,
    {
        Some(Box::new(AsyncSetup(f)))
    }

    /// A `build` that answers a future.
    pub fn build<F, Fut>(f: F) -> Option<Box<dyn BuildFn>>
    where
        F: Fn(&BuildContext) -> Fut + 'static,
        Fut: Future<Output = Result<Box<dyn BuildController>>> + 'static,
    {
        Some(Box::new(SyncBuild(f)))
    }

    /// An asynchronous `build`, `build: async |ctx| { ... }`.
    pub fn build_async<F>(f: F) -> Option<Box<dyn BuildFn>>
    where
        F: AsyncFn(&BuildContext) -> Result<Box<dyn BuildController>> + 'static,
    {
        Some(Box::new(AsyncBuild(f)))
    }

    /// A `middleware` closure that answers a future, `|c, next| async move { ... }`.
    ///
    /// The output of the future picks the form of the member, the response
    /// form for `Result<Response>` and the value form for `Result<()>` and
    /// `Result<Option<Response>>`.
    pub fn middleware<F, Fut>(f: F) -> Option<PluginMiddleware>
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future + 'static,
        Fut::Output: IntoMiddlewareValue,
    {
        Some(<Fut::Output as IntoMiddlewareValue>::plugin_middleware(f))
    }

    /// An asynchronous `middleware`, `middleware: async |c, next| { ... }`,
    /// whose output picks the form as for [`middleware`].
    pub fn middleware_async<F, R>(f: F) -> Option<PluginMiddleware>
    where
        F: AsyncFn(Context, Next) -> R + 'static,
        R: IntoMiddlewareValue,
    {
        Some(R::plugin_middleware_async(f))
    }

    /// The hook `onPluginInit`.
    pub fn on_plugin_init<F>(f: F) -> Option<OnPluginInitFn>
    where
        F: Fn(&mut OnPluginInit<'_>) + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onEnveloped`.
    pub fn on_enveloped<F>(f: F) -> Option<OnEnvelopedFn>
    where
        F: Fn(&mut OnEnveloped<'_>) -> core::result::Result<(), Value> + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onSchemaChange`.
    pub fn on_schema_change<F>(f: F) -> Option<OnSchemaChangeFn>
    where
        F: Fn(&mut OnSchemaChange<'_>) + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onContextBuilding`.
    pub fn on_context_building<F>(f: F) -> Option<OnContextBuildingFn>
    where
        F: Fn(&mut OnContextBuilding<'_>) -> Option<ContextBuildingHook> + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onParse`.
    pub fn on_parse<F>(f: F) -> Option<OnParseFn>
    where
        F: Fn(&mut OnParse<'_>) -> core::result::Result<Option<OnParseDoneHook>, Value> + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onValidate`.
    pub fn on_validate<F>(f: F) -> Option<OnValidateFn>
    where
        F: Fn(&mut OnValidate<'_>) -> Option<OnValidateDoneHook> + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onExecute`, whose closure answers its execute hooks, as
    /// `on_execute: |_| on_execute_done(|done| ...)` writes it, or a `Result`
    /// of them.
    pub fn on_execute<F, R>(f: F) -> Option<OnExecuteFn>
    where
        F: Fn(&mut OnExecute<'_>) -> R + 'static,
        R: IntoExecuteHooks,
    {
        Some(Box::new(move |execute| f(execute).into_execute_hooks()))
    }

    /// The hook `onSubscribe`.
    pub fn on_subscribe<F>(f: F) -> Option<OnSubscribeFn>
    where
        F: Fn(&mut OnSubscribe<'_>) -> core::result::Result<Option<OnSubscribeHooks>, Value>
            + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onRequest`.
    pub fn on_request<F>(f: F) -> Option<OnRequestFn>
    where
        F: Fn(&mut OnRequest<'_>) -> core::result::Result<(), Value> + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onResponse`.
    pub fn on_response<F>(f: F) -> Option<OnResponseFn>
    where
        F: Fn(&mut OnResponse<'_>) -> core::result::Result<(), Value> + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onDispose`.
    pub fn on_dispose<F>(f: F) -> Option<OnDisposeFn>
    where
        F: Fn() + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onYogaInit`.
    pub fn on_yoga_init<F>(f: F) -> Option<OnYogaInitFn>
    where
        F: Fn(&mut OnYogaInit<'_>) + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onRequestParse`.
    pub fn on_request_parse<F>(f: F) -> Option<OnRequestParseFn>
    where
        F: Fn(
                &mut OnRequestParse<'_>,
            ) -> core::result::Result<Option<OnRequestParseDoneHook>, Value>
            + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onParams`.
    pub fn on_params<F>(f: F) -> Option<OnParamsFn>
    where
        F: Fn(&mut OnParams<'_>) -> core::result::Result<(), Value> + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onExecutionResult`.
    pub fn on_execution_result<F>(f: F) -> Option<OnExecutionResultFn>
    where
        F: Fn(&mut OnExecutionResult<'_>) -> core::result::Result<(), Value> + 'static,
    {
        Some(Box::new(f))
    }

    /// The hook `onResultProcess`.
    pub fn on_result_process<F>(f: F) -> Option<OnResultProcessFn>
    where
        F: Fn(&mut OnResultProcess<'_>) -> core::result::Result<(), Value> + 'static,
    {
        Some(Box::new(f))
    }

    /// Returns a future that is ready with `value`, for a member written as a
    /// plain value.
    pub fn ready<T: 'static>(value: T) -> LocalBoxFuture<'static, T> {
        Box::pin(async move { value })
    }
}

/// A synchronous setup.
struct SyncSetup<F>(F);

impl<F, R> SetupFn for SyncSetup<F>
where
    F: Fn(&mut App) -> R + 'static,
    R: IntoSetupResult + 'static,
{
    fn call<'a>(&'a self, app: &'a mut App) -> LocalBoxFuture<'a, Result<()>> {
        let result = (self.0)(app).into_setup_result();
        Box::pin(async move { result })
    }
}

/// An asynchronous setup.
struct AsyncSetup<F>(F);

impl<F, R> SetupFn for AsyncSetup<F>
where
    F: AsyncFn(&mut App) -> R + 'static,
    R: IntoSetupResult + 'static,
{
    fn call<'a>(&'a self, app: &'a mut App) -> LocalBoxFuture<'a, Result<()>> {
        Box::pin(async move { (self.0)(app).await.into_setup_result() })
    }
}

/// A build hook that answers a future.
struct SyncBuild<F>(F);

impl<F, Fut> BuildFn for SyncBuild<F>
where
    F: Fn(&BuildContext) -> Fut + 'static,
    Fut: Future<Output = Result<Box<dyn BuildController>>> + 'static,
{
    fn call<'a>(
        &'a self,
        ctx: &'a BuildContext,
    ) -> LocalBoxFuture<'a, Result<Box<dyn BuildController>>> {
        Box::pin((self.0)(ctx))
    }
}

/// An asynchronous build hook.
struct AsyncBuild<F>(F);

impl<F> BuildFn for AsyncBuild<F>
where
    F: AsyncFn(&BuildContext) -> Result<Box<dyn BuildController>> + 'static,
{
    fn call<'a>(
        &'a self,
        ctx: &'a BuildContext,
    ) -> LocalBoxFuture<'a, Result<Box<dyn BuildController>>> {
        Box::pin(async move { (self.0)(ctx).await })
    }
}
