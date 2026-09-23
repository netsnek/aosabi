// SPDX-License-Identifier: MIT
/*
 * homa wire full: the layout of homa_wire.h at HomaModule commit 50d65305.
 *
 * The layout is that of homa_wire.h lines 93 to 490 at
 * https://github.com/PlatformLab/HomaModule/tree/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3,
 * read, with the rules of the Homa design note, second version,
 * homa/design.md section 3.3, lines 509 to 561: the fields HomaModule does
 * not use written as 0, doff set as HomaModule sets it, the incast mark in
 * bit 0 of the first pad byte of DATA, and FREEZE decoded and never written.
 */

//! The full form.
//!
//! Every packet opens with the common header of 28 bytes that `homa_wire.h`
//! shapes like a TCP header, and every multi byte field is big endian.
//!
//! ```text
//! Common header, 28 bytes
//!
//!  0  2  sport        the Homa port of the sender
//!  2  2  dport        the Homa port of the receiver
//!  4  4  sequence     0, not used by Homa
//!  8  3  ack          0, not used by Homa
//! 11  1  type         0x10 DATA to 0x19 START_MSG
//! 12  1  doff         0xd0 on DATA, 13 words of its header less the segment
//!                     offset, and 0x50 on every other type
//! 13  1  flags        0
//! 14  2  window       0
//! 16  2  checksum     0
//! 18  2  urgent       0, and 0xb97d marks a packet of TCP hijacking
//! 20  8  sender_id    the RPC as the sender names it
//!
//! DATA 0x10         56 bytes, then the payload
//!   28  4  msg_length   32  4  reserved1, 0
//!   36  8  ack.client_id, 0 for none   44  2  ack.server_port
//!   46  2  cutoff_version   48  1  retransmit   49  3  pad, bit 0 of the
//!   first byte the incast mark   52  4  offset of the payload
//! GRANT 0x11        33 bytes: offset 4, priority 1
//! RESEND 0x12       37 bytes: offset 4, length 4, priority 1
//! RPC_UNKNOWN 0x13, BUSY 0x14, FREEZE 0x16, NEED_ACK 0x17   28 bytes
//! CUTOFFS 0x15      62 bytes: eight cutoffs of 4 bytes, the version 2
//! ACK 0x18          80 bytes: num_acks 2, five entries of a client
//!                   identifier 8 and a server port 2
//! START_MSG 0x19    32 bytes: msg_length 4
//! ```
//!
//! A DATA packet carries one segment, so the offset of its segment is the
//! offset of its payload. The ACK packet is always 80 bytes, with `num_acks`
//! naming how many of its five entries count, as `HomaModule` writes it.
//!
//! # Decoding
//!
//! [`Full::decode`] checks a packet in this order and answers the first rule
//! it breaks:
//!
//! 1. [`DecodeError::Short`] when the packet is shorter than the 28 bytes of
//!    the common header.
//! 2. [`DecodeError::Form`] for LIMITS `0x20`, which the compact form has and
//!    this form does not, and [`DecodeError::Type`] for any other code
//!    outside `0x10` to `0x19`, which `HomaModule` refuses the same way.
//! 3. [`DecodeError::Reserved`] when `urgent` holds the marker of TCP
//!    hijacking, `0xb97d`, which the crate never takes since it never
//!    hijacks, and when a DATA packet sets a bit of its pad other than the
//!    incast mark.
//! 4. [`DecodeError::Length`] when the packet is shorter than the header of
//!    its type, when a control packet is longer than its header, and when a
//!    DATA packet of a message that is not empty carries no payload.
//! 5. [`DecodeError::Range`] when a GRANT or a RESEND names a level above 7,
//!    when an ACK counts more than five entries, and when the payload of a
//!    DATA packet runs past its `msg_length`.
//!
//! The fields `HomaModule` does not use, `sequence`, the three `ack` bytes,
//! `doff`, `flags`, `window`, `checksum` and the `reserved1` of DATA, are not
//! read, since segmentation offload may write `sequence`. A FREEZE decodes
//! into [`Packet::Freeze`], which the endpoint drops and counts.

