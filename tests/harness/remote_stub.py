# SPDX-License-Identifier: MIT
"""`tests/notebooks/90_federation_transforms.ipynb` runs whole delegations with no
served pair: the sidecar `tests/sidecar/federation.mjs` and the devtool
primitive `aosabi-devtool federation` each execute a front document whose root
resolvers delegate through a federation, and the federation's URL is this stub.
The stub answers the introspection request of a case with the introspection JSON
the sidecar computed from the case's target SDL, the answer to the query the
federation sends, and the n-th data request of a case with the case's n-th
canned answer, and it records every request. So the answers never depend on the
bytes being compared, and the recorded requests of the two sides are what the
notebook compares.

Each case lives under its own path, `/<side>/<case>/graphql`, so a whole batch
of requests streams through one process of either side while the stub keeps
the count of data requests per side and case. The two sides therefore reach
the stub at two URLs, and the mask `federation-url` of `masks.py` closes the
difference in a message that names the URL.

The stub uses the standard library alone, binds the loopback on the port it
is given, never zero, and serves until `stop`.

    stub = RemoteStub(4950)
    with stub:
        url = stub.add_case("ts", "dg-0001", introspection, [answer])
        ...
        for one in stub.requests("ts", "dg-0001"):
            print(one.kind, one.body)
"""

from __future__ import annotations

import http.server
import json
import re
import threading
from dataclasses import dataclass, field

#: The characters a side and a case may carry, so each sits in a URL path as written.
NAME = re.compile(r"^[A-Za-z0-9._-]{1,128}$")

#: The content type of every answer of the stub.
CONTENT_TYPE = "application/json; charset=utf-8"


@dataclass(frozen=True)
class Canned:
    """One answer of the stub: a status, a body and its content type."""

    body: bytes
    status: int = 200
    content_type: str = CONTENT_TYPE


@dataclass
class Recorded:
    """One request the stub received, in the order it arrived, and what it answered."""

    index: int
    method: str
    target: str
    headers: list
    body: bytes
    kind: str
    status: int
    answer: bytes


@dataclass
class _Case:
    introspection: Canned
    answers: list
    requests: list = field(default_factory=list)
    served: int = 0


def canned(value) -> Canned:
    """An answer of the stub from bytes, a text, a JSON value or a `Canned`.

    A JSON value is written compact with its non ASCII text as UTF-8, the form a
    remote of the reference writes.
    """
    if isinstance(value, Canned):
        return value
    if isinstance(value, bytes):
        return Canned(value)
    if isinstance(value, str):
        return Canned(value.encode("utf-8"))
    return Canned(json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode("utf-8"))


def is_introspection(content_type: str, body: bytes) -> bool:
    """Whether a request is the introspection the federation sends first.

    That request is a JSON body whose query selects `__schema`. A multipart body
    is an upload and never an introspection.
    """
    if "multipart/" in content_type.lower():
        return False
    try:
        query = json.loads(body.decode("utf-8")).get("query")
    except (ValueError, AttributeError, UnicodeDecodeError):
        return False
    return isinstance(query, str) and "__schema" in query


def read_body(handler: http.server.BaseHTTPRequestHandler) -> bytes:
    """The body of a request, by its length or by its chunks."""
    length = handler.headers.get("Content-Length")
    if length is not None:
        return handler.rfile.read(int(length))
    if "chunked" not in (handler.headers.get("Transfer-Encoding") or "").lower():
        return b""
    chunks = []
    while True:
        size = int(handler.rfile.readline().split(b";", 1)[0].strip(), 16)
        if size == 0:
            while handler.rfile.readline() not in (b"\r\n", b"\n", b""):
                pass
            return b"".join(chunks)
        chunks.append(handler.rfile.read(size))
        handler.rfile.readline()


