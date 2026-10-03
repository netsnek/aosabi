// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-bluetooth profile: the fixed numbers of the carrier and the
 * values of its two links that a device card fills, core only.
 *
 * No analogue on the reference side.
 */

//! The numbers and the profiles of the carrier Bluetooth.
//!
//! The service of Aosabi carries three characteristics: the PSM of the
//! channel, read with encryption, RX, written without response, and TX,
//! notified, the last two the GATT data path.
//!
//! [`Pace`] holds the values the GATT data path is paced at, since no
//! platform signal tells a driver of Linux user space that a notification
//! left the controller, and [`Pace::PLANNING`] is the planning link of the
//! design note, which a driver takes until its card names its own.

use homa::{
    Limits,
    Nanos,
    Profile, //
};

/// The fixed SPSM of the channel, ADR-0023 section Fourth amendment.
pub const SPSM: u16 = 0x00E0;

/// The UUID of the service of Aosabi, `ab46be00-ba01-4a5f-ba66-9fa7080a2ff4`.
pub const SERVICE: [u8; 16] = uuid(0x00);

/// The UUID of the characteristic that holds the PSM of the channel,
/// `ab46be01-ba01-4a5f-ba66-9fa7080a2ff4`.
pub const PSM_CHARACTERISTIC: [u8; 16] = uuid(0x01);

/// The UUID of RX, the characteristic a central writes packets to without a
/// response, `ab46be02-ba01-4a5f-ba66-9fa7080a2ff4`.
pub const RX: [u8; 16] = uuid(0x02);

/// The UUID of TX, the characteristic the carrier notifies packets on,
/// `ab46be03-ba01-4a5f-ba66-9fa7080a2ff4`.
pub const TX: [u8; 16] = uuid(0x03);

/// The `ATT_MTU` every link starts at, whose values hold 20 bytes.
pub const DEFAULT_ATT_MTU: u16 = 23;

/// The RPCs a central may have open at this end on either link, the eight
/// slots of the device configuration of the Homa design note, second version,
/// section 8.3.
pub const MAX_RPCS: u8 = 8;

/// The packets the GATT data path hands `BlueZ` that the link has not
/// carried yet, at most, one connection event of the planning link.
///
/// A driver without a signal of a packet sent counts the packets it handed
/// over and not yet confirmed sent and answers ready while that count is below
/// this depth, so the preemption granularity of SRPT is one connection event.
pub const TX_DEPTH: u16 = 2;

/// The connection interval the GATT data path is paced at until a card names
/// its own, 30 ms in nanoseconds.
pub const CONNECTION_INTERVAL: Nanos = 30_000_000;

/// The packets a connection event carries to a central until a card names its
/// own, two notifications of an `ATT_MTU` of 185.
pub const PACKETS_PER_EVENT: u16 = 2;

/// The pace of the GATT data path: the connection interval of its link, the
/// packets one connection event carries to the central and the packets the
/// data path hands `BlueZ` that the link has not carried yet, at most.
///
/// The data path counts each notification it hands over, a control packet as
/// well as DATA, and a slot of the pace every `interval` retires at most
/// `per_event` of them, so at most `tx_depth` packets wait below the engine
/// and the rest of a long answer waits in it, where SRPT puts a short answer
/// first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pace {
    /// The connection interval in nanoseconds.
    pub interval: Nanos,
    /// The packets one connection event carries to the central.
    pub per_event: u16,
    /// The packets handed over and not yet carried, at most.
    pub tx_depth: u16,
}

impl Pace {
    /// The pace of the planning link: a connection interval of
    /// [`CONNECTION_INTERVAL`], [`PACKETS_PER_EVENT`] packets an event and a
    /// depth of [`TX_DEPTH`].
    pub const PLANNING: Self = Self {
        interval: CONNECTION_INTERVAL,
        per_event: PACKETS_PER_EVENT,
        tx_depth: TX_DEPTH,
    };
}

/// Returns the UUID of the family of the service whose fourth byte is
/// `member`, in the order of its text form.
const fn uuid(member: u8) -> [u8; 16] {
    [
        0xab, 0x46, 0xbe, member, 0xba, 0x01, 0x4a, 0x5f, 0xba, 0x66, 0x9f, 0xa7, 0x08, 0x0a, 0x2f,
        0xf4,
    ]
}

/// Returns `max_packet` on the channel to a peer whose MPS is `peer_mps`, the
/// k rule of the design note: the largest whole k for which k times the MPS
/// less 6 stays at or under 512, and 512 in one K-frame for an MPS of 518 or
/// more.
#[must_use]
pub const fn channel_max_packet(peer_mps: u16) -> u16 {
    Profile::bluetooth_channel(peer_mps, 0).max_packet
}

/// Returns the profile of the channel to a peer whose MPS is `peer_mps` at a
/// connection `interval`, 0 where this side does not know it.
#[must_use]
pub const fn channel_profile(peer_mps: u16, interval: Nanos) -> Profile {
    Profile::bluetooth_channel(peer_mps, interval)
}

/// Returns the profile of the GATT data path at `att_mtu` and a connection
/// `interval`, 0 where this side does not know it.
#[must_use]
pub const fn gatt_profile(att_mtu: u16, interval: Nanos) -> Profile {
    Profile::bluetooth_gatt(att_mtu, interval)
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
