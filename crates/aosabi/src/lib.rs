// SPDX-License-Identifier: MIT
/*
 * aosabi: the facade crate a service depends on.
 *
 * This file is the root of the crate. It re-exports the items of the crates
 * behind it, and defines the functions serve_app and serve, the constant
 * BUILTINS and the macro build_output, which name the engine and the two
 * plugins every boot adds, so that no crate behind it names the carrier or the
 * engine of another. The builder of an app and the prelude stand in the modules
 * app and prelude.
 */

//! The facade of Aosabi, the one crate a service names under `[dependencies]`.
//!
//! An application names `aosabi` under `[dependencies]` and nothing else of the
//! workspace. It is a plain crate, which holds no attribute, derive or macro of
//! Aosabi, and the builder of `aosabi-build` reads its schema from its source,
//! starting from the entry, the one public function of its crate root that
//! answers an app. The glue the builder writes into the generated copy of the
//! crate names every item through `::aosabi::`, `::aosabi::descriptor::` and
//! `::aosabi::__private::`, so an application that renames this dependency
//! breaks the glue.
//!
//! | Path | Holds |
//! | --- | --- |
//! | [`IntoApp`], [`IntoEntry`], [`App`], [`AppConfig`], [`Plugin`], [`PluginObject`] | the application, what its entry answers, the builder of an app a plain crate writes its index with, its configuration and the plugin contract |
//! | [`prelude`] | the names an application imports at once, `use aosabi::prelude::*;` |
//! | [`Error`], [`Result`] | the error of a resolver and the `Result` it answers, `Result<T, E = Error>` |
//! | [`Stream`], [`stream`] | the stream a subscription field answers and `stream::task`, beside [`Repeater`] |
//! | [`graphql!`], [`resolver`], [`decorate`], [`plugin!`], [`plugins!`] | the macros of the derive path, each a compile error that names its native form under the default features |
//! | [`Context`], [`get_context`], [`fn@get_env`], [`get_in_context`] | the context of a request and the stores a resolver reads |
//! | [`Logger`], [`fn@logger`], [`get_logger`] | the logger |
//! | [`fn@mutation`], [`UserError`], [`create_decorator`] | the authoring run time of the derive path, `create_decorator` a compile error that names its native form under the default features |
//! | [`MutationPayload`] | the answer of a mutation method, the payload of `mutation(...)` with nullable fields and `userErrors` |
//! | [`Repeater`], [`Push`], [`Stop`] | the stream a subscription field returns and the two arguments of its executor |
//! | [`PubSub`], [`Topic`], [`experimental_create_pub_sub`], [`TopicId`], [`Payload`] | the pub sub a service keeps in a `static` |
//! | [`Bindings`], [`Variables`], [`Key`] | the typed environment and the variables of a request, the two derives of the derive path a compile error under the default features |
//! | [`descriptor`] | the static description of a type, which the glue and the macros write |
//! | [`ID`], [`Int`], [`Float`], [`Number`], [`Date`], [`File`], [`JSON`], [`Any`] | the scalar markers, the twins of the TypeScript types that name a built in scalar, and of `number` |
//! | [`Service`], [`request_stream`] |  |
//! | [`mod@json`] | the JSON value types a resolver builds, `json::Map` and `json::Value`, and [`json!`] of the derive path, a compile error under the default features |
//! | [󰀀](mod@fetch) | the Fetch types a route answers with, `Response` over a string, bytes or a stream |
//! | [`time`], [`task`] | the timers of a request and of a setup and the job queue of a request, `sleep`, `set_timeout`, `delay` and `tick` |
//! | [`create_graphql_error`], [`GraphQLError`], [`GraphQLErrorOptions`], [`MaskedErrors`] | `createGraphQLError` of graphql-yoga, the `GraphQLError` it makes and its options, and the type of the key of Yoga a configuration sets, `MaskedErrors` with its options |
//! | [`get_cookie`], [`get_cookies`], [`set_cookie`], [`delete_cookie`], [`CookieOptions`] | the cookie helpers of hono, the signed forms aside |
//! | [`fn@body_limit`], [`BodyLimitOptions`] | the middleware `body_limit(options)` of hono under its names, with the options `max_size` and `on_error`, which `app.use_boxed(path, Rc::new(body_limit(options)))` registers in place of the default bound of 1 MiB on that path, divergence D76 |
//! | [`fn@cors`], [`CorsOptions`], [`CorsOrigin`] | the middleware `cors(options)` of hono under its names, with the option `origin`, which `app.use_boxed(path, Rc::new(cors(options)))` registers |
//! | [`fn@csrf`], [`CsrfOptions`], [`CsrfOrigin`], [`CsrfSecFetchSite`] | the middleware `csrf(options)` of hono under its names, with the options `origin` and `sec_fetch_site`, which `app.use_boxed(path, Rc::new(csrf(options)))` registers in place of the default csrf on that path, divergence D70 |
//! | [`on_execute_done`], [`ExecutionOutcome`] | the done hook of a plugin literal and the result it reads |
//! | [`current_role`], [`Role`] | the run role of the process |
//! | [`db`] | `ForbiddenError` |
//! | [`auth`], [`auth::contract`], [`auth::plugin`] | the auth root, its contract and the identity plugin |
//! | `auth::zitadel` | the Zitadel login routes and provider, under the feature `auth-zitadel`, which turns `fetch` on |
//! | `use_sentry`, `sentry` | the Sentry plugin and the SDK it reports through, under the feature `sentry` |
//! | `fetch`, `FetchError`, `Request`, `Headers`, `Body` | the outbound HTTP call of a service, its failure and the request it sends, under the feature `fetch`, which `sentry`, `auth-zitadel` and `federation` turn on |
//! | `Lazy` | a member whose value is computed when it is selected, the twin of a property `() => value`, a form of the derive path under the hidden feature `mirror` |
//! | `create_gateway`, `Gateway`, `GatewayFactory`, `GatewayOptions`, `DelegateCall`, `passthrough`, `Passthrough`, `Patches`, `pass`, `FieldPolicy`, `PatchPolicy`, `Force`, `Pass`, `HeadersFn`, `Needs`, `headers!`, `needs!`, `patches!`, `policy!`, `federation` | the federation, `createGateway` with its patches, policies, needs and headers, under the feature `federation`, the four literals a compile error under the default features |
//! | `remote` |  |
//! | [`BUILTINS`] | the viewer and the landing page every boot adds |
//! | `serve_app`, `BuildOutput`, `build_output!`, `use_node_server` | the engine, under the feature `serve` on a native target |
//! | `serve_static`, `ServeStaticOptions` | the middleware that serves the files of a directory, `serveStatic` of the Node adapter, under the feature `serve` on a native target |
//! | [`fn@compress`] | the middleware `compress()` of hono under its name, which codes the answers of the routes after it in deflate, since the base pipeline codes none, divergence D72 |
//!
//! A plain crate builds its app with the builder [`App`], which [`prelude`]
//! exports as well, `App::new().query(Query)`. The chain carries the
//! routes, the composed apps, the plugins and the options of
//! `aosabi.config.ts` as well, and `serve_app` serves the configuration the app
//! carries, so a project holds no `aosabi.config.rs`.
//!
//! Under the default features each macro of the derive path, each key of
//! `#[aosabi(..)]`, `json!`, the four literals of the federation and
//! `create_decorator` answer a compile error that names the native form, and
//! the command line refuses the entry `default` of the derive path and
//! `aosabi.config.rs`. The feature keeps `aosabi::serve`, which serves the
//! configuration of `aosabi.config.rs`, beside `serve_app`, and keeps `Lazy`
//! and the module `remote`, the contract of the registries of `aosabi pull`, at
//! their paths, so under the default features each of these names answers
//! rustc's own error.
//!
//! The defaults of the crate are the translation alone, which every card
//! links, and the package of a card names the engine and the coding of its
//! card as features: `serve`, `unix-socket`, `deflate` and `linux` for the
//! Linux cards with the opt ins `tls` and `h2c`, and `platform-coding` for the
//! Worker, as `Cargo.toml` writes them. A Linux card that names a device
//! carrier adds `serial`, `usb` or `bluetooth`, and one that names
//! `GraphTP`(Homa) on Network adds `homa-network`, whose driver the boot runs
//! beside the others. The feature `fetch` of a service names no card: on a
//! native target `serve` hands the client of `aosabi-fetch` to the boot as the
//! driver of the network capability, and on every other target a call answers
//! `FetchError::NoNetwork` until the card hands a driver over.
//!
//! The file server `serve_static` reads the file system of the process, so it
//! stands on a native target under the feature `serve` alone, and a build for
//! `wasm32` meets the missing item at compile time. A service that calls it
//! names the facade a second time under `[target.'cfg(unix)'.dependencies]`
//! with the features of its cards and writes the call under `#[cfg(unix)]`, as
//! the scaffold of `aosabi new --features pages` does, so its build for the
//! Worker card resolves the facade without `serve`.
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
//!         String::from("world")
//!     }
//! }
//!
//! pub fn app() -> impl IntoApp {
//!     App::new().query(Query)
//! }
//! ```

