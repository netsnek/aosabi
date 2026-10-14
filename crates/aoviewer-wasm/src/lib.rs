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
//! notebooks can link the same code the module carries.
//!
//! The judging notebooks are `tests/notebooks/51_viewer_core.ipynb` and
//! `tests/notebooks/75_docgen.ipynb`. There is no test in this crate, which
//! is the rule of CONTRIBUTING.md section 3.

mod abi;
mod host;
