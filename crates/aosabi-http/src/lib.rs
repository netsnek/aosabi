// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the HTTP layer of Aosabi below the server.
 *
 * No analogue of its own on the reference side. This file holds the module
 * list, each module behind its feature.
 */

//! The HTTP layer of Aosabi below the server.
//!
//! The HTTP/1.1 and HTTP/2 server is the fork of hyper under
//! `third_party/rust/`, which `aosabi-serve` drives. This crate holds what the
//! translation of a request into an answer needs of HTTP on every target, the
//! ESP32-C3 and a Worker among them, and names no fork.
//!
//! | Module | Feature | Holds |
//! | --- | --- | --- |
//! | `coding` | `deflate` | the zlib wrapper of HTTP, the content coding `deflate`, around `graphtp::deflate` |
//! | `message` | `messages` | the neutral messages the Node adapter of `aosabi-serve` and the serve layer exchange |
//! | `node` | `node`, which enables `messages` | the tables of Node and the helpers of the record of the Node adapter |
//! | `fetch` | `web` | the Fetch types of a request and its answer, as the Node adapter fills them |
//! | `web_app` | `web` | [`WebApp`], the app of routes and middleware of `hono-base.js`, and its handler traits |
//! | `compose` | `web` | the chain of the middleware of `compose.js` |
//! | `context` | `web` | the context of a request of `context.js` |
//! | `date` | `web` | the time value of a `Date` and the wall clock, ECMA-262 `TimeClip` and `Date.now` |
//! | `request` | `web` | [`RequestView`], the view of a request the handlers read, `c.req()` of `request.js` |
//! | `router` | `web` | the trie router of `router/trie-router/` |
//! | `utils` | `web` | the path and the query of a request URL, and the modules `cookie`, `mime` and `filepath` |
//! | `http_exception` | `web` | an error that carries its answer, `http-exception.js` |
//! | `helper` | `web` | the `html` template, `env` and the cookie helpers |
//! | `middleware` | `web` | `compress`, `except`, `cors`, `csrf` and `body_limit`, and the module `serve_static` |
//! | `path` | `web` | `join` and `normalize` of `path.posix` of node 24.13.0 |
//!
//! The features `deflate`, `messages` and `node` are on by default, and every
//! crate that names this one turns them off and names the features it needs.
//! The items the interfaces use are reached by the path of their module and at
//! the root of the crate, each behind the feature of its module.
//!
//! The app of `aosabi-core` runs on it and the GraphQL layer of
//! `graphql-over-http` names its Fetch types, so it stands below both, and the
//! Node adapter of `@hono/node-server` 1.19.9 that writes its answers stands
//! in `aosabi-serve`. Under it the feature `node-server` makes the Fetch types
//! answer as the Node adapter fills them, the platform [`NODE_SERVER`] names,
//! `web-deflate` links the encoder of `compress()`, `platform-coding` hands
//! the coding of `compress()` to the host, which is the Worker's way,
//! `web-deflate` winning where both are on, and `host-platform` is the
//! platform of the host of the TypeScript option. Every handler and middleware
//! future of the layer is `'static` and not `Send`, because a served binary
//! runs one thread.
//!
//! Without the feature `web` the crate builds with no standard library and
//! needs an allocator, because the ESP32-C3 builds it for
//! `riscv32imc-unknown-none-elf`, and then it holds no `Arc`, opens no socket,
//! reads no clock and starts nothing. The web layer reads the clock, the
//! environment and the cells of a thread of the standard library on a target
//! with an operating system. On a target with none, `target_os = "none"`, the
//! crate stays without the standard library whatever its features, and the web
//! layer builds there once its files name `core` and `alloc` alone, with a
//! board form of the clock, of the environment and of the cells of a thread
//! beside each host form.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebooks are `tests/notebooks/30_http_tier1.ipynb`
//! for the tables, the record and the web layer, and
//! `tests/notebooks/37_content_coding.ipynb` and
//! `tests/notebooks/35_http_budget.ipynb` for the wrapper.

#![cfg_attr(any(not(feature = "web"), target_os = "none"), no_std)]

// A build with every feature off holds no module, and a crate that names this
// one with default-features = false and no feature gets that build, so whether
// alloc is used depends on the features, which is the case for allow over
// expect in Documentation/coding-style.md section 9.
#[allow(
    unused_extern_crates,
    reason = "alloc is used by the modules of the features alone"
)]
extern crate alloc;

#[cfg(feature = "deflate")]
pub mod coding;
#[cfg(feature = "web")]
pub mod compose;
#[cfg(feature = "web")]
pub mod context;
#[cfg(feature = "web")]
pub mod date;
#[cfg(feature = "web")]
pub mod fetch;
#[cfg(feature = "web")]
pub mod helper;
#[cfg(feature = "web")]
pub mod http_exception;
#[cfg(feature = "messages")]
pub mod message;
#[cfg(feature = "web")]
pub mod middleware;
#[cfg(feature = "node")]
pub mod node;
#[cfg(feature = "web")]
pub mod path;
#[cfg(feature = "web")]
pub mod request;
#[cfg(feature = "web")]
pub mod router;
#[cfg(feature = "web")]
pub mod utils;
#[cfg(feature = "web")]
pub mod web_app;

/// Whether the Fetch types answer as the Node adapter fills them, the platform a Fetch ponyfill tells apart.
///
/// `@whatwg-node/fetch` answers the ponyfill of `@whatwg-node/node-fetch` on
/// Node and the platform's own classes elsewhere, and the feature
/// `node-server` is on exactly on the cards that serve through the Node
/// adapter of `aosabi-serve`, so a crate above the web layer reads the
/// platform here.
#[cfg(feature = "web")]
pub const NODE_SERVER: bool = cfg!(feature = "node-server");

#[cfg(feature = "messages")]
pub use crate::message::{
    BodyError,
    ChunkError,
    ChunkSource,
    EmptyBody,
    FirstWrite,
    IncomingBody,
    Line,
    NodeBody,
    NodeIncoming,
    NodeOutgoing, //
};
#[cfg(feature = "node")]
pub use crate::node::{
    RecordError,
    STATUS_CODES,
    append_cached_length,
    append_read_length,
    build_outgoing_http_headers,
    check_record,
    status_text,
    value_bytes, //
};
// The error of a Fetch body is `fetch::body::BodyError`, since the name at the
// root is the one of the neutral messages.
#[cfg(feature = "web")]
pub use crate::{
    compose::{
        Next,
        compose, //
    },
    context::{
        Arg,
        Context,
        Env,
        ExecutionCtx,
        Html,
        Key,
        Vars, //
    },
    fetch::{
        body::{
            Body,
            BodyStream,
            LocalBoxFuture,
            Stream, //
        },
        headers::Headers,
        request::Request,
        response::{
            FetchError,
            HeadersInit,
            Response,
            ResponseInit, //
        },
        url::{
            Url,
            parse_url,
            url_search_params, //
        }, //
    },
    helper::html::{
        HtmlEscaped,
        html,
        raw, //
    },
    http_exception::HttpException,
    middleware::{
        body_limit::body_limit,
        combine::except,
        compress::{
            compress,
            compress_with, //
        },
        cors::cors,
        csrf::csrf, //
    },
    request::RequestView,
    web_app::{
        ErrorHandler,
        HandlerFn,
        MiddlewareFn,
        RouteError,
        RouteRecord,
        WebApp, //
    },
};