pub mod app;
pub mod prelude;

/// The engine's `createDecorator`, under the hidden feature `mirror`.
#[cfg(feature = "mirror")]
pub use aosabi_core::create_decorator;
pub use aosabi_core::descriptor::{
    Any,
    Date,
    File,
    Float,
    ID,
    Int,
    JSON,
    Number,
    Promise, //
};
/// The member of the derive path computed when it is selected, under the hidden
/// feature `mirror`, which keeps it at its path for the twins that wait for the
/// TypeScript judge.
#[cfg(feature = "mirror")]
pub use aosabi_core::lazy::Lazy;
/// The contract of the registries `aosabi pull` writes, which carry the derive
/// of the derive path, under the hidden feature `mirror`, which keeps it at its
/// path with the `Unchecked` it re-exports.
#[cfg(feature = "mirror")]
pub use aosabi_core::remote;
pub use aosabi_core::{
    AppConfig,
    Bindings,
    Builtins,
    Context,
    Decorator,
    DecoratorArgs,
    Error,
    FileStream,
    InContext,
    IntoApp,
    IntoConfig,
    IntoEntry,
    Key,
    LogLevel,
    Logger,
    LoggerConfig,
    LoggerOption,
    MutationPayload,
    Payload,
    Plugin,
    PluginObject,
    PluginRole,
    PubSub,
    Push,
    Repeater,
    Role,
    Service,
    ServiceError,
    ServiceErrorExtensions,
    Stop,
    Strategy,
    Stream,
    Topic,
    TopicId,
    UserError,
    Variables,
    current_role,
    descriptor,
    experimental_create_pub_sub,
    get_context,
    get_env,
    get_in_context,
    get_locale,
    get_logger,
    get_resolve_info,
    logger::{
        LogFormat,
        logger, //
    },
    mutation,
    on_execute_done,
    request_stream, //
};
pub use aosabi_hooks::ExecutionOutcome;
pub use aosabi_http::{
    Response,
    helper::cookie::{
        delete_cookie,
        get_cookie,
        get_cookie_with,
        get_cookies,
        set_cookie, //
    },
    middleware::{
        compress::compress,
        cors::cors, //
    },
    utils::cookie::{
        CookieError,
        CookieOptions,
        CookiePrefix,
        Priority,
        SameSite, //
    }, //
};
/// The literal of the JSON values of the derive path, under the hidden
/// feature `mirror`.
#[cfg(feature = "mirror")]
pub use aosabi_json::json;
pub use aosabi_macros::{
    Aosabi,
    Bindings,
    Variables,
    decorate,
    graphql,
    plugin,
    plugins,
    resolver,
    resolvers,
    scalar, //
};