class _Handler(http.server.BaseHTTPRequestHandler):
    """Answers one request of the stub and records it."""

    protocol_version = "HTTP/1.1"
    server_version = "aosabi-remote-stub"
    sys_version = ""

    def log_message(self, format, *args):  # noqa: A002, the name of the base class
        """The stub writes no log line, its record is the log."""

    def do_POST(self):  # noqa: N802, the name the base class dispatches on
        self._answer()

    def do_GET(self):  # noqa: N802, the name the base class dispatches on
        self._answer()

    def _answer(self):
        stub: RemoteStub = self.server.stub
        body = read_body(self)
        status, answer, content_type = stub._serve(self.command, self.path, list(self.headers.items()), body)
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(answer)))
        self.end_headers()
        self.wfile.write(answer)


class RemoteStub:
    """The stub on one port, with the cases a run adds and what each received."""

    def __init__(self, port: int, host: str = "127.0.0.1") -> None:
        if not isinstance(port, int) or port <= 0:
            raise ValueError("the stub takes an explicit port, never 0")
        self.port = port
        self.host = host
        self._cases: dict[tuple[str, str], _Case] = {}
        self._unknown: list[Recorded] = []
        self._lock = threading.Lock()
        self._arrivals = 0
        self._server = None
        self._thread = None

    def url(self, side: str, case: str) -> str:
        """The URL a federation of `side` names for `case`."""
        return "http://%s:%d/%s/%s/graphql" % (self.host, self.port, side, case)

    def add_case(self, side: str, case: str, introspection, answers=()) -> str:
        """Adds a case for one side and answers its URL.

        `introspection` is the answer to the introspection request, and
        `answers` the answers to the data requests in the order they arrive.
        Adding a case again replaces it and forgets what it received.
        """
        for name in (side, case):
            if not NAME.match(name):
                raise ValueError("a side and a case are names of [A-Za-z0-9._-], %r is not" % name)
        with self._lock:
            self._cases[(side, case)] = _Case(canned(introspection), [canned(one) for one in answers])
        return self.url(side, case)

    def requests(self, side: str, case: str) -> list:
        """What the case received on one side, in arrival order."""
        with self._lock:
            return list(self._cases[(side, case)].requests)

    def unknown(self) -> list:
        """The requests to a path that names no case."""
        with self._lock:
            return list(self._unknown)

    def _serve(self, method: str, target: str, headers: list, body: bytes):
        """The status, the answer and its content type for one request, recorded."""
        content_type = next((value for name, value in headers if name.lower() == "content-type"), "")
        parts = target.split("?", 1)[0].strip("/").split("/")
        with self._lock:
            self._arrivals += 1
            index = self._arrivals
            entry = self._cases.get((parts[0], parts[1])) if len(parts) == 3 and parts[2] == "graphql" else None
            if entry is None:
                answer = canned({"errors": [{"message": "the remote stub has no case at %s" % target}]})
                self._unknown.append(Recorded(index, method, target, headers, body, "unknown", 404, answer.body))
                return 404, answer.body, answer.content_type
            if is_introspection(content_type, body):
                kind, answer = "introspection", entry.introspection
            else:
                kind = "data"
                if entry.served < len(entry.answers):
                    answer = entry.answers[entry.served]
                else:
                    answer = canned({"errors": [{"message": "the remote stub has no answer %d for this case" % (entry.served + 1)}]})
                entry.served += 1
            entry.requests.append(Recorded(index, method, target, headers, body, kind, answer.status, answer.body))
            return answer.status, answer.body, answer.content_type

    def start(self) -> "RemoteStub":
        """Binds the port and serves on a thread of this process."""
        server = http.server.ThreadingHTTPServer((self.host, self.port), _Handler)
        server.daemon_threads = True
        server.stub = self
        self._server = server
        self._thread = threading.Thread(target=server.serve_forever, name="remote-stub-%d" % self.port, daemon=True)
        self._thread.start()
        return self

    def stop(self) -> None:
        """Stops serving and frees the port."""
        if self._server is not None:
            self._server.shutdown()
            self._server.server_close()
            self._server = None
        if self._thread is not None:
            self._thread.join(timeout=5)
            self._thread = None

    def __enter__(self) -> "RemoteStub":
        return self.start()

    def __exit__(self, *exc) -> None:
        self.stop()
