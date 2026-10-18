// SPDX-License-Identifier: MIT
/*
 * aosabi-infer-wasm: the module infer.wasm of the npm package aosabi-build,
 * the schema inference of Aosabi over the live checker of typescript behind
 * the host boundary of the author mode of the TypeScript option.
 *
 * No analogue in the reference.
 */

//! The module `infer.wasm` of the npm package `aosabi-build`.
//!
//! The command `aosabi build` of the package makes a `ts.Program` of an app and
//! hands this module the questions of the inference to answer. Then it prints
//! the schema text, writes the JSON form of the IR through `aosabi-ir`, writes
//! `resolvers.js` and validates the schema text through the writers of
//! `aosabi-build`, so the bytes of `.aosabi/schema.graphql`, `ir.json` and
//! `resolvers.js` are the bytes the Rust option writes for a twin.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `abi` | The three export lines and the one import block, where `unsafe_code` is expected |
//! | `host` | The input and output buffers and the reader and writer of a record |
//! | `source` | The type source of the host, `TypeSource` over the imports |
//! | `infer` | The run of the inference, the writers and the answer record |
//!
//! The crate takes the two exceptions of `aosabi-js` and its three conditions.
//! The host runs the JavaScript of every import inside a `try`, so no
//! exception of the checker crosses a frame of the module, and answers a call
//! that throws with an empty answer while it keeps the failure, which fails
//! the build once the export returns. A trap of the module ends the build with
//! the trap, and the host calls that instance no more. Every range the host
//! writes is the input buffer the export `alloc` sizes and exposes through
//! `as_mut_ptr`, written while no reference of the module to it is alive.
//!
//! The crate root admits `wasm32-unknown-unknown` alone, as `graphtp-wasm`
//! does, since a C library of WASI or Emscripten defines symbols an export
//! could take the place of. Every module but this root and `abi` forbids
//! `unsafe_code`, and no unsafe block exists. The root holds no code but the
//! declarations of its modules, and a forbid there would bind `abi` as well.
//! The judging notebooks are `tests/notebooks/10_schema_tier1.ipynb`, whose
//! side `ts` holds the three files of `aosabi build` to the sidecar for every
//! fixture G8 counts, `tests/notebooks/39_ts1_parity.ipynb` in the author
//! mode, and `tests/notebooks/08_card_builds.ipynb` and
//! `tests/notebooks/32_graphtp_fetch.ipynb`, which count the boundary, and
//! there is no test in this crate, the rule of `CONTRIBUTING.md` section 3.
#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]

mod abi;
mod host;
mod infer;
mod source;