pub use aosabi_http::middleware::body_limit::body_limit;

/// The options of [`fn@body_limit`], `bodyLimit(options)` of hono, over the
/// thrown value of the core.
///
/// The default is the bound of the base pipeline, 1 MiB, with hono's answer,
/// the status 413 over the text `Payload Too Large`.
pub type BodyLimitOptions =
    aosabi_http::middleware::body_limit::BodyLimitOptions<aosabi_core::Thrown>;

/// The options of [`fn@cors`], `cors(options)` of hono, over the thrown value of the core.
///
/// The defaults are hono's, the origin `*` and the methods `GET`, `HEAD`,
/// `PUT`, `POST`, `DELETE` and `PATCH`.
pub type CorsOptions = aosabi_http::middleware::cors::CorsOptions<aosabi_core::Thrown>;

/// The option `origin` of [`CorsOptions`], the origins [`fn@cors`] allows:
/// `*`, one origin, a list of origins, or a function of the service.
pub type CorsOrigin = aosabi_http::middleware::cors::CorsOrigin<aosabi_core::Thrown>;

pub use aosabi_http::middleware::csrf::csrf;

/// The options of [`fn@csrf`], `csrf(options)` of hono, over the thrown value of the core.
///
/// The defaults are hono's, the origin of the URL of the request and the
/// `Sec-Fetch-Site` `same-origin`.
pub type CsrfOptions = aosabi_http::middleware::csrf::CsrfOptions<aosabi_core::Thrown>;

/// The option `origin` of [`CsrfOptions`], the origins [`fn@csrf`] allows:
/// one origin, a list of origins, or a function of the service.
pub type CsrfOrigin = aosabi_http::middleware::csrf::CsrfOrigin<aosabi_core::Thrown>;

/// The option `sec_fetch_site` of [`CsrfOptions`], the values of
/// `Sec-Fetch-Site` [`fn@csrf`] allows: one value, a list of values, or a
/// function of the service.
pub type CsrfSecFetchSite = aosabi_http::middleware::csrf::CsrfSecFetchSite<aosabi_core::Thrown>;

pub use crate::{
    app::App,
    prelude::Result, //
};

/// Returns the decorator of `callback`, `createDecorator(callback)` of the
/// derive path, which retires.
///
/// Under the default features every call answers a compile error that names the
/// native form, a guard function the body of a method calls first, since no
/// type implements the bound of `callback`, and the hidden feature `mirror`
/// keeps the engine's `create_decorator` at this path for the twins that wait
/// for the TypeScript judge.
#[cfg(not(feature = "mirror"))]
pub fn create_decorator<F: __private::RetiredDecorator>(callback: F) -> Decorator {
    callback.decorator()
}

/// Builds a JSON value from a literal, `json!` of the derive path, which
/// retires.
///
/// Under the default features it answers a compile error that names the native
/// form, `json::Map::from([..])`, `json::Value::from(..)` or `collect()`, and
/// the hidden feature `mirror` keeps the literal of the engine at this path for
/// the twins that wait for the TypeScript judge.
#[cfg(not(feature = "mirror"))]
#[macro_export]
macro_rules! json {
    ($($body:tt)*) => {
        ::core::compile_error!(
            "`json!` is retired, and an application builds a JSON value with `json::Map::from([..])`, `json::Value::from(..)` or `collect()`, as docs/authoring.md section 20.3 writes it"
        )
    };
}

