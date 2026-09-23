// SPDX-License-Identifier: MIT
/*
 * homa wire compact: version 0 of the compact form, for links between two
 * ends.
 *
 * The layout is version 0 of the Homa design note, first version,
 * homa/design-v1.md section 2.3, lines 402 to 503, which its second version
 * keeps unchanged for every carrier but Network, homa/design.md section 3.3,
 * lines 509 to 561.
 */

//! The compact form.
//!
//! Every packet opens with a common header of 6 bytes, and every multi byte
//! field is big endian.
//!
//! ```text
//! Common header, 6 bytes
//!
//!  0  1  type       Homa's code, and 0x20 for LIMITS
//!  1  1  flags      bit 0 RETRANSMIT, bit 1 ACK, bit 2 WHOLE, bit 3 NEED_LIMITS,
//!                   bits 4 and 5 reserved, bits 6 and 7 the version, 0
//!  2  4  sender_id  the RPC as the sender names it, 0 on LIMITS
//!
//! DATA 0x10, a whole message   6 bytes, 10 with ACK, then the message
//! DATA 0x10, a part            6 + msg_length 4 + offset 4, 14 bytes, 18 with
//!                              ACK, then the part
//! GRANT 0x11                   6 + offset 4, 10 bytes
//! RESEND 0x12                  6 + offset 4 + length 4, 14 bytes
//! RPC_UNKNOWN 0x13, BUSY 0x14,
//! NEED_ACK 0x17                6 bytes
//! ACK 0x18                     6 + 4 per client identifier, 1 to 5 of them
//! LIMITS 0x20                  6 + max_rpcs 1 + reserved 1 + max_packet 2
//!                              + unsched_bytes 4 + max_message 4, 18 bytes
//! ```
//!
//! The ACK of a DATA packet, present with the flag ACK, stands after the
//! common header of a whole message and after the offset of a part.
//!
//! # Decoding
//!
//! [`Compact::decode`] checks a packet in this order and answers the first
//! rule it breaks:
//!
//! 1. [`DecodeError::Short`] when the packet is shorter than 6 bytes.
//! 2. [`DecodeError::Version`] when the version bits are not 0.
//! 3. [`DecodeError::Reserved`] when flag bit 4 or 5 is set.
//! 4. [`DecodeError::Form`] for CUTOFFS `0x15`, FREEZE `0x16` and `START_MSG`
//!    `0x19`, which Homa has and this form does not, and
//!    [`DecodeError::Type`] for any other code outside the table above.
//! 5. [`DecodeError::Reserved`] when a type other than DATA sets RETRANSMIT,
//!    ACK or WHOLE, and when LIMITS sets its reserved byte or names a sender.
//! 6. [`DecodeError::Length`] when the length does not match the type: a
//!    GRANT, RESEND, `RPC_UNKNOWN`, BUSY, `NEED_ACK` or LIMITS of another
//!    length than its own, an ACK of other than 10 to 26 bytes or not 6 plus
//!    a multiple of 4, a DATA shorter than its header, and a part of DATA with
//!    no byte of payload.
//! 7. [`DecodeError::Range`] when a part of DATA runs past its `msg_length`,
//!    and when LIMITS names a `max_rpcs` of 0, a `max_packet` below 20, or an
//!    `unsched_bytes` or `max_message` of 0.
//!
//! A decoded GRANT or RESEND carries [`Priority::HIGHEST`], since the form
//! carries no level, and every decoded packet carries the ports 0.

use crate::{
    wire::{
        be_u16,
        be_u32,
        AckEntry,
        Acks,
        Data,
        DecodeError,
        Decoded,
        Form,
        Packet,
        Ports,
        ACK,
        BUSY,
        CUTOFFS,
        DATA,
        FREEZE,
        GRANT,
        LIMITS,
        NEED_ACK,
        RESEND,
        RPC_UNKNOWN,
        START_MSG, //
    },
    Limits,
    NoRoom,
    Priority, //
};

/// The version of the form, which bits 6 and 7 of the flags carry.
pub const VERSION: u8 = 0;

/// The bytes of the common header.
pub const COMMON_LEN: usize = 6;

/// The bytes of the largest header [`Compact::header`] writes, an ACK with
/// five entries, so an output of this length holds the header of every packet
/// the form has a layout for.
///
/// The header of DATA is at most 18 bytes, a part with an ACK, which the first
/// version of the Homa design note calls the largest header, section 2.3,
/// where the same section gives an ACK of five entries 26 bytes. The entries
/// of an ACK are its header here, as `HomaModule`'s largest header of 90 bytes
/// covers its ACK of 80 bytes with five entries.
pub const MAX_HEADER_LEN: usize = 26;

/// The smallest `max_packet` a LIMITS may name, which lets a LIMITS travel in
/// the 20 bytes of an ATT value at the default ATT MTU of 23.
pub const MIN_PACKET: u16 = 20;

/// The most client identifiers one ACK carries, which is Homa's own bound.
pub const MAX_ACKS: usize = 5;

