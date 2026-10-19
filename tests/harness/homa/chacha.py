# SPDX-License-Identifier: MIT
"""ChaCha20-Poly1305 of RFC 8439, written from the RFC in the standard library.

The reference of `tests/notebooks/06_lora_frame.ipynb` for the AEAD the LoRa
frame of `aosabi-carrier-lora` seals with, sharing no code with the crate
`chacha20poly1305` the carrier takes. Every function follows the section of
RFC 8439 its docstring names, and the vectors of its sections 2.8.2 and A.5
that `vectors.py` parses out of that text judge this file before it judges the
crate.

The arithmetic of Poly1305 runs on the integers of Python, which have no
bound, so the reference carries no limb of its own. The comparison of a tag
is not in constant time, which a reference that runs on the development
machine against its own vectors needs no more than a clear rule.
"""

MASK32 = 0xFFFFFFFF
CONSTANTS = (0x61707865, 0x3320646E, 0x79622D32, 0x6B206574)
"""The four words "expand 32-byte k" of RFC 8439 section 2.3."""

P1305 = (1 << 130) - 5
"""The prime 2^130 - 5 of Poly1305, RFC 8439 section 2.5."""


def _rotl(value: int, bits: int) -> int:
    """Return `value` rotated left by `bits` within 32 bits."""
    return ((value << bits) | (value >> (32 - bits))) & MASK32


def quarter_round(state: list, a: int, b: int, c: int, d: int) -> None:
    """Run the quarter round of RFC 8439 section 2.1 on four words of `state`."""
    state[a] = (state[a] + state[b]) & MASK32
    state[d] = _rotl(state[d] ^ state[a], 16)
    state[c] = (state[c] + state[d]) & MASK32
    state[b] = _rotl(state[b] ^ state[c], 12)
    state[a] = (state[a] + state[b]) & MASK32
    state[d] = _rotl(state[d] ^ state[a], 8)
    state[c] = (state[c] + state[d]) & MASK32
    state[b] = _rotl(state[b] ^ state[c], 7)


def block(key: bytes, counter: int, nonce: bytes) -> bytes:
    """Return the 64 bytes of the ChaCha20 block function, RFC 8439 section 2.3.

    The state is the four constants, the eight words of the key, the block
    counter and the three words of the nonce, all little endian, and the
    block is the state after 20 rounds, ten column and diagonal pairs, added
    to the state it started from.
    """
    if len(key) != 32 or len(nonce) != 12:
        raise ValueError("a key of 32 bytes and a nonce of 12")
    start = list(CONSTANTS)
    start += [int.from_bytes(key[i:i + 4], "little") for i in range(0, 32, 4)]
    start.append(counter & MASK32)
    start += [int.from_bytes(nonce[i:i + 4], "little") for i in range(0, 12, 4)]
    state = list(start)
    for _ in range(10):
        quarter_round(state, 0, 4, 8, 12)
        quarter_round(state, 1, 5, 9, 13)
        quarter_round(state, 2, 6, 10, 14)
        quarter_round(state, 3, 7, 11, 15)
        quarter_round(state, 0, 5, 10, 15)
        quarter_round(state, 1, 6, 11, 12)
        quarter_round(state, 2, 7, 8, 13)
        quarter_round(state, 3, 4, 9, 14)
    return b"".join(((s + t) & MASK32).to_bytes(4, "little") for s, t in zip(state, start))


def chacha20(key: bytes, counter: int, nonce: bytes, data: bytes) -> bytes:
    """Return `data` XORed with the key stream from block `counter`, RFC 8439 section 2.4."""
    out = bytearray()
    for index in range(0, len(data), 64):
        stream = block(key, counter + index // 64, nonce)
        chunk = data[index:index + 64]
        out += bytes(x ^ y for x, y in zip(chunk, stream))
    return bytes(out)


def poly1305(key: bytes, message: bytes) -> bytes:
    """Return the tag of `message` under the one time `key`, RFC 8439 section 2.5.

    The first half of the key is r, clamped, and the second half s. Each block
    of 16 bytes, the last one shorter, is read little endian with a byte of 1
    above it, added to the accumulator and multiplied by r modulo 2^130 - 5,
    and the tag is the accumulator plus s, its low 128 bits little endian.
    """
    if len(key) != 32:
        raise ValueError("a one time key of 32 bytes")
    r = int.from_bytes(key[:16], "little") & 0x0FFFFFFC0FFFFFFC0FFFFFFC0FFFFFFF
    s = int.from_bytes(key[16:], "little")
    accumulator = 0
    for index in range(0, len(message), 16):
        chunk = message[index:index + 16] + b"\x01"
        accumulator = ((accumulator + int.from_bytes(chunk, "little")) * r) % P1305
    return ((accumulator + s) & ((1 << 128) - 1)).to_bytes(16, "little")


def poly1305_key(key: bytes, nonce: bytes) -> bytes:
    """Return the one time key of Poly1305, RFC 8439 section 2.6: block 0, its first 32 bytes."""
    return block(key, 0, nonce)[:32]


def _pad16(data: bytes) -> bytes:
    """Return the zero bytes that pad `data` to a multiple of 16."""
    return b"\x00" * (-len(data) % 16)


def _mac_data(aad: bytes, ciphertext: bytes) -> bytes:
    """Return what Poly1305 authenticates, RFC 8439 section 2.8.

    The associated data padded to 16 bytes, the ciphertext padded the same
    way, and the lengths of both in eight bytes each, little endian.
    """
    return (
        aad
        + _pad16(aad)
        + ciphertext
        + _pad16(ciphertext)
        + len(aad).to_bytes(8, "little")
        + len(ciphertext).to_bytes(8, "little")
    )


def seal(key: bytes, nonce: bytes, aad: bytes, plaintext: bytes) -> tuple:
    """Return the ciphertext and the tag of AEAD_CHACHA20_POLY1305, RFC 8439 section 2.8."""
    one_time = poly1305_key(key, nonce)
    ciphertext = chacha20(key, 1, nonce, plaintext)
    return ciphertext, poly1305(one_time, _mac_data(aad, ciphertext))


def open_(key: bytes, nonce: bytes, aad: bytes, ciphertext: bytes, tag: bytes):
    """Return the plaintext of a sealed message, or None where the tag does not verify."""
    one_time = poly1305_key(key, nonce)
    if poly1305(one_time, _mac_data(aad, ciphertext)) != tag:
        return None
    return chacha20(key, 1, nonce, ciphertext)
