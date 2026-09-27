// SPDX-License-Identifier: MIT
/*
 * graphtp version: the protocol version a request head carries in its first
 * two bits.
 *
 * No analogue in the reference. The two bits and their values are those of the
 * specification.
 */

//! The protocol version of a request head, `01` for GraphTP/1, with `00`
//! refused and `11` kept as the escape a later version defines.

/// The protocol version a request head carries in bits 7 and 6 of its first
/// byte.
///
/// The bits `01` are GraphTP/1, `10` is GraphTP/2 and `11` is kept as an escape
/// for a version number that a later version puts in a byte of its own. The
/// bits `00` are never a version, so a zeroed buffer is never a request, and no
/// value of this type holds them. An answer carries no version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Version(u8);

impl Version {
    /// GraphTP/1, the bits `01`, the one version this crate reads.
    pub const V1: Version = Version(1);

    /// GraphTP/2, the bits `10`.
    pub const V2: Version = Version(2);

    /// The escape, the bits `11`, kept for a version number that a later
    /// version puts in a byte of its own.
    pub const ESCAPE: Version = Version(3);

    /// Returns the version the two bits `bits` name.
    ///
    /// The bits `00` name no version, and a value above 3 is no pair of bits,
    /// so both return `None`.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Version> {
        match bits {
            1..=3 => Some(Version(bits)),
            _ => None,
        }
    }

    /// Returns the two bits of the version, a value of 1 to 3.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}
