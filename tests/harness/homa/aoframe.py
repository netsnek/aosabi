# SPDX-License-Identifier: MIT
"""AoFrame in both forms, the framing of one Homa packet on a byte stream.

The reference of `tests/notebooks/04_aoframe.ipynb` for `homa::stream`,
sharing no code with it, written from the Homa design note, first version,
section 5, and the readings the track's close page records for the bytes the
note leaves open.

The delimited form, on a serial line and USB serial, is a zero byte, the COBS
of the packet and its CRC-32C least significant byte first, and a zero byte.
A receiver collects the bytes that are not zero into a buffer of fixed size,
ignores an empty frame, drops a frame that overflows the buffer as `TooLong`
at the delimiter that ends it, decodes COBS and then checks the CRC, and names
the check that meets a damage first, `Cobs` or `Crc`.

The prefixed form, on a stream Unix Socket and the Bluetooth channel, is a
four byte big endian length and the packet. A receiver refuses a length above
its buffer as `TooLong` once the fourth byte of the prefix arrives, before a
byte of the packet is stored, and skips the bytes of the refused packet so
the stream stays in step. A length of 0 is an empty packet.
"""

from . import cobs as _cobs
from .crc32c import appended, crc32c

DELIMITER = 0
CRC_LEN = 4
PREFIX_LEN = 4

DELIMITED = "delimited"
PREFIXED = "prefixed"


def encode(mode: str, packet: bytes) -> bytes:
    """Return the frame of `packet` in the form `mode`."""
    if mode == DELIMITED:
        return b"\x00" + _cobs.encode(packet + appended(packet)) + b"\x00"
    if mode == PREFIXED:
        return len(packet).to_bytes(PREFIX_LEN, "big") + packet
    raise ValueError("no form of AoFrame is named %r" % mode)


def frame_len(mode: str, packet_len: int) -> int:
    """Return the most bytes one frame of a `packet_len` byte packet takes."""
    if mode == DELIMITED:
        return _cobs.max_len(packet_len + CRC_LEN) + 2
    return packet_len + PREFIX_LEN


def buffer_len(mode: str, max_packet: int) -> int:
    """Return the buffer a receiver of packets up to `max_packet` holds."""
    if mode == DELIMITED:
        return _cobs.max_len(max_packet + CRC_LEN)
    return max_packet


class Decoder:
    """A receiver of one byte stream with a buffer of fixed size.

    `push` takes one byte and returns None, or a pair once a frame ends:
    `("packet", bytes)` for a checked packet and `("error", name)` for a
    dropped frame, where the name is `TooLong`, `Cobs` or `Crc`. `stored`
    counts the bytes of the frame the buffer holds now, which a notebook reads
    to show that a refused prefix stored nothing.
    """

    def __init__(self, mode: str, max_packet: int):
        if mode not in (DELIMITED, PREFIXED):
            raise ValueError("no form of AoFrame is named %r" % mode)
        self.mode = mode
        self.capacity = buffer_len(mode, max_packet)
        self.buf = bytearray()
        self.overflow = False
        self.prefix = bytearray()
        self.need = None
        self.skip = 0

    @property
    def stored(self) -> int:
        return len(self.buf)

    def push(self, byte: int):
        if self.mode == DELIMITED:
            return self._push_delimited(byte)
        return self._push_prefixed(byte)

    def _push_delimited(self, byte: int):
        if byte != DELIMITER:
            if not self.overflow:
                if len(self.buf) < self.capacity:
                    self.buf.append(byte)
                else:
                    self.overflow = True
                    self.buf.clear()
            return None
        overflowed = self.overflow
        frame = bytes(self.buf)
        self.overflow = False
        self.buf.clear()
        if overflowed:
            return ("error", "TooLong")
        if not frame:
            return None
        try:
            decoded = _cobs.decode(frame)
        except _cobs.CobsError:
            return ("error", "Cobs")
        if len(decoded) < CRC_LEN:
            return ("error", "Crc")
        packet, sent = decoded[:-CRC_LEN], decoded[-CRC_LEN:]
        if crc32c(packet) != int.from_bytes(sent, "little"):
            return ("error", "Crc")
        return ("packet", packet)

    def _push_prefixed(self, byte: int):
        if self.skip:
            self.skip -= 1
            return None
        if self.need is None:
            self.prefix.append(byte)
            if len(self.prefix) < PREFIX_LEN:
                return None
            length = int.from_bytes(self.prefix, "big")
            self.prefix.clear()
            if length > self.capacity:
                self.skip = length
                return ("error", "TooLong")
            if length == 0:
                return ("packet", b"")
            self.need = length
            return None
        self.buf.append(byte)
        if len(self.buf) < self.need:
            return None
        packet = bytes(self.buf)
        self.buf.clear()
        self.need = None
        return ("packet", packet)


def feed(mode: str, stream: bytes, max_packet: int) -> list:
    """Push `stream` through one decoder and return every frame that ended.

    Each entry is `(offset, kind, value)`, the offset of the byte that ended
    the frame, `"packet"` with the packet or `"error"` with the name of the
    drop.
    """
    decoder = Decoder(mode, max_packet)
    events = []
    for offset, byte in enumerate(stream):
        ended = decoder.push(byte)
        if ended is not None:
            events.append((offset, ended[0], ended[1]))
    return events
