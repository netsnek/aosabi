// SPDX-License-Identifier: MIT
/*
 * aoviewer-wasm abi: the nine exports of the module.
 *
 * The names and the signatures are the ones the viewer builds on,
 * ao_allocate, ao_free, ao_feed, ao_build, ao_event, ao_take, ao_export,
 * ao_version and ao_reset, and each forwards to the host code with no unsafe
 * block.
 */

//! The exports of the module.

// Whether the exports move to `#[unsafe(no_mangle)]` is an open question.
// Such exports would be smaller than the glue of `wasm-bindgen`.
