// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-network: the carrier Network.
 *
 * This file holds the module list.
 */

//! The carrier Network.
//!
//! `GraphTP`(Homa) reaches Network over IP protocol 146 in the full form of
//! `homa_wire.h`, never over UDP, on the crate's own engine over a raw socket
//! by default, and on a host that loads the crate's kernel module over its
//! socket from stage H5 on. A peer is an address with its Homa port, so two
//! client processes of one host are two peers to a server. The Homa port of a
//! service is its HTTP port, and a card names how the path under Homa is
//! protected, since Homa carries no security of its own. The crate serves the
//! general build on Linux through `driver` in the shape of the engine of
//! `aosabi-serve`, beside the HTTP(TCP) listener of the same booted app.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `network` | `Path`, `Protection`, `Requirement`, `HomaNetwork`, `NetworkError`, `profile` and the configuration of a card's endpoint |
//! | `homa_ip` | `serve_ip` over the raw socket of `homa::ip`, its task and its counts, under the feature `homa-ip` |
//! | `homa_kernel` | `serve_kernel` over the socket of the kernel module, under the feature `homa-kernel`, which stage H5 writes |
//! | `serve` | `driver` and `HomaListen`, the driver `serve_with` starts |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebook is `tests/notebooks/29_homa_raw.ipynb`.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(any(feature = "homa-ip", feature = "homa-kernel"))]
extern crate alloc;
#[cfg(any(feature = "homa-ip", feature = "homa-kernel"))]
extern crate std;

#[cfg(feature = "homa-ip")]
mod homa_ip;
#[cfg(feature = "homa-ip")]
pub use homa_ip::*;
#[cfg(feature = "homa-kernel")]
mod homa_kernel;
#[cfg(any(feature = "homa-ip", feature = "homa-kernel"))]
mod network;
#[cfg(any(feature = "homa-ip", feature = "homa-kernel"))]
pub use network::*;
#[cfg(any(feature = "homa-ip", feature = "homa-kernel"))]
mod serve;
#[cfg(any(feature = "homa-ip", feature = "homa-kernel"))]
pub use serve::*;
