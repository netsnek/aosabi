// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs: the documentation generator of Aosabi.
 *
 * The generator is a step of the command line and not a build script, because
 * a build script should not write outside its own output directory.
 */

//! The documentation generator of Aosabi.
//!
//! The generator reads a schema from a build directory, an `ir.json`, an SDL
//! file or an introspection result, renders the tree of its reference through
//! `aoviewer-core`, and writes the tree or checks it against the tree on
//! disk. It runs on the host alone, and no served crate depends on it.
//!
//! The judging notebook is `tests/notebooks/75_docgen.ipynb`. There is no
//! test in this crate, which is the rule of CONTRIBUTING.md section 3.

pub mod check;
pub mod config;
pub mod device;
pub mod error;
pub mod generate;
pub mod input;
pub mod request;
pub mod since;
pub mod toml;
pub mod write;
