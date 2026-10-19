# SPDX-License-Identifier: MIT
"""Aosabi harness: the raw HTTP/1.1 client every served notebook sends through.

A served comparison needs the bytes a server wrote, the header lines as they
arrived, their spelling and their order, and the framing, so this client writes
a request's bytes as they are over a socket and reads the answer by its framing,
with no HTTP library between the wire and the notebook. `requests` stays for
readiness probes alone. The client speaks over HTTP(TCP), given a port, and over
HTTP(Unix Socket), given the path of the socket, the same way.

It does four more things the wire family H of tests/gen/wire.py asks: it sends a
case in the writes and pauses the case names, it reads an answer whose
connection closes without a head, it reads two or three answers from one socket,
and it prints the raw head of both sides line by line with the differences in
the spelling of a name, in the order of the names and in the framing marked.

An answer's body is kept as it came on the wire, its chunks joined into the
payload, and the payload decompressed from `gzip` or `deflate` with the standard
`zlib`, because compressed bytes are compared after decompression. No Python
package enters tests/pyproject.toml for it.
"""

from __future__ import annotations

import socket
import threading
import time
import zlib
from dataclasses import dataclass, field

from .masks import apply as apply_masks


@dataclass(frozen=True)
class Endpoint:
    """Where a server listens: a TCP port on the loopback address or a Unix socket path."""

    port: int | None = None
    path: str | None = None

    def connect(self, timeout: float = 10.0) -> socket.socket:
        """A connected socket with blocking reads and no delay on writes."""
        if self.path is not None:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.settimeout(timeout)
            sock.connect(self.path)
        else:
            if not self.port:
                raise ValueError("the port is explicit and never 0")
            sock = socket.create_connection(("127.0.0.1", self.port), timeout=timeout)
            sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        sock.settimeout(None)
        return sock


def tcp(port: int) -> Endpoint:
    """The endpoint of HTTP(TCP) on the loopback address."""
    return Endpoint(port=port)


def unix(path) -> Endpoint:
    """The endpoint of HTTP(Unix Socket) at `path`."""
    return Endpoint(path=str(path))


@dataclass
class Answer:
    """One answer as it arrived: its head lines raw, its framing and its body."""

    status: int
    status_line: str
    header_lines: list
    framing: str
    wire_body: bytes
    payload: bytes
    decoded: bytes
    raw: bytes
    interim: list = field(default_factory=list)

    def header(self, name: str):
        """The first value of the field `name`, compared case folded, or `None`."""
        for key, value in self.header_lines:
            if key.lower() == name.lower():
                return value
        return None


def dechunk(data: bytes) -> tuple[bytes, int]:
    """The payload of a chunked body and the offset after its last chunk and trailers, or -1."""
    out = bytearray()
    at = 0
    while True:
        line = data.find(b"\r\n", at)
        if line < 0:
            return bytes(out), -1
        try:
            size = int(data[at:line].split(b";")[0], 16)
        except ValueError:
            return bytes(out), -1
        if size == 0:
            if data[line:line + 4] == b"\r\n\r\n":
                return bytes(out), line + 4
            trailer = data.find(b"\r\n\r\n", line)
            return bytes(out), (-1 if trailer < 0 else trailer + 4)
        if len(data) < line + 2 + size + 2:
            return bytes(out), -1
        out += data[line + 2:line + 2 + size]
        at = line + 2 + size + 2


def decode(payload: bytes, encoding) -> bytes:
    """The payload with its `gzip` or `deflate` coding undone by the standard zlib."""
    coding = (encoding or "").strip().lower()
    if coding in ("gzip", "x-gzip"):
        return zlib.decompress(payload, 16 + zlib.MAX_WBITS)
    if coding == "deflate":
        try:
            return zlib.decompress(payload)
        except zlib.error:
            return zlib.decompress(payload, -zlib.MAX_WBITS)
    return payload


def parse_answer(data: bytes, method: str = "GET", closed: bool = False):
    """The first whole answer at the start of `data` and the offset after it, or `(None, -1)`.

    A 1xx head is kept among the interim heads of the answer it precedes. A
    `HEAD` answer and a 204 or 304 carry no body, a length frames the body,
    chunked framing ends at the last chunk and its trailers, and an answer with
    neither ends at the close, so it is whole only when `closed` says the
    connection has ended.
    """
    offset = 0
    interim = []
    while True:
        end = data.find(b"\r\n\r\n", offset)
        if end < 0:
            return None, -1
        head = data[offset:end]
        lines = head.decode("latin-1").split("\r\n")
        status_line = lines[0]
        parts = status_line.split(" ", 2)
        status = int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else 0
        if 100 <= status < 200:
            interim.append(status_line)
            offset = end + 4
            continue
        break
    header_lines = []
    for line in lines[1:]:
        name, _, value = line.partition(":")
        header_lines.append((name, value.strip(" \t")))
    lower = {name.lower(): value for name, value in header_lines}
    start = end + 4
    if method == "HEAD" or status in (204, 304):
        framing, wire, payload, after = "none", b"", b"", start
    elif "content-length" in lower and lower["content-length"].isdigit():
        total = start + int(lower["content-length"])
        if len(data) < total:
            return None, -1
        framing, wire, after = "content-length", data[start:total], total
        payload = wire
    elif "chunked" in lower.get("transfer-encoding", "").lower():
        payload, finish = dechunk(data[start:])
        if finish < 0:
            return None, -1
        framing, wire, after = "chunked", data[start:start + finish], start + finish
    else:
        if not closed:
            return None, -1
        framing, wire, after = "close", data[start:], len(data)
        payload = wire
    try:
        decoded = decode(payload, lower.get("content-encoding"))
    except zlib.error:
        decoded = payload
    answer = Answer(status, status_line, header_lines, framing, wire, payload, decoded, data[:after], interim)
    return answer, after


