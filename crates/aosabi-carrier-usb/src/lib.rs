// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-usb: the carrier USB.
 *
 * This file holds the module list.
 */

//! The carrier USB.
//!
//! USB bulk is a vendor interface with one bulk IN and one bulk OUT endpoint,
//! one compact Homa packet a transfer, a transfer that is an exact multiple of
//! `wMaxPacketSize` below the largest packet ended by a zero length packet.
//! USB serial is the tty of the ACM function with delimited `AoFrame`. The
//! crate gives the values of both links as functions a device card fills, so
//! no carrier gets an engine of its own, and serves the general build on
//! Linux through `driver` in the shape of the engine of `aosabi-serve`, the
//! gadget side served by the `GraphTP` listener of `graphql-over-graphtp`.
//! The vendor and the product id belong to the gadget the card's setup
//! composes and never to this crate.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `profile` | `Speed`, `bulk_profile`, `serial_profile` and `needs_zlp`, core only |
//! | `linux` | `Bulk`, `GadgetTty`, `serve_bulk`, `serve_serial`, the relay of an endpoint and `driver`, under the feature `linux` with `bulk` and `serial` |
//! | `bare` |  |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebook is `tests/notebooks/27_carrier_usb.ipynb`.

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
