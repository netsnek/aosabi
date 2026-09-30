// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the environment an application reads.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/get-env.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! `get_env` and `EnvView`.
//!
//! `getEnv()` reads the environment of the context of the request, `context.env`,
//! and the environment of the process outside a request, and it writes `NODE_ENV`
//! onto what it read when the key is unset, with the value of the process or
//! `development`. On Node the environment of a context is the pair the adapter
//! hands `fetch` until `getContext()` replaces it with the process, so a resolver
//! that calls `getEnv()` before any `getContext()` of its request reads the pair
//! and finds no variable but `NODE_ENV`.
//!
//! The write onto the process lands in an overlay of this thread and not in
//! the environment of the process, which Rust changes only through an unsafe
//! call, and every read of the process through [`EnvView`] and through the
//! alias table sees the overlay first.

use core::cell::RefCell;

use aosabi_http::Env;

use crate::context::ambient_context;

thread_local! {
    /// The variables written onto the environment of the process.
    static OVERLAY: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
}

/// Returns the variable `name` the overlay of the process holds.
#[must_use]
pub fn process_overlay(name: &str) -> Option<String> {
    OVERLAY
        .try_with(|overlay| {
            overlay
                .borrow()
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        })
        .ok()
        .flatten()
}

/// Writes the variable `name` into the overlay of the process, which every
/// read of the process through [`EnvView`] and through the alias table sees
/// first.
///
/// A host whose module reads no environment of its own, the module
/// `core.wasm` of the TypeScript option, hands the engine the variables of
/// its process here before the boot, so the logger, the mask of Yoga and
/// `getEnv()` read what a TypeScript service reads on that host.
pub fn set_process_var(name: &str, value: &str) {
    set_process_overlay(name, value);
}

/// Writes the variable `name` into the overlay of the process.
fn set_process_overlay(name: &str, value: &str) {
    let _ = OVERLAY.try_with(|overlay| {
        let mut overlay = overlay.borrow_mut();
        match overlay.iter_mut().find(|(key, _)| key == name) {
            Some(entry) => entry.1 = String::from(value),
            None => overlay.push((String::from(name), String::from(value))),
        }
    });
}

/// Returns the variable `name` of the process, the overlay first.
fn process_var(name: &str) -> Option<String> {
    process_overlay(name).or_else(|| std::env::var(name).ok())
}

/// The environment `getEnv()` answers.
#[derive(Clone, Debug)]
pub struct EnvView {
    /// The environment read.
    env: Env,
    /// The `NODE_ENV` written onto bindings, which are no process.
    node_env: Option<String>,
}

impl EnvView {
    /// Returns the variable `name`, `getEnv()[name]`.
    #[must_use]
    pub fn var(&self, name: &str) -> Option<String> {
        match &self.env {
            Env::Process => process_var(name),
            bindings @ Env::Bindings(_) => {
                if name == "NODE_ENV" && self.node_env.is_some() {
                    return self.node_env.clone();
                }
                bindings.var(name)
            }
        }
    }
}

/// Returns the environment of the running request, `getEnv()`.
///
/// The environment of the context, or of the process outside a request, with
/// `NODE_ENV` written when it is unset.
#[must_use]
pub fn get_env() -> EnvView {
    let Some(context) = ambient_context() else {
        // `context.env` of no context throws, and the catch answers the
        // process as it stands.
        return EnvView {
            env: Env::Process,
            node_env: None,
        };
    };
    let env = context.env();
    let node_env = || {
        process_var("NODE_ENV")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| String::from("development"))
    };
    match &env {
        Env::Process => {
            if process_var("NODE_ENV").is_none_or(|value| value.is_empty()) {
                set_process_overlay("NODE_ENV", &node_env());
            }
            EnvView {
                env,
                node_env: None,
            }
        }
        bindings @ Env::Bindings(_) => {
            let own = bindings.var("NODE_ENV").filter(|value| !value.is_empty());
            EnvView {
                node_env: Some(own.unwrap_or_else(node_env)),
                env,
            }
        }
    }
}
