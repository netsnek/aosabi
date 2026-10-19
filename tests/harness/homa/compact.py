# SPDX-License-Identifier: MIT
"""The compact wire form of Homa, version 0, written from its design.

The reference of `tests/notebooks/03_homa_packets.ipynb` for
`homa::wire::Compact`, sharing no code with it. The layouts are those of the
Homa design note, first version, section 2.3, which its second version keeps
for every carrier but Network. The order in which a decoder checks a packet,
and so which error it names first, is reading 5 of the track's close page,
which the note leaves open.

A packet is a dictionary whose `type` names it and whose other keys are the
fields of the packet as the devtool writes them: `DATA` with `id`,
`msg_length`, `offset`, `retransmit`, `ack`, `cutoff_version`, `incast` and
`payload` as lowercase hex, `GRANT` with `id`, `offset` and `priority`,
`RESEND` with `id`, `offset`, `length` and `priority`, `RPC_UNKNOWN`, `BUSY`
and `NEED_ACK` with `id`, `ACK` with `id` and `acks`, each entry a
`client_id` and a `server_port`, and `LIMITS` with `max_packet`,
`max_message`, `unsched_bytes` and `max_rpcs`. A decoded GRANT or RESEND
carries the level 7, since the form carries none, and every decoded packet
carries the ports 0.

    Common header, 6 bytes
     0  type       Homa's code, 0x20 for LIMITS
     1  flags      bit 0 RETRANSMIT, bit 1 ACK, bit 2 WHOLE, bit 3 NEED_LIMITS,
                   bits 4 and 5 reserved, bits 6 and 7 the version, 0
     2  sender_id  4 bytes, the RPC as the sender names it, 0 on LIMITS

    DATA, a whole message   6 bytes, 10 with ACK, then the message
    DATA, a part            6 + msg_length 4 + offset 4, 14, 18 with ACK
    GRANT                   6 + offset 4
    RESEND                  6 + offset 4 + length 4
    RPC_UNKNOWN, BUSY, NEED_ACK   6
    ACK                     6 + 4 per client identifier, 1 to 5
    LIMITS                  6 + max_rpcs 1 + reserved 1 + max_packet 2
                            + unsched_bytes 4 + max_message 4, 18 bytes
"""

DATA = 0x10
GRANT = 0x11
RESEND = 0x12
RPC_UNKNOWN = 0x13
BUSY = 0x14
CUTOFFS = 0x15
FREEZE = 0x16
NEED_ACK = 0x17
ACK = 0x18
START_MSG = 0x19
LIMITS = 0x20

TYPES = {
    DATA: "DATA",
    GRANT: "GRANT",
    RESEND: "RESEND",
    RPC_UNKNOWN: "RPC_UNKNOWN",
    BUSY: "BUSY",
    CUTOFFS: "CUTOFFS",
    FREEZE: "FREEZE",
    NEED_ACK: "NEED_ACK",
    ACK: "ACK",
    START_MSG: "START_MSG",
    LIMITS: "LIMITS",
}
CODES = {name: code for code, name in TYPES.items()}

RETRANSMIT = 1 << 0
CARRIES_ACK = 1 << 1
WHOLE = 1 << 2
NEED_LIMITS = 1 << 3
RESERVED = 0b0011_0000
DATA_ONLY = RETRANSMIT | CARRIES_ACK | WHOLE

COMMON_LEN = 6
MAX_ACKS = 5
MIN_PACKET = 20
HIGHEST = 7
U32 = 0xFFFFFFFF


class DecodeError(ValueError):
    """The packet does not decode, with the name of the rule it breaks."""

    def __init__(self, name: str):
        super().__init__(name)
        self.name = name


class NoRoom(ValueError):
    """The form has no layout for the packet."""


def _u32(data: bytes, at: int):
    """Return the big endian u32 at `at`, or None past the end of `data`."""
    if at + 4 > len(data):
        return None
    return int.from_bytes(data[at : at + 4], "big")


def decode(data: bytes) -> dict:
    """Return the decoded packet, or raise `DecodeError` with the rule it breaks.

    The rules are checked in the order of reading 5: `Short` below 6 bytes,
    `Version`, `Reserved` for flag bit 4 or 5, `Form` for the codes 0x15,
    0x16 and 0x19 and `Type` for any other unknown code, `Reserved` for a
    flag only DATA may set on another type and for a LIMITS with its reserved
    byte or a sender, `Length` for a length that does not match the type and
    a part of DATA without payload, and `Range` for a part of DATA past its
    length and for a LIMITS outside its bounds.
    """
    if len(data) < COMMON_LEN:
        raise DecodeError("Short")
    kind, flags = data[0], data[1]
    sender = int.from_bytes(data[2:6], "big")
    if flags >> 6 != 0:
        raise DecodeError("Version")
    if flags & RESERVED:
        raise DecodeError("Reserved")
    if kind == DATA:
        packet = _decode_data(data, flags, sender)
    elif kind in (GRANT, RESEND, RPC_UNKNOWN, BUSY, NEED_ACK, ACK, LIMITS):
        if flags & DATA_ONLY:
            raise DecodeError("Reserved")
        packet = _decode_control(data, kind, sender)
    elif kind in (CUTOFFS, FREEZE, START_MSG):
        raise DecodeError("Form")
    else:
        raise DecodeError("Type")
    return {
        "ports": {"src": 0, "dst": 0},
        "packet": packet,
        "need_limits": bool(flags & NEED_LIMITS),
    }


