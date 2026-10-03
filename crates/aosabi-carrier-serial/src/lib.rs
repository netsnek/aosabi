// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-serial: the carrier Serial Line.
 *
 * This file holds the module list.
 */

//! The carrier Serial Line.
//!
//! A UART carries compact Homa packets of at most 256 bytes in delimited
//! `AoFrame`, 8N1 with RTS and CTS where wired, and reports no event of its
//! link, so a client that restarts on it sends a CANCEL of handle 0 before
//! its first OPERATION. The crate gives the values of the link as functions a
//! device card fills, so no carrier gets an engine of its own, and serves the
//! general build on Linux through `driver` in the shape of the engine of
//! `aosabi-serve`, a tty served by the `GraphTP` listener of
//! `graphql-over-graphtp`.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `profile` | `profile`, `limits` and `MAX_PACKET`, core only |
//! | `linux` | `Tty`, `serve`, `driver` and `SerialListen`, under the feature `linux` |
//! | `bare` |  |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebook is `tests/notebooks/26_carrier_serial.ipynb`.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(feature = "linux")]
extern crate alloc;
#[cfg(feature = "linux")]
extern crate std;

#[cfg(feature = "bare")]
mod bare;
#[cfg(feature = "linux")]
mod linux;
#[cfg(feature = "linux")]
pub use linux::*;
mod profile;
pub use profile::*;
