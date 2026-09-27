// SPDX-License-Identifier: MIT
/*
 * aosabi-kernel: layer 0 of Aosabi, the core a bare metal build links and
 * nothing above it.
 *
 * No analogue in the reference.
 */

//! Layer 0 of Aosabi.
//!
//! Documentation/architecture.md section 3.1 gives it three things, the framing, the
//! publish primitive and the entry traits. The framing is `AoFrame`, which the crate `homa`
//! holds as `homa::stream`, so one implementation exists, and a build without either
//! feature has none.
//!
//! | Module | Holds | State |
//! | --- | --- | --- |
//! | `stream` | `AoFrame`, delimited and prefixed, the re-export of `homa::stream` | in place under `client` and `homa` |
//! | [`publish`] | the publish primitive, its fan out with the coalescing per slot, the allowlist of a carrier and the lock the environment supplies | in place, the hub under `alloc` |
//! | [`execute_closed`] | the entry below HTTP |  |
//! | [`cbor`] | the CBOR codec with its per operation integer key maps |  |
//! | [`document_table`] | the persisted document table |  |
//!
//! The crate is `no_std`. The `alloc` feature is the heap of the cards that have one, the
//! hub of [`publish`] is the one module behind it, and the crate builds and answers without
//! it, which is what the 2 KB class needs. There is no test in this crate, which is the
//! rule of CONTRIBUTING.md section 3.

#![no_std]

// The hub of `publish` reaches `Vec`, `VecDeque` and `String` through `alloc`,
// which the feature of the same name links.
#[cfg(feature = "alloc")]
extern crate alloc;

pub mod cbor;
pub mod document_table;
pub mod execute_closed;
pub mod publish;
#[cfg(any(feature = "client", feature = "homa"))]
pub use homa::stream;
