# SPDX-License-Identifier: MIT
"""The LoRa frame, the time on air, the replay window and the duty cycle.

The reference of `tests/notebooks/06_lora_frame.ipynb` and
`tests/notebooks/29_carrier_lora.ipynb` for `aosabi-carrier-lora`, sharing no
code with it, written from the Homa design note, second version, sections 6.2
to 6.7, with the formula of the time on air of the SX1261/2 datasheet,
revision 1.2, section 6.1.4, which that note reads, and the AEAD of
`chacha.py`.

The frame is a byte of the link, the sender's counter in four bytes big
endian, the compact Homa packet sealed with ChaCha20-Poly1305 and the tag of
16 bytes over the header and the sealed bytes. The nonce is the link, the byte
of the direction, 0 toward the device and 1 toward the client, six zero bytes
and the counter big endian. A receiver checks a frame in the order `Short`,
`TooLong`, `Link`, `Counter`, `Tag`, and then its window of 64 counters,
`Replay`, as `aosabi-carrier-lora` documents it.

The duty cycle is the on air time of one transmitter over any continuous hour,
so `hour_air` counts the part of each transmission inside the hour, and a
transmission is beyond the duty cycle where the air of its radio in the hour
that ends with it passes the share of its band.
"""

import bisect

from . import chacha

HEADER_LEN = 5
TAG_LEN = 16
OVERHEAD = HEADER_LEN + TAG_LEN
MAX_PAYLOAD = 255
MAX_SEALED = MAX_PAYLOAD - OVERHEAD
WINDOW = 64
SPENT = 0xFFFFFFFF
"""The counter that marks the counters of a link as spent."""

TO_DEVICE = "to_device"
TO_CLIENT = "to_client"
DIRECTION_BYTE = {TO_DEVICE: 0, TO_CLIENT: 1}

HOUR_NS = 3_600_000_000_000
BURST_NS = 2_000_000_000
DUTY_PERMILLE = {"P": 100, "M": 10}
"""The duty cycle of the band P, 869.4 to 869.65 MHz, and of the band M, 868.0 to 868.6 MHz."""

LONGEST_TRANSMISSION_NS = {"P": 36_000_000_000, "M": 3_600_000_000}
"""The advisory longest single transmission of each band, the note's section 6.3."""


