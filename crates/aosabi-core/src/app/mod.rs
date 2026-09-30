// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the application and its boot.
 *
 * No file of its own on the reference side.
 */

//! The application.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `entry` | none, the Rust form of the default export of a TypeScript app | [`IntoApp`], [`IntoEntry`], [`Fragment`], [`NoGraphql`] and [`Children`] |
//! | `application` | `app/index.ts:27-384` | [`App`], [`Composed`], [`Gate`], [`IntoGate`], [`Listen`], [`BootedApp`], [`Next`], [`merge_fragment`] and [`gate_resolvers`] |
//! | `request_id` | `app/index.ts:12-25` | [`fn@request_id`] and [`new_id`] |
//! | `handler` | the GraphQL handler, lines 25-80 and 190-386 | [`fn@handler`], [`HandlerOptions`], [`build_schema`] and [`build_http_options`] |
//! | `execute_config` | the GraphQL handler, lines 82-188, `emit-server-glue.ts:146-158` | [`fn@execute_config`], [`ExecuteConfigArgs`], [`Builtins`], [`boot`], [`BootError`], [`load_plugins_middleware`], [`current_role`] and [`role_allows`] |
//! | `plugin_order` | `app/plugin-order.ts:1-45` | [`topo_sort_plugins`], [`SortedOrder`] and [`PluginOrderError`] |
//! | `route_error` | none | `impl RouteError for Thrown` and [`PluginError`] |
//! | `env_alias` | `docs/cli-and-targets.md` section 6.2 | [`env_alias::var`] |

pub mod application;
pub mod entry;
pub mod env_alias;
pub mod execute_config;
pub mod handler;
pub mod plugin_order;
pub mod request_id;
pub mod route_error;

pub use self::{
    application::{
        App,
        BootedApp,
        Composed,
        Gate,
        GateCall,
        IntoGate,
        Listen,
        Next,
        gate_resolvers,
        merge_fragment, //
    },
    entry::{
        Children,
        Fragment,
        IntoApp,
        IntoEntry,
        NoGraphql, //
    },
    execute_config::{
        BootError,
        Builtins,
        ExecuteConfigArgs,
        boot,
        current_role,
        execute_config,
        load_plugins_middleware,
        role_allows, //
    },
    handler::{
        HandlerOptions,
        build_http_options,
        build_schema,
        handler, //
    },
    plugin_order::{
        PluginOrderError,
        SortedOrder,
        topo_sort_plugins, //
    },
    request_id::{
        new_id,
        request_id, //
    },
    route_error::PluginError,
};
