# SPDX-License-Identifier: MIT
"""Aosabi harness: the byte comparison every notebook assertion goes through.

A golden and a live response are bytes, and the verdict of this project is a
byte comparison and nothing else. This module holds that comparison, the
exception it raises and the hexdump the exception carries, so that a failing
cell explains itself in the executed notebook without a second run.
"""

from __future__ import annotations

import json

BYTES_PER_LINE = 16
DEFAULT_WINDOW = 64

# The runner reads the failure out of the kernel as text, because the kernel is
# another process and the exception object never crosses it. The message
# therefore ends with one machine readable line behind this sentinel, and
# tests/run.py copies its fields into report.json.
SENTINEL = "BYTEDIFF-JSON "


def hexdump(data: bytes, offset: int, window: int = DEFAULT_WINDOW) -> str:
    """Render `window` bytes of `data` around `offset` as a hexdump.

    The window starts on a sixteen byte boundary so that the two sides of a
    comparison line up column by column, and the line holding `offset` carries
    a leading marker.
    """
    data = bytes(data)
    start = max(0, offset - window // 2)
    start -= start % BYTES_PER_LINE
    end = min(len(data), start + window)
    if start >= end:
        return "  %08x  (no bytes at this offset, length is %d)" % (start, len(data))

    lines = []
    for line_start in range(start, end, BYTES_PER_LINE):
        chunk = data[line_start : line_start + BYTES_PER_LINE]
        hex_part = " ".join("%02x" % b for b in chunk)
        hex_part = hex_part.ljust(BYTES_PER_LINE * 3 - 1)
        text_part = "".join(chr(b) if 32 <= b < 127 else "." for b in chunk)
        mark = ">" if line_start <= offset < line_start + BYTES_PER_LINE else " "
        lines.append("%s %08x  %s  |%s|" % (mark, line_start, hex_part, text_part))
    return "\n".join(lines)


class ByteDiff(AssertionError):
    """Two byte strings differ, with the first differing offset and both sides.

    It derives from AssertionError because a notebook assertion raises it and a
    failing cell is the failure of the notebook.
    """

    def __init__(
        self,
        label: str,
        offset: int,
        left: bytes,
        right: bytes,
        window: int = DEFAULT_WINDOW,
    ) -> None:
        self.label = label
        self.offset = offset
        self.window = window
        self.left_len = len(left)
        self.right_len = len(right)
        self.left_hexdump = hexdump(left, offset, window)
        self.right_hexdump = hexdump(right, offset, window)
        super().__init__(self.render())

    def as_dict(self) -> dict:
        """The failure as plain data, which is what report.json carries."""
        return {
            "label": self.label,
            "offset": self.offset,
            "left_len": self.left_len,
            "right_len": self.right_len,
            "left_hexdump": self.left_hexdump,
            "right_hexdump": self.right_hexdump,
        }

    def render(self) -> str:
        """The message a reader sees, with the machine readable line last."""
        head = "%s differs at offset %d" % (self.label, self.offset)
        if self.left_len != self.right_len:
            head += " and the lengths differ"
        return "\n".join(
            [
                head,
                "left length %d bytes, right length %d bytes"
                % (self.left_len, self.right_len),
                "left, the golden or the reference side",
                self.left_hexdump,
                "right, the side under test",
                self.right_hexdump,
                SENTINEL + json.dumps(self.as_dict(), sort_keys=True),
            ]
        )


def first_difference(left: bytes, right: bytes) -> int:
    """The offset of the first differing byte, or the shorter length."""
    shared = min(len(left), len(right))
    for i in range(shared):
        if left[i] != right[i]:
            return i
    return shared


def bytediff(a: bytes, b: bytes, label: str) -> int:
    """Compare two byte strings verbatim and return the length compared.

    Nothing is normalized here. Goldens are bytes, compared as bytes, with no
    trailing newline normalization and no reformatting of any kind, so every
    normalization a case needs happens in the caller and is visible in the
    notebook.
    """
    if isinstance(a, str) or isinstance(b, str):
        raise TypeError(
            "bytediff compares bytes, encode the text side before calling it"
        )
    left = bytes(a)
    right = bytes(b)
    if left == right:
        return len(left)
    raise ByteDiff(label, first_difference(left, right), left, right)