def time_on_air(sf: int, length: int, bandwidth_hz: int = 125_000, coding_rate: int = 1) -> int:
    """Return the time on air of a payload of `length` bytes in nanoseconds, rounded up.

    The SX1261/2 datasheet, section 6.1.4: a symbol lasts 2^SF / BW, and a
    packet takes the preamble of 8 symbols, 4.25 and 8 of the header, and
    ceil(max(8 PL + CRC - 4 SF + 8 + HEADER, 0) / (4 (SF - 2 LDRO))) (CR + 4)
    symbols, with CRC 16 and HEADER 20 in explicit mode and LDRO where a
    symbol lasts 16.38 ms or more. The count runs in quarter symbols, so the
    arithmetic stays whole.
    """
    sf = min(max(sf, 7), 12)
    coding_rate = min(max(coding_rate, 1), 4)
    symbol_scaled = (1 << sf) * 1_000_000_000
    ldro = 1 if symbol_scaled >= 16_380_000 * bandwidth_hz else 0
    bits = max(8 * length + 16 - 4 * sf + 8 + 20, 0)
    per_block = 4 * (sf - 2 * ldro)
    payload_symbols = -(-bits // per_block) * (coding_rate + 4)
    quarters = 4 * 8 + 17 + 32 + 4 * payload_symbols
    return -(-quarters * symbol_scaled // (4 * bandwidth_hz))


def max_frame(band: str, sf: int) -> int:
    """Return the longest frame: a whole payload in the band P, the LoRaWAN ceiling in M.

    The ceiling of the spreading factor in the band M is 255 bytes at SF7 and
    SF8, 128 at SF9 and 64 above.
    """
    if band == "P" or sf <= 8:
        return MAX_PAYLOAD
    return 128 if sf == 9 else 64


def longest_response(band: str, sf: int) -> int:
    """Return the longest response of a card's link, from the rules that fix it.
    """
    if band == "P" and sf <= 9:
        return 8192
    quarter = HOUR_NS * DUTY_PERMILLE[band] // 1000 // 4
    packet = max_frame(band, sf) - OVERHEAD
    per_packet = packet - 14
    full_air = time_on_air(sf, packet + OVERHEAD)
    full = quarter // full_air
    whole = full * per_packet
    if whole >= 2048:
        return 2048
    left = quarter - full * full_air
    rest = per_packet
    while rest > 0 and time_on_air(sf, rest + 14 + OVERHEAD) > left:
        rest -= 1
    return min(whole + rest, 2048)


def nonce(link: int, direction: str, counter: int) -> bytes:
    """Return the nonce of a frame: the link, the direction, six zero bytes and the counter."""
    return bytes([link, DIRECTION_BYTE[direction]]) + b"\x00" * 6 + counter.to_bytes(4, "big")


def seal(key: bytes, link: int, direction: str, counter: int, packet: bytes):
    """Return the frame of `packet`, or the name of the refusal."""
    if counter == SPENT:
        return "Counter"
    if len(packet) > MAX_SEALED:
        return "TooLong"
    header = bytes([link]) + counter.to_bytes(4, "big")
    sealed, tag = chacha.seal(key, nonce(link, direction, counter), header, packet)
    return header + sealed + tag


def open_(key: bytes, link: int, direction: str, frame: bytes):
    """Return the counter and the packet of `frame`, or the name of the refusal."""
    if len(frame) < OVERHEAD:
        return "Short"
    if len(frame) > MAX_PAYLOAD:
        return "TooLong"
    if frame[0] != link:
        return "Link"
    counter = int.from_bytes(frame[1:5], "big")
    if counter == SPENT:
        return "Counter"
    packet = chacha.open_(key, nonce(link, direction, counter), frame[:5], frame[5:-TAG_LEN], frame[-TAG_LEN:])
    if packet is None:
        return "Tag"
    return counter, packet


class Replay:
    """The highest counter taken and a window of the 64 counters up to it."""

    def __init__(self):
        self.highest = None
        self.seen = set()

    def accept(self, counter: int) -> bool:
        """Take `counter` and return True, or return False for a replay or a counter below the window."""
        if self.highest is not None and counter <= self.highest:
            if self.highest - counter >= WINDOW or counter in self.seen:
                return False
            self.seen.add(counter)
            return True
        self.highest = counter
        self.seen = {c for c in self.seen if counter - c < WINDOW}
        self.seen.add(counter)
        return True


def open_all(key: bytes, link: int, direction: str, frames: list) -> list:
    """Open `frames` in order through one window, as a receiver does."""
    window = Replay()
    results = []
    for frame in frames:
        opened = open_(key, link, direction, frame)
        if isinstance(opened, str):
            results.append({"error": opened})
        elif not window.accept(opened[0]):
            results.append({"error": "Replay"})
        else:
            results.append({"counter": opened[0], "hex": opened[1].hex()})
    return results


def boots(uses: list, block: int = 1024) -> list:
    """Return, for each boot, the first counter and the store's bound after it.

    A boot starts at the bound the store holds and reserves one block past
    it, and a use past the reserved block reserves the next one, the rule of
    the note's section 6.5.
    """
    stored = 0
    out = []
    for count in uses:
        first = stored
        reserved = first + block
        stored = reserved
        used = first + count
        while used > reserved:
            reserved += block
            stored = reserved
        out.append({"first": first, "count": count, "reserved": stored})
    return out


def hour_air(transmissions: list, start: int, window: int = HOUR_NS) -> int:
    """Return the air of `transmissions`, sorted by start, inside [start, start + window).

    Each transmission counts the part of it inside the window, so
    transmissions that overlap, such as the charges of a ledger's script,
    count each their own part.
    """
    end = start + window
    total = 0
    for at, air in transmissions:
        if at >= end:
            break
        total += max(0, min(at + air, end) - max(at, start))
    return total


class Hours:
    """The air of transmissions over any window, in O(log n) a query.

    The transmissions may overlap, as the charges of a ledger's script do, and
    each counts the part of it inside a window. The air before a time t is
    the sum over the transmissions that started before t of t less their
    start, less the sum over those that ended before t of t less their end,
    so two sorted lists with their prefix sums answer it, and the air of a
    window is the air before its end less the air before its start.
    """

    def __init__(self, transmissions: list):
        self.items = sorted(transmissions)
        self.starts = [at for at, _ in self.items]
        self.ends = sorted(at + air for at, air in self.items)
        self.start_sums = [0]
        for at in self.starts:
            self.start_sums.append(self.start_sums[-1] + at)
        self.end_sums = [0]
        for end in self.ends:
            self.end_sums.append(self.end_sums[-1] + end)

    def before(self, t: int) -> int:
        """Return the air of every transmission before the time `t`."""
        started = bisect.bisect_left(self.starts, t)
        ended = bisect.bisect_left(self.ends, t)
        return (started * t - self.start_sums[started]) - (ended * t - self.end_sums[ended])

    def air(self, start: int, end: int) -> int:
        """Return the air inside [start, end)."""
        return self.before(end) - self.before(start)

    def beyond(self, limit: int, window: int = HOUR_NS) -> list:
        """Return every transmission whose hour, the window that ends with it, holds more than `limit`."""
        out = []
        for at, air in self.items:
            end = at + air
            if self.air(end - window, end) > limit:
                out.append((at, air))
        return out

    def most(self, window: int = HOUR_NS) -> int:
        """Return the most air any window holds.

        The air of a window moving later grows while more transmissions run at
        its end than at its start, and turns from growing to shrinking only
        where its start meets a start or its end meets an end, so the windows
        that start at a transmission or end with one hold the most.
        """
        best = 0
        for at, air in self.items:
            best = max(best, self.air(at, at + window), self.air(at + air - window, at + air))
        return best