/// The viewer in the first slot and the landing page of the unhandled route,
/// the two plugins every boot adds, `useViewer()` and `useUnhandledRoute()`.
pub const BUILTINS: Builtins = Builtins {
    first: None,
    viewer: aosabi_viewer::use_viewer,
    unhandled_route: aosabi_landing::use_unhandled_route,
};

/// The JSON value types a resolver builds and a `ServiceError` carries.
///
/// [󰀀](json::Map) and [󰀁](json::Value) are the object and the value under the
/// names of `serde_json`, which an application builds with no macro.
pub mod json {
    #[cfg(feature = "mirror")]
    pub use aosabi_json::{
        JsObject,
        JsString,
        JsValue,
        into_json::IntoJson, //
    };
    pub use aosabi_json::{
        Map,
        Value, //
    };
}

/// The forms of the derive path that left the root of the facade, under the
/// hidden feature `mirror`.
#[cfg(feature = "mirror")]
#[doc(hidden)]
pub mod mirror {
    pub use aosabi_core::{
        App,
        Result,
        Thrown,
        Unchecked, //
    };
    pub use aosabi_json::{
        JsObject,
        JsString,
        JsValue, //
    };
}

/// The stream a subscription field of the native layer answers and the task
/// that builds one, `stream::task(|push, stop| async move {..})`, the twin of
/// `new Repeater(async (push, stop) => {..})`.
pub mod stream {
    pub use aosabi_core::glue::stream::{
        Stream,
        task, //
    };
}

/// The Fetch types a route answers with beyond the helpers of the context,
/// `new Response(body, init)` over a string, bytes or a stream.
pub mod fetch {
    pub use aosabi_http::{
        Arg,
        Body,
        BodyStream,
        Headers,
        HeadersInit,
        Response,
        ResponseInit,
        Stream,
        fetch::body::BodyError, //
    };
}

/// The timers of a request and of a setup.
///
/// The await `sleep(ms).await?` is `await new Promise(r => setTimeout(r, ms))`,
/// the timer of the agent of a request, which keeps its place among the jobs
/// of the request, and the call `set_timeout(move || {...}, ms)` is
/// `setTimeout(() => {...}, ms)`, the detached timer whose callback runs after
/// the request may have answered. `delay` is the timer of the clock of the
/// runtime, which a setup of the boot awaits outside any request.
pub mod time {
    pub use aosabi_core::{
        clock::{
            Delay,
            delay, //
        },
        execute::promise::{
            set_timeout,
            sleep, //
        }, //
    };
}

/// The job queue of a request.
pub mod task {
    pub use aosabi_core::execute::promise::tick;
}

/// `createGraphQLError` of the package `graphql-yoga`, which a
/// resolver throws a `GraphQLError` with its extensions through, and the
/// error and its options, at the root as the root of the package `aosabi`
/// exports them, and the type of the key of Yoga that a configuration
/// spreads into the options of Yoga, which the builder of an app sets
/// through `.masked_errors(..)`. The keys `batching` and `cors` are no keys
/// of Aosabi, the named divergences D58 and D46.
pub use aosabi_core::{
    GraphQLError,
    GraphQLErrorOptions,
    execute::errors::create_graphql_error,
    plugin::config::{
        MaskedErrors,
        MaskedErrorsConfig, //
    }, //
};

/// The module `db`, whose `ForbiddenError` is the one of the
/// auth contract.
pub mod db {
    pub use aosabi_auth_contract::ForbiddenError;
}

/// The subpath `aosabi/auth`, the gates on the principal of a
/// request and what a caller of them names.
pub mod auth {
    pub use aosabi_auth::{
        AsyncProvider,
        ForbiddenError,
        IdentityProvider,
        IntoIdentity,
        PRINCIPAL,
        Principal,
        PrincipalId,
        PrincipalRef,
        SyncProvider,
        authorize,
        get_principal,
        has_permission,
        has_role,
        require_role, //
    };

    /// The subpath `aosabi/auth/contract`, the contract every
    /// authentication plugin shares, generic over its context and its error.
    pub mod contract {
        pub use aosabi_auth_contract::{
            AsyncProvider,
            ForbiddenError,
            IdentityProvider,
            IntoIdentity,
            PRINCIPAL_KEY,
            Principal,
            PrincipalId,
            PrincipalRef,
            ProviderFuture,
            SyncProvider,
            has_permission,
            has_role, //
        };
    }

    /// The subpath `aosabi/auth/plugin`, the identity plugin.
    pub mod plugin {
        pub use aosabi_auth::plugin::{
            IdentityPlugin,
            use_identity, //
        };
    }

