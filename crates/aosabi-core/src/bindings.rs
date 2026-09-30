// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the bindings and the variables of a context.
 *
 * No analogue on the reference side as a file.
 */

//! `Bindings`, `FromBinding` and `Variables`.
//!
//! Declaration merging has no Rust form, so each augmented interface is a
//! struct under a derive. `#[derive(Bindings)]` implements [`Bindings`],
//! whose `current()` reads every field from the environment of the context
//! of the request, and `#[derive(Variables)]` gives every field a typed
//! [`Key`], so `c.set(Variables::requestId, id)` reads as
//! `c.set('requestId', id)` does and is checked.

use aosabi_http::Env;
pub use aosabi_http::Key;

use crate::{
    context::get_context,
    glue::Thrown,
    result::Result, //
};

/// The typed bindings of a service, the twin of `interface Bindings`.
pub trait Bindings: Sized {
    /// Reads the bindings from the environment of the context of the request.
    ///
    /// # Errors
    ///
    /// Returns `Context not defined` outside a request and the error of a
    /// binding a field cannot take.
    fn current() -> Result<Self>;
}

/// A field type of the bindings, read from the environment by name.
pub trait FromBinding: Sized {
    /// Reads the binding `name` from `env`.
    ///
    /// # Errors
    ///
    /// Returns the error of a binding the type cannot take.
    fn from_binding(env: &Env, name: &str) -> Result<Self>;
}

impl FromBinding for String {
    fn from_binding(env: &Env, name: &str) -> Result<Self> {
        env.var(name)
            .ok_or_else(|| Thrown::type_error(&format!("The binding {name} is not set.")))
    }
}

impl FromBinding for Option<String> {
    fn from_binding(env: &Env, name: &str) -> Result<Self> {
        Ok(env.var(name))
    }
}

/// The typed variables of a context, the twin of `interface Variables`.
pub trait Variables {}

/// Returns the binding `name` of the environment of the running request, the
/// read `#[derive(Bindings)]` emits for each field.
///
/// # Errors
///
/// Returns what `getContext()` throws outside a request and what the
/// [`FromBinding`] of `T` returns.
pub fn binding<T: FromBinding>(name: &str) -> Result<T> {
    let context = get_context()?;
    T::from_binding(&context.env(), name)
}
