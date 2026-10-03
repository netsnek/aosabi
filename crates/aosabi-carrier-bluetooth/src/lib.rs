// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-bluetooth: the carrier Bluetooth.
 *
 * This file holds the module list.
 */

//! The carrier Bluetooth.
//!
//! The GATT data path carries one compact Homa packet in one ATT value, RX
//! written without response and TX notified, with LIMITS as the first
//! notification, and every card with Bluetooth links it unless it switches
//! the feature `gatt` off. The L2CAP LE credit based channel on the fixed
//! SPSM 0x00E0, published in a characteristic that requires encryption,
//! carries prefixed `AoFrame` in both directions. The crate gives the values
//! of both links as functions a device card fills, the seam of a bare metal
//! LE host, and serves the general build on Linux through `driver` in the
//! shape of the engine of `aosabi-serve`, `BlueZ` reached through a D-Bus
//! client written in the project and the `GraphTP` listener of
//! `graphql-over-graphtp` in front.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `profile` | `SPSM`, the UUIDs, `channel_max_packet`, `channel_profile`, `gatt_profile`, and `Pace`, the pace of the GATT data path, with `TX_DEPTH` and the planning link, core only |
//! | `host` | the trait `LeHost`, the seam of a bare metal LE host, core only |
//! | `linux` | the D-Bus client, `Gatt` held to the depth of its pace, `serve_gatt`, whose task wakes at the slots of that pace, `serve_channel` and `driver`, under the feature `linux` |
//! | `bare` |  |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebook is
//! `tests/notebooks/28_carrier_bluetooth.ipynb`.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(feature = "linux")]
extern crate alloc;
#[cfg(feature = "linux")]
extern crate std;

#[cfg(feature = "bare")]
mod bare;
mod host;
pub use host::*;
#[cfg(feature = "linux")]
mod linux;
#[cfg(feature = "linux")]
pub use linux::*;
mod profile;
pub use profile::*;
