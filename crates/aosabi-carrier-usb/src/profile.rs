// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-usb profile: the values of USB bulk and USB serial that a
 * device card fills, core only.
 *
 * No analogue on the reference side. The values are the rows of USB bulk and
 * USB serial of the Homa design note, first version, homa/design-v1.md
 * section 3.1 in the design folder outside this repository, which
 * homa::Profile::usb_bulk and homa::Profile::usb_serial give, and the rule of
 * the zero length packet is that of its section 3.3.
 */

//! The profiles of USB bulk and USB serial.
//!
//! Bulk takes packets of 16,384 bytes at high speed and 4,096 at full speed,
//! one packet a transfer, and serial packets of 512 bytes in delimited
//! `AoFrame`. A transfer ends with a short packet, so one whose length is an
//! exact multiple of `wMaxPacketSize` and below the largest packet the
//! receiver reads is followed by a zero length packet.

use homa::{
    Limits,
    Profile, //
};

/// The speed of the bus a bulk function runs at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    /// Full speed, 12 Mbit/s, bulk packets of 64 bytes.
    Full,
    /// High speed, 480 Mbit/s, bulk packets of 512 bytes, and `SuperSpeed`,
    /// which takes the same profile.
    High,
}

impl Speed {
    /// Returns the `wMaxPacketSize` of a bulk endpoint at this speed, 64 at
    /// full speed and 512 at high speed.
    #[must_use]
    pub const fn max_packet_size(self) -> u16 {
        match self {
            Self::Full => 64,
            Self::High => 512,
        }
    }
}

/// The RPCs a peer may have open at this end on either link, the eight slots
/// of the device configuration of the Homa design note, second version,
/// section 8.3.
pub const MAX_RPCS: u8 = 8;

/// Returns the profile of USB bulk at `speed`.
#[must_use]
pub const fn bulk_profile(speed: Speed) -> Profile {
    Profile::usb_bulk(matches!(speed, Speed::High))
}

/// Returns the profile of USB serial, delimited `AoFrame` at 512 bytes a
/// packet.
#[must_use]
pub const fn serial_profile() -> Profile {
    Profile::usb_serial()
}

/// Returns what this end accepts on a link of `profile`, messages of at most
/// `max_message` bytes, with [`MAX_RPCS`] open at once.
#[must_use]
pub fn limits(profile: &Profile, max_message: u32) -> Limits {
    Limits {
        max_packet: profile.max_packet,
        max_message,
        unsched_bytes: profile.unsched_bytes.min(max_message),
        max_rpcs: MAX_RPCS,
    }
}

/// Returns whether a transfer of `len` bytes needs a zero length packet
/// after it, which holds for a length that is an exact multiple of
/// `max_packet_size` and below `max_packet`, the largest packet the receiver
/// reads at once.
///
/// A transfer of no bytes is itself a zero length packet and needs none, and
/// a transfer of `max_packet` bytes fills the receiver's read, which ends
/// with it.
#[must_use]
pub const fn needs_zlp(len: usize, max_packet_size: u16, max_packet: u16) -> bool {
    let size = max_packet_size as usize;
    len > 0 && size > 0 && len.is_multiple_of(size) && len < max_packet as usize
}
