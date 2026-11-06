// SPDX-License-Identifier: MIT
/*
 * aosabi embed: the engine and the roots of an app inside the program of a host.
 */

//! The engine and the roots of an app inside the program of a host.
//!
//! A card that serves sockets boots the app in its own binary and answers
//! the requests of its listeners. A host that links the app into its own
//! program instead, such as a module of WebAssembly that a Web Worker of a
//! page runs, boots it with [`Embedded::boot`] and hands each GraphQL request
//! to [`Embedded::answer`] as the JSON body a GraphQL request over HTTP
//! carries, a JSON object with `query` and optionally `variables` and
//! `operationName`. The answer is the response body. No socket, no HTTP, no
//! clock and no thread are needed, so it runs on every target the library
//! of a service builds for, `wasm32-unknown-unknown` among them.
//!
//! The service is the one of the mode `--requests` of a served binary: the
//! roots of the entry over the schema the build wrote, with the type
//! resolution of its unions and interfaces. The routes, the middleware and
//! the plugins of the app take no part, since no request reaches them, so a
//! host that needs them serves the app through a card. Every request runs on
//! a fresh agent with virtual timers, which the host never drives.
//!
//! The build writes the schema and the type resolution beside the generated
//! copy of the crate. A build script of the host takes them into its
//! `OUT_DIR` with `aosabi_build::take_artifacts`, as the build script of a
//! server package does, and [`embed_output!`](crate::embed_output) includes
//! them.
//!
//! # Examples
//!
//! ```ignore
//! let engine = aosabi::embed::Embedded::boot(my_app::__aosabi::entry, aosabi::embed_output!())?;
//! let body = engine.answer(r#"{"query":"{ hello }"}"#);
//! ```

use aosabi_core::{
    BootError, IntoApp as _, IntoEntry, ResolverMap, Service,
    glue::resolve_type::{
        ResolveTypeRule,
        type_resolver, //
    },
};

/// What the build of an app hands an embedding host: the schema and the
/// type resolution of every union and interface.
#[derive(Clone, Copy, Debug)]
pub struct EmbedOutput {
    /// The schema text, `schema.graphql`.
    pub schema: &'static str,
    /// The type resolution of every union and interface, which the build
    /// writes beside the schema.
    pub resolve_types: &'static [ResolveTypeRule],
}

/// Returns the [`EmbedOutput`] a build script took into `OUT_DIR` with
/// `aosabi_build::take_artifacts`.
#[macro_export]
macro_rules! embed_output {
    () => {
        $crate::embed::EmbedOutput {
            schema: include_str!(concat!(env!("OUT_DIR"), "/schema.graphql")),
            resolve_types: include!(concat!(env!("OUT_DIR"), "/aosabi_resolve_types.rs")),
        }
    };
}

/// The engine with the roots of one app, booted inside the program of a host.
pub struct Embedded {
    /// The service over the roots of the app.
    service: Service,
}

impl Embedded {
    /// Boots the roots of the app of `entry` over the schema of `output`.
    ///
    /// # Errors
    ///
    /// Returns the text of the error the entry answered, or of the schema
    /// that does not build over the roots of the app.
    pub fn boot<E: IntoEntry>(entry: fn() -> E, output: EmbedOutput) -> Result<Self, String> {
        let app = entry().into_entry()?.into_app();
        let mut resolvers = ResolverMap::default();
        for rule in output.resolve_types {
            resolvers
                .resolve_types
                .insert(String::from(rule.type_name), type_resolver(rule));
        }
        let service = Service::builder(output.schema)
            .roots(app.graphql().clone())
            .resolvers(resolvers)
            .build()
            .map_err(|error| BootError::Schema(error).to_string())?;
        Ok(Self { service })
    }

    /// Answers the GraphQL request of the JSON body `body` with the JSON text
    /// of its response body, see [`aosabi_core::embed::answer`].
    #[must_use]
    pub fn answer(&self, body: &str) -> String {
        let bytes = aosabi_core::embed::answer(&self.service, body.as_bytes());
        String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
    }

    /// Returns the schema text the engine answers with.
    #[must_use]
    pub fn schema(&self) -> &'static str {
        self.service.type_defs()
    }
}
