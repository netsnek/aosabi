// SPDX-License-Identifier: MIT AND Apache-2.0
// Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/authz.ts,
// Apache-2.0. Changed by the Aosabi authors.

//! `get_principal`, `authorize`, `require_role`, `use_identity` and the
//! forbidden mapping.
//!
//! The capability tier of authorization asks whether the principal of a
//! request may run an operation. It needs the principal alone and no data
//! layer, so a resolver, a route, a middleware and a gate secure themselves
//! over the core and an identity provider. The principal lives in the variable
//! `principal` of the request context, which the identity plugin sets from its
//! provider on every request and [`get_principal`] reads through
//! `getContext()`.
//!
//! A gate returns the throw it makes, `Err` of the value a `ForbiddenError`
//! is, so a caller writes `require_role(&["admin"])?` where the reference
//! calls `requireRole('admin')` and lets it throw. The done hook of the
//! identity plugin maps every error whose original error is a `ForbiddenError`
//! to a `GraphQLError` with the code `FORBIDDEN`, which the masking of Yoga
//! keeps as a client error, so a refusal reaches the client unmasked.

use core::{
    fmt,
    marker::PhantomData, //
};
use std::rc::Rc;

use aosabi_auth_contract::{
    ForbiddenError,
    PRINCIPAL_KEY,
    Principal,
    ProviderFuture,
    has_role,
    provider as contract, //
};
use aosabi_core::{
    Context,
    IntoPlugin,
    Key,
    Next,
    Plugin,
    PluginObject,
    Result,
    Thrown,
    execute::errors::copy_graphql_error,
    get_context,
    on_execute_done,
    original_host,
    plugin::object::__wrap, //
};
use aosabi_hooks::{
    LocalBoxFuture,
    plugin::{
        ExecutionOutcome,
        OnExecuteDone, //
    }, //
};
use aosabi_json::JsObject;
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions, //
};

use crate::provider::IdentityProvider;

/// The typed key of the bound principal, the twin of `c.set(PRINCIPAL_KEY, ...)`.
///
/// The variable holds `None` for public access, as the reference sets the
/// key to `undefined` when the provider answers none.
pub const PRINCIPAL: Key<Option<Principal>> = Key::new(PRINCIPAL_KEY);

/// The name of the identity plugin, `name: 'identity'`.
///
/// The name is the anchor of the order of the plugins, since the data plugin
/// declares `dependsOn: ['identity']`.
const PLUGIN_NAME: &str = "identity";

/// Returns the principal of the current request, or none outside a request.
///
/// The variable `principal` of the context `getContext()` answers, and `None`
/// when no request context is bound, which is the reference's `try` around
/// `getContext()`. The principal is a clone of the bound one, so the caller
/// owns what it reads, as the reference answers a plain object.
#[must_use]
pub fn get_principal() -> Option<Principal> {
    let context = get_context().ok()?;
    context.get(PRINCIPAL).flatten()
}

/// Refuses the call with `ForbiddenError` unless `check` accepts the principal.
///
/// The capability gate that works on the core alone. `check` receives the
/// principal of the current request, or `None` for public access.
///
/// # Errors
///
/// Returns `new ForbiddenError()` as the value a resolver throws when
/// `check` answers `false`.
pub fn authorize<F>(check: F) -> Result<()>
where
    F: FnOnce(Option<&Principal>) -> bool,
{
    let principal = get_principal();
    if check(principal.as_ref()) {
        Ok(())
    } else {
        Err(Thrown::from(ForbiddenError::new()))
    }
}

/// Refuses the call unless the principal holds any of `roles`.
///
/// The sugar `authorize(p => hasRole(p, ...roles))`, so a call with no role
/// refuses every caller.
///
/// # Errors
///
/// Returns `new ForbiddenError()` as the value a resolver throws when the
/// principal is absent or holds none of `roles`.
pub fn require_role(roles: &[&str]) -> Result<()> {
    authorize(|p| has_role(p, roles))
}

/// A provider with its form erased, which the plugin shares with its
/// middleware.
trait DynIdentity {
    /// Returns the future of the principal of `context`, or of what the provider throws.
    fn identify<'a>(&'a self, context: &'a Context) -> ProviderFuture<'a, Thrown>;
}

/// A provider written in the form `M`.
struct Erased<P, M> {
    /// The provider.
    provider: P,
    /// The marker of its form.
    form: PhantomData<fn() -> M>,
}

impl<P, M> DynIdentity for Erased<P, M>
where
    P: IdentityProvider<M>,
{
    fn identify<'a>(&'a self, context: &'a Context) -> ProviderFuture<'a, Thrown> {
        contract::IdentityProvider::<Context, Thrown, M>::identify(&self.provider, context)
    }
}