/// The flag of a DATA packet sent again in answer to a RESEND.
const RETRANSMIT: u8 = 1 << 0;
/// The flag of a DATA packet that carries one acknowledgement.
const CARRIES_ACK: u8 = 1 << 1;
/// The flag of a DATA packet that carries a whole message at offset 0.
const WHOLE: u8 = 1 << 2;
/// The flag of a sender that holds no LIMITS of its peer.
const NEED_LIMITS: u8 = 1 << 3;
/// The reserved flag bits.
const RESERVED: u8 = 0b0011_0000;
/// The flags only DATA may set.
const DATA_ONLY: u8 = RETRANSMIT | CARRIES_ACK | WHOLE;

/// Version 0 of the compact form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Compact;

impl Form for Compact {
    const MAX_HEADER: usize = MAX_HEADER_LEN;
    const ID_BITS: u32 = 32;

    fn decode(bytes: &[u8]) -> Result<Decoded<'_>, DecodeError> {
        let (&kind, rest) = bytes.split_first().ok_or(DecodeError::Short)?;
        let (&flags, _) = rest.split_first().ok_or(DecodeError::Short)?;
        let id = be_u32(bytes, 2).ok_or(DecodeError::Short)?;
        if flags >> 6 != VERSION {
            return Err(DecodeError::Version);
        }
        if flags & RESERVED != 0 {
            return Err(DecodeError::Reserved);
        }
        let packet = match kind {
            DATA => Packet::Data(data(bytes, flags, id)?),
            GRANT | RESEND | RPC_UNKNOWN | BUSY | NEED_ACK | ACK | LIMITS => {
                if flags & DATA_ONLY != 0 {
                    return Err(DecodeError::Reserved);
                }
                control(bytes, kind, id)?
            }
            CUTOFFS | FREEZE | START_MSG => return Err(DecodeError::Form),
            _ => return Err(DecodeError::Type),
        };
        Ok(Decoded {
            ports: Ports::default(),
            packet,
            need_limits: flags & NEED_LIMITS != 0,
        })
    }

    fn header(
        packet: &Packet<'_>,
        _ports: Ports,
        need_limits: bool,
        out: &mut [u8],
    ) -> Result<usize, NoRoom> {
        let mut writer = Writer { out, len: 0 };
        let flags = if need_limits { NEED_LIMITS } else { 0 };
        match *packet {
            Packet::Data(data) => write_data(&mut writer, &data, flags)?,
            Packet::Grant { id, offset, .. } => {
                writer.common(GRANT, flags, id)?;
                writer.put(&offset.to_be_bytes())?;
            }
            Packet::Resend {
                id, offset, length, ..
            } => {
                writer.common(RESEND, flags, id)?;
                writer.put(&offset.to_be_bytes())?;
                writer.put(&length.to_be_bytes())?;
            }
            Packet::RpcUnknown { id } => writer.common(RPC_UNKNOWN, flags, id)?,
            Packet::Busy { id } => writer.common(BUSY, flags, id)?,
            Packet::NeedAck { id } => writer.common(NEED_ACK, flags, id)?,
            Packet::Ack { id, acks } => {
                if acks.is_empty() || acks.len() > MAX_ACKS {
                    return Err(NoRoom);
                }
                writer.common(ACK, flags, id)?;
                for entry in &acks {
                    writer.put(&low_bits(entry.client_id).to_be_bytes())?;
                }
            }
            Packet::Limits(limits) => {
                writer.common(LIMITS, flags, 0)?;
                writer.put(&[limits.max_rpcs, 0])?;
                writer.put(&limits.max_packet.to_be_bytes())?;
                writer.put(&limits.unsched_bytes.to_be_bytes())?;
                writer.put(&limits.max_message.to_be_bytes())?;
            }
            Packet::Cutoffs { .. } | Packet::Freeze | Packet::StartMsg { .. } => {
                return Err(NoRoom)
            }
        }
        Ok(writer.len)
    }
}

/// Reads a DATA packet whose common header is already checked.
fn data(bytes: &[u8], flags: u8, id: u32) -> Result<Data<'_>, DecodeError> {
    let whole = flags & WHOLE != 0;
    let mut at = if whole { COMMON_LEN } else { COMMON_LEN + 8 };
    let (msg_length, offset) = if whole {
        (0, 0)
    } else {
        (
            be_u32(bytes, COMMON_LEN).ok_or(DecodeError::Length)?,
            be_u32(bytes, COMMON_LEN + 4).ok_or(DecodeError::Length)?,
        )
    };
    let ack = if flags & CARRIES_ACK == 0 {
        None
    } else {
        let client_id = be_u32(bytes, at).ok_or(DecodeError::Length)?;
        at += 4;
        Some(AckEntry {
            client_id: u64::from(client_id),
            server_port: 0,
        })
    };
    let payload = bytes.get(at..).ok_or(DecodeError::Length)?;
    let carried = u32::try_from(payload.len()).map_err(|_| DecodeError::Length)?;
    let msg_length = if whole {
        carried
    } else {
        if carried == 0 {
            return Err(DecodeError::Length);
        }
        let end = offset.checked_add(carried).ok_or(DecodeError::Range)?;
        if end > msg_length {
            return Err(DecodeError::Range);
        }
        msg_length
    };
    Ok(Data {
        id: u64::from(id),
        msg_length,
        offset,
        retransmit: flags & RETRANSMIT != 0,
        ack,
        cutoff_version: 0,
        incast: false,
        payload,
    })
}

