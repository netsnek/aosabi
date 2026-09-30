// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the crate of Aosabi an application links.
 *
 * This file holds the module list. The two extracted literals beside it,
 * in_context.graphql and number_scalar_messages.rs, are written by
 * tools/provenance and are no modules of this crate.
 */

//! The core of Aosabi.
//!
//! The crate is the home of everything an application links at run time, which is
//! the application, its plugin contract and its context.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`descriptor`] | no analogue | the static description of a Rust type that the macros of `aosabi-macros` emit and the inference of `aosabi-infer` reads |
//! | [`resolvers`] | the resolver definitions of the core | the resolver wrapping, the selection walk with its two caches and [`ServiceError`] |
//! | [`context`] | `core/context.ts` | the request context and the ambient cell both stores of the reference live in |
//! | [`resolve_info`] | `core/resolve-info.ts` | the store a resolver reads its `info` and its selected fields from |
//! | [`in_context`] | `core/in-context.ts`, `plugins/use-in-context.ts` | the `@inContext` directive as a resolver sees it |
//! | [`create_schema`](mod@create_schema) | the GraphQL handler, `addResolversToSchema.js` | the resolver map and the schema build |
//! | [`scalars`] | the GraphQL handler | the seven bound scalars |
//! | [`service`] | no analogue | the service an application builds and a request or a subscription runs through |
//! | [`request_stream`] | no analogue | the request lines a twin answers without a socket |
//! | [`glue`] | no analogue | the traits the generated code calls |
//! | [`app`] | `app/index.ts`, the GraphQL handler, `app/plugin-order.ts` | the application, its boot and its GraphQL handler |
//! | [`mod@boot`] | no analogue | the schema record of an app whose host hands in its SDL and its IR when it boots, the TypeScript option |
//! | [`cache_budget`] | no analogue | the budget of the request cache of each card |
//! | [`single_pass`] | no analogue | the build settings of the single pass and the bound on nesting of each card |
//! | [`plugin`] | `core/index.ts` | the plugin contract and the config contract |
//! | [`plugins`] | `plugins/use-graphql-error-logger.ts`, `plugins/use-in-context.ts` | the two plugins the core defines |
//! | [`logger`] | `core/logger.ts`, `core/logger-pretty.ts` | the logger |
//! | [`lazy`] | no analogue | `Lazy`, the twin of a property whose value is a function with no parameter |
//! | [`result`] | no analogue | the result of an authoring function |
//! | [`decorator`] | `core/create-decorator.ts` | the decorators |
//! | [`mod@mutation`] | `core/mutation.ts` | the mutation wrapper and its user errors |
//! | [`mod@get_env`] | `core/get-env.ts` | the environment an application reads |
//! | [`bindings`] | `core/context.ts`, the two augmentations | the bindings and the variables of a context |
//! | [`mod@file`] | the web `File` | the run time methods of the marker `File` |
//! | [`remote`] | no analogue, the counterpart of `RemoteRegistry` of `cli/pull/index.ts:162-202` | the contract of a registry `aosabi pull` writes |
//! | [`carrier`] | no analogue | the carrier policy a request carries on its context and the revisions of a hub fed stream |
//! | [`repeater`] | `@repeaterjs/repeater` | the constructor of the typed [`Repeater<T>`](Repeater), [`Push`] and [`Stop`] |
//! | [`pub_sub`] | `@graphql-yoga/subscription` | [`experimental_create_pub_sub`] over the hub of the kernel, [`PubSub`] and [`Topic`] |
//! | [`net`] | no analogue | the network capability, the outbound HTTP call a service asks its context for |
//! | [`random`] | no analogue | the randomness capability, the random bytes a service asks its context for |
//! | [`clock`] | no analogue | the clock capability, the timer a setup awaits outside a request, and the turns of the loop of the runtime |
//! | [`console`] | the `console.log` of the logger and of the serving plugin | the lines the reference writes through the console, and the watcher an SDK reads them with |
//! | `graphtp` | no analogue | the `GraphTP` listener of `graphql-over-graphtp`, behind the feature `homa` |
//!
//! The descriptor types live here and not beside the macros, because a procedural
//! macro crate exports macros and nothing else. The module uses `core` and `alloc`
//! alone, so a split for a bare metal build moves one directory.
//!
//! Every module but [`descriptor`] and `graphtp` sits behind the feature
//! `runtime`, which is on by default. With that feature off the crate builds
//! with no standard library and needs an allocator, which is how a build
//! script links the descriptors and not the executor. The executor crate is
//! re-exported as [`execute`] and the kernel as [`kernel`], so a caller of
//! this crate names one dependency.
//! The module `graphtp` sits behind the feature `homa` and names the crate
//! `graphql-over-graphtp`, which uses `core` and `alloc` alone, so a card with
//! no standard library links the listener beside the descriptors.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebooks are `tests/notebooks/10_schema_tier1.ipynb`
//! for the descriptors, `tests/notebooks/20_execute.ipynb` and
//! `tests/notebooks/31_http_errors.ipynb` for the run time,
//! `tests/notebooks/40_sse.ipynb` and `tests/notebooks/40_pubsub.ipynb` for
//! the repeater, the pub sub and the subscriptions, and
//! `tests/notebooks/25_carrier_unix.ipynb` and
//! `tests/notebooks/38_card_limits.ipynb` for the listener.

