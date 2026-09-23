// SPDX-License-Identifier: MIT
/*
 * homa wire: a packet as the engine sees it, whatever wire form carries it.
 *
 * The shapes are those of the Homa design note, second version, homa/design.md
 * section 7.5, lines 2540 to 2629, with one packet type for both forms, so the
 * engine is written once and each form only writes and reads its own layout.
 */

//! The packets of the engine and the two wire forms that carry them.
//!
//! [`Packet`] is a packet as the engine sees it, and a [`Form`] writes the
//! header of one and reads a whole one back. The compact form is version 0 of
//! the first version of the Homa design note, 6 bytes of common header, 32
//! bit identifiers and LIMITS, for every carrier but Network. The full form is
//! `homa_wire.h` of `HomaModule` at the pinned commit, with the ports of both
//! ends, for Network. Each form refuses the packets it has no layout for with
//! [`DecodeError::Form`], so the engine never sends a packet the form lacks,
//! and every length from the wire is checked before a byte past it is read,
//! so no length from the wire is ever an allocation request.
//!
//! Every multi byte field of both forms is big endian. The full form keeps the
//! order of `homa_wire.h`, whose every multi byte field is `__be16`, `__be32`
//! or `__be64`, and the compact form takes the same order.

#[cfg(feature = "wire-compact")]
pub mod compact;
#[cfg(feature = "wire-full")]
pub mod full;

#[cfg(feature = "wire-compact")]
pub use compact::Compact;
#[cfg(feature = "wire-full")]
pub use full::Full;

use core::fmt;

use crate::{
    Limits,
    NoRoom,
    Priority, //
};

/// The type code of DATA, which the compact form writes in the first byte of a
/// packet and the full form in byte 11 of its common header, as they write
/// every code below. The codes from DATA to `START_MSG` are those of the enum
/// `homa_packet_type`, and the type follows 11 bytes of ports, sequence and
/// acknowledgement in the common header.
pub const DATA: u8 = 0x10;
/// The type code of GRANT.
pub const GRANT: u8 = 0x11;
/// The type code of RESEND.
pub const RESEND: u8 = 0x12;
/// The type code of `RPC_UNKNOWN`.
pub const RPC_UNKNOWN: u8 = 0x13;
/// The type code of BUSY.
pub const BUSY: u8 = 0x14;
/// The type code of CUTOFFS.
pub const CUTOFFS: u8 = 0x15;
/// The type code of FREEZE.
pub const FREEZE: u8 = 0x16;
/// The type code of `NEED_ACK`.
pub const NEED_ACK: u8 = 0x17;
/// The type code of ACK.
pub const ACK: u8 = 0x18;
/// The type code of `START_MSG`.
pub const START_MSG: u8 = 0x19;
/// The type code of LIMITS, which the compact form adds outside Homa's range.
pub const LIMITS: u8 = 0x20;

/// A packet as the engine sees it, whatever form carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Packet<'a> {
    /// A message or a part of one.
    Data(Data<'a>),
    /// The receiver lets the sender transmit every byte below `offset`.
    Grant {
        /// The RPC as the sender of the GRANT names it.
        id: u64,
        /// The first byte the sender may not transmit yet.
        offset: u32,
        /// The level the granted bytes go at, the highest on the compact form.
        priority: Priority,
    },
    /// The receiver asks for a range again.
    Resend {
        /// The RPC as the sender of the RESEND names it.
        id: u64,
        /// The first byte to transmit again.
        offset: u32,
        /// The bytes to transmit again, `u32::MAX` for everything sent so far.
        length: u32,
        /// The level the bytes go at, the highest on the compact form.
        priority: Priority,
    },
    /// The sender knows no RPC of that identifier.
    RpcUnknown {
        /// The RPC as the sender names it.
        id: u64,
    },
    /// The sender has the RPC and chooses not to send now.
    Busy {
        /// The RPC as the sender names it.
        id: u64,
    },
    /// The receiver's unscheduled cutoffs, full form only.
    Cutoffs {
        /// The longest unscheduled message of each level, the lowest first.
        cutoffs: [u32; 8],
        /// The version of the cutoffs, which every DATA to the receiver names.
        version: u16,
    },
    /// A request to freeze a timetrace, full form only, which the crate never
    /// sends and drops when it arrives.
    Freeze,
    /// The server asks the client to acknowledge a response.
    NeedAck {
        /// The RPC as the server names it.
        id: u64,
    },
    /// The client acknowledges responses the server may free.
    Ack {
        /// The RPC a `NEED_ACK` named, or 0 for an acknowledgement nobody asked
        /// for.
        id: u64,
        /// The acknowledged client identifiers.
        acks: Acks<'a>,
    },
    /// A message longer than the unscheduled bytes announces itself, full form
    /// only.
    StartMsg {
        /// The RPC as the sender names it.
        id: u64,
        /// The bytes of the whole message.
        msg_length: u32,
    },
    /// What the sender accepts, compact form only.
    Limits(Limits),
}

