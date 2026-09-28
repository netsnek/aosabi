// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the four plugins of the hook layer, the engine, schema,
 * context and masked errors plugins.
 *
 * No file of its own on the reference side. Each file of the directory names
 * the file of @envelop/core 5.5.1 whose answers it follows.
 */

//! The engine, schema, context and masked errors plugins.

pub mod use_engine;
pub mod use_extend_context;
pub mod use_masked_errors;
pub mod use_schema;
