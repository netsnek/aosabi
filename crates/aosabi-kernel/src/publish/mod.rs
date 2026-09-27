// SPDX-License-Identifier: MIT
/*
 * aosabi-kernel publish: the publish primitive, the fan out with its
 * coalescing per slot, the allowlist of a carrier and the lock the
 * environment supplies.
 *
 * No analogue in the reference, whose pub sub dispatches on an EventTarget. The
 * allowlist and the lock build without alloc, and the hub needs it.
 */

//! The publish primitive of the kernel.
//!
//! Any part of a service publishes through it, a resolver, a timer or a driver
//! with no request, and every protocol of a build reads the same events from it.
//! The module has no thread, no clock, no atomic and no dependency, and it names
//! no type of a layer above, so a carrier crate and a build with no HTTP linked
//! use it as they are.
//!
//! | Module | Holds | Needs |
//! | --- | --- | --- |
//! | [`allow`] | [`CarrierId`], [`Allow`], [`CarrierPolicy`], [`REFUSED_ON_CARRIER`] and [`write_refusal`] | `core` |
//! | [`lock`] | [`Lock`] and [`LocalLock`] | `core` |
//! | `hub` | `Hub`, `SlotId`, `Delivered`, `Published`, `Take`, `SlotState` and `publish` over a [`Lock`] | `alloc` |
//!
//! # The rules
//!
//! The plan of the kernel numbers the rules of the module, and the push model of
//! the harness states them in the same words. Rules 1 to 5 are the hub's and
//! stand in its module doc, rule 9 is the allowlist's and stands in [`allow`],
//! and rule 8 is the one of [`Revision`]:
//!
//! 8. The revision rides on every delivered event for the pump and the devtool,
//!    and it reaches no frame of HTTP(TCP) and no event of GraphTP(Homa), which
//!    carry only what the payload carries. In the lean
//!    frame of `GraphTP` an entry `next` holds the bytes after `data: ` of one
//!    `next` event of the service's stream verbatim, and a stream entry carries
//!    a handle and a batch count `seq` and no revision. The revision is the
//!    hub's, so a service has one counter per pub sub, and a service with one
//!    pub sub, which every example and the wasabi service write, has the
//!    one global counter of ADR-0017, Rules kept from the prototype.

pub mod allow;
#[cfg(feature = "alloc")]
pub mod hub;
pub mod lock;

#[cfg(feature = "alloc")]
pub use self::hub::{
    Delivered,
    Hub,
    Published,
    SlotId,
    SlotState,
    Take,
    publish, //
};
pub use self::{
    allow::{
        Allow,
        CarrierId,
        CarrierPolicy,
        REFUSED_ON_CARRIER,
        write_refusal, //
    },
    lock::{
        LocalLock,
        Lock, //
    }, //
};

/// A revision of the service's state, one per changed section.
///
/// A publish of the hub increments it once, and a slot listens on one topic, so
/// one topic is one section. A hub starts at 0, and its first publish carries 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(pub u64);
