# SPDX-License-Identifier: MIT
"""Aosabi harness: the reader of a server sent event stream and the case id of a served case.

A subscription answers `text/event-stream`, a body that never ends by its length
and whose frames arrive over time, so the raw client of `http.py`, which reads an
answer whole, cannot judge it. `StreamReader` writes one request over a
connection of its own, parses the head once it arrived, undoes the chunked
framing as the bytes come and splits the payload into frames on the blank line,
each with the milliseconds after the request at which its last byte arrived. The
arrival time is what the mask of a keepalive frame names and what the cadence
cases of `40_sse` read.

A stream whose payload is compressed cannot be split while it arrives, so its
frames are split once the body is whole, with the arrival time of the byte that
ended it. A server answers a stream with `Content-Encoding: none`, which keeps
hono's compression off, so a compressed stream is a failure the notebook reports,
and the reader keeps its bytes rather than losing them.

An answer that is no stream, such as the 406 of a subscription whose `Accept`
names no event stream, is read to the end of its framing and carries its body
and no frame.

The case id of a served case is the first twelve hex digits of the SHA-256 of its
canonical request, the object `request.json` of a golden holds, with the keys
sorted and no space in the separators. The capture script keeps its own copy,
because it runs as a script of its own without this package on its path.
"""

from __future__ import annotations

import hashlib
import json
import socket
import threading
import time
import zlib
from dataclasses import dataclass, field

from .http import Endpoint, decode

#: The first frame of every stream, the comment the processor enqueues in
#: `start`.
PRELUDE = ":\n\n"

#: The frame of a ping, the same comment the interval enqueues.
KEEPALIVE = ":\n\n"

#: The last frame of a finite stream.
TERMINATOR = "event: complete\ndata:\n\n"

#: The media type of a stream.
EVENT_STREAM = "text/event-stream"

#: The version header of the reference, data of the vendored reference.
REFERENCE_VERSION_HEADER = "x-pylon-version"

#: The header fields compared byte for byte, `connection` only for a stream.
#: `content-length` is compared only under `Accept-Encoding: identity`, which the
#: caller decides.
COMPARED_HEADERS = (
    "content-type",
    "content-encoding",
    "content-length",
    REFERENCE_VERSION_HEADER,
    "set-cookie",
    "vary",
    "cache-control",
    "allow",
    "accept",
    "access-control-allow-origin",
    "access-control-allow-credentials",
    "access-control-expose-headers",
    "location",
    "connection",
)