use crate::{
    wire::{
        be_u16,
        be_u32,
        be_u64,
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
    NoRoom,
    Priority, //
};

/// The bytes of the common header.
pub const COMMON_LEN: usize = 28;

/// The bytes of the header of DATA, the segment offset included.
pub const DATA_LEN: usize = 56;

/// The bytes of GRANT.
pub const GRANT_LEN: usize = 33;

/// The bytes of RESEND.
pub const RESEND_LEN: usize = 37;

/// The bytes of CUTOFFS.
pub const CUTOFFS_LEN: usize = 62;

/// The bytes of ACK with its five entries.
pub const ACK_LEN: usize = 80;

/// The bytes of `START_MSG`.
pub const START_MSG_LEN: usize = 32;

/// The bytes of the largest header [`Full::header`] writes, the ACK, so an
/// output of this length holds the header of every packet the form writes.
///
/// `HomaModule` names 90 bytes as its largest header, `HOMA_MAX_HEADER`,
/// which the header buffer of the endpoint holds, and no packet of this
/// layout reaches it.
pub const MAX_HEADER_LEN: usize = ACK_LEN;

/// The most entries one ACK carries.
pub const MAX_ACKS: usize = 5;

/// The bytes of one entry of an ACK, a client identifier and a server port.
const ACK_ENTRY_LEN: usize = 10;

/// The marker of TCP hijacking in `urgent`.
pub const HIJACK_URGENT: u16 = 0xb97d;

/// The byte `doff` of DATA: the 52 bytes of its header before the segment
/// offset in 4 byte words, in the top four bits, as `homa_set_doff` writes it
/// from `sizeof(struct homa_data_hdr) - sizeof(struct homa_seg_hdr)`.
pub const DOFF_DATA: u8 = 13 << 4;

/// The byte `doff` of every other type, 5 words, as `__homa_xmit_control`
/// writes it.
pub const DOFF_CONTROL: u8 = 5 << 4;

/// The incast mark, bit 0 of the first pad byte of DATA, which `HomaModule`
/// never reads.
pub const INCAST_MARK: u8 = 1;

/// The offset of `urgent` in the common header.
const URGENT_AT: usize = 18;

/// The offset of the first pad byte of DATA.
const PAD_AT: usize = 49;

/// The layout of `homa_wire.h` at `HomaModule` commit `50d65305`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Full;

impl Form for Full {
    const MAX_HEADER: usize = MAX_HEADER_LEN;
    const ID_BITS: u32 = 64;

    fn decode(bytes: &[u8]) -> Result<Decoded<'_>, DecodeError> {
        let common = bytes.get(..COMMON_LEN).ok_or(DecodeError::Short)?;
        let kind = common[11];
        if kind == LIMITS {
            return Err(DecodeError::Form);
        }
        if !(DATA..=START_MSG).contains(&kind) {
            return Err(DecodeError::Type);
        }
        if be_u16(common, URGENT_AT) == Some(HIJACK_URGENT) {
            return Err(DecodeError::Reserved);
        }
        let ports = Ports {
            src: be_u16(common, 0).ok_or(DecodeError::Short)?,
            dst: be_u16(common, 2).ok_or(DecodeError::Short)?,
        };
        let id = be_u64(common, 20).ok_or(DecodeError::Short)?;
        let packet = match kind {
            DATA => Packet::Data(data(bytes, id)?),
            _ => control(bytes, kind, id)?,
        };
        Ok(Decoded {
            ports,
            packet,
            need_limits: false,
        })
    }

    fn header(
        packet: &Packet<'_>,
        ports: Ports,
        _need_limits: bool,
        out: &mut [u8],
    ) -> Result<usize, NoRoom> {
        let (kind, id, len) = match *packet {
            Packet::Data(data) => (DATA, data.id, DATA_LEN),
            Packet::Grant { id, .. } => (GRANT, id, GRANT_LEN),
            Packet::Resend { id, .. } => (RESEND, id, RESEND_LEN),
            Packet::RpcUnknown { id } => (RPC_UNKNOWN, id, COMMON_LEN),
            Packet::Busy { id } => (BUSY, id, COMMON_LEN),
            Packet::Cutoffs { .. } => (CUTOFFS, 0, CUTOFFS_LEN),
            Packet::NeedAck { id } => (NEED_ACK, id, COMMON_LEN),
            Packet::Ack { id, acks } => {
                if acks.len() > MAX_ACKS {
                    return Err(NoRoom);
                }
                (ACK, id, ACK_LEN)
            }
            Packet::StartMsg { id, .. } => (START_MSG, id, START_MSG_LEN),
            // The crate never sends FREEZE, and LIMITS has no layout here.
            Packet::Freeze | Packet::Limits(_) => return Err(NoRoom),
        };
        let head = out.get_mut(..len).ok_or(NoRoom)?;
        head.fill(0);
        head[0..2].copy_from_slice(&ports.src.to_be_bytes());
        head[2..4].copy_from_slice(&ports.dst.to_be_bytes());
        head[11] = kind;
        head[12] = if kind == DATA {
            DOFF_DATA
        } else {
            DOFF_CONTROL
        };
        head[20..28].copy_from_slice(&id.to_be_bytes());
        let body = &mut head[COMMON_LEN..];
        match *packet {
            Packet::Data(data) => write_data(body, &data),
            Packet::Grant {
                offset, priority, ..
            } => {
                body[0..4].copy_from_slice(&offset.to_be_bytes());
                body[4] = priority.level();
            }
            Packet::Resend {
                offset,
                length,
                priority,
                ..
            } => {
                body[0..4].copy_from_slice(&offset.to_be_bytes());
                body[4..8].copy_from_slice(&length.to_be_bytes());
                body[8] = priority.level();
            }
            Packet::Cutoffs { cutoffs, version } => {
                for (field, cutoff) in body.chunks_exact_mut(4).zip(cutoffs) {
                    field.copy_from_slice(&cutoff.to_be_bytes());
                }
                body[32..34].copy_from_slice(&version.to_be_bytes());
            }
            Packet::Ack { acks, .. } => {
                let count = u16::try_from(acks.len()).map_err(|_| NoRoom)?;
                body[0..2].copy_from_slice(&count.to_be_bytes());
                let entries = body[2..].chunks_exact_mut(ACK_ENTRY_LEN);
                for (field, entry) in entries.zip(acks.iter()) {
                    field[0..8].copy_from_slice(&entry.client_id.to_be_bytes());
                    field[8..10].copy_from_slice(&entry.server_port.to_be_bytes());
                }
            }
            Packet::StartMsg { msg_length, .. } => {
                body[0..4].copy_from_slice(&msg_length.to_be_bytes());
            }
            Packet::RpcUnknown { .. }
            | Packet::Busy { .. }
            | Packet::NeedAck { .. }
            | Packet::Freeze
            | Packet::Limits(_) => {}
        }
        Ok(len)
    }
}

