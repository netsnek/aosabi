// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the result of an authoring function.
 *
 * No analogue on the reference side. The result is the Rust form of a function
 * that may throw.
 */

//! `Result`.
//!
//! A function of the reference may throw any value, and its Rust form returns
//! [`Result`], whose error is the thrown value. `Err(e)?` is the throw of an
//! error type, which converts `e` into the thrown value and returns.

use crate::glue::Thrown;

/// The result of a resolver, a route handler, a gate, a decorator or a setup,
/// `Result<T, Thrown>`.
pub type Result<T> = core::result::Result<T, Thrown>;
