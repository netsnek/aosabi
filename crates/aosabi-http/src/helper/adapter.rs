// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the environment of a request as the runtime gives it.
 *
 * Derived from hono 4.11.9, dist/helper/adapter/index.js, MIT.
 */

//! `env` and `get_runtime_key`.
//!
//! The helper `env(c)` answers the environment of the runtime: the variables
//! of the process on Node, and the bindings of the context on workerd. A build
//! with the Node adapter runs on the server cards, whose runtime key is
//! `node`, and a build without it is the Worker, whose key is `workerd`.

use crate::context::{
    Context,
    Env, //
};

/// Returns the key of the runtime, `getRuntimeKey()`.
#[must_use]
pub fn get_runtime_key() -> &'static str {
    if cfg!(feature = "node-server") {
        "node"
    } else {
        "workerd"
    }
}

/// Returns the environment of the runtime for `c`, `env(c)`.
///
/// The process on Node and the bindings of the context on workerd.
#[must_use]
pub fn env(c: &Context) -> Env {
    match get_runtime_key() {
        "node" => Env::Process,
        _ => c.env(),
    }
}
