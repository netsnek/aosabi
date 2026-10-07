// SPDX-License-Identifier: MIT
/*
 * graphtp-wasm: the wasm module of the npm package graphtp, the codec of
 * GraphTP/1 and the client role of graphtp-homa and homa behind a raw binary
 * interface of plain integers that imports nothing.
 *
 * No analogue in the reference. The module is the one of
 * docs/graphtp-fetch.md section 2.2, which the glue of the package loads and
 * drives: the glue hands it the bytes a link received and the time, and takes
 * the bytes it sends and the events of each exchange.
 */

//! The wasm module of the npm package `graphtp`.
//!
//! The module writes and reads every message of GraphTP/1, codes and decodes
//! its zlib profile, keeps the Homa engine of each link with its batch numbers
//! and its parked PULL, and applies the rule of at most once, so the JavaScript
//! of the package moves bytes and never reimplements a byte of the protocol.
//!
//! The module [`buffer`] holds the memory the host shares with the module,
//! [`link`] the engine of one link, [`codec`] the messages written and read
//! whole, and [`host`] the state of the module and the calls behind the
//! exports. Every other module opens with `#![forbid(unsafe_code)]`.
//!
//! The module `exports` builds for `wasm32-unknown-unknown` alone, under
//! `cfg(all(target_arch = "wasm32", target_os = "unknown"))`. The wider
//! `cfg(target_family = "wasm")` admits WASI and Emscripten, whose C library
//! defines `free` and `abort` that the exports of those names would take the
//! place of, and `wasm64-unknown-unknown`, which this crate never built.
//!
//! The module links the standard library, whose allocator serves `alloc` on
//! `wasm32-unknown-unknown`, since an allocator of the crate's own would need
//! `unsafe`. The judging notebooks are `tests/notebooks/32_graphtp_fetch.ipynb`
//! and the package half of `tests/notebooks/24_graphtp.ipynb`. There is no
//! test in this crate, which is the rule of CONTRIBUTING.md section 3.

pub mod buffer;
pub mod codec;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod exports;
pub mod host;
pub mod link;