def _decode_data(data: bytes, flags: int, sender: int) -> dict:
    whole = bool(flags & WHOLE)
    at = COMMON_LEN if whole else COMMON_LEN + 8
    if whole:
        msg_length, offset = 0, 0
    else:
        msg_length = _u32(data, COMMON_LEN)
        offset = _u32(data, COMMON_LEN + 4)
        if msg_length is None or offset is None:
            raise DecodeError("Length")
    ack = None
    if flags & CARRIES_ACK:
        client = _u32(data, at)
        if client is None:
            raise DecodeError("Length")
        ack = {"client_id": client, "server_port": 0}
        at += 4
    if at > len(data):
        raise DecodeError("Length")
    payload = data[at:]
    if whole:
        msg_length = len(payload)
    else:
        if not payload:
            raise DecodeError("Length")
        if offset + len(payload) > U32 or offset + len(payload) > msg_length:
            raise DecodeError("Range")
    return {
        "type": "DATA",
        "id": sender,
        "msg_length": msg_length,
        "offset": offset,
        "retransmit": bool(flags & RETRANSMIT),
        "ack": ack,
        "cutoff_version": 0,
        "incast": False,
        "payload": payload.hex(),
    }


def _decode_control(data: bytes, kind: int, sender: int) -> dict:
    def exact(length):
        if len(data) != length:
            raise DecodeError("Length")

    if kind == GRANT:
        exact(10)
        return {"type": "GRANT", "id": sender, "offset": _u32(data, 6), "priority": HIGHEST}
    if kind == RESEND:
        exact(14)
        return {
            "type": "RESEND",
            "id": sender,
            "offset": _u32(data, 6),
            "length": _u32(data, 10),
            "priority": HIGHEST,
        }
    if kind in (RPC_UNKNOWN, BUSY, NEED_ACK):
        exact(6)
        return {"type": TYPES[kind], "id": sender}
    if kind == ACK:
        entries = len(data) - COMMON_LEN
        if entries == 0 or entries % 4 or entries // 4 > MAX_ACKS:
            raise DecodeError("Length")
        acks = [
            {"client_id": _u32(data, at), "server_port": 0}
            for at in range(COMMON_LEN, len(data), 4)
        ]
        return {"type": "ACK", "id": sender, "acks": acks}
    if sender != 0 or (len(data) > 7 and data[7] != 0):
        raise DecodeError("Reserved")
    exact(18)
    limits = {
        "type": "LIMITS",
        "max_packet": int.from_bytes(data[8:10], "big"),
        "max_message": _u32(data, 14),
        "unsched_bytes": _u32(data, 10),
        "max_rpcs": data[6],
    }
    if (
        limits["max_rpcs"] == 0
        or limits["max_packet"] < MIN_PACKET
        or limits["unsched_bytes"] == 0
        or limits["max_message"] == 0
    ):
        raise DecodeError("Range")
    return limits


def _common(kind: int, flags: int, ident: int) -> bytes:
    return bytes([kind, flags]) + (ident & U32).to_bytes(4, "big")


def encode(packet: dict, need_limits: bool = False) -> bytes:
    """Return the bytes of `packet`, its header and the payload of a DATA.

    A DATA packet whose payload is the whole message at offset 0 is written
    with WHOLE. CUTOFFS, FREEZE, START_MSG and an ACK with no entry or more
    than five have no layout on this form and raise `NoRoom`.
    """
    kind = packet["type"]
    flags = NEED_LIMITS if need_limits else 0
    ident = packet.get("id", 0)
    if kind == "DATA":
        payload = bytes.fromhex(packet.get("payload", ""))
        offset = packet.get("offset", 0)
        msg_length = packet.get("msg_length", 0)
        whole = offset == 0 and len(payload) == msg_length
        ack = packet.get("ack")
        if packet.get("retransmit"):
            flags |= RETRANSMIT
        if ack is not None:
            flags |= CARRIES_ACK
        if whole:
            flags |= WHOLE
        out = _common(DATA, flags, ident)
        if not whole:
            out += msg_length.to_bytes(4, "big") + offset.to_bytes(4, "big")
        if ack is not None:
            out += (ack["client_id"] & U32).to_bytes(4, "big")
        return out + payload
    if kind == "GRANT":
        return _common(GRANT, flags, ident) + packet["offset"].to_bytes(4, "big")
    if kind == "RESEND":
        return (
            _common(RESEND, flags, ident)
            + packet["offset"].to_bytes(4, "big")
            + packet["length"].to_bytes(4, "big")
        )
    if kind in ("RPC_UNKNOWN", "BUSY", "NEED_ACK"):
        return _common(CODES[kind], flags, ident)
    if kind == "ACK":
        acks = packet.get("acks", [])
        if not acks or len(acks) > MAX_ACKS:
            raise NoRoom("an ACK carries 1 to 5 entries on the compact form")
        return _common(ACK, flags, ident) + b"".join(
            (entry["client_id"] & U32).to_bytes(4, "big") for entry in acks
        )
    if kind == "LIMITS":
        return (
            _common(LIMITS, flags, 0)
            + bytes([packet["max_rpcs"], 0])
            + packet["max_packet"].to_bytes(2, "big")
            + packet["unsched_bytes"].to_bytes(4, "big")
            + packet["max_message"].to_bytes(4, "big")
        )
    raise NoRoom("the compact form has no layout for %s" % kind)


def header_len(packet: dict) -> int:
    """Return the bytes of the header of `packet`, its payload left out."""
    encoded = encode(packet)
    if packet["type"] == "DATA":
        return len(encoded) - len(bytes.fromhex(packet.get("payload", "")))
    return len(encoded)