/// Writes the part of the DATA header behind the common header into `body`,
/// which holds exactly those 28 bytes, zeroed.
fn write_data(body: &mut [u8], data: &Data<'_>) {
    body[0..4].copy_from_slice(&data.msg_length.to_be_bytes());
    if let Some(ack) = data.ack {
        body[8..16].copy_from_slice(&ack.client_id.to_be_bytes());
        body[16..18].copy_from_slice(&ack.server_port.to_be_bytes());
    }
    body[18..20].copy_from_slice(&data.cutoff_version.to_be_bytes());
    body[20] = u8::from(data.retransmit);
    if data.incast {
        body[21] = INCAST_MARK;
    }
    body[24..28].copy_from_slice(&data.offset.to_be_bytes());
}

/// Reads a DATA packet whose common header is already checked.
fn data(bytes: &[u8], id: u64) -> Result<Data<'_>, DecodeError> {
    let pad = bytes.get(PAD_AT).copied().ok_or(DecodeError::Length)?;
    let rest = bytes
        .get(PAD_AT + 1..PAD_AT + 3)
        .ok_or(DecodeError::Length)?;
    if pad & !INCAST_MARK != 0 || rest.iter().any(|&byte| byte != 0) {
        return Err(DecodeError::Reserved);
    }
    let header = bytes.get(..DATA_LEN).ok_or(DecodeError::Length)?;
    let field32 = |at: usize| be_u32(header, at).ok_or(DecodeError::Length);
    let msg_length = field32(28)?;
    let client_id = be_u64(header, 36).ok_or(DecodeError::Length)?;
    let server_port = be_u16(header, 44).ok_or(DecodeError::Length)?;
    let cutoff_version = be_u16(header, 46).ok_or(DecodeError::Length)?;
    let retransmit = header[48] != 0;
    let offset = field32(52)?;
    let payload = bytes.get(DATA_LEN..).ok_or(DecodeError::Length)?;
    let carried = u32::try_from(payload.len()).map_err(|_| DecodeError::Length)?;
    if carried == 0 && msg_length > 0 {
        return Err(DecodeError::Length);
    }
    let end = offset.checked_add(carried).ok_or(DecodeError::Range)?;
    if end > msg_length || (carried == 0 && offset > 0) {
        return Err(DecodeError::Range);
    }
    let ack = (client_id != 0).then_some(AckEntry {
        client_id,
        server_port,
    });
    Ok(Data {
        id,
        msg_length,
        offset,
        retransmit,
        ack,
        cutoff_version,
        incast: pad & INCAST_MARK != 0,
        payload,
    })
}

