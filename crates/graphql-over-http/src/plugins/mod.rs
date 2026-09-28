// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the plugins of the hook layer the crate offers.
 *
 * No file of its own on the reference side. Each file of the directory names
 * the file it translates.
 */

//! The plugins of the hook layer the crate offers.
//!
//! The introspection refusal is a plugin an app installs when its
//! configuration sets `graphiql: false`, and the cache of parsed and
//! validated documents is the plugin of the hook layer the function places
//! after the plugins of the app.

pub mod disable_introspection;
pub mod use_parser_and_validation_cache;