def parse_answers(data: bytes, methods: list, closed: bool = False) -> list:
    """Every whole answer at the start of `data`, one per method given, in order."""
    answers = []
    offset = 0
    for method in methods:
        answer, after = parse_answer(data[offset:], method, closed)
        if answer is None:
            break
        answers.append(answer)
        offset += after
    return answers


class Conversation:
    """One connection read by a thread of its own, so writes and reads interleave.

    The reader appends every byte and then sets the way the connection ended,
    both under one lock, and `snapshot` takes both under it, so a reader of
    the conversation that sees the end also sees every byte that came before
    it.
    """

    def __init__(self, endpoint: Endpoint):
        self.sock = endpoint.connect()
        self.data = bytearray()
        self.closed = None
        self.last = time.monotonic()
        self._lock = threading.Lock()
        self._reader = threading.Thread(target=self._read, daemon=True)
        self._reader.start()

    def _read(self):
        while True:
            end = None
            try:
                chunk = self.sock.recv(65536)
            except ConnectionResetError:
                end = "reset"
            except OSError:
                end = "error"
            else:
                if not chunk:
                    end = "fin"
            with self._lock:
                if end is not None:
                    self.closed = end
                    return
                self.data += chunk
                self.last = time.monotonic()

    def write(self, data: bytes) -> bool:
        """Writes `data`, and reports whether the socket took it."""
        try:
            self.sock.sendall(data)
        except OSError:
            return False
        self.last = time.monotonic()
        return True

    def shut(self) -> None:
        """Shuts the write half, as a client that is done sending does."""
        try:
            self.sock.shutdown(socket.SHUT_WR)
        except OSError:
            pass

    def received(self) -> bytes:
        """Every byte received so far."""
        with self._lock:
            return bytes(self.data)

    def snapshot(self) -> tuple[bytes, str | None]:
        """Every byte received so far and the way the connection ended, or `None`, read together."""
        with self._lock:
            return bytes(self.data), self.closed

    def wait_answers(self, methods: list, timeout: float = 5.0) -> list:
        """Waits until one whole answer per method arrived or the connection closed.

        Each poll reads the bytes and the end together, so an answer whose
        last bytes came just before the close is whole when the close is seen.
        """
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            data, closed = self.snapshot()
            answers = parse_answers(data, methods, closed is not None)
            if len(answers) >= len(methods) or closed is not None:
                return answers
            time.sleep(0.005)
        data, closed = self.snapshot()
        return parse_answers(data, methods, closed is not None)

    def settle(self, idle: float = 0.4, cap: float = 15.0) -> None:
        """Waits until the connection closed or no byte came for `idle` seconds."""
        started = time.monotonic()
        while time.monotonic() - started < cap:
            if self.closed is not None or time.monotonic() - self.last >= idle:
                return
            time.sleep(0.02)

    def close(self) -> None:
        """Shuts the socket down, waits up to 5 seconds for the reader to end and then closes the socket.

        The shutdown ends the reader's `recv` before the descriptor is freed,
        so the next conversation, which may take the same descriptor number,
        never hands its answer to a reader of this one.
        """
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self._reader.join(5)
        try:
            self.sock.close()
        except OSError:
            pass


def send(endpoint: Endpoint, raw: bytes, method: str = "GET", timeout: float = 5.0):
    """Writes one request and reads one whole answer, then closes the connection.

    It returns the answer, or `None` when the connection closed without a head,
    and the way the connection ended while it was read: `fin`, `reset`,
    `error` or `None` for a connection still open.
    """
    talk = Conversation(endpoint)
    talk.write(raw)
    talk.wait_answers([method], timeout)
    data, closed = talk.snapshot()
    talk.close()
    answers = parse_answers(data, [method], closed is not None)
    return (answers[0] if answers else None), closed


def exchange(endpoint: Endpoint, raw: bytes, method: str = "GET", timeout: float = 5.0) -> tuple[bytes, str | None]:
    """Writes one request and returns every byte read until one whole answer arrived.

    The read ends when one whole answer arrived and 50 ms more passed, when the
    connection closed or after `timeout` seconds, and the connection is closed
    then. The bytes are those the capture rows C45, C47 and C50 recorded, the
    bytes after the answer within the 50 ms among them.
    """
    talk = Conversation(endpoint)
    talk.write(raw)
    talk.wait_answers([method], timeout)
    time.sleep(0.05)
    data, closed = talk.snapshot()
    talk.close()
    return data, closed


