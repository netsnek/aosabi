// SPDX-License-Identifier: MIT
/*
 * aoviewer-wasm: the wasm module of aoviewer, nine exports over the core.
 *
 * No file of the reference is translated here. The module is a cdylib for
 * wasm32-unknown-unknown with a raw binary interface of nine exports and zero
 * imports, and no generated glue.
 */

//! The wasm module of aoviewer.
//!
//! The module `abi` holds the exports and the module `host` the safe code they
//! forward to. The host build of the crate is an rlib, so the seam of the
//! notebooks can link the same code the module carries. The exports wait for a
//! named exception to the lint, because a symbol name is an unsafe attribute
//! in edition 2024 and the workspace forbids `unsafe_code`.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3, and no notebook judges the module yet.

mod abi;
mod host;
