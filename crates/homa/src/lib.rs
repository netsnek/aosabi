// SPDX-License-Identifier: MIT
/*
 * homa: the Homa transport protocol over any carrier.
 */

//! The Homa transport protocol over any carrier that moves whole packets.

#![no_std]
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(any(feature = "unix", feature = "ip", feature = "kernel-socket"))]
extern crate std;

pub mod abi;
mod endpoint;
pub use endpoint::*;
pub mod grant;
pub mod heap;
#[cfg(feature = "ip")]
pub mod ip;
#[cfg(feature = "kernel-socket")]
pub mod ksock;
pub mod pacer;
#[cfg(feature = "prio")]
pub mod prio;
#[cfg(feature = "sim")]
pub mod sim;
pub mod siphash;
mod store;
pub use store::*;
#[cfg(any(feature = "stream-delimited", feature = "stream-prefixed"))]
pub mod stream;
pub mod table;
mod types;
pub use types::*;
#[cfg(feature = "unix")]
pub mod unix;
pub mod wire;
