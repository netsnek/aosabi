// SPDX-License-Identifier: MIT
/*
 * graphtp kind: the kind of a request, OPERATION, PULL or CANCEL, and the
 * unassigned values 3 to 7.
 *
 * No analogue in the reference. The kinds and their values are those of the
 * specification.
 */

//! The kind of a request: OPERATION, PULL and CANCEL, and the unassigned values
//! 3 to 7.

/// The kind of a request, bits 5 to 3 of its first byte.
///
/// GraphTP/1 assigns the values 0 to 2 and leaves 3 to 7 unassigned, and a
/// server answers a kind it does not know with 501.0, so a later kind enters
/// GraphTP/1 without a new version. The unassigned values are kinds of their
/// own here, so a head of any kind can be written. [`Kind::from_bits`] reads 3
/// to 7 back as [`Kind::Unknown`], while [`RequestHead::parse`] refuses them
/// with 501.0.
///
/// [`RequestHead::parse`]: crate::head::RequestHead::parse
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// OPERATION, the value 0: the credential and the content of a GraphQL
    /// request.
    Operation,
    /// PULL, the value 1: nothing, parked until an entry of a stream of the
    /// peer is ready.
    Pull,
    /// CANCEL, the value 2: a stream handle, or 0 for every stream of the peer.
    Cancel,
    /// One of the values 3 to 7, which GraphTP/1 leaves unassigned.
    Unknown(Unknown),
}

/// One of the kinds 3 to 7, which GraphTP/1 leaves unassigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unknown(u8);

impl Unknown {
    /// Returns the unassigned kind of the value `bits`.
    ///
    /// A value of 0 to 2 is an assigned kind and a value above 7 is no kind,
    /// so both return `None`.
    #[must_use]
    pub const fn new(bits: u8) -> Option<Unknown> {
        match bits {
            3..=7 => Some(Unknown(bits)),
            _ => None,
        }
    }

    /// Returns the value of the kind, 3 to 7.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

impl Kind {
    /// Returns the kind of the three bits `bits`, or `None` for a value above 7.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Kind> {
        match bits {
            0 => Some(Kind::Operation),
            1 => Some(Kind::Pull),
            2 => Some(Kind::Cancel),
            3..=7 => Some(Kind::Unknown(Unknown(bits))),
            _ => None,
        }
    }

    /// Returns the three bits of the kind, a value of 0 to 7.
    #[must_use]
    pub const fn bits(self) -> u8 {
        match self {
            Kind::Operation => 0,
            Kind::Pull => 1,
            Kind::Cancel => 2,
            Kind::Unknown(unknown) => unknown.0,
        }
    }

    /// Tells whether GraphTP/1 assigns the kind, which OPERATION, PULL and
    /// CANCEL are.
    #[must_use]
    pub const fn is_assigned(self) -> bool {
        !matches!(self, Kind::Unknown(_))
    }
}
