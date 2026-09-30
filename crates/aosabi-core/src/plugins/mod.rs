// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the plugins the core defines.
 *
 * No file of its own on the reference side.
 */

//! The plugins of the core.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `graphql_error_logger` | `plugins/use-graphql-error-logger.ts` | [`use_graphql_error_logger`] |
//! | `in_context` | `plugins/use-in-context.ts` | [`use_in_context`] |

pub mod graphql_error_logger;
pub mod in_context;

pub use self::{
    graphql_error_logger::use_graphql_error_logger,
    in_context::use_in_context, //
};
