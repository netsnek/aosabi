// SPDX-License-Identifier: MIT
/*
 * aosabi-kernel publish allow: the carriers and the subscription root fields
 * each of them serves.
 *
 * No analogue in the reference. The allowlist exists on the smallest build, so
 * it uses core alone, and the carriers take the order of ADR-0023 with LoRa
 * the sixth.
 */

//! The allowlist of a carrier.
//!
//! A carrier with a small budget refuses a subscription of a high rate rather
//! than dropping its frames, so a card names, per carrier, the subscription
//! root fields it serves and how many events one subscriber keeps in order
//! before the hub coalesces them. The module names `core` alone.
//!
//! # The rule
//!
//! The rule carries the number of the plan's section KERNEL:
//!
//! 9. The allowlist is consulted before a subscription runs, and a refusal
//!    never reaches the executor. [`CarrierPolicy::NETWORK`] allows every
//!    field with depth 1024, so no byte of HTTP(TCP) moves, and a device card
//!    sets the policy of its other carriers, a depth of 1 and the fields its
//!    link budget allows. `GraphTP` never drops, merges or reorders an event,
//!    so the coalescing a depth causes is the publish primitive's alone, below
//!    every protocol. The replay of `32_graphtp_replay` meets none, because
//!    its one stream case, the golden `03d1b0055e5b` of `ticks`, publishes
//!    nothing.

use core::fmt;

/// A carrier, in the order of ADR-0023, `LoRa` the sixth.
///
/// The value is the place of the carrier in that order counted from 0, so a
/// card names a carrier in one byte, and the six constants are every value
/// there is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CarrierId(u8);

impl CarrierId {
    /// Network, any link with an IP stack.
    pub const NETWORK: Self = Self(0);
    /// Unix Socket, a socket in the file system of one host.
    pub const UNIX_SOCKET: Self = Self(1);
    /// USB, USB serial and USB bulk.
    pub const USB: Self = Self(2);
    /// Bluetooth.
    pub const BLUETOOTH: Self = Self(3);
    /// Serial Line.
    pub const SERIAL_LINE: Self = Self(4);
    /// `LoRa`, point to point.
    pub const LORA: Self = Self(5);

    /// Returns the proper name, `Network`, `Unix Socket`, `USB`, `Bluetooth`, `Serial Line` or `LoRa`.
    ///
    /// ADR-0023 writes each carrier as a proper name without an article, and
    /// the refusal of [`write_refusal`] spells it so.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self.0 {
            0 => "Network",
            1 => "Unix Socket",
            2 => "USB",
            3 => "Bluetooth",
            4 => "Serial Line",
            // The field is private, so 5 is the one value left.
            _ => "LoRa",
        }
    }
}

/// The subscription root fields a carrier serves.
#[derive(Clone, Copy, Debug)]
pub enum Allow {
    /// Every field.
    All,
    /// The fields named, each the name of a root field of the `Subscription` type.
    Only(&'static [&'static str]),
}

impl Allow {
    /// Returns whether the subscription root field `field` is allowed.
    ///
    /// A name compares byte for byte, as a field name of GraphQL does.
    #[must_use]
    pub fn allows(&self, field: &str) -> bool {
        match self {
            Self::All => true,
            Self::Only(fields) => fields.contains(&field),
        }
    }
}

/// What a carrier grants a subscription: which fields, and how many events one subscriber keeps.
///
/// The `depth` is the bound of rule 2 of the hub, the count of events a slot
/// keeps in publish order before the newest pending one is replaced, and a
/// depth of 0 keeps one as a depth of 1 does.
#[derive(Clone, Copy, Debug)]
pub struct CarrierPolicy {
    /// The carrier the policy belongs to.
    pub carrier: CarrierId,
    /// The fields the carrier serves.
    pub allow: Allow,
    /// The events one subscriber keeps in order before the hub coalesces.
    pub depth: u16,
}

impl CarrierPolicy {
    /// Network with HTTP(TCP): every field and the queue bound of the reference repeater.
    ///
    /// The depth is `MAX_QUEUE_LENGTH` of `@repeaterjs/repeater` 3.0.6, 1024,
    /// so a subscriber of Network keeps what the repeater of the reference
    /// keeps before it overflows.
    pub const NETWORK: Self = Self {
        carrier: CarrierId::NETWORK,
        allow: Allow::All,
        depth: 1024,
    };
}

impl Default for CarrierPolicy {
    /// Returns [`CarrierPolicy::NETWORK`], the policy of a request no carrier set one for.
    fn default() -> Self {
        Self::NETWORK
    }
}

/// The name of a refused subscription in the listener's log line.
///
/// On GraphTP(Homa) the answer is 403.1 Stream Not Allowed with an empty body,
/// and this name reaches no byte of either protocol.
pub const REFUSED_ON_CARRIER: &str = "REFUSED_ON_CARRIER";

/// Writes `Subscription field "<field>" is refused on <carrier>.` to `out`.
///
/// The text is the message of a refusal of `field` on `carrier`, a log line of
/// the listener and never a response body, and the carrier takes its proper
/// name from [`CarrierId::name`].
///
/// # Errors
///
/// Returns the error of `out` when a write of it fails.
pub fn write_refusal(field: &str, carrier: CarrierId, out: &mut dyn fmt::Write) -> fmt::Result {
    write!(
        out,
        "Subscription field \"{field}\" is refused on {}.",
        carrier.name()
    )
}