/// Reads a control packet of type `kind` whose common header is already
/// checked.
fn control(bytes: &[u8], kind: u8, id: u64) -> Result<Packet<'_>, DecodeError> {
    let len = match kind {
        GRANT => GRANT_LEN,
        RESEND => RESEND_LEN,
        CUTOFFS => CUTOFFS_LEN,
        ACK => ACK_LEN,
        START_MSG => START_MSG_LEN,
        _ => COMMON_LEN,
    };
    if bytes.len() != len {
        return Err(DecodeError::Length);
    }
    let field32 = |at: usize| be_u32(bytes, at).ok_or(DecodeError::Length);
    let level = |at: usize| {
        bytes
            .get(at)
            .and_then(|&level| Priority::new(level))
            .ok_or(DecodeError::Range)
    };
    let packet = match kind {
        GRANT => Packet::Grant {
            id,
            offset: field32(28)?,
            priority: level(32)?,
        },
        RESEND => Packet::Resend {
            id,
            offset: field32(28)?,
            length: field32(32)?,
            priority: level(36)?,
        },
        RPC_UNKNOWN => Packet::RpcUnknown { id },
        BUSY => Packet::Busy { id },
        CUTOFFS => {
            let mut cutoffs = [0u32; 8];
            for (i, cutoff) in cutoffs.iter_mut().enumerate() {
                *cutoff = field32(COMMON_LEN + 4 * i)?;
            }
            Packet::Cutoffs {
                cutoffs,
                version: be_u16(bytes, 60).ok_or(DecodeError::Length)?,
            }
        }
        FREEZE => Packet::Freeze,
        NEED_ACK => Packet::NeedAck { id },
        ACK => {
            let count = usize::from(be_u16(bytes, COMMON_LEN).ok_or(DecodeError::Length)?);
            if count > MAX_ACKS {
                return Err(DecodeError::Range);
            }
            let start = COMMON_LEN + 2;
            let entries = bytes
                .get(start..start + count * ACK_ENTRY_LEN)
                .ok_or(DecodeError::Length)?;
            Packet::Ack {
                id,
                acks: Acks::from_wire(entries, ACK_ENTRY_LEN),
            }
        }
        _ => Packet::StartMsg {
            id,
            msg_length: field32(28)?,
        },
    };
    Ok(packet)
}
