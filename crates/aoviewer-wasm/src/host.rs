// SPDX-License-Identifier: MIT
/*
 * aoviewer-wasm host: the safe code behind the nine exports.
 *
 * The schema, the options, the input buffer and the output buffer live in one
 * cell, which a module without threads holds as plain state and which the
 * host rlib compiles as well.
 */

//! The state and the calls behind the exports.