    /// The subpath `aosabi/auth/zitadel`, the Zitadel/OIDC
    /// integration, under the feature `auth-zitadel`.
    ///
    /// `zitadel_auth` is the identity provider for `use_identity`, and
    /// `zitadel_login` adds the browser OAuth routes
    /// (`/auth/login|callback|logout`) that set the session cookie
    /// `zitadel_auth` then reads. Every request to the issuer goes through
    /// the network capability, which the feature turns on with `fetch`.
    #[cfg(feature = "auth-zitadel")]
    pub mod zitadel {
        pub use aosabi_auth_zitadel::{
            OidcUser,
            ZitadelAuth,
            ZitadelAuthOptions,
            ZitadelLoginOptions,
            ZitadelLoginPlugin,
            ZitadelProvider,
            zitadel_auth,
            zitadel_login,
            zitadel_principal, //
        };
    }
}

/// The outbound HTTP call of a service, the global `fetch`, and the failure
/// a call answers, under the feature `fetch`.
///
/// A failed call thrown with `?` is the `TypeError` `fetch failed` of the
/// global `fetch`.
#[cfg(feature = "fetch")]
pub use aosabi_core::net::{
    FetchError,
    fetch, //
};
/// The federation, its root exports and the Rust helpers of its options,
/// under the feature `federation`.
///
/// `create_gateway`, `passthrough` and `pass` are the functions exported
/// with `createGateway`, and `FieldPolicy` and `PatchPolicy` are the
/// policy types beside them. `Gateway`, `GatewayFactory`, `GatewayOptions`,
/// `DelegateCall`, `Patches` and `Passthrough` are the configured federation,
/// its factory, its options, one delegation, the patches the literal
/// [`patches!`] builds and the identity patch `passthrough()` answers, and
/// [`needs!`] builds the needs of a delegation. `Force`, `Pass`, `HeadersFn`
/// and `Needs` are what a policy forces, what `pass` answers, the headers
/// function of a federation and the needs of a delegation, and the literals
/// [`headers!`] and [`policy!`] build the headers a headers function answers
/// and the policy `pass` takes. The headers object itself is
/// [`federation::Headers`], since `Headers` at the root is the global `Headers`
/// of the Fetch API that the feature `fetch` exports.
#[cfg(feature = "federation")]
pub use aosabi_federation::{
    gateway::{
        DelegateCall,
        Gateway,
        GatewayFactory,
        GatewayOptions,
        create_gateway,
        passthrough, //
    },
    headers::HeadersFn,
    needs::Needs,
    patches::{
        Passthrough,
        Patches, //
    },
    policy::{
        FieldPolicy,
        Force,
        Pass,
        PatchPolicy,
        pass, //
    }, //
};
/// The request a service hands [`fetch`](fn@fetch), with its headers and its body, the
/// globals `Request` and `Headers` of a JavaScript runtime, under the
/// feature `fetch`.
#[cfg(feature = "fetch")]
pub use aosabi_http::fetch::{
    body::Body,
    headers::Headers,
    request::Request, //
};

/// The surface of the federation in one place, the root items of the feature
/// `federation` with the headers object, the forced function and the
/// collected policies beside them.
#[cfg(feature = "federation")]
pub mod federation {
    pub use aosabi_federation::{
        gateway::{
            DelegateCall,
            Gateway,
            GatewayFactory,
            GatewayOptions,
            create_gateway,
            passthrough,
            reset_schema_cache, //
        },
        headers::{
            Headers,
            HeadersFn, //
        },
        needs::{
            Needs,
            NeedsBuilder,
            NeedsNode,
            NeedsTarget, //
        },
        patches::{
            Fallible,
            IntoPatch,
            OneArg,
            Passthrough,
            PatchKey,
            Patches,
            TwoArgs, //
        },
        policy::{
            FieldPolicy,
            Force,
            ForceFn,
            Pass,
            PatchPolicy,
            Policies,
            collect_policies,
            pass, //
        }, //
    };

    pub use crate::{
        headers,
        needs,
        patches,
        policy, //
    };
}

/// Builds the headers a headers function of a federation answers, from a
/// literal of names and values, under the feature `federation` and the
/// hidden feature `mirror`.
///
/// A name is an identifier or a string literal, and a value is an expression
/// that converts into a `String`. A name written twice keeps its first place
/// and its last value, as in an object literal.
///
/// # Examples
///
/// ```ignore
/// fn fwd(ctx: Option<&aosabi::Context>) -> aosabi::federation::Headers {
///     aosabi::headers! {
///         authorization: ctx
///             .and_then(|c| c.req().header("authorization"))
///             .unwrap_or_default(),
///     }
/// }
/// ```
#[cfg(all(feature = "federation", feature = "mirror"))]
#[macro_export]
macro_rules! headers {
    ($($body:tt)*) => {
        $crate::__private::aosabi_federation::headers!($($body)*)
    };
}

