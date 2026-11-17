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
//! [`Embedded`] is `Send` and `Sync`, yet only the thread that booted it holds
//! the roots of the app, so a host calls it there. Each answer keys its thread
//! first. On a target whose standard library draws its keys from the operating
//! system, the maps of a request hash under those keys, and the tables of the
//! engine that keep a key of their own, the cache of parsed documents among
//! them, take sixteen bytes of the installed [`Randomness`], or of the
//! operating system with none installed. A host on a target whose standard
//! library has no such source, the WebAssembly of a page or of a Worker among
//! them, installs a [`Randomness`] with [`install_randomness`] before its
//! first answer, one that reads `crypto.getRandomValues` there. Until it does
//! and the provider fills, every answer is the response with one error and no
//! data whose message names [`install_randomness`], and no body is read.
//!
//! The service is the one of the mode `--requests` of a served binary: the
//! roots of the entry over the schema the build wrote, with the type
//! resolution of its unions and interfaces, under the cap on the tokens of a
//! request document the chain of the app sets with `.max_tokens(..)`, or
//! 120,008 where it sets none. The routes, the middleware and the plugins of
//! the app take no part, since no request reaches them, so a host that needs
//! them serves the app through a card. Of the configuration of the app the key
//! `maskedErrors` takes part, as it does over HTTP: an app that sets no key
//! masks no error, so an unexpected error answers its own message, and an app
//! that sets `.masked_errors(MaskedErrors::On)` answers `Unexpected error.`
//! with the code `INTERNAL_SERVER_ERROR` and its `locations` and `path` in its
//! place, a named divergence. The engine keeps the key without a mask function
//! of its own, so [`Embedded::boot`] answers an error for an app whose options
//! set `mask_error`. The key `introspection` takes part as well: an app that
//! sets `.introspection(false)` answers a document that selects a field of an
//! introspection type with the errors the HTTP road answers it with, a named
//! divergence, and an app that sets no key answers it. Every request runs on a
//! fresh agent with virtual timers, which the host never drives.
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
    plugin::config::{
        MaskedErrors,
        MaskedErrorsConfig, //
    },
};
// The randomness a host installs before its first answer, see the module doc.
pub use aosabi_core::random::{
    RandomError,
    Randomness,
    install_randomness, //
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
    /// The key `maskedErrors` of the app, off where it sets none.
    mask: Mask,
    /// Whether introspection stays on, `false` where the key `introspection`
    /// of the app is `false`, as the HTTP road reads it.
    introspection: bool,
}

// The engine is `Send` and `Sync`, and each thread that calls it keys itself.
const _: () = {
    const fn shared<T: Send + Sync>() {}
    shared::<Embedded>();
};

/// The error of the boot of an app whose key `maskedErrors` sets a mask
/// function of its own.
const MASK_FUNCTION: &str = "The embed road takes the key maskedErrors without a mask function of \
     its own, and the key of the app sets mask_error. A card serves the app with it over HTTP.";

/// The key `maskedErrors` of an app in a form a host shares between its
/// threads, which [`Embedded::answer`] turns into the key at each call.
#[derive(Clone, Debug)]
enum Mask {
    /// No mask, where the app sets no key or `MaskedErrors::Off`.
    Off,
    /// The mask of Yoga, `MaskedErrors::On`.
    On,
    /// The mask of Yoga with the message and `isDev` of the options of the
    /// app, which set no mask function.
    Options {
        /// `errorMessage`.
        error_message: Option<String>,
        /// `isDev`.
        is_dev: Option<bool>,
    },
}

impl Mask {
    /// Returns the form of `key`, the key of an app, and the error
    /// [`MASK_FUNCTION`] where the key sets a mask function of its own.
    fn of(key: Option<&MaskedErrors>) -> Result<Self, String> {
        match key {
            None | Some(MaskedErrors::Off) => Ok(Self::Off),
            Some(MaskedErrors::On) => Ok(Self::On),
            Some(MaskedErrors::Options(config)) if config.mask_error.is_some() => {
                Err(String::from(MASK_FUNCTION))
            }
            Some(MaskedErrors::Options(config)) => Ok(Self::Options {
                error_message: config.error_message.clone(),
                is_dev: config.is_dev,
            }),
        }
    }

    /// Returns the key `maskedErrors` the form stands for.
    fn key(&self) -> MaskedErrors {
        match self {
            Self::Off => MaskedErrors::Off,
            Self::On => MaskedErrors::On,
            Self::Options {
                error_message,
                is_dev,
            } => MaskedErrors::Options(MaskedErrorsConfig {
                mask_error: None,
                error_message: error_message.clone(),
                is_dev: *is_dev,
            }),
        }
    }
}

impl Embedded {
    /// Boots the roots of the app of `entry` over the schema of `output`.
    ///
    /// # Errors
    ///
    /// Returns the text of the error the entry answered, of the schema that
    /// does not build over the roots of the app, or of a key `maskedErrors`
    /// that sets a mask function of its own, which the embed road does not
    /// take.
    pub fn boot<E: IntoEntry>(entry: fn() -> E, output: EmbedOutput) -> Result<Self, String> {
        let app = entry().into_entry()?.into_app();
        // No mask where the app sets no key, as the HTTP road reads it.
        let mask = Mask::of(
            app.config()
                .and_then(|config| config.masked_errors.as_ref()),
        )?;
        // Introspection stays on unless the key turns it off.
        let introspection = app.config().and_then(|config| config.introspection) != Some(false);
        let mut resolvers = ResolverMap::default();
        for rule in output.resolve_types {
            resolvers
                .resolve_types
                .insert(String::from(rule.type_name), type_resolver(rule));
        }
        let mut builder = Service::builder(output.schema)
            .roots(app.graphql().clone())
            .resolvers(resolvers);
        if let Some(max_tokens) = app.config().and_then(|config| config.max_tokens) {
            builder = builder.max_tokens(max_tokens);
        }
        let service = builder
            .build()
            .map_err(|error| BootError::Schema(error).to_string())?;
        Ok(Self {
            service,
            mask,
            introspection,
        })
    }

    /// Answers the GraphQL request of the JSON body `body` with the JSON text
    /// of its response body, with the errors the key `maskedErrors` of the
    /// app masks and the introspection its key `introspection` refuses, see
    /// [`aosabi_core::embed::answer`].
    #[must_use]
    pub fn answer(&self, body: &str) -> String {
        let bytes = aosabi_core::embed::answer(
            &self.service,
            body.as_bytes(),
            &self.mask.key(),
            self.introspection,
        );
        String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
    }

    /// Returns the schema text the engine answers with.
    #[must_use]
    pub fn schema(&self) -> &'static str {
        self.service.type_defs()
    }
}
