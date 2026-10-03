// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-usb linux app: the fetch of the booted app behind the
 * GraphTP listener, and the randomness the endpoint of the driver starts
 * from.
 *
 * No analogue on the reference side.
 */

//! The fetch of the booted app and the seed of an endpoint.
//!
//! [`app_fetch`] turns the booted app into the fetch function of the
//! listener, the fetch of a carrier of `aosabi_core::graphtp::served` with
//! the driver of a stream body spawned on the runtime of the thread: each
//! OPERATION runs straight through the hooks and the engine of the app with
//! `content-type`, `accept` and the credential as `authorization` as its
//! request, an app whose router holds more than the core answers it as
//! `POST http://localhost/graphql` through its whole pipeline, and the
//! status, the substatus the HTTP layer decides, the media type and the body
//! go back to the listener, a stream body piece by piece. A value the road
//! throws is answered 500 with an empty body, as the Node adapter answers
//! it. [`seed`] reads the first identifier and the key of the table of an
//! endpoint.

use alloc::rc::Rc;
use std::{
    fs::File,
    io::{
        self,
        Read as _, //
    }, //
};

use aosabi_core::{
    BootedApp,
    graphtp::served, //
};
use aosabi_serve::runtime::spawn_local;
use graphql_over_graphtp::Fetch;

/// Returns the fetch function of the listener over the booted app `app`.
#[must_use]
pub fn app_fetch(app: Rc<BootedApp>) -> Fetch {
    served::carrier_fetch(app, spawn_local)
}

/// The first identifier and the key of the table of an endpoint.
#[derive(Clone, Copy, Debug)]
pub struct Seed {
    /// The first client identifier, even and not 0.
    pub first_id: u64,
    /// The key of the table hash.
    pub hash_key: [u8; 16],
}

/// Reads a [`Seed`] from `/dev/urandom`.
///
/// # Errors
///
/// Returns the error of the file.
pub fn seed() -> io::Result<Seed> {
    let mut bytes = [0u8; 24];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let mut first = [0u8; 8];
    first.copy_from_slice(&bytes[..8]);
    let mut hash_key = [0u8; 16];
    hash_key.copy_from_slice(&bytes[8..]);
    // The compact form carries 32 bits of identifier, so the low half is
    // made even and never 0.
    let first_id = u64::from_le_bytes(first) & 0xFFFF_FFFE;
    Ok(Seed {
        first_id: if first_id == 0 { 2 } else { first_id },
        hash_key,
    })
}