/// The plugin `useIdentity` returns.
///
/// The plugin is named `identity`, declares no strategy and no roles, binds
/// the principal of every request in its middleware and maps the refusals of
/// an execution in the done hook of its `onExecute`, so its members are
/// `name`, `middleware` and `onExecute`. It turns into a plugin object through
/// [`IntoPlugin`], as the other plugins outside the core do, and a clone
/// shares the provider, so the carrier path that has no onion keeps one to
/// call [`IdentityPlugin::bind`] with.
#[derive(Clone)]
pub struct IdentityPlugin {
    /// The provider with its form erased.
    provider: Rc<dyn DynIdentity>,
}

impl IdentityPlugin {
    /// Binds the principal of `c`, the body of the middleware.
    ///
    /// A path without the onion calls it as the middleware does. The body
    /// awaits the provider and sets the variable `principal` to its answer on
    /// every request, `None` included, so the insertion order of the variables
    /// of the context is the reference's.
    ///
    /// # Errors
    ///
    /// The future returns what the provider threw, which the error mapping of
    /// the root answers for a GraphQL request as for a route.
    #[must_use]
    pub fn bind<'a>(&'a self, c: &'a Context) -> LocalBoxFuture<'a, Result<()>> {
        bind(self.provider.as_ref(), c)
    }
}

impl fmt::Debug for IdentityPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IdentityPlugin")
            .field("name", &PLUGIN_NAME)
            .finish_non_exhaustive()
    }
}

impl IntoPlugin for IdentityPlugin {
    fn into_plugin(self) -> Rc<dyn Plugin> {
        let provider = self.provider;
        Rc::new(PluginObject {
            name: __wrap::name(PLUGIN_NAME),
            middleware: __wrap::middleware(move |c: Context, next: Next| {
                let provider = Rc::clone(&provider);
                async move {
                    bind(provider.as_ref(), &c).await?;
                    next.run(c).await
                }
            }),
            on_execute: __wrap::on_execute(|_| on_execute_done(remap_result)),
            ..PluginObject::default()
        })
    }
}

/// Returns the plugin that binds the principal of every request from `provider`.
///
/// `provider` is a plain function, an `async fn` or an async closure over the
/// request context that answers the principal or none, or a `Result` of it for
/// a provider that throws, and the compiler picks its form `M`. A closure
/// written inline names its parameter and its return type.
#[must_use]
pub fn use_identity<M: 'static, P>(provider: P) -> IdentityPlugin
where
    P: IdentityProvider<M>,
{
    IdentityPlugin {
        provider: Rc::new(Erased {
            provider,
            form: PhantomData,
        }),
    }
}

/// Returns the errors with every `ForbiddenError` remapped, or `None` when none changed.
///
/// Every other error stays as it is, a class of an application that carries
/// the name `ForbiddenError` among them. The errors are borrowed, so each is
/// copied before it is mapped, which the done hook of the plugin does not
/// need, since it owns the errors it maps.
#[must_use]
pub fn map_forbidden_errors(errors: &[GraphQLError]) -> Option<Vec<GraphQLError>> {
    if !errors.iter().any(|err| forbidden_of(err).is_some()) {
        return None;
    }
    Some(errors.iter().map(copy_graphql_error).map(remap).collect())
}

/// Binds the principal `provider` answers for `c`.
fn bind<'a>(provider: &'a dyn DynIdentity, c: &'a Context) -> LocalBoxFuture<'a, Result<()>> {
    Box::pin(async move {
        let principal = provider.identify(c).await?;
        c.set(PRINCIPAL, principal);
        Ok(())
    })
}

/// Remaps the refusals of the result of `done`, the done hook of the plugin.
///
/// The errors move out of the result and through [`remap`] and back, so an
/// error that stays is the same error and a refusal keeps the nodes and the
/// path of the error it replaces.
fn remap_result(done: &mut OnExecuteDone<'_>) {
    let ExecutionOutcome::Single(result) = &mut *done.result else {
        return;
    };
    if !result.errors.iter().any(|err| forbidden_of(err).is_some()) {
        return;
    }
    let errors = core::mem::take(&mut result.errors);
    result.errors = errors.into_iter().map(remap).collect();
}

/// Returns the `ForbiddenError` the original error of `err` carries.
fn forbidden_of(err: &GraphQLError) -> Option<&ForbiddenError> {
    original_host::<ForbiddenError>(err)
}

/// Returns the error `FORBIDDEN` that `err` maps to, or `err` as it is when
/// its original error is no `ForbiddenError`.
///
/// The new error takes the nodes and the path of `err`, as the reference hands
/// the same arrays to `new GraphQLError`, so no syntax tree is copied.
fn remap(mut err: GraphQLError) -> GraphQLError {
    let Some(forbidden) = forbidden_of(&err) else {
        return err;
    };
    let message = forbidden.message.clone();
    let mut extensions = JsObject::new();
    extensions.insert("code", ForbiddenError::CODE);
    if let Some(feature) = forbidden.true_feature() {
        extensions.insert("feature", feature.clone());
    }
    GraphQLError::new(
        message,
        GraphQLErrorOptions {
            nodes: err.nodes.take(),
            path: err.path.take(),
            extensions: Some(extensions),
            ..GraphQLErrorOptions::default()
        },
    )
}
