// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-serial profile: the values of Serial Line that a device card
 * fills, core only.
 *
 * No analogue on the reference side. The values are the row of Serial Line of
 * the Homa design note, first version, homa/design-v1.md section 3.1 in the
 * design folder outside this repository, which homa::Profile::serial_line
 * gives, scaled by the rate of the line.
 */

//! The profile of Serial Line.
//!
//! A packet is at most [`MAX_PACKET`] bytes in delimited `AoFrame`, 512 bytes
//! of a message leave before the first GRANT and a window of 1,024, the
//! RESEND waits 550 ms at 115,200 baud and scales inversely with the rate,
//! and a peer is dead after 5 RESENDs.

use homa::{
    Limits,
    Profile, //
};

/// The bytes of the largest packet on Serial Line, header included.
pub const MAX_PACKET: u16 = 256;

/// The RPCs a peer may have open at this end, the eight slots of the device
/// configuration of the Homa design note, second version, section 8.3.
pub const MAX_RPCS: u8 = 8;

/// Returns the profile of Serial Line at `baud`, the design note's row scaled
/// by the rate.
#[must_use]
pub fn profile(baud: u32) -> Profile {
    let mut profile = Profile::serial_line(baud);
    profile.max_packet = MAX_PACKET;
    profile
}

/// Returns what this end accepts on Serial Line at `baud`, messages of at
/// most `max_message` bytes.
///
/// The unscheduled bytes are those of the profile, never more than the
/// message, and a peer may hold [`MAX_RPCS`] RPCs open at this end.
#[must_use]
pub fn limits(baud: u32, max_message: u32) -> Limits {
    let profile = profile(baud);
    Limits {
        max_packet: profile.max_packet,
        max_message,
        unsched_bytes: profile.unsched_bytes.min(max_message),
        max_rpcs: MAX_RPCS,
    }
}
