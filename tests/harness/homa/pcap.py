# SPDX-License-Identifier: MIT
"""A reader of the capture files `tcpdump -w` writes, for the Homa packets of
the raw IP and interop halves of `29_homa_raw` and the capture half of
`05_homa_wire_full`.

The file is the classic pcap format, a global header of 24 bytes and a record
header of 16 before each frame, in either byte order and with a fraction of
microseconds or of nanoseconds by its magic number. A frame is read by its
link type: Ethernet 1 with its 802.1Q tags, the raw IP of 101 and 12, the
loopback of BSD 0, and Linux cooked captures 113 and 276, which
`tcpdump -i any` writes. Each IP packet of protocol 146 becomes a
`HomaPacket` with its addresses, its TOS or traffic class and the Homa bytes,
which `full.py` decodes, so the DSCP of every captured packet can be compared
with the level the engine's cutoffs give. `write` writes a file of raw IP
packets, which lets a notebook check this reader against bytes it built
itself.
"""

from __future__ import annotations

import ipaddress
import struct
from dataclasses import dataclass

IPPROTO_HOMA = 146
MAGIC_MICRO = 0xA1B2C3D4
MAGIC_NANO = 0xA1B23C4D

LINKTYPE_NULL = 0
LINKTYPE_ETHERNET = 1
LINKTYPE_RAW_OLD = 12
LINKTYPE_RAW = 101
LINKTYPE_LINUX_SLL = 113
LINKTYPE_LINUX_SLL2 = 276

ETHERTYPE_IPV4 = 0x0800
ETHERTYPE_IPV6 = 0x86DD
ETHERTYPE_VLAN = (0x8100, 0x88A8)

# The IPv6 extension headers a packet may carry before its payload, each of
# which names the next header in its first byte and its length in its second.
IPV6_EXTENSIONS = (0, 43, 60)


class PcapError(ValueError):
    """The file is no pcap file this reader knows."""


@dataclass(frozen=True)
class HomaPacket:
    """One IP packet of protocol 146 of a capture."""

    time: float
    version: int
    source: str
    destination: str
    tos: int
    ttl: int
    homa: bytes

    @property
    def level(self):
        """The Homa priority level the packet carries, the three high bits of
        its TOS byte or traffic class."""
        return self.tos >> 5


def _ip(data):
    """Return the Homa packet of the IP packet `data`, or None."""
    if not data:
        return None
    version = data[0] >> 4
    if version == 4 and len(data) >= 20:
        header = (data[0] & 0x0F) * 4
        total = struct.unpack_from(">H", data, 2)[0]
        if data[9] != IPPROTO_HOMA or len(data) < header:
            return None
        return (4, str(ipaddress.IPv4Address(data[12:16])), str(ipaddress.IPv4Address(data[16:20])), data[1],
                data[8], data[header:min(total, len(data))])
    if version == 6 and len(data) >= 40:
        tclass = ((data[0] & 0x0F) << 4) | (data[1] >> 4)
        payload_length = struct.unpack_from(">H", data, 4)[0]
        nxt, at = data[6], 40
        while nxt in IPV6_EXTENSIONS and len(data) >= at + 2:
            nxt, at = data[at], at + 8 * (data[at + 1] + 1)
        if nxt != IPPROTO_HOMA:
            return None
        return (6, str(ipaddress.IPv6Address(data[8:24])), str(ipaddress.IPv6Address(data[24:40])), tclass,
                data[7], data[at:min(40 + payload_length, len(data))])
    return None


def _network(linktype, frame):
    """Return the IP packet of a frame of `linktype`, or None."""
    if linktype in (LINKTYPE_RAW, LINKTYPE_RAW_OLD):
        return frame
    if linktype == LINKTYPE_NULL:
        return frame[4:]
    if linktype == LINKTYPE_ETHERNET:
        at = 12
        ethertype = struct.unpack_from(">H", frame, at)[0] if len(frame) >= 14 else 0
        while ethertype in ETHERTYPE_VLAN and len(frame) >= at + 6:
            at += 4
            ethertype = struct.unpack_from(">H", frame, at)[0]
        return frame[at + 2:] if ethertype in (ETHERTYPE_IPV4, ETHERTYPE_IPV6) else None
    if linktype == LINKTYPE_LINUX_SLL:
        if len(frame) < 16:
            return None
        protocol = struct.unpack_from(">H", frame, 14)[0]
        return frame[16:] if protocol in (ETHERTYPE_IPV4, ETHERTYPE_IPV6) else None
    if linktype == LINKTYPE_LINUX_SLL2:
        if len(frame) < 20:
            return None
        protocol = struct.unpack_from(">H", frame, 0)[0]
        return frame[20:] if protocol in (ETHERTYPE_IPV4, ETHERTYPE_IPV6) else None
    raise PcapError("the link type %d is not one this reader knows" % linktype)


def read(path):
    """Return the link type of the capture at `path` and its Homa packets."""
    with open(path, "rb") as fh:
        data = fh.read()
    if len(data) < 24:
        raise PcapError("%s is shorter than a pcap header" % path)
    for order in ("<", ">"):
        magic = struct.unpack_from(order + "I", data, 0)[0]
        if magic in (MAGIC_MICRO, MAGIC_NANO):
            break
    else:
        raise PcapError("%s has no pcap magic number, a pcapng file needs tcpdump -w with a pcap format" % path)
    scale = 1e-9 if magic == MAGIC_NANO else 1e-6
    linktype = struct.unpack_from(order + "I", data, 20)[0] & 0x0FFFFFFF
    packets = []
    at = 24
    while at + 16 <= len(data):
        seconds, fraction, captured, _original = struct.unpack_from(order + "IIII", data, at)
        frame = data[at + 16:at + 16 + captured]
        at += 16 + captured
        network = _network(linktype, frame)
        found = _ip(network) if network is not None else None
        if found is not None:
            version, source, destination, tos, ttl, homa = found
            packets.append(HomaPacket(seconds + fraction * scale, version, source, destination, tos, ttl, bytes(homa)))
    return linktype, packets


def ipv4(source, destination, homa, tos=0, ttl=64):
    """Return an IPv4 packet of protocol 146 that carries `homa`, with a
    header checksum of 0, which a reader of a capture does not check."""
    header = struct.pack(">BBHHHBBH4s4s", 0x45, tos, 20 + len(homa), 0, 0, ttl, IPPROTO_HOMA, 0,
                         ipaddress.IPv4Address(source).packed, ipaddress.IPv4Address(destination).packed)
    return header + bytes(homa)


def write(path, packets, linktype=LINKTYPE_RAW):
    """Write the frames `packets`, each `(time, bytes)`, as a pcap file with
    microseconds in the byte order of this machine."""
    with open(path, "wb") as fh:
        fh.write(struct.pack("=IHHiIII", MAGIC_MICRO, 2, 4, 0, 0, 262_144, linktype))
        for stamp, frame in packets:
            seconds = int(stamp)
            fh.write(struct.pack("=IIII", seconds, int(round((stamp - seconds) * 1e6)), len(frame), len(frame)))
            fh.write(frame)
