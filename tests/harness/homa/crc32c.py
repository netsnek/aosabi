# SPDX-License-Identifier: MIT
"""CRC-32C, the check of a delimited AoFrame, written from its definition.

The reference of `tests/notebooks/04_aoframe.ipynb` for the CRC of the crate
`homa`, sharing no code with it. CRC-32C is the CRC of the Castagnoli
generator that RFC 3720 section 12.1 names, whose byte vectors appendix B.4
publishes. The CRC catalogue gives it as the polynomial 0x1EDC6F41 with
reflected input and output, an initial value and a final XOR of 0xFFFFFFFF,
and the check value 0xE3069283 over the text 123456789.

`crc32c_bitwise` runs the reflected polynomial bit by bit, the plain
definition. `crc32c` looks each byte up in a table of 256 entries that the
same definition fills at import, since a notebook checks a hundred thousand
frames and a pass bit by bit over each would take minutes. Neither shares the
table of 16 entries of the crate. The standard library of Python carries no
CRC-32C, which is why this file exists.
"""

POLY_REFLECTED = 0x82F63B78
"""The polynomial 0x1EDC6F41 with its bits reflected."""


def crc32c_bitwise(data: bytes, crc: int = 0) -> int:
    """Return the CRC-32C of `data` bit by bit, continuing from `crc`."""
    value = crc ^ 0xFFFFFFFF
    for byte in data:
        value ^= byte
        for _ in range(8):
            if value & 1:
                value = (value >> 1) ^ POLY_REFLECTED
            else:
                value >>= 1
    return value ^ 0xFFFFFFFF


def _remainder(byte: int) -> int:
    """Return the remainder of one byte under the polynomial, bit by bit."""
    value = byte
    for _ in range(8):
        value = (value >> 1) ^ POLY_REFLECTED if value & 1 else value >> 1
    return value


TABLE = tuple(_remainder(byte) for byte in range(256))
"""The remainder of each byte value, filled by the definition above."""


def crc32c(data: bytes, crc: int = 0) -> int:
    """Return the CRC-32C of `data`, continuing from the value `crc` returned."""
    value = crc ^ 0xFFFFFFFF
    table = TABLE
    for byte in data:
        value = table[(value ^ byte) & 0xFF] ^ (value >> 8)
    return value ^ 0xFFFFFFFF


def appended(data: bytes) -> bytes:
    """Return the four bytes a delimited frame appends, least significant first."""
    return crc32c(data).to_bytes(4, "little")