/// A message or a part of one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Data<'a> {
    /// The RPC as the sender names it.
    pub id: u64,
    /// The bytes of the whole message.
    pub msg_length: u32,
    /// The offset of the first payload byte in the message.
    pub offset: u32,
    /// Whether the packet is sent again in answer to a RESEND.
    pub retransmit: bool,
    /// One acknowledgement the packet carries along.
    pub ack: Option<AckEntry>,
    /// The version of the receiver's cutoffs the sender used, 0 on the compact
    /// form.
    pub cutoff_version: u16,
    /// Whether the request is incast controlled, false on the compact form.
    pub incast: bool,
    /// The bytes of the message from `offset` on.
    pub payload: &'a [u8],
}

/// One acknowledgement: a client identifier and the server port it was sent
/// to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct AckEntry {
    /// The identifier of the client RPC whose response may be freed.
    pub client_id: u64,
    /// The server port the RPC was sent to, 0 on the compact form.
    pub server_port: u16,
}

/// The ports of a full form packet, both 0 on the compact form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Ports {
    /// The Homa port of the sender.
    pub src: u16,
    /// The Homa port of the receiver.
    pub dst: u16,
}

/// One decoded packet with what every type can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decoded<'a> {
    /// The ports of the packet, both 0 on the compact form.
    pub ports: Ports,
    /// The packet.
    pub packet: Packet<'a>,
    /// Whether the sender holds no LIMITS of its peer, compact form only.
    pub need_limits: bool,
}

/// Why a packet does not decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The packet is shorter than the common header.
    Short,
    /// The packet names another version of the form.
    Version,
    /// A reserved bit or byte is set.
    Reserved,
    /// The type code names no packet of Homa.
    Type,
    /// The length does not match the type.
    Length,
    /// A field holds a value outside its range.
    Range,
    /// The type is Homa's and the form has no layout for it.
    Form,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Short => "the packet is shorter than the common header",
            Self::Version => "the packet names another version of the form",
            Self::Reserved => "a reserved bit or byte is set",
            Self::Type => "the type code names no packet of Homa",
            Self::Length => "the length does not match the type",
            Self::Range => "a field holds a value outside its range",
            Self::Form => "the form has no layout for the type",
        })
    }
}

impl core::error::Error for DecodeError {}

/// A wire form.
pub trait Form {
    /// Bytes of the largest header [`Form::header`] writes, so an output of
    /// this length holds the header of every packet the form has a layout
    /// for.
    const MAX_HEADER: usize;
    /// Bits of the RPC identifier on the wire.
    const ID_BITS: u32;

    /// Reads one packet, and never panics whatever `bytes` holds.
    ///
    /// # Errors
    ///
    /// Returns why the packet does not decode.
    fn decode(bytes: &[u8]) -> Result<Decoded<'_>, DecodeError>;

    /// Writes the header of `packet` into `out` and returns its length.
    ///
    /// The payload of DATA is not written, since the engine hands the carrier
    /// the header and the payload apart, and its length decides whether the
    /// packet carries the whole message.
    ///
    /// # Errors
    ///
    /// Returns [`NoRoom`] when the header does not fit `out`, and when the
    /// form has no layout for `packet`, which no output can hold.
    fn header(
        packet: &Packet<'_>,
        ports: Ports,
        need_limits: bool,
        out: &mut [u8],
    ) -> Result<usize, NoRoom>;
}

