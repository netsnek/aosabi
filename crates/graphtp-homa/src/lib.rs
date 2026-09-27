// SPDX-License-Identifier: MIT
/*
 * graphtp-homa: the binding of GraphTP to Homa, a sans I/O server and client
 * of GraphTP/1 and the one adapter that calls Homa.
 *
 * No analogue in the reference. The specification is docs/graphtp.md, whose
 * section 10.5 names the modules below and their public items, and whose
 * sections 2.4, 2.7, 4 and 5 give the rules they keep.
 */

//! The binding of `GraphTP` to Homa: one Homa RPC carries one exchange, a
//! stream is served by one parked PULL per peer, and a client that cannot know
//! the outcome of an exchange says so.
//!
//! The modules `server`, `handler` and `client` are sans I/O. They decide every
//! answer and every message and make no call of Homa, so a server, a client and
//! a test drive them with the calls of the Homa engine they have. The module
//! [`rpcs`] is the one adapter that calls Homa, with the loops over
//! [`homa::Rpcs`] and over [`homa::Endpoint`] with the server role alone, and a
//! caller that drives its engine itself leaves it unused.
//!
//! The feature `server` builds the modules `server` and `handler` over the
//! server role of `homa`, and the feature `client` builds the module `client`
//! over its client role. Both are on by default, and a build with one role
//! holds the modules of that role alone, so these docs name the modules of a
//! role without a link. The crate builds with no standard library, needs an
//! allocator and takes the crates `graphtp` and `homa` and no other. Its judges
//! are the Jupyter notebooks `tests/notebooks/25_carrier_unix.ipynb` and
//! `tests/notebooks/38_card_limits.ipynb`, and there is no test in this crate,
//! which is the rule of CONTRIBUTING.md section 3.

#![no_std]
#![forbid(unsafe_code)]

// A build with neither role holds no module that allocates.
#[cfg(any(feature = "client", feature = "server"))]
extern crate alloc;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod handler;
pub mod rpcs;
#[cfg(feature = "server")]
pub mod server;
