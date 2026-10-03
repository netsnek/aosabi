// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-unix: the carrier Unix Socket.
 *
 * The carrier follows section 3.2 of the Homa design note. This file holds
 * the module list.
 */

//! The carrier Unix Socket.
//!
//! The crate serves HTTP(Unix Socket) through `serve_stream` of
//! `aosabi-serve`, so a socket answers exactly what a TCP socket answers, and
//! on a Linux card it checks the uid of every peer. The facade hands its
//! driver to the boot, so `aosabi-serve` never names this crate.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `http` | the listener of HTTP(Unix Socket) |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebook is `tests/notebooks/30_http_tier1.ipynb`.

pub mod http;
