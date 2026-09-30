// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core resolve_info: the store a resolver reads its info and its
 * selected fields from.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/resolve-info.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The resolve info of the running resolver.
//!
//! A TypeScript service calls a resolver with positional arguments and no `info`, so it
//! keeps the `info` of the field in a second `AsyncLocalStorage` and hands it
//! out through `getResolveInfo`. The store is in place for the length of one
//! call of a user function, and it follows an async function across its
//! awaits, which the ambient cell of [`crate::context`] does for both stores.
//!
//! [`ExecutionContext`] is the name the reference gives this store. The
//! executor crate has a type of the same name for the `exeContext` of
//! `execute.js`, and the two never meet in one signature.

use std::rc::Rc;

use aosabi_execute::execute::ResolveInfo;

use crate::{
    context::{
        ambient_execution_context,
        run_with_ambient_execution_context, //
    },
    glue::Thrown,
    resolvers::SelectedField, //
};

/// The message of `getResolveInfo` with no store in place, `resolve-info.ts:19`.
const RESOLVE_INFO_NOT_AVAILABLE: &str = "Resolve info is not available";

/// The store of one resolver call.
pub struct ExecutionContext {
    /// The `info` argument of the field the resolver answers.
    pub info: Rc<ResolveInfo>,
    /// The fields selected below that field, with the injected discriminators.
    pub selected_fields: Rc<Vec<SelectedField>>,
}

/// Returns the store of the running resolver.
///
/// # Errors
///
/// Returns the plain error `Resolve info is not available` when no resolver
/// runs.
pub fn get_resolve_info() -> Result<Rc<ExecutionContext>, Thrown> {
    ambient_execution_context().ok_or_else(|| Thrown::error(RESOLVE_INFO_NOT_AVAILABLE))
}

/// Runs `body` with `context` in place, which is `executionAsyncContext.run(context, body)`.
///
/// The store that was in place before comes back when `body` returns. A
/// future that `body` spawns keeps `context` for all of its polls.
pub fn run_with_execution_context<R>(context: Rc<ExecutionContext>, body: impl FnOnce() -> R) -> R {
    run_with_ambient_execution_context(context, body)
}
