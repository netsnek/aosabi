// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the config contract.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! `AppConfig`, `PluginList` and `IntoConfig`.
//!
//! The configuration of the reference is
//! `{landingPage?, graphiql?, logger?, plugins?}`, and the handler spreads the
//! whole configuration into the options of Yoga, so a key of Yoga a configuration
//! names reaches Yoga. The keys of Yoga Aosabi ports are fields as well,
//! `maskedErrors`, `graphqlEndpoint`, `healthCheckEndpoint` and `multipart`, and
//! a key Aosabi does not port does not exist on the struct and fails the build.
//! The key `cors` is no field, since the engine answers no CORS, the named
//! divergence D46, so an app writes the middleware `cors` of the web server under
//! hono's names and a configuration that names the key fails the build. The key
//! `batching` is no field, since the function of `graphql-over-http` holds no
//! batching, the named divergence D58 of `docs/byte-equality.md` section 3, so a
//! configuration that names it fails the build. Every key is an `Option`, because
//! the reference tells an absent key from a present one.
//!
//! `defineConfig` is the identity at run time, and its factory form is
//! `pub async fn config()`, which [`IntoConfig`] takes beside the plain
//! `pub fn config()`, so the facade carries one form.
//!
//! The module re-exports the types of the keys of Yoga and of their options, so
//! an application that sets a key on the builder of its app, which writes every
//! key of the configuration, names them through the facade and never names
//! `graphql-over-http`.
//!
//! The key `viewer` is Aosabi's own and no key of the reference: `true` opens
//! the plugin list with the viewer, which serves `/viewer`, the named divergence D48 of
//! `docs/byte-equality.md` section 3.

use core::future::Future;

use aosabi_hooks::LocalBoxFuture;
pub use graphql_over_http::{
    MaskedErrors,
    MaskedErrorsConfig, //
};

use super::PluginSlot;
use crate::logger::LoggerOption;

/// The configuration of a service.
#[derive(Clone, Default)]
pub struct AppConfig {
    /// `landingPage`, the landing page on an unhandled route only when `true`,
    /// the named divergence D47 of `docs/byte-equality.md` section 3.
    pub landing_page: Option<bool>,
    /// `graphiql`, the page of `GraphiQL` only when `true`, the named
    /// divergence D48 of `docs/byte-equality.md` section 3, where Yoga serves
    /// it unless the key is `false`, which turns off introspection as well.
    pub graphiql: Option<bool>,
    /// `viewer`, Aosabi's own key, the viewer at `/viewer` only when `true`,
    /// D48.
    pub viewer: Option<bool>,
    /// `logger`, `false` for no access line or the object form.
    pub logger: Option<LoggerOption>,
    /// `plugins`, which the boot writes back with the built array (quirk
    /// `Q01_DUPLICATE_PLUGIN_WRITEBACK`).
    pub plugins: Option<PluginList>,
    /// `maskedErrors` of Yoga.
    pub masked_errors: Option<MaskedErrors>,
    /// `graphqlEndpoint` of Yoga, which the handler sets first and the
    /// configuration overrides.
    pub graphql_endpoint: Option<String>,
    /// `healthCheckEndpoint` of Yoga.
    pub health_check_endpoint: Option<String>,
    /// `multipart` of Yoga, the parser of uploads only when `true`, the named
    /// divergence D49 of `docs/byte-equality.md` section 3.
    pub multipart: Option<bool>,
}

impl core::fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AppConfig")
            .field("landing_page", &self.landing_page)
            .field("graphiql", &self.graphiql)
            .field("viewer", &self.viewer)
            .field("logger", &self.logger)
            .field("plugins", &self.plugins)
            .field("graphql_endpoint", &self.graphql_endpoint)
            .field("health_check_endpoint", &self.health_check_endpoint)
            .field("multipart", &self.multipart)
            .finish_non_exhaustive()
    }
}

/// The plugins of a configuration, `plugins`, in their order.
#[derive(Clone, Debug, Default)]
pub struct PluginList(pub Vec<PluginSlot>);

impl PluginList {
    /// Returns the list of `slots`.
    #[must_use]
    pub const fn new(slots: Vec<PluginSlot>) -> Self {
        Self(slots)
    }

    /// Returns the slots in order.
    #[must_use]
    pub fn slots(&self) -> &[PluginSlot] {
        &self.0
    }
}

/// A configuration, or a future of one, which is the factory of `defineConfig`.
pub trait IntoConfig {
    /// Returns the configuration as a future, which is ready at once for a
    /// plain one.
    fn into_config(self) -> LocalBoxFuture<'static, AppConfig>;
}

impl IntoConfig for AppConfig {
    fn into_config(self) -> LocalBoxFuture<'static, AppConfig> {
        Box::pin(async move { self })
    }
}

impl<F: Future<Output = AppConfig> + 'static> IntoConfig for F {
    fn into_config(self) -> LocalBoxFuture<'static, AppConfig> {
        Box::pin(self)
    }
}