#![cfg_attr(not(feature = "runtime"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime")]
pub mod app;
#[cfg(feature = "runtime")]
pub mod bindings;
#[cfg(feature = "runtime")]
pub mod boot;
#[cfg(feature = "runtime")]
pub mod cache_budget;
#[cfg(feature = "runtime")]
pub mod carrier;
#[cfg(feature = "runtime")]
pub mod clock;
#[cfg(feature = "runtime")]
pub mod console;
#[cfg(feature = "runtime")]
pub mod context;
#[cfg(feature = "runtime")]
pub mod create_schema;
#[cfg(feature = "runtime")]
pub mod decorator;
pub mod descriptor;
#[cfg(feature = "runtime")]
pub mod file;
#[cfg(feature = "runtime")]
pub mod get_env;
#[cfg(feature = "runtime")]
pub mod glue;
#[cfg(feature = "homa")]
pub mod graphtp;
#[cfg(feature = "runtime")]
pub mod in_context;
#[cfg(feature = "runtime")]
pub mod lazy;
#[cfg(feature = "runtime")]
pub mod logger;
#[cfg(feature = "runtime")]
pub mod mutation;
#[cfg(feature = "runtime")]
pub mod net;
#[cfg(feature = "runtime")]
pub mod plugin;
#[cfg(feature = "runtime")]
pub mod plugins;
#[cfg(feature = "runtime")]
pub mod pub_sub;
#[cfg(feature = "runtime")]
pub mod random;
#[cfg(feature = "runtime")]
pub mod remote;
#[cfg(feature = "runtime")]
pub mod repeater;
#[cfg(feature = "runtime")]
pub mod request_stream;
#[cfg(feature = "runtime")]
pub mod resolve_info;
#[cfg(feature = "runtime")]
pub mod resolvers;
#[cfg(feature = "runtime")]
pub mod result;
#[cfg(feature = "runtime")]
pub mod scalars;
#[cfg(feature = "runtime")]
pub mod service;
#[cfg(feature = "runtime")]
pub mod single_pass;

#[cfg(feature = "runtime")]
pub use aosabi_execute as execute;
#[cfg(feature = "runtime")]
pub use aosabi_kernel as kernel;
/// The error of graphql-js a resolver throws, and the options it is made
/// with, which `createGraphQLError` of graphql-yoga takes.
#[cfg(feature = "runtime")]
pub use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions, //
};

#[cfg(feature = "runtime")]
pub use crate::{
    app::{
        App,
        BootError,
        BootedApp,
        Builtins,
        Children,
        ExecuteConfigArgs,
        Fragment,
        HandlerOptions,
        IntoApp,
        IntoEntry,
        IntoGate,
        Listen,
        Next,
        NoGraphql,
        PluginError,
        boot,
        current_role,
        execute_config,
        handler,
        role_allows,
        topo_sort_plugins, //
    },
    bindings::{
        Bindings,
        FromBinding,
        Key,
        Variables, //
    },
    context::{
        Context,
        RequestState,
        get_context,
        request_state,
        run_with_context,
        set_context,
        with_context, //
    },
    create_schema::{
        CreateSchemaError,
        ResolverMap,
        create_schema,
        make_executable_schema,
        merge_resolver_maps, //
    },
    decorator::{
        Decorator,
        DecoratorArgs,
        create_decorator, //
    },
    descriptor::Repeater,
    file::FileStream,
    get_env::{
        EnvView,
        get_env, //
    },
    glue::{
        Error,
        MutationPayload,
        Stream,
        Thrown,
        Unchecked,
        thrown::original_host, //
    },
    in_context::{
        IN_CONTEXT_KEY,
        IN_CONTEXT_SDL,
        InContext,
        get_in_context,
        get_locale, //
    },
    logger::{
        LogLevel,
        Logger,
        LoggerConfig,
        LoggerOption,
        configure_logger,
        get_logger,
        get_root_logger, //
    },
    mutation::{
        UserError,
        mutation,
        to_user_errors, //
    },
    plugin::{
        AppConfig,
        BuildContext,
        BuildController,
        IntoConfig,
        IntoPlugin,
        Plugin,
        PluginList,
        PluginObject,
        PluginRole,
        PluginSlot,
        Role,
        Strategy,
        on_execute_done, //
    },
    plugins::{
        use_graphql_error_logger,
        use_in_context, //
    },
    pub_sub::{
        Payload,
        PubSub,
        SharedPayload,
        StdLock,
        Topic,
        TopicId,
        experimental_create_pub_sub, //
    },
    repeater::{
        Push,
        PushFuture,
        Stop,
        StopFuture, //
    },
    resolve_info::{
        ExecutionContext,
        get_resolve_info,
        run_with_execution_context, //
    },
    resolvers::{
        CallState,
        Resolvers,
        SelectedField,
        ServiceError,
        ServiceErrorExtensions,
        get_selected_fields,
        resolvers_to_graphql_resolvers,
        wrap_resolver, //
    },
    result::Result,
    service::{
        CoreState,
        Service,
        ServiceBuilder,
        ServiceStream, //
    },
};