def case_id(request: dict) -> str:
    """The first twelve hex digits of the SHA-256 of the canonical request."""
    canonical = json.dumps(request, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf8")).hexdigest()[:12]


def request_bytes(request: dict, extra_headers=()) -> bytes:
    """The bytes of a canonical request, as the reader writes them.

    The request is `{"method", "path", "headers", "body"}` with `headers` a
    mapping in the order the fields are sent and `body` a text or absent.
    `Host: localhost` comes first, so the bytes do not depend on the port a side
    listens on, the fields of `extra_headers` follow the request's own, such as
    the `Accept-Encoding: identity` of the second send of a case, and a body
    carries its `Content-Length` last.
    """
    lines = ["%s %s HTTP/1.1" % (request["method"], request["path"]), "Host: localhost"]
    for name, value in (request.get("headers") or {}).items():
        lines.append("%s: %s" % (name, value))
    for name, value in extra_headers:
        lines.append("%s: %s" % (name, value))
    body = request.get("body")
    data = b"" if body is None else body.encode("utf-8")
    if body is not None:
        lines.append("Content-Length: %d" % len(data))
    return ("\r\n".join(lines) + "\r\n\r\n").encode("latin-1") + data


def split_frames(text: str) -> tuple[list[str], str]:
    """The whole frames of `text`, each with its blank line, and the rest."""
    frames = []
    rest = text
    while True:
        index = rest.find("\n\n")
        if index < 0:
            return frames, rest
        frames.append(rest[: index + 2])
        rest = rest[index + 2 :]


def is_keepalive(frame: str) -> bool:
    """Says whether a frame is the comment of the prelude or of a ping."""
    return frame == KEEPALIVE


def keepalives_after_prelude(frames: list[str]) -> int:
    """The number of pings after the first frame."""
    return sum(1 for frame in frames[1:] if is_keepalive(frame))


def data_frames(frames: list[str]) -> list[str]:
    """The frames after the prelude that are no ping, in order."""
    return [frame for frame in frames[1:] if not is_keepalive(frame)]


def keepalive_gaps(arrivals: list, with_prelude: bool = True) -> list[int]:
    """The milliseconds between two consecutive pings, the prelude first when asked."""
    times = [at for at, frame in arrivals if is_keepalive(frame)]
    if not with_prelude and arrivals and is_keepalive(arrivals[0][1]):
        times = times[1:]
    return [later - earlier for earlier, later in zip(times, times[1:])]


@dataclass
class Stream:
    """What one read of an answer left: the head, the frames with their arrivals and the body."""

    status: int | None
    status_line: str
    header_lines: list
    framing: str
    frames: list[str]
    arrivals: list
    body: bytes
    wire_body: bytes
    ended: str
    held_ms: int
    headers: dict = field(default_factory=dict)

    @property
    def is_stream(self) -> bool:
        """Says whether the answer names the media type of a stream."""
        return self.headers.get("content-type", "").startswith(EVENT_STREAM)

    @property
    def text(self) -> str:
        """The frames joined, the text a `stream.sse` golden holds."""
        return "".join(self.frames)

    @property
    def complete(self) -> bool:
        """Says whether the last frame is the terminator."""
        return bool(self.frames) and self.frames[-1] == TERMINATOR

    def compared_headers(self, identity: bool = False) -> dict:
        """The compared header fields in the order received, names folded to lower case.

        `connection` belongs to a stream alone, and `content-length` is compared
        only when the request named `Accept-Encoding: identity`.
        """
        out = {}
        for name, value in self.header_lines:
            folded = name.lower()
            if folded not in COMPARED_HEADERS or folded in out:
                continue
            if folded == "connection" and not self.is_stream:
                continue
            if folded == "content-length" and not identity:
                continue
            out[folded] = value
        return out


class StreamReader:
    """One request on a connection of its own, read by a thread as its bytes arrive."""

    def __init__(self, endpoint: Endpoint, raw: bytes, timeout: float = 10.0):
        self._sock = endpoint.connect(timeout)
        self._started = time.monotonic()
        self._lock = threading.Lock()
        self._data = bytearray()
        self._head_end = -1
        self._cursor = 0
        self._remaining = -1
        self._payload = bytearray()
        self._pending = ""
        self.status = None
        self.status_line = ""
        self.header_lines: list = []
        self.headers: dict = {}
        self.framing = "none"
        self.frames: list[str] = []
        self.arrivals: list = []
        self.body_done = False
        self.closed: str | None = None
        self._reader = threading.Thread(target=self._read, daemon=True)
        self._reader.start()
        try:
            self._sock.sendall(raw)
        except OSError:
            self.closed = "error"

    def _now(self) -> int:
        return round((time.monotonic() - self._started) * 1000)

    def _read(self) -> None:
        while True:
            try:
                chunk = self._sock.recv(65536)
            except ConnectionResetError:
                self._end("reset")
                return
            except OSError:
                self._end("error")
                return
            if not chunk:
                self._end("fin")
                return
            with self._lock:
                self._data += chunk
                self._parse(self._now())

    def _end(self, how: str) -> None:
        with self._lock:
            if self.closed is None:
                self.closed = how
            if self.framing == "close":
                self.body_done = True
            self._split(self._now(), final=True)

    def _parse(self, now: int) -> None:
        if self._head_end < 0 and not self._parse_head():
            return
        data = self._data
        if self.framing == "chunked":
            while not self.body_done:
                line = data.find(b"\r\n", self._cursor)
                if line < 0:
                    break
                try:
                    size = int(bytes(data[self._cursor:line]).split(b";")[0], 16)
                except ValueError:
                    break
                if size == 0:
                    if data.find(b"\r\n\r\n", line) >= 0:
                        self.body_done = True
                    break
                if len(data) < line + 2 + size + 2:
                    break
                self._payload += data[line + 2:line + 2 + size]
                self._cursor = line + 2 + size + 2
        elif self.framing == "content-length":
            take = min(self._remaining, len(data) - self._cursor)
            self._payload += data[self._cursor:self._cursor + take]
            self._cursor += take
            self._remaining -= take
            self.body_done = self._remaining == 0
        elif self.framing == "close":
            self._payload += data[self._cursor:]
            self._cursor = len(data)
        self._split(now, final=self.body_done)

    def _parse_head(self) -> bool:
        data = self._data
        offset = 0
        while True:
            end = data.find(b"\r\n\r\n", offset)
            if end < 0:
                return False
            lines = bytes(data[offset:end]).decode("latin-1").split("\r\n")
            parts = lines[0].split(" ", 2)
            status = int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else 0
            if 100 <= status < 200:
                offset = end + 4
                continue
            break
        self.status = status
        self.status_line = lines[0]
        self.header_lines = []
        for line in lines[1:]:
            name, _, value = line.partition(":")
            self.header_lines.append((name, value.strip(" \t")))
        self.headers = {name.lower(): value for name, value in self.header_lines}
        self._head_end = end + 4
        self._cursor = self._head_end
        length = self.headers.get("content-length", "")
        if "chunked" in self.headers.get("transfer-encoding", "").lower():
            self.framing = "chunked"
        elif length.isdigit():
            self.framing = "content-length"
            self._remaining = int(length)
        else:
            self.framing = "close"
        return True

    def _coded(self) -> bool:
        return self.headers.get("content-encoding", "").strip().lower() in ("gzip", "x-gzip", "deflate")

    def _split(self, now: int, final: bool) -> None:
        if not self.headers.get("content-type", "").startswith(EVENT_STREAM):
            return
        if self._coded():
            if not final or self.frames:
                return
            try:
                text = decode(bytes(self._payload), self.headers.get("content-encoding")).decode(
                    "utf-8", "surrogateescape"
                )
            except zlib.error:
                return
            fresh, self._pending = split_frames(text)
        else:
            consumed = sum(len(frame.encode("utf-8", "surrogateescape")) for frame in self.frames)
            text = bytes(self._payload[consumed:]).decode("utf-8", "surrogateescape")
            fresh, self._pending = split_frames(text)
        for frame in fresh:
            self.frames.append(frame)
            self.arrivals.append((now, frame))

    def _wait(self, ready, timeout: float) -> bool:
        deadline = time.monotonic() + timeout
        while True:
            with self._lock:
                if ready():
                    return True
                if self.closed is not None:
                    return bool(ready())
            if time.monotonic() >= deadline:
                return False
            time.sleep(0.002)

    def wait_head(self, timeout: float = 10.0) -> bool:
        """Waits until the head arrived, and says whether it did."""
        return self._wait(lambda: self._head_end >= 0, timeout)

    def wait_frames(self, count: int, timeout: float = 10.0) -> bool:
        """Waits until `count` frames arrived, the prelude and the pings among them."""
        return self._wait(lambda: len(self.frames) >= count, timeout)

    def wait_data(self, count: int, timeout: float = 10.0) -> bool:
        """Waits until `count` frames after the prelude that are no ping arrived."""
        return self._wait(lambda: len(data_frames(self.frames)) >= count, timeout)

    def wait_end(self, timeout: float = 10.0) -> bool:
        """Waits until the body ended by its framing or the connection closed."""
        return self._wait(lambda: self.body_done, timeout)

    def close(self) -> None:
        """Closes the connection from the client side, as a client that goes away does."""
        with self._lock:
            if self.closed is None:
                self.closed = "client"
        try:
            self._sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        try:
            self._sock.close()
        except OSError:
            pass

    def result(self) -> Stream:
        """A snapshot of what arrived so far."""
        with self._lock:
            if self.body_done and self.framing != "none":
                ended = "complete" if self.frames and self.frames[-1] == TERMINATOR else "body"
            else:
                ended = self.closed or "open"
            try:
                body = decode(bytes(self._payload), self.headers.get("content-encoding"))
            except zlib.error:
                body = bytes(self._payload)
            wire = bytes(self._data[self._head_end:]) if self._head_end >= 0 else b""
            return Stream(
                status=self.status,
                status_line=self.status_line,
                header_lines=list(self.header_lines),
                framing=self.framing,
                frames=list(self.frames),
                arrivals=list(self.arrivals),
                body=body,
                wire_body=wire,
                ended=ended,
                held_ms=self._now(),
                headers=dict(self.headers),
            )


def read_stream(endpoint: Endpoint, request: dict, extra_headers=(), timeout: float = 10.0,
                close_after: int | None = None) -> Stream:
    """Sends one canonical request and reads its answer to the end.

    A stream is read until its body ends, which follows the terminator, until
    the server closes it or until `timeout` seconds passed. With `close_after`
    the client closes the connection once that many frames after the prelude
    that are no ping arrived, which is the client disconnect of cases S29 and
    S30 of `40_sse`. An answer that is no stream is read to the end of its
    framing.
    """
    reader = StreamReader(endpoint, request_bytes(request, extra_headers), timeout)
    try:
        if reader.wait_head(timeout):
            if close_after is not None and reader.headers.get("content-type", "").startswith(EVENT_STREAM):
                reader.wait_data(close_after, timeout)
            else:
                reader.wait_end(timeout)
    finally:
        reader.close()
    return reader.result()


def compare_frames(reference: list[str], twin: list[str], keepalive_slack: int | None = None) -> list[str]:
    """The reasons two frame lists differ under the reading of the keepalive mask, or none.

    The prelude is asserted literally as the first frame of each side, and the
    terminator literally as the last frame of each side when the reference ends
    with it. With `keepalive_slack` of `None` neither side may carry a ping after
    the prelude, so nothing leaves a stream silently. With a number, the count of
    pings after the prelude may differ by that much, and the frames that are no
    ping are compared as bytes, which is the reading of the mask of the arrival
    time of a keepalive frame.
    """
    reasons = []
    for side, frames in (("reference", reference), ("twin", twin)):
        if not frames or frames[0] != PRELUDE:
            reasons.append("the first frame of the %s is no prelude" % side)
    if reference and reference[-1] == TERMINATOR and (not twin or twin[-1] != TERMINATOR):
        reasons.append("the twin does not end with the terminator")
    left = keepalives_after_prelude(reference)
    right = keepalives_after_prelude(twin)
    if keepalive_slack is None:
        if left or right:
            reasons.append("pings after the prelude, %d on the reference and %d on the twin" % (left, right))
        if reference != twin:
            reasons.append("the frames differ")
        return reasons
    if abs(left - right) > keepalive_slack:
        reasons.append("the ping counts %d and %d differ by more than %d" % (left, right, keepalive_slack))
    if data_frames(reference) != data_frames(twin):
        reasons.append("the frames that are no ping differ")
    return reasons


@dataclass
class ScenarioRun:
    """What one run of a scenario of `tests/gen/streams.py` left on one server.

    `streams` holds the stream of each subscriber as it was when its client
    closed, `sends` the answer of each request a step sent, in their order, and
    `missed` each wait of a step that timed out.
    """

    streams: dict
    sends: list
    missed: list


def run_scenario(endpoint: Endpoint, scenario, extra_headers=(), timeout: float = 10.0,
                 attach_wait_ms: int = 0) -> ScenarioRun:
    """Runs the steps of one scenario on the server of `endpoint` and returns what it left.

    A step `open` sends its request on a connection of its own, `prelude` waits
    for the first frame of its subscriber, `wait_ms` waits its milliseconds and
    at least `attach_wait_ms`, `send` sends its request on a new connection and
    reads its answer whole, `frames` waits until its subscriber holds its count
    of frames after the prelude that are no ping, and `close` closes the
    connection of its subscriber from the client side. Every request carries the
    fields `extra_headers` after its own, and every wait gives up after `timeout`
    seconds, which `missed` records.
    """
    readers = {}
    sends = []
    missed = []
    try:
        for step in scenario.steps:
            if "open" in step:
                readers[step["open"]] = StreamReader(endpoint, request_bytes(step["request"], extra_headers),
                                                     timeout)
            elif "prelude" in step:
                if not readers[step["prelude"]].wait_frames(1, timeout):
                    missed.append("no prelude on %s" % step["prelude"])
            elif "wait_ms" in step:
                time.sleep(max(step["wait_ms"], attach_wait_ms) / 1000)
            elif "send" in step:
                sends.append(read_stream(endpoint, step["send"], extra_headers, timeout=timeout))
            elif "frames" in step:
                if not readers[step["frames"]].wait_data(step["count"], timeout):
                    missed.append("%s holds fewer than %d frames" % (step["frames"], step["count"]))
            elif "close" in step:
                readers[step["close"]].close()
    finally:
        for reader in readers.values():
            reader.close()
    return ScenarioRun({name: reader.result() for name, reader in readers.items()}, sends, missed)