/// The literal of the headers of a federation, `headers!` of the derive path,
/// which retires, under the feature `federation`.
///
/// Under the default features it answers a compile error that names the native
/// form, `federation::Headers::new()` with `set(name, value)`, and the hidden
/// feature `mirror` keeps the literal for the fronts that wait for wave 2.
#[cfg(all(feature = "federation", not(feature = "mirror")))]
#[macro_export]
macro_rules! headers {
    ($($body:tt)*) => {
        ::core::compile_error!(
            "`headers!` is retired, and an application builds the headers with `federation::Headers::new()` and `set(name, value)`, as docs/authoring.md section 28 writes it"
        )
    };
}

/// Builds the policy `pass` attaches to a patch, from a literal of field
/// policies, under the feature `federation` and the hidden feature `mirror`.
///
/// Each field holds `args`, the names a caller may set, and `force`, the forced
/// arguments, each optional and in either order. A forced value is a closure of
/// the context or an expression of a JSON kind.
///
/// # Examples
///
/// ```ignore
/// let policy = aosabi::policy! {
///     orders: { args: [], force: { status: "ACTIVE" } },
/// };
/// ```
#[cfg(all(feature = "federation", feature = "mirror"))]
#[macro_export]
macro_rules! policy {
    ($($body:tt)*) => {
        $crate::__private::aosabi_federation::policy!($($body)*)
    };
}

/// The literal of a policy of a federation, `policy!` of the derive path, which
/// retires, under the feature `federation`.
///
/// Under the default features it answers a compile error that names the native
/// form, the struct `FieldPolicy { args, force }` per field, and the hidden
/// feature `mirror` keeps the literal for the fronts that wait for wave 2.
#[cfg(all(feature = "federation", not(feature = "mirror")))]
#[macro_export]
macro_rules! policy {
    ($($body:tt)*) => {
        ::core::compile_error!(
            "`policy!` is retired, and an application writes the struct `FieldPolicy { args, force }` for each field, as docs/authoring.md section 28 writes it"
        )
    };
}

/// Builds the selection of a delegation from a literal of field names, the
/// `needs` object of the federation, under the feature `federation` and the
/// hidden feature `mirror`.
///
/// The literal is a closure over the builder of the record the delegated field
/// answers, so a name the record lacks is refused at that name. A key is a
/// field name, alone or with `true`, with `false`, or with a nested literal,
/// whose key `__args` holds the arguments of its field.
///
/// # Examples
///
/// ```ignore
/// users().delegate(users::Query::user { id }, aosabi::needs! { id, email })
/// ```
#[cfg(all(feature = "federation", feature = "mirror"))]
#[macro_export]
macro_rules! needs {
    ($($body:tt)*) => {
        $crate::__private::aosabi_federation::needs!($($body)*)
    };
}

/// The literal of the selection of a delegation, `needs!` of the derive path,
/// which retires, under the feature `federation`.
///
/// Under the default features it answers a compile error that says the literal
/// has no native form yet, since the plain form of a selection stays open, as
/// the key `spread` answers, and the hidden feature `mirror` keeps the literal
/// for the fronts that wait for wave 2.
#[cfg(all(feature = "federation", not(feature = "mirror")))]
#[macro_export]
macro_rules! needs {
    ($($body:tt)*) => {
        ::core::compile_error!(
            "`needs!` is retired with no native form yet, as docs/authoring.md section 28 writes it"
        )
    };
}

/// Builds the patches of a federation from a literal of remote type names
/// and patches, under the feature `federation` and the hidden feature
/// `mirror`.
///
/// A patch is a closure that takes the type its key names,
/// `pass(closure, policy! { .. })`, or the bare `passthrough()`, and a key that
/// names another type, or a type the registry lacks, fails to compile.
///
/// # Examples
///
/// ```ignore
/// let patches = aosabi::patches! {
///     User: |u: users::User| u.firstName,
///     Money: passthrough(),
/// };
/// ```
#[cfg(all(feature = "federation", feature = "mirror"))]
#[macro_export]
macro_rules! patches {
    ($($body:tt)*) => {
        $crate::__private::aosabi_federation::patches!($($body)*)
    };
}

/// The literal of the patches of a federation, `patches!` of the derive path,
/// which retires, under the feature `federation`.
///
/// Under the default features it answers a compile error that names the native
/// form, `federation::Patches::new()` and its calls, and the hidden feature
/// `mirror` keeps the literal for the fronts that wait for wave 2.
#[cfg(all(feature = "federation", not(feature = "mirror")))]
#[macro_export]
macro_rules! patches {
    ($($body:tt)*) => {
        ::core::compile_error!(
            "`patches!` is retired, and an application builds the patches with `federation::Patches::new()` and its calls, as docs/authoring.md section 28 writes it"
        )
    };
}

#[cfg(feature = "sentry")]
pub use aosabi_sentry::{
    EventIdKey,
    SentryPlugin,
    SentryPluginOptions,
    use_sentry, //
};

