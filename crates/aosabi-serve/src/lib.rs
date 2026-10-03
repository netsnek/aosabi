// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the engine of Aosabi, the part that serves sockets.
 *
 * This file holds the module list. The abort message of the build step is the
 * literal file config_abort_message.rs.
 */

//! The engine of Aosabi.
//!
//! The crate boots the app, runs the runtime of one thread and serves every
//! accepted socket through the fork of hyper, HTTP/1.1 through its Node
//! connection and HTTP/2 through its server over the fork of `h2`, with TLS in
//! between under the feature `tls`. It builds for the native targets of
//! `cfg(unix)`.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `serve` | `cli/builder/bundler/emit-server-glue.ts` | the boot and the serving of one stream |
//! | `use_node_server` | `plugins/use-node-server.ts` | the serving plugin |
//! | `runtime` | no analogue | the executor, the reactors, the timers, the sockets and the runtime traits of hyper |
//! | `listener` | `@hono/node-server`, `server.js` | the TCP listener |
//! | `protocols` | no analogue | the stream type of every socket and the selection of the protocol of an accepted socket |
//! | `node_handler` | `@hono/node-server`, `listener.js` | the service hyper calls |
//! | `node_adapter` | `@hono/node-server` 1.19.9 | the Node adapter that answers a neutral request through the fetch of the booted app |
//! | `serve_body` | no analogue | the body hyper sends |
//! | `tls` | no analogue | TLS over rustls, behind the feature `tls` |
//! | `h2` | no analogue | HTTP/2, behind the feature `http2` |
//! | `build_step` | `cli/builder/bundler/bundler.ts` | the build step and the abort message |
//! | `serve_static` | `@hono/node-server`, `serve-static.js` | the file system provider of `serve_static` |
//! | `modes` | no analogue | the harness modes |
//!
//! Every feature is off by default. The feature `linux` brings the epoll
//! reactor and the peer credential through `rustix` on a Linux target, and the
//! features `h2c` and `tls` each enable `http2`.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebooks are `tests/notebooks/30_http_tier1.ipynb`
//! and `tests/notebooks/33_http2_tls.ipynb`.

pub mod build_step;
#[cfg(feature = "http2")]
pub mod h2;
pub mod listener;
pub mod modes;
pub mod node_adapter;
pub mod node_handler;
pub mod protocols;
pub mod runtime;
pub mod serve;
pub mod serve_body;
pub mod serve_static;
#[cfg(feature = "tls")]
pub mod tls;
pub mod use_node_server;

pub use crate::{
    listener::TcpListen,
    node_handler::NodeHandler,
    protocols::{
        Stream,
        serve_accepted, //
    },
    serve::{
        BuildOutput,
        Capabilities,
        Driver,
        FetchDriver,
        serve_app_with,
        serve_stream,
        serve_with,
        serve_with_reactor, //
    },
    serve_body::ServeBody,
    use_node_server::{
        NodeServerOptions,
        PLUGIN_NAME,
        port_rule,
        use_node_server,
        use_node_server_with, //
    },
};
