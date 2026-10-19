# SPDX-License-Identifier: MIT
"""COBS, the byte stuffing of a delimited AoFrame, written from the paper.

The reference of `tests/notebooks/04_aoframe.ipynb` for the COBS of the crate
`homa`, sharing no code with it.

The paper lets a block of 254 bytes that ends the input be followed by the
code 1 of an empty block or by nothing. The encoder below writes nothing, the
shorter ending, which is the reading of the crate that the track's close page
records as reading 6, and the decoder reads both endings.
"""


class CobsError(ValueError):
    """The input is no COBS encoding: a code byte is zero or runs past the end."""


def max_len(length: int) -> int:
    """Return the most bytes the encoding of `length` bytes takes."""
    return length + length // 254 + 1


def encode(data: bytes) -> bytes:
    """Return the COBS encoding of `data`, which holds no zero byte."""
    out = bytearray()
    block = bytearray()
    after_full = False
    for byte in data:
        after_full = False
        if byte == 0:
            out.append(len(block) + 1)
            out += block
            block.clear()
            continue
        block.append(byte)
        if len(block) == 254:
            out.append(255)
            out += block
            block.clear()
            after_full = True
    if not after_full or not data:
        out.append(len(block) + 1)
        out += block
    return bytes(out)


def decode(data: bytes) -> bytes:
    """Return the bytes the COBS encoding `data` holds, or raise `CobsError`."""
    out = bytearray()
    read = 0
    while read < len(data):
        code = data[read]
        if code == 0:
            raise CobsError("a code byte is zero at offset %d" % read)
        read += 1
        end = read + code - 1
        if end > len(data):
            raise CobsError("the block at offset %d runs past the end" % (read - 1))
        out += data[read:end]
        read = end
        if code != 255 and read < len(data):
            out.append(0)
    return bytes(out)