/// The SDK the Sentry plugin reports through, `Sentry` of `@sentry/node` and
/// `getSentry` of `@hono/sentry`, under the feature `sentry`.
///
/// `default_skip_error` stands here and not at the root, since the root
/// exports `use_sentry` and its options and no `default_skip_error`.
#[cfg(feature = "sentry")]
pub mod sentry {
    pub use aosabi_sentry::{
        ClientOptions,
        ExecuteArgs,
        Scope,
        Toucan,
        default_skip_error,
        envelope::Breadcrumb,
        get_sentry,
        init, //
    };
}

/// The engine of a card that serves sockets, under the feature `serve` on a
/// native target.
#[cfg(all(feature = "serve", unix))]
mod engine {
    use std::{
        io,
        process::ExitCode, //
    };

    #[cfg(feature = "mirror")]
    use aosabi_core::IntoConfig;
    use aosabi_core::{
        IntoEntry,
        Listen, //
    };
    use aosabi_serve::{
        BuildOutput,
        Capabilities,
        Driver, //
    };

    use crate::BUILTINS;

    /// The driver of a carrier the card leaves out, which starts nothing.
    // Every driver takes one signature, so this one wraps its answer too, and
    // a card that names every carrier leaves it unused.
    #[allow(
        dead_code,
        clippy::unnecessary_wraps,
        reason = "the signature of every driver, unused where the card names every carrier"
    )]
    fn none(_build: &BuildOutput) -> io::Result<Option<Box<dyn Listen>>> {
        Ok(None)
    }

    /// The driver of HTTP(Unix Socket), under the feature `unix-socket`.
    #[cfg(feature = "unix-socket")]
    const UNIX_SOCKET: Driver = aosabi_carrier_unix::http::driver;
    /// No driver of HTTP(Unix Socket) without the feature `unix-socket`.
    #[cfg(not(feature = "unix-socket"))]
    const UNIX_SOCKET: Driver = none;

    /// The driver of Serial Line, under the feature `serial` on Linux.
    #[cfg(all(feature = "serial", target_os = "linux"))]
    const SERIAL: Driver = aosabi_carrier_serial::driver;
    /// No driver of Serial Line without the feature `serial` on Linux.
    #[cfg(not(all(feature = "serial", target_os = "linux")))]
    const SERIAL: Driver = none;

    /// The driver of USB, under the feature `usb` on Linux.
    #[cfg(all(feature = "usb", target_os = "linux"))]
    const USB: Driver = aosabi_carrier_usb::driver;
    /// No driver of USB without the feature `usb` on Linux.
    #[cfg(not(all(feature = "usb", target_os = "linux")))]
    const USB: Driver = none;

    /// The driver of Bluetooth, under the feature `bluetooth` on Linux.
    #[cfg(all(feature = "bluetooth", target_os = "linux"))]
    const BLUETOOTH: Driver = aosabi_carrier_bluetooth::driver;
    /// No driver of Bluetooth without the feature `bluetooth` on Linux.
    #[cfg(not(all(feature = "bluetooth", target_os = "linux")))]
    const BLUETOOTH: Driver = none;

    /// The driver of `GraphTP`(Homa) on Network, under the feature
    /// `homa-network` on Linux.
    #[cfg(all(feature = "homa-network", target_os = "linux"))]
    const HOMA_NETWORK: Driver = aosabi_carrier_network::driver;
    /// No driver of `GraphTP`(Homa) on Network without the feature
    /// `homa-network` on Linux.
    #[cfg(not(all(feature = "homa-network", target_os = "linux")))]
    const HOMA_NETWORK: Driver = none;

    /// The drivers of the card, each the driver of a carrier its package
    /// names or one that starts nothing.
    const DRIVERS: &[Driver] = &[UNIX_SOCKET, SERIAL, USB, BLUETOOTH, HOMA_NETWORK];

    /// Runs this process as the relay of an endpoint file of the bulk
    /// function where the driver of USB started it as one.
    #[cfg(all(feature = "usb", target_os = "linux"))]
    fn relay() -> Option<ExitCode> {
        aosabi_carrier_usb::relay_from_env()
    }

    /// No relay runs in a build without the driver of USB.
    #[cfg(not(all(feature = "usb", target_os = "linux")))]
    fn relay() -> Option<ExitCode> {
        None
    }

    /// The capabilities of the card, the client of `aosabi-fetch` as the
    /// driver of the network capability under the feature `fetch`.
    #[cfg(feature = "fetch")]
    const CAPABILITIES: Capabilities = Capabilities {
        fetch: Some(aosabi_fetch::driver),
    };

    /// The capabilities of the card, no network without the feature `fetch`,
    /// so an outbound call answers `FetchError::NoNetwork`.
    #[cfg(not(feature = "fetch"))]
    const CAPABILITIES: Capabilities = Capabilities { fetch: None };

    /// Boots the app of `entry` with the configuration of `config` and the
    /// output of the build, and serves it until the process ends, under the
    /// hidden feature `mirror`.
    ///
    /// The boot installs the capabilities of the card before its first pass,
    /// the network under the feature `fetch`. The entry answers an app or a
    /// `Result` of one, [`IntoEntry`], and a configuration the app carries is
    /// left out, since `config` is the configuration of the project.
    ///
    /// The whole `main` of the server crate of a card of a project with
    /// `aosabi.config.rs`:
    ///
    /// ```ignore
    /// fn main() -> std::process::ExitCode {
    ///     aosabi::serve(my_app::default, my_app::config, aosabi::build_output!())
    /// }
    /// ```
    #[cfg(feature = "mirror")]
    #[must_use]
    pub fn serve<E: IntoEntry, C: IntoConfig>(
        entry: fn() -> E,
        config: fn() -> C,
        build: BuildOutput,
    ) -> ExitCode {
        if let Some(code) = relay() {
            return code;
        }
        aosabi_serve::serve_with(entry, config, build, BUILTINS, DRIVERS, CAPABILITIES)
    }

    /// Boots the app of `entry` with the configuration the app carries and
    /// the output of the build, and serves it until the process ends.
    ///
    /// The configuration of an app of the builder of [`crate::app::App`]
    /// stands in the chain of its entry, its plugins and its options, so the
    /// entry runs under the abort hook of a configuration, and an entry that
    /// answers an error or panics ends the boot with the exit 1 and the abort
    /// message of a configuration that throws at load, its cause the text of
    /// the error. An app that carries no configuration boots with the
    /// default one.
    ///
    /// The whole `main` of the server crate of a card:
    ///
    /// ```ignore
    /// fn main() -> std::process::ExitCode {
    ///     aosabi::serve_app(my_app::__aosabi::entry, aosabi::build_output!())
    /// }
    /// ```
    #[must_use]
    pub fn serve_app<E: IntoEntry>(entry: fn() -> E, build: BuildOutput) -> ExitCode {
        if let Some(code) = relay() {
            return code;
        }
        aosabi_serve::serve_app_with(entry, build, BUILTINS, DRIVERS, CAPABILITIES)
    }
}

