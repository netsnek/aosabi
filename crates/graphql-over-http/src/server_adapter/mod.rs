// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the server context of a mount and the plugin surface of
 * the server adapter of @whatwg-node/server.
 *
 * No file of its own on the reference side. Each file of the directory names
 * the file of @whatwg-node/server it translates.
 */

//! The server context of a mount and the plugin surface of the server adapter.
//!
//! A mount of an app builds the state of the function once,
//! [`HttpServer::new`](crate::server::HttpServer::new) as
//! `createYoga(options)`, and answers each request with
//! [`crate::answer::answer`] over the server context it builds from the
//! environment and the execution context of that request, [`ServerContext`],
//! as `yoga.fetch(request, env, executionCtx)`. The hooks `onRequest` and
//! `onResponse` of an app's plugin are [`ServerAdapterPlugin`], which the
//! function calls at the points of the adapter (rule 2 of section YOGA of the
//! old page of the plan). This module also holds the types every file of the
//! directory shares: the request handler and the server context a fetch
//! passes.

use std::rc::Rc;

use aosabi_execute::Value;
use aosabi_hooks::{
    HookContext,
    LocalBoxFuture, //
};
use aosabi_http::{
    Env,
    ExecutionCtx,
    Request,
    Response, //
};

pub mod fetch_api;
pub mod plugin;

pub use crate::server_adapter::{
    fetch_api::{
        NO_HEADERS,
        ResponseBody,
        empty_response,
        new_response, //
    },
    plugin::{
        LazyUrl,
        OnRequest,
        OnResponse,
        ServerAdapterPlugin, //
    }, //
};

/// The request handler a request runs, `handle(request, serverContext)`.
pub type RequestHandler =
    Rc<dyn Fn(Rc<Request>, HookContext) -> LocalBoxFuture<'static, Result<Response, Value>>>;

/// What a fetch of the adapter passes beside the request, `yoga.fetch(request, env, executionCtx)`.
///
/// The app passes the environment of hono and its execution context. The
/// reference assigns the own keys of both into one object, the server
/// context. A Rust environment has no own keys to spread, so the server
/// context holds the environment under the key `env` and the execution
/// context under the key `executionCtx`.
#[derive(Clone)]
pub struct ServerContext {
    /// The environment of hono.
    pub env: Env,
    /// The execution context of the platform, when it has one.
    pub execution_ctx: Option<ExecutionCtx>,
}

impl ServerContext {
    /// Returns the server context as the object Yoga extends per request.
    #[must_use]
    pub fn into_context(self) -> HookContext {
        let context = HookContext::new();
        context.extend("env", Rc::new(self.env));
        if let Some(execution_ctx) = self.execution_ctx {
            context.extend("executionCtx", Rc::new(execution_ctx));
        }
        context
    }
}
