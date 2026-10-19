# SPDX-License-Identifier: MIT
"""The full wire form of Homa, `homa_wire.h` at HomaModule commit 50d65305,
written from the header file.

The reference of `tests/notebooks/05_homa_wire_full.ipynb` for
`homa::wire::Full`, sharing no code with it. The layouts are those of
`homa_wire.h` lines 93 to 490 at
https://github.com/PlatformLab/HomaModule/tree/50d65305aeef9b6ae5cf193a808c7e95f9b1dcb3,
and the fields HomaModule fills are filled as it fills them: `doff` as
`homa_set_doff` writes it, the 52 bytes of the DATA header before its segment
offset in words on DATA and 5 words on every other type (`homa_outgoing.c`
lines 256, 257 and 516), every field it does not use 0, and the ACK always 80
bytes (`homa_incoming.c` lines 1083 to 1093). The incast mark in bit 0 of the
first pad byte of DATA is the design note's addition, which HomaModule reads
no byte of. The order in which a decoder checks a packet, and so which error
it names first, is the reading of the track's close page for the full form.

A packet is a dictionary whose `type` names it and whose other keys are the
fields of the packet as the devtool writes them: `DATA` with `id`,
`msg_length`, `offset`, `retransmit`, `ack`, `cutoff_version`, `incast` and
`payload` as lowercase hex, `GRANT` with `id`, `offset` and `priority`,
`RESEND` with `id`, `offset`, `length` and `priority`, `RPC_UNKNOWN`, `BUSY`
and `NEED_ACK` with `id`, `CUTOFFS` with `cutoffs` and `version`, `FREEZE`,
`ACK` with `id` and `acks`, each entry a `client_id` and a `server_port`, and
`START_MSG` with `id` and `msg_length`. A decoded packet carries its ports as
`{"src", "dst"}`.

    Common header, 28 bytes
     0  sport 2      2  dport 2     4  sequence 4, 0     8  ack 3, 0
    11  type 1      12  doff 1     13  flags 1, 0       14  window 2, 0
    16  checksum 2, 0               18  urgent 2, 0, 0xb97d hijacking
    20  sender_id 8

    DATA 56      28 msg_length 4, 32 reserved1 4, 36 ack.client_id 8,
                 44 ack.server_port 2, 46 cutoff_version 2, 48 retransmit 1,
                 49 pad 3, bit 0 of the first the incast mark, 52 offset 4
    GRANT 33     28 offset 4, 32 priority 1
    RESEND 37    28 offset 4, 32 length 4, 36 priority 1
    RPC_UNKNOWN, BUSY, FREEZE, NEED_ACK    28
    CUTOFFS 62   28 eight cutoffs of 4, 60 version 2
    ACK 80       28 num_acks 2, 30 five entries of client_id 8, server_port 2
    START_MSG 32 28 msg_length 4
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
}
CODES = {name: code for code, name in TYPES.items()}

COMMON_LEN = 28
LENGTHS = {
    DATA: 56,
    GRANT: 33,
    RESEND: 37,
    RPC_UNKNOWN: 28,
    BUSY: 28,
    CUTOFFS: 62,
    FREEZE: 28,
    NEED_ACK: 28,
    ACK: 80,
    START_MSG: 32,
}
MAX_ACKS = 5
HIJACK_URGENT = 0xB97D
DOFF_DATA = 13 << 4
DOFF_CONTROL = 5 << 4
INCAST_MARK = 1


def _be(value, width):
    return value.to_bytes(width, "big")


def _get(bytes_, at, width):
    return int.from_bytes(bytes_[at:at + width], "big")


def encode(packet, ports=None):
    """Return the bytes of `packet` with the ports `ports`, the header
    followed by the payload of DATA, or None for a packet the form has no
    layout for, LIMITS, which the compact form has, and FREEZE, which the
    crate never writes."""
    ports = ports or {"src": 0, "dst": 0}
    kind = packet["type"]
    if kind not in CODES or kind == "FREEZE":
        return None
    code = CODES[kind]
    if kind == "ACK" and len(packet["acks"]) > MAX_ACKS:
        return None
    ident = 0 if kind == "CUTOFFS" else packet["id"]
    head = bytearray(LENGTHS[code])
    head[0:2] = _be(ports["src"], 2)
    head[2:4] = _be(ports["dst"], 2)
    head[11] = code
    head[12] = DOFF_DATA if code == DATA else DOFF_CONTROL
    head[20:28] = _be(ident, 8)
    if kind == "DATA":
        head[28:32] = _be(packet["msg_length"], 4)
        ack = packet.get("ack")
        if ack is not None:
            head[36:44] = _be(ack["client_id"], 8)
            head[44:46] = _be(ack["server_port"], 2)
        head[46:48] = _be(packet["cutoff_version"], 2)
        head[48] = 1 if packet["retransmit"] else 0
        head[49] = INCAST_MARK if packet["incast"] else 0
        head[52:56] = _be(packet["offset"], 4)
        return bytes(head) + bytes.fromhex(packet["payload"])
    if kind == "GRANT":
        head[28:32] = _be(packet["offset"], 4)
        head[32] = packet["priority"]
    elif kind == "RESEND":
        head[28:32] = _be(packet["offset"], 4)
        head[32:36] = _be(packet["length"], 4)
        head[36] = packet["priority"]
    elif kind == "CUTOFFS":
        for i, cutoff in enumerate(packet["cutoffs"]):
            head[28 + 4 * i:32 + 4 * i] = _be(cutoff, 4)
        head[60:62] = _be(packet["version"], 2)
    elif kind == "ACK":
        head[28:30] = _be(len(packet["acks"]), 2)
        for i, entry in enumerate(packet["acks"]):
            at = 30 + 10 * i
            head[at:at + 8] = _be(entry["client_id"], 8)
            head[at + 8:at + 10] = _be(entry["server_port"], 2)
    elif kind == "START_MSG":
        head[28:32] = _be(packet["msg_length"], 4)
    return bytes(head)


def decode(bytes_):
    """Return `(decoded, None)` for a packet the form reads, the decoded
    packet as `{"ports", "packet", "need_limits"}`, or `(None, error)` with
    the name of the rule it breaks.

    The rules in order: Short below 28 bytes, Form for LIMITS and Type for
    any other code outside 0x10 to 0x19, Reserved for the marker of TCP
    hijacking in `urgent` and for a bit of the pad of DATA other than the
    incast mark, Length for a packet shorter than its type's header, a
    control packet longer than its header and a DATA packet of a message
    that is not empty with no payload, and Range for a level above 7 in
    GRANT or RESEND, an ACK of more than five entries and a DATA payload
    past its `msg_length`."""
    if len(bytes_) < COMMON_LEN:
        return None, "Short"
    code = bytes_[11]
    if code == LIMITS:
        return None, "Form"
    if code not in TYPES:
        return None, "Type"
    if _get(bytes_, 18, 2) == HIJACK_URGENT:
        return None, "Reserved"
    ports = {"src": _get(bytes_, 0, 2), "dst": _get(bytes_, 2, 2)}
    ident = _get(bytes_, 20, 8)
    kind = TYPES[code]
    if code == DATA:
        if len(bytes_) < 52:
            return None, "Length"
        if bytes_[49] & ~INCAST_MARK or bytes_[50] or bytes_[51]:
            return None, "Reserved"
        if len(bytes_) < LENGTHS[DATA]:
            return None, "Length"
        msg_length = _get(bytes_, 28, 4)
        offset = _get(bytes_, 52, 4)
        payload = bytes_[56:]
        if not payload and msg_length > 0:
            return None, "Length"
        if offset + len(payload) > msg_length or (not payload and offset > 0):
            return None, "Range"
        client_id = _get(bytes_, 36, 8)
        packet = {
            "type": kind,
            "id": ident,
            "msg_length": msg_length,
            "offset": offset,
            "retransmit": bytes_[48] != 0,
            "ack": {"client_id": client_id, "server_port": _get(bytes_, 44, 2)} if client_id else None,
            "cutoff_version": _get(bytes_, 46, 2),
            "incast": bool(bytes_[49] & INCAST_MARK),
            "payload": payload.hex(),
        }
        return {"ports": ports, "packet": packet, "need_limits": False}, None
    if len(bytes_) != LENGTHS[code]:
        return None, "Length"
    packet = {"type": kind}
    if kind != "CUTOFFS" and kind != "FREEZE":
        packet["id"] = ident
    if kind == "GRANT":
        if bytes_[32] > 7:
            return None, "Range"
        packet.update(offset=_get(bytes_, 28, 4), priority=bytes_[32])
    elif kind == "RESEND":
        if bytes_[36] > 7:
            return None, "Range"
        packet.update(offset=_get(bytes_, 28, 4), length=_get(bytes_, 32, 4), priority=bytes_[36])
    elif kind == "CUTOFFS":
        packet.update(cutoffs=[_get(bytes_, 28 + 4 * i, 4) for i in range(8)], version=_get(bytes_, 60, 2))
    elif kind == "ACK":
        count = _get(bytes_, 28, 2)
        if count > MAX_ACKS:
            return None, "Range"
        packet["acks"] = [
            {"client_id": _get(bytes_, 30 + 10 * i, 8), "server_port": _get(bytes_, 38 + 10 * i, 2)}
            for i in range(count)
        ]
    elif kind == "START_MSG":
        packet["msg_length"] = _get(bytes_, 28, 4)
    return {"ports": ports, "packet": packet, "need_limits": False}, None