/// Reads a control packet of type `kind` whose flags are already checked.
fn control(bytes: &[u8], kind: u8, id: u32) -> Result<Packet<'_>, DecodeError> {
    let id64 = u64::from(id);
    let exact = |len: usize| {
        if bytes.len() == len {
            Ok(())
        } else {
            Err(DecodeError::Length)
        }
    };
    let field = |at: usize| be_u32(bytes, at).ok_or(DecodeError::Length);
    match kind {
        GRANT => {
            exact(10)?;
            Ok(Packet::Grant {
                id: id64,
                offset: field(6)?,
                priority: Priority::HIGHEST,
            })
        }
        RESEND => {
            exact(14)?;
            Ok(Packet::Resend {
                id: id64,
                offset: field(6)?,
                length: field(10)?,
                priority: Priority::HIGHEST,
            })
        }
        RPC_UNKNOWN => exact(6).map(|()| Packet::RpcUnknown { id: id64 }),
        BUSY => exact(6).map(|()| Packet::Busy { id: id64 }),
        NEED_ACK => exact(6).map(|()| Packet::NeedAck { id: id64 }),
        ACK => {
            let entries = bytes.len().saturating_sub(COMMON_LEN);
            if entries == 0 || entries % 4 != 0 || entries / 4 > MAX_ACKS {
                return Err(DecodeError::Length);
            }
            Ok(Packet::Ack {
                id: id64,
                acks: Acks::from_wire(&bytes[COMMON_LEN..], 4),
            })
        }
        _ => limits(bytes, id).map(Packet::Limits),
    }
}

/// Reads a LIMITS packet whose flags are already checked.
fn limits(bytes: &[u8], id: u32) -> Result<Limits, DecodeError> {
    if id != 0 || bytes.get(7).is_some_and(|&reserved| reserved != 0) {
        return Err(DecodeError::Reserved);
    }
    if bytes.len() != 18 {
        return Err(DecodeError::Length);
    }
    let limits = Limits {
        max_rpcs: bytes[6],
        max_packet: be_u16(bytes, 8).ok_or(DecodeError::Length)?,
        unsched_bytes: be_u32(bytes, 10).ok_or(DecodeError::Length)?,
        max_message: be_u32(bytes, 14).ok_or(DecodeError::Length)?,
    };
    if limits.max_rpcs == 0
        || limits.max_packet < MIN_PACKET
        || limits.unsched_bytes == 0
        || limits.max_message == 0
    {
        return Err(DecodeError::Range);
    }
    Ok(limits)
}

/// Writes the header of a DATA packet, whole where the payload is the whole
/// message at offset 0.
fn write_data(writer: &mut Writer<'_>, data: &Data<'_>, flags: u8) -> Result<(), NoRoom> {
    let whole = data.offset == 0 && u32::try_from(data.payload.len()) == Ok(data.msg_length);
    let mut flags = flags;
    if data.retransmit {
        flags |= RETRANSMIT;
    }
    if data.ack.is_some() {
        flags |= CARRIES_ACK;
    }
    if whole {
        flags |= WHOLE;
    }
    writer.common(DATA, flags, data.id)?;
    if !whole {
        writer.put(&data.msg_length.to_be_bytes())?;
        writer.put(&data.offset.to_be_bytes())?;
    }
    if let Some(ack) = data.ack {
        writer.put(&low_bits(ack.client_id).to_be_bytes())?;
    }
    Ok(())
}

/// Returns the low 32 bits of an identifier, which the form carries.
fn low_bits(id: u64) -> u32 {
    u32::try_from(id & u64::from(u32::MAX)).unwrap_or(u32::MAX)
}

/// A header written into an output slice in order.
struct Writer<'o> {
    /// The output.
    out: &'o mut [u8],
    /// The bytes written.
    len: usize,
}

impl Writer<'_> {
    /// Writes the common header.
    fn common(&mut self, kind: u8, flags: u8, id: u64) -> Result<(), NoRoom> {
        self.put(&[kind, flags | (VERSION << 6)])?;
        self.put(&low_bits(id).to_be_bytes())
    }

    /// Appends `bytes`, or fails with [`NoRoom`] past the end of the output.
    fn put(&mut self, bytes: &[u8]) -> Result<(), NoRoom> {
        let end = self.len.checked_add(bytes.len()).ok_or(NoRoom)?;
        let target = self.out.get_mut(self.len..end).ok_or(NoRoom)?;
        target.copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }
}