/// The acknowledged client identifiers of an ACK, read with [`Acks::iter`].
///
/// A decoded ACK borrows the entries from the packet in the layout of its
/// form, and an ACK the engine sends borrows them from a slice of
/// [`AckEntry`] through [`Acks::from_entries`]. Two lists are equal when they
/// hold the same entries in the same order, whatever they borrow from.
#[derive(Clone, Copy, Debug)]
pub struct Acks<'a> {
    /// Where the entries are read from.
    source: Source<'a>,
}

/// Where the entries of an [`Acks`] are read from.
#[derive(Clone, Copy, Debug)]
enum Source<'a> {
    /// The bytes of a packet, one entry every `stride` bytes.
    Wire {
        /// The entries in the layout of the form.
        bytes: &'a [u8],
        /// The bytes of one entry, 4 on the compact form and 10 on the full.
        stride: usize,
    },
    /// Entries the engine holds.
    Entries(&'a [AckEntry]),
}

impl<'a> Acks<'a> {
    /// Returns the list of `entries`.
    #[must_use]
    pub const fn from_entries(entries: &'a [AckEntry]) -> Self {
        Self {
            source: Source::Entries(entries),
        }
    }

    /// Returns the list of the entries `bytes` holds in the layout of a form,
    /// one every `stride` bytes: a big endian identifier of 32 bits in 4 bytes
    /// on the compact form, or one of 64 bits and a big endian port of 16 bits
    /// in 10 on the full form.
    #[must_use]
    pub const fn from_wire(bytes: &'a [u8], stride: usize) -> Self {
        Self {
            source: Source::Wire { bytes, stride },
        }
    }

    /// Returns the count of entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        match self.source {
            Source::Wire { bytes, stride } => match bytes.len().checked_div(stride) {
                Some(count) => count,
                None => 0,
            },
            Source::Entries(entries) => entries.len(),
        }
    }

    /// Returns whether the list holds no entry.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the entries in order.
    #[must_use]
    pub fn iter(&self) -> AcksIter<'a> {
        AcksIter {
            acks: *self,
            next: 0,
        }
    }
}

impl PartialEq for Acks<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl Eq for Acks<'_> {}

impl<'a> IntoIterator for Acks<'a> {
    type Item = AckEntry;
    type IntoIter = AcksIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &Acks<'a> {
    type Item = AckEntry;
    type IntoIter = AcksIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The entries of an [`Acks`] in order.
#[derive(Clone, Debug)]
pub struct AcksIter<'a> {
    /// The list read.
    acks: Acks<'a>,
    /// The index of the next entry.
    next: usize,
}

impl Iterator for AcksIter<'_> {
    type Item = AckEntry;

    fn next(&mut self) -> Option<AckEntry> {
        let entry = match self.acks.source {
            Source::Entries(entries) => entries.get(self.next).copied(),
            Source::Wire { bytes, stride } => {
                let start = self.next.checked_mul(stride)?;
                let end = start.checked_add(stride)?;
                bytes.get(start..end).and_then(entry)
            }
        }?;
        self.next += 1;
        Some(entry)
    }
}

/// Returns the entry that `bytes` holds, an identifier of 4 bytes, or one of
/// 8 bytes followed by a port of 2.
fn entry(bytes: &[u8]) -> Option<AckEntry> {
    if bytes.len() >= 10 {
        Some(AckEntry {
            client_id: be_u64(bytes, 0)?,
            server_port: be_u16(bytes, 8)?,
        })
    } else {
        Some(AckEntry {
            client_id: u64::from(be_u32(bytes, 0)?),
            server_port: 0,
        })
    }
}

/// Returns the big endian `u16` at `at`, or `None` past the end of `bytes`.
pub(crate) fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let field = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([field[0], field[1]]))
}

/// Returns the big endian `u32` at `at`, or `None` past the end of `bytes`.
pub(crate) fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let field = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([field[0], field[1], field[2], field[3]]))
}

/// Returns the big endian `u64` at `at`, or `None` past the end of `bytes`.
pub(crate) fn be_u64(bytes: &[u8], at: usize) -> Option<u64> {
    let field = bytes.get(at..at.checked_add(8)?)?;
    let mut word = [0u8; 8];
    word.copy_from_slice(field);
    Some(u64::from_be_bytes(word))
}