#[cfg(all(feature = "serve", unix))]
pub use aosabi_serve::{
    BuildOutput,
    NodeServerOptions,
    use_node_server,
    use_node_server_with, //
};

#[cfg(all(feature = "serve", feature = "mirror", unix))]
pub use crate::engine::serve;
#[cfg(all(feature = "serve", unix))]
pub use crate::engine::serve_app;

/// The options of [`serve_static`], the union of those of hono and of the
/// Node adapter, under the feature `serve` on a native target.
#[cfg(all(feature = "serve", unix))]
pub use aosabi_http::middleware::serve_static::ServeStaticOptions;
/// The middleware `serveStatic(options)` of the Node adapter over the file
/// system of the process, which `app.r#use(path, ...)` registers, under the
/// feature `serve` on a native target.
#[cfg(all(feature = "serve", unix))]
pub use aosabi_serve::serve_static::serve_static;

/// Returns the [`BuildOutput`] the build script of the server crate wrote
/// into `OUT_DIR`, the schema, the table of the type resolvers and the name of
/// the application crate.
///
/// The build script calls `aosabi_build::emit_from` over the entry of the
/// application, which writes the three files this macro includes.
#[cfg(all(feature = "serve", unix))]
#[macro_export]
macro_rules! build_output {
    () => {
        $crate::BuildOutput {
            schema: include_str!(concat!(env!("OUT_DIR"), "/schema.graphql")),
            resolve_types: include!(concat!(env!("OUT_DIR"), "/aosabi_resolve_types.rs")),
            name: include_str!(concat!(env!("OUT_DIR"), "/aosabi_name.txt")),
        }
    };
}

/// The items the generated code names that are no part of the interface.
///
/// The glue of a plain crate names the seven types of `alloc` the builder
/// describes, `String`, `Vec`, `VecDeque`, `Box`, `Rc`, `Arc` and `Cow`,
/// through this module and the types of `core` from `::core`, never `::std`
/// or a name of the prelude of the standard library, so a crate without the
/// standard library takes the same glue, whether it names `alloc` by an
/// `extern crate` of its own or not.
#[doc(hidden)]
pub mod __private {
    pub use std::{
        borrow::Cow,
        boxed::Box,
        collections::VecDeque,
        rc::Rc,
        string::String,
        sync::Arc,
        vec::Vec, //
    };

    pub use aosabi_core::glue;
    #[cfg(feature = "federation")]
    pub use aosabi_federation;

    /// The bound of the callback of `create_decorator` under the default
    /// features, which no type implements, so every call answers the message
    /// that names the native form of a decorator.
    #[cfg(not(feature = "mirror"))]
    #[diagnostic::on_unimplemented(
        message = "`create_decorator` is retired, and the body of a method calls its guard function first, as docs/authoring.md section 23 writes it",
        label = "the callback of a decorator of the derive path"
    )]
    pub trait RetiredDecorator {
        /// Returns the decorator of the callback, which no call reaches,
        /// since no type implements the trait.
        fn decorator(self) -> crate::Decorator;
    }
}
