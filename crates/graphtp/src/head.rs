// SPDX-License-Identifier: MIT
/*
 * graphtp head: the head of a request, 3 bytes, and the head of an answer, 2
 * bytes.
 *
 * No analogue in the reference. Both heads, their sizes and their fields are
 * those of the specification, and the reader of a request head checks rules 1
 * to 4 of section 5.3.
 */

//! The head of a request, its 3 bytes, and the head of an answer, its 2.
//!
//! Every integer of a head is big endian. Both readers take the whole message
//! and read its first bytes, so a message is never copied to be read.

use crate::{
    kind::Kind,
    status::{
        Code,
        Status,
        Substatus, //
    },
    version::Version,
    wire::WireError, //
};

/// The bit of the first byte of a request that marks a coded content.
const COMPRESSED: u8 = 0b0000_0100;

/// The two reserved bits of the first byte of a request, sent as 0.
const RESERVED: u8 = 0b0000_0011;

/// The head of a request, its first 3 bytes.
///
/// Byte 1 holds the version in bits 7 and 6, the kind in bits 5 to 3, the
/// compressed bit C in bit 2 and two reserved bits sent as 0, and bytes 2 and 3
/// hold the length of the credential, 0 for none. So an OPERATION opens with
/// `0x40` plain and `0x44` coded, a PULL with `0x48` and a CANCEL with `0x50`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestHead {
    /// The protocol version.
    pub version: Version,
    /// The kind of the request.
    pub kind: Kind,
    /// Whether the content is one zlib stream of the profile of section 3.2.
    pub compressed: bool,
    /// The length of the credential that follows the head, in bytes.
    pub credential_len: u16,
}

impl RequestHead {
    /// The length of a request head, 3 bytes.
    pub const LEN: usize = 3;

    /// Reads the head at the start of the request `message`.
    ///
    /// The checks are rules 1 to 4 of `docs/graphtp.md` section 5.3 in their
    /// order, so a message that breaks two of them is refused by the first.
    ///
    /// # Errors
    ///
    /// Returns [`Code::MALFORMED_MESSAGE`] for a message of fewer than 3
    /// bytes, [`Code::HTTP_VERSION_NOT_SUPPORTED`] for a version other than
    /// GraphTP/1, the bits `00` among them, [`Code::NOT_IMPLEMENTED`] for a
    /// kind of the values 3 to 7, and [`Code::MALFORMED_MESSAGE`] for a
    /// reserved bit that is set.
    pub fn parse(message: &[u8]) -> Result<RequestHead, Code> {
        let [first, high, low, ..] = *message else {
            return Err(Code::MALFORMED_MESSAGE);
        };
        let version = Version::from_bits(first >> 6)
            .filter(|version| *version == Version::V1)
            .ok_or(Code::HTTP_VERSION_NOT_SUPPORTED)?;
        let kind = Kind::from_bits((first >> 3) & 0b111)
            .filter(|kind| kind.is_assigned())
            .ok_or(Code::NOT_IMPLEMENTED)?;
        if first & RESERVED != 0 {
            return Err(Code::MALFORMED_MESSAGE);
        }
        Ok(RequestHead {
            version,
            kind,
            compressed: first & COMPRESSED != 0,
            credential_len: u16::from_be_bytes([high, low]),
        })
    }

    /// Returns the 3 bytes of the head, with the reserved bits 0.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 3] {
        let [high, low] = self.credential_len.to_be_bytes();
        let compressed = if self.compressed { COMPRESSED } else { 0 };
        let first = (self.version.bits() << 6) | (self.kind.bits() << 3) | compressed;
        [first, high, low]
    }
}

/// The head of an answer, its first 2 bytes: the status byte and the
/// substatus.
///
/// An answer carries no version, so every version keeps this head and a client
/// reads a refusal of a version it sent whatever the server speaks. Every pair
/// of bytes is a head.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnswerHead {
    /// The status and the substatus of the answer.
    pub code: Code,
}

impl AnswerHead {
    /// The length of an answer head, 2 bytes.
    pub const LEN: usize = 2;

    /// Reads the head at the start of the answer `message`.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Short`] for a message of fewer than 2 bytes, which
    /// `docs/graphtp.md` section 5.4 reads as malformed.
    pub fn parse(message: &[u8]) -> Result<AnswerHead, WireError> {
        let [status, substatus, ..] = *message else {
            return Err(WireError::Short);
        };
        let code = Code::new(Status::from_byte(status), Substatus::new(substatus));
        Ok(AnswerHead { code })
    }

    /// Returns the 2 bytes of the head.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 2] {
        [self.code.status.to_byte(), self.code.substatus.get()]
    }
}