FOLLOW_UP = b"GET /graphql?query=%7B__typename%7D HTTP/1.1\r\nHost: h\r\nConnection: close\r\n\r\n"


@dataclass
class WireResult:
    """What one case of family H left: the bytes before the follow up and the follow up's."""

    raw: bytes
    closed: str | None
    followup: bytes | None
    followup_closed: str | None


def send_case(endpoint: Endpoint, case, followup: bytes = FOLLOW_UP, idle: float = 0.4) -> WireResult:
    """Sends one case of family H in its writes and pauses, then a follow up request.

    A write whose wait is `answer` waits until the connection has delivered one
    whole answer per earlier write. After the last write the client shuts its
    write half when the case says so, and waits until the connection closed or
    stayed quiet for `idle` seconds. The follow up is written only to a
    connection that is still open, and is read to the close its
    `Connection: close` asks. This is the procedure capture row C48 recorded the
    reference with.
    """
    talk = Conversation(endpoint)
    counts = any(write.wait == "answer" for write in case.writes)
    methods = []
    for write in case.writes:
        if write.wait == "answer" and write.amount:
            talk.wait_answers(methods, 5.0)
        elif write.amount:
            time.sleep(write.amount / 1000)
        if talk.closed is not None or not talk.write(write.data):
            break
        if counts:
            methods.append(write.data.split(b" ", 1)[0].decode("latin-1"))
    if case.shut:
        talk.shut()
    time.sleep(0.2)
    talk.settle(idle=idle)
    raw, closed = talk.snapshot()
    follow = None
    follow_closed = None
    if closed is None and not case.shut and followup:
        talk.write(followup)
        talk.settle(idle=1.0, cap=10.0)
        data, follow_closed = talk.snapshot()
        follow = data[len(raw):]
    talk.close()
    return WireResult(raw, closed, follow, follow_closed)


def head_lines(raw: bytes, masks=("date",)) -> list:
    """The lines of the first head of `raw` as received, after the masks named."""
    end = raw.find(b"\r\n\r\n")
    head = raw if end < 0 else raw[:end + 2]
    masked = apply_masks(head, masks)
    return [line.decode("latin-1") for line in masked.split(b"\r\n") if line]


def framing_of(lines: list) -> str:
    """The framing a head names: `content-length`, `chunked` or `none`."""
    names = {line.split(":", 1)[0].lower(): line for line in lines[1:]}
    if "transfer-encoding" in names and "chunked" in names["transfer-encoding"].lower():
        return "chunked"
    if "content-length" in names:
        return "content-length"
    return "none"


def head_report(reference: bytes, twin: bytes, masks=("date",)) -> dict:
    """The raw heads of both sides line by line, with each kind of difference marked.

    The kinds are the spelling of a name that both heads carry, the order of the
    names both carry, a name one head carries and the other does not, and the
    framing. Each line of the report is the reference's line and the twin's
    beside it with a mark: `=` for equal bytes, `s` for another spelling of the
    same name, `o` for a line out of order, `+` and `-` for a line one side
    lacks, and `v` for another value.
    """
    left = head_lines(reference, masks)
    right = head_lines(twin, masks)
    left_names = [line.split(":", 1)[0] for line in left[1:]]
    right_names = [line.split(":", 1)[0] for line in right[1:]]
    folded_left = [name.lower() for name in left_names]
    folded_right = [name.lower() for name in right_names]
    common = [name for name in folded_left if name in folded_right]
    common_right = [name for name in folded_right if name in folded_left]
    counts = {"spelling": 0, "order": 0, "presence": 0, "value": 0, "framing": 0}
    lines = []
    status = "=" if left[:1] == right[:1] else "v"
    if status != "=":
        counts["value"] += 1
    lines.append("%s %s | %s" % (status, left[0] if left else "", right[0] if right else ""))
    for index, name in enumerate(folded_left):
        line = left[index + 1]
        if name not in folded_right:
            counts["presence"] += 1
            lines.append("- %s |" % line)
            continue
        other = right[1 + folded_right.index(name)]
        mark = "="
        if line.split(":", 1)[0] != other.split(":", 1)[0]:
            mark = "s"
            counts["spelling"] += 1
        if line.split(":", 1)[1:] != other.split(":", 1)[1:]:
            mark = "v"
            counts["value"] += 1
        if common.index(name) != common_right.index(name):
            mark = "o" if mark == "=" else mark
            counts["order"] += 1
        lines.append("%s %s | %s" % (mark, line, other))
    for index, name in enumerate(folded_right):
        if name not in folded_left:
            counts["presence"] += 1
            lines.append("+ | %s" % right[index + 1])
    left_framing = framing_of(left)
    right_framing = framing_of(right)
    if left_framing != right_framing:
        counts["framing"] += 1
        lines.append("f framing %s | framing %s" % (left_framing, right_framing))
    return {"lines": lines, "counts": counts}
