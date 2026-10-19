# SPDX-License-Identifier: MIT
"""Aosabi harness: the recording proxy in front of one remote of a federation.

A served pair of `tests/notebooks/90_federation.ipynb` puts one proxy in front
of every remote on each side, and the front reaches its remote through it. The
proxy records every request with its method, its target, its headers in wire
order and its body, forwards it to the remote, records the remote's answer and
answers the front. The outbound request of a federation is what row C87 records
and what the notebook compares, so the proxy is the place both sides are seen
from.

Two delegations a front sends at once reach the proxy over two connections, and
either one can arrive first. The second of the two delegations of one document
arrives first in 6 of 21 runs of the reference answered in arrival order, so the
arrival order is no order to compare. The proxy gathers every request that
arrives within 40 ms of the first one of a group and orders the group by its
request bodies, ascending by default and descending on request. Answers leave
one at a time, a group in its order before any later group, at least 40 ms
apart. The order in which overlapping delegations settle at the front of the
reference is then a function of the bodies sent, and the same on the Aosabi side
when the bodies are equal. Forwarding runs at once on the thread of the request,
and only the answer waits for its turn, so the record of a group lists it in the
order of its answers.

A mode answers in place of the remote:

| Mode | What the proxy does |
| --- | --- |
| `forward` | forwards every request, the default |
| `empty` | answers a data request with an empty body, case gw1-18 |
| `not_json` | answers a data request with a body that is no JSON, case gw1-19 |
| `empty_result` | answers a data request with `{}`, case gw1-20 |
| `remote_error` | answers a data request with a null root field and a remote error with extensions, case gw1-21 |
| `nested_error` | answers a data request with a nested null and an error at its path, case gw1-22 |
| `fail_once` | refuses the first introspection and forwards every request after it, case gw4-12 and the retry of gw1-17 |
| `hostile_<n>` | answers a data request with the n-th of the fifteen malformed responses, cases gw4-13 to gw4-27 |
| `down` | binds nothing, so the port is closed, case gw1-17 |
| `redirect_<status>` |  |
| `nested_<n>` |  |
| `deep_error_<n>` |  |
| `intro_<kind>_<n>` |  |

The introspection a federation sends first is forwarded in every mode but
`fail_once`, `down`, `redirect_<status>` and `intro_<kind>_<n>`, so a mode
meets the delegation and not the schema. The crafted parts of
`intro_<kind>_<n>` are the kinds of `INTROSPECTION_KINDS`: `list`, a root
field `deep` of `String` wrapped in n lists, `inputs`, a root field `chain`
whose argument is the first of n input objects that each hold the next as a
non null field, `default`, a root field `defaulted` whose argument of
`[String]` has a default value nested in n lists, `objects`, a root field
`objected` whose argument of `String` has a default value nested in n input
objects, `mixed`, a root field `mixed` whose argument of `String` has a
default value of n pairs of a list and an input object, `wrapped`, a root
field `wrapped` whose argument of `String` has a default value of 1,000 lists
around n nested input objects, `escape`, a root field `escaped` whose
argument of `String` has a default value that opens with a string the lexer
of graphql-js refuses for its escape `\\q` and nests n lists after it,
`types`, n object types no field names, `dunder`, an object type named `__Mine`
that no field names, `location`, a directive with a location graphql-js 16.12.0
does not know, and `empty`, an object type with no field, a schema that builds
and that every delegation refuses.

The proxy uses `http.server` and `http.client` of the standard library and
nothing else, binds the loopback on its own explicit port, never zero, and
serves until `stop`.

    proxy = RecordingProxy(4902, upstream_port=4901)
    with proxy:
        ...
        for one in proxy.exchanges():
            print(one.kind, one.body, one.response_body)
"""

from __future__ import annotations

import http.client
import http.server
import json
import threading
import time
from dataclasses import dataclass

from .remote_stub import is_introspection, read_body

#: The time between two answers of the proxy, in seconds.
GAP_S = 0.040

#: The time a group gathers the requests that arrive after its first one, in seconds.
GATHER_S = 0.040

#: The orders of a gathered group, by the bytes of its request bodies.
ORDERS = ("ascending", "descending")

#: The connections the port queues before the proxy accepts them, the default
#: of `socketserver`. A caller that opens more connections at once passes a
#: larger one, since a full queue drops a connect until the client sends it
#: again a second later.
BACKLOG = 5

#: The length of the sixth hostile body, one byte over the reader's bound of 16 MiB.
BIG_BODY_BYTES = 16 * 1024 * 1024 + 1

#: The modes that answer in place of the remote, besides the modes that carry a number.
MODES = ("forward", "empty", "not_json", "empty_result", "remote_error", "nested_error", "fail_once", "down")

#: The statuses of `redirect_<status>`, the five the fetch of the reference follows.
REDIRECTS = (301, 302, 303, 307, 308)

#: The path a redirect of the proxy names before the path of the request.
REDIRECTED = "/redirected"

#: The crafted parts of `intro_<kind>_<n>`.
INTROSPECTION_KINDS = ("list", "inputs", "default", "objects", "mixed", "wrapped", "escape", "types", "dunder",
                       "location", "empty")

#: The lists `wrapped` opens around its input objects.
WRAPPING_LISTS = 1000

#: The deepest nesting of `nested_<n>` the reader of a federation admits, its
#: depth of 1,000 less the three objects around the title.
NESTED_MAX = 997

#: The headers that belong to one connection and never travel through the proxy.
HOP_BY_HOP = frozenset(
    ("connection", "keep-alive", "proxy-connection", "transfer-encoding", "te", "trailer", "upgrade", "content-length")
)


def _json(value) -> bytes:
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def _head(status_line: str, headers, body: bytes = b"", length: bool = True) -> bytes:
    lines = [status_line] + ["%s: %s" % pair for pair in headers]
    if length:
        lines.append("Content-Length: %d" % len(body))
    return ("\r\n".join(lines) + "\r\n\r\n").encode("latin-1") + body


def hostile_responses() -> list:
    """An entry of `None` closes the connection before a byte of the head. The
    body over 16 MiB is 16 MiB and 1 byte of valid JSON, so its length is the
    only thing wrong with it.
    """
    json_type = ("Content-Type", "application/json; charset=utf-8")
    prefix, suffix = b'{"data":{"x":"', b'"}}'
    big = prefix + b"a" * (BIG_BODY_BYTES - len(prefix) - len(suffix)) + suffix
    assert len(big) == BIG_BODY_BYTES, "The sixth hostile body is 16 MiB and 1 byte"
    deep = b'{"data":' + b"[" * 10000 + b"]" * 10000 + b"}"
    return [
        # 1, a truncated status line
        b"HTTP/1.1 20",
        # 2, a head over 64 KiB
        _head("HTTP/1.1 200 OK", [json_type, ("X-Padding", "a" * (64 * 1024 + 1))], b'{"data":{}}'),
        # 3, a negative chunk length
        _head("HTTP/1.1 200 OK", [json_type, ("Transfer-Encoding", "chunked")], length=False) + b"-5\r\nabcde\r\n0\r\n\r\n",
        # 4, an oversized chunk length
        _head("HTTP/1.1 200 OK", [json_type, ("Transfer-Encoding", "chunked")], length=False)
        + b"ffffffffffffffff\r\n{}",
        # 5, a body shorter than its length
        _head("HTTP/1.1 200 OK", [json_type, ("Content-Length", "100")], length=False) + b'{"data":',
        # 6, a body over 16 MiB
        _head("HTTP/1.1 200 OK", [json_type], big),
        # 7, a redirect, to a port of the loopback nothing listens on
        _head("HTTP/1.1 302 Found", [("Location", "http://127.0.0.1:1/graphql")]),
        # 8, a status 500
        _head("HTTP/1.1 500 Internal Server Error", [("Content-Type", "text/plain; charset=utf-8")], b"Internal Server Error"),
        # 9, invalid JSON
        _head("HTTP/1.1 200 OK", [json_type], b'{"data": {'),
        # 10, JSON nested beyond the reader's depth
        _head("HTTP/1.1 200 OK", [json_type], deep),
        # 11, data missing
        _head("HTTP/1.1 200 OK", [json_type], b'{"extensions":{}}'),
        # 12, errors present
        _head("HTTP/1.1 200 OK", [json_type], b'{"errors":[{"message":"The remote refused the request"}]}'),
        # 13, data of the wrong type
        _head("HTTP/1.1 200 OK", [json_type], b'{"data":"not an object"}'),
        # 14, an introspection without __schema
        _head("HTTP/1.1 200 OK", [json_type], b'{"data":{"__type":null}}'),
        # 15, a connection closed before the head
        None,
    ]


def _named(kind: str, name: str) -> dict:
    return {"kind": kind, "name": name, "ofType": None}


def _input_value(name: str, type_ref: dict, default=None) -> dict:
    return {"name": name, "description": None, "type": type_ref, "defaultValue": default, "isDeprecated": False,
            "deprecationReason": None}


def _field(name: str, type_ref: dict, args=()) -> dict:
    return {"name": name, "description": None, "args": list(args), "type": type_ref, "isDeprecated": False,
            "deprecationReason": None}


def _object(name: str, fields: list) -> dict:
    return {"kind": "OBJECT", "name": name, "description": None, "specifiedByURL": None, "isOneOf": None,
            "fields": fields, "inputFields": None, "interfaces": [], "enumValues": None, "possibleTypes": None}


def crafted_introspection(answer: dict, kind: str, size: int) -> dict:
    """The answer to the introspection with the crafted part `kind` of size `size` added.

    The parts are those the module documentation names under
    `intro_<kind>_<n>`. A root field the part adds is a field of the query
    type, which no delegation of the cases selects."""
    schema = answer["data"]["__schema"]
    types = schema["types"]
    query = next(one for one in types if one["name"] == schema["queryType"]["name"])
    string = _named("SCALAR", "String")
    if kind == "list":
        type_ref = string
        for _ in range(size):
            type_ref = {"kind": "LIST", "name": None, "ofType": type_ref}
        query["fields"].append(_field("deep", type_ref))
    elif kind == "inputs":
        query["fields"].append(_field("chain", string, [_input_value("a", _named("INPUT_OBJECT", "In0"))]))
        for index in range(size):
            field = (_input_value("next", {"kind": "NON_NULL", "name": None,
                                           "ofType": _named("INPUT_OBJECT", "In%d" % (index + 1))})
                     if index + 1 < size else _input_value("last", string))
            types.append({"kind": "INPUT_OBJECT", "name": "In%d" % index, "description": None,
                          "specifiedByURL": None, "isOneOf": False, "fields": None, "inputFields": [field],
                          "interfaces": None, "enumValues": None, "possibleTypes": None})
    elif kind == "default":
        listed = {"kind": "LIST", "name": None, "ofType": string}
        query["fields"].append(_field("defaulted", string, [_input_value("a", listed, "[" * size + "]" * size)]))
    elif kind == "objects":
        query["fields"].append(_field("objected", string, [_input_value("a", string, "{a:" * size + "1" + "}" * size)]))
    elif kind == "mixed":
        query["fields"].append(_field("mixed", string, [_input_value("a", string, "[{a:" * size + "1" + "}]" * size)]))
    elif kind == "wrapped":
        text = "[" * WRAPPING_LISTS + "{a:" * size + "1" + "}" * size + "]" * WRAPPING_LISTS
        query["fields"].append(_field("wrapped", string, [_input_value("a", string, text)]))
    elif kind == "escape":
        query["fields"].append(_field("escaped", string, [_input_value("a", string, '"\\q"' + "[" * size)]))
    elif kind == "types":
        types.extend(_object("Extra%d" % index, [_field("f", string)]) for index in range(size))
    elif kind == "dunder":
        types.append(_object("__Mine", [_field("f", string)]))
    elif kind == "location":
        schema["directives"].append({"name": "later", "description": None, "isRepeatable": False,
                                     "locations": ["FIELD", "FRAGMENT_VARIABLE_DEFINITION"], "args": []})
    elif kind == "empty":
        types.append(_object("Empty", []))
    return answer


@dataclass
class Exchange:
    """One request the proxy received and what it answered, in the order of the answers."""

    index: int
    method: str
    target: str
    headers: list
    body: bytes
    kind: str
    answered_by: str
    status: int | None = None
    response_headers: list | None = None
    response_body: bytes = b""
    arrived_at: float | None = None
    group: int | None = None
    ticket: int | None = None
    answered_at: float | None = None


@dataclass
class _Group:
    """The requests that arrived within the gather time of the first one."""

    number: int
    deadline: float
    members: list


class _Handler(http.server.BaseHTTPRequestHandler):
    """Takes one request, records it, gets its answer and waits for its turn."""

    protocol_version = "HTTP/1.1"
    server_version = "aosabi-recording-proxy"
    sys_version = ""

    def log_message(self, format, *args):  # noqa: A002, the name of the base class
        """The proxy writes no log line, its record is the log."""

    def do_POST(self):  # noqa: N802, the name the base class dispatches on
        self._exchange()

    def do_GET(self):  # noqa: N802, the name the base class dispatches on
        self._exchange()

    def _exchange(self):
        proxy: RecordingProxy = self.server.proxy
        headers = list(self.headers.items())
        body = read_body(self)
        exchange = proxy._arrive(self.command, self.path, headers, body)
        try:
            raw = proxy._respond(exchange)
        except Exception as error:  # noqa: BLE001, a failed answer still takes its turn
            raw = proxy._record(exchange, 502, [("Content-Type", "text/plain; charset=utf-8")],
                                ("the recording proxy failed: %s" % error).encode("utf-8"), "HTTP/1.1 502 Bad Gateway")
        ticket = proxy._ticket(exchange)
        proxy._wait_turn(ticket, exchange)
        try:
            if raw is None:
                self.close_connection = True
                return
            self.wfile.write(raw)
            self.wfile.flush()
            if exchange.answered_by.startswith("hostile_"):
                self.close_connection = True
        finally:
            proxy._done(ticket, exchange)


class RecordingProxy:
    """One proxy on its port in front of one remote, with its record."""

    def __init__(self, port: int, upstream_port: int, upstream_host: str = "127.0.0.1",
                 mode: str = "forward", host: str = "127.0.0.1", gap_s: float = GAP_S,
                 field: str = "user", nested: str = "orders", gather_s: float = GATHER_S,
                 order: str = "ascending", backlog: int = BACKLOG) -> None:
        if not isinstance(port, int) or port <= 0:
            raise ValueError("the proxy takes an explicit port, never 0")
        self.port = port
        self.host = host
        self.backlog = backlog
        self.upstream = (upstream_host, upstream_port)
        self.gap_s = gap_s
        self.gather_s = gather_s
        self.field = field
        self.nested = nested
        self._mode = self._checked(mode)
        self._order = self._checked_order(order)
        self._failed_once = False
        self._lock = threading.Condition()
        self._exchanges: list[Exchange] = []
        self._group: _Group | None = None
        self._groups = 0
        self._next_ticket = 0
        self._serving = 0
        self._last_answer = 0.0
        self._server = None
        self._started = False
        self._hostile = None

    @staticmethod
    def _checked(mode: str) -> str:
        if mode in MODES:
            return mode
        if mode.startswith("hostile_") and mode[len("hostile_"):].isdigit() and 1 <= int(mode[len("hostile_"):]) <= 15:
            return mode
        head, _, number = mode.rpartition("_")
        if number.isdigit():
            if head == "redirect" and int(number) in REDIRECTS:
                return mode
            if head in ("nested", "deep_error") and 1 <= int(number) <= NESTED_MAX:
                return mode
            if head.startswith("intro_") and head[len("intro_"):] in INTROSPECTION_KINDS:
                return mode
        raise ValueError("no proxy mode named %s" % mode)

    @staticmethod
    def _checked_order(order: str) -> str:
        if order in ORDERS:
            return order
        raise ValueError("no proxy order named %s" % order)

    @property
    def mode(self) -> str:
        return self._mode

    @property
    def order(self) -> str:
        return self._order

    def set_order(self, order: str) -> None:
        """Changes the order of the groups that close from now on."""
        order = self._checked_order(order)
        with self._lock:
            self._order = order

    def set_mode(self, mode: str) -> None:
        """Changes the mode, binding or closing the port where `down` begins or ends."""
        mode = self._checked(mode)
        was_down = self._mode == "down"
        with self._lock:
            self._mode = mode
            self._failed_once = False
        if mode == "down" and self._server is not None:
            self._close()
        elif was_down and mode != "down" and self._started:
            self._bind()

    def url(self, path: str = "/graphql") -> str:
        """The URL a front names to reach the remote through this proxy."""
        return "http://%s:%d%s" % (self.host, self.port, path)

    def exchanges(self) -> list:
        """What the proxy received and answered, a closed group in the order of its answers."""
        with self._lock:
            return list(self._exchanges)

    def clear(self) -> None:
        """Forgets the record, so a case starts from an empty one."""
        with self._lock:
            self._exchanges = []

    def _arrive(self, method, target, headers, body):
        content_type = next((value for name, value in headers if name.lower() == "content-type"), "")
        kind = "introspection" if is_introspection(content_type, body) else "data"
        now = time.monotonic()
        with self._lock:
            self._close_due(now)
            if self._group is None:
                self._group = _Group(self._groups, now + self.gather_s, [])
                self._groups += 1
            mode = self._mode
            if mode.startswith("redirect_"):
                answered_by = "remote" if target.startswith(REDIRECTED) else mode
            elif kind == "introspection":
                answered_by = "remote"
                if mode == "fail_once" and not self._failed_once:
                    self._failed_once = True
                    answered_by = "fail_once"
                elif mode.startswith("intro_"):
                    answered_by = mode
            else:
                answered_by = "remote" if mode in ("forward", "fail_once") or mode.startswith("intro_") else mode
            exchange = Exchange(len(self._exchanges) + 1, method, target, headers, body, kind, answered_by,
                                arrived_at=now, group=self._group.number)
            self._group.members.append(exchange)
            self._exchanges.append(exchange)
        return exchange

    def _close_due(self, now: float) -> None:
        """Orders the open group once its gather time passed, under the lock.

        The members take consecutive tickets in the order of their bodies, and
        the record lists them in that order in the places they took on arrival.
        """
        group = self._group
        if group is None or now < group.deadline:
            return
        self._group = None
        ordered = sorted(group.members, key=lambda one: one.body, reverse=self._order == "descending")
        for one in ordered:
            one.ticket = self._next_ticket
            self._next_ticket += 1
        members = {id(one) for one in group.members}
        places = [place for place, one in enumerate(self._exchanges) if id(one) in members]
        kept = {id(self._exchanges[place]) for place in places}
        for place, one in zip(places, [one for one in ordered if id(one) in kept]):
            self._exchanges[place] = one
            one.index = place + 1
        self._lock.notify_all()

    def _ticket(self, exchange: Exchange) -> int:
        """Waits until the group of one exchange is ordered and answers the exchange's ticket."""
        with self._lock:
            while exchange.ticket is None:
                now = time.monotonic()
                self._close_due(now)
                if exchange.ticket is None:
                    self._lock.wait(self._group.deadline - now)
            return exchange.ticket

    def _respond(self, exchange: Exchange) -> bytes | None:
        """The raw answer of one exchange, forwarded or written by the mode."""
        mode = exchange.answered_by
        json_type = [("Content-Type", "application/json; charset=utf-8")]
        if mode == "remote":
            return self._forward(exchange)
        if mode == "fail_once":
            body = _json({"errors": [{"message": "The remote is not ready"}]})
            return self._record(exchange, 503, json_type, body, "HTTP/1.1 503 Service Unavailable")
        if mode == "empty":
            return self._record(exchange, 200, json_type, b"")
        if mode == "not_json":
            return self._record(exchange, 200, [("Content-Type", "text/plain; charset=utf-8")], b"not json")
        if mode == "empty_result":
            return self._record(exchange, 200, json_type, b"{}")
        if mode == "remote_error":
            body = _json({"data": {self.field: None}, "errors": [
                {"message": "Session not found", "path": [self.field], "extensions": {"code": "UNAUTHENTICATED"}}]})
            return self._record(exchange, 200, json_type, body)
        if mode == "nested_error":
            body = _json({"data": {self.field: {"__typename": "User", "id": "u1", self.nested: None}}, "errors": [
                {"message": "The rows are not available", "path": [self.field, self.nested],
                 "extensions": {"code": "UNAVAILABLE"}}]})
            return self._record(exchange, 200, json_type, body)
        if mode.startswith("redirect_"):
            status = int(mode[len("redirect_"):])
            reason = http.HTTPStatus(status).phrase
            location = REDIRECTED + exchange.target
            return self._record(exchange, status, [("Location", location)], b"", "HTTP/1.1 %d %s" % (status, reason))
        if mode.startswith(("nested_", "deep_error_")):
            depth = int(mode.rpartition("_")[2])
            title = b"[" * depth + b"]" * depth
            body = (b'{"data":{"product":{"__typename":"Product","id":"p1","title":' + title
                    + b',"status":"ACTIVE","isPublished":true,"price":{"__typename":"Money","amount":12.5}}}')
            if mode.startswith("deep_error_"):
                path = _json(["product", "title"] + [0] * (depth - 1))
                body += b',"errors":[{"message":"The title is out of reach","path":' + path + b'}]'
            return self._record(exchange, 200, json_type, body + b"}")
        if mode.startswith("intro_"):
            kind, _, size = mode[len("intro_"):].rpartition("_")
            self._forward(exchange, identity=True)
            crafted = _json(crafted_introspection(json.loads(exchange.response_body), kind, int(size)))
            return self._record(exchange, exchange.status, json_type, crafted)
        if self._hostile is None:
            self._hostile = hostile_responses()
        raw = self._hostile[int(mode[len("hostile_"):]) - 1]
        exchange.response_body = raw or b""
        return raw

    def _record(self, exchange, status, headers, body, status_line=None):
        exchange.status = status
        exchange.response_headers = headers
        exchange.response_body = body
        return _head(status_line or ("HTTP/1.1 %d OK" % status), headers, body)

    def _forward(self, exchange: Exchange, identity: bool = False) -> bytes:
        """Sends one request to the remote and answers what the remote answered.

        A request to a path under `/redirected` reaches the remote at the path
        after that prefix, the path a redirect of the proxy carried along. With
        `identity` the request asks the remote for no content coding, so the
        proxy can read the answer it crafts from."""
        target = exchange.target
        if target.startswith(REDIRECTED):
            target = target[len(REDIRECTED):] or "/"
        connection = http.client.HTTPConnection(*self.upstream, timeout=60)
        try:
            connection.putrequest(exchange.method, target, skip_host=True, skip_accept_encoding=True)
            for name, value in exchange.headers:
                if name.lower() in HOP_BY_HOP or (identity and name.lower() == "accept-encoding"):
                    continue
                connection.putheader(name, value)
            if identity:
                connection.putheader("Accept-Encoding", "identity")
            connection.putheader("Content-Length", str(len(exchange.body)))
            connection.endheaders(exchange.body)
            response = connection.getresponse()
            body = response.read()
            headers = [(name, value) for name, value in response.getheaders() if name.lower() not in HOP_BY_HOP]
            status_line = "HTTP/1.1 %d %s" % (response.status, response.reason)
            return self._record(exchange, response.status, headers, body, status_line)
        except OSError as error:
            body = _json({"errors": [{"message": "the recording proxy could not reach the remote: %s" % error}]})
            return self._record(exchange, 502, [("Content-Type", "application/json; charset=utf-8")], body,
                                "HTTP/1.1 502 Bad Gateway")
        finally:
            connection.close()

    def _wait_turn(self, ticket: int, exchange: Exchange) -> None:
        """Waits until every earlier answer left and the gap after the last one passed.

        The exchange takes the moment its answer leaves before a byte of it is
        written, so a reader of the record that has the answer finds the moment.
        """
        with self._lock:
            while self._serving != ticket:
                self._lock.wait()
            pause = self._last_answer + self.gap_s - time.monotonic()
        if pause > 0:
            time.sleep(pause)
        with self._lock:
            exchange.answered_at = time.monotonic()

    def _done(self, ticket: int, exchange: Exchange) -> None:
        with self._lock:
            self._last_answer = time.monotonic()
            self._serving = ticket + 1
            self._lock.notify_all()

    def _bind(self) -> None:
        server = http.server.ThreadingHTTPServer((self.host, self.port), _Handler, bind_and_activate=False)
        server.request_queue_size = self.backlog
        try:
            server.server_bind()
            server.server_activate()
        except BaseException:
            server.server_close()
            raise
        server.daemon_threads = True
        server.proxy = self
        self._server = server
        threading.Thread(target=server.serve_forever, name="recording-proxy-%d" % self.port, daemon=True).start()

    def _close(self) -> None:
        server, self._server = self._server, None
        if server is not None:
            server.shutdown()
            server.server_close()

    def start(self) -> "RecordingProxy":
        """Binds the port unless the mode is `down`, and serves on a thread."""
        self._started = True
        if self._mode != "down":
            self._bind()
        return self

    def stop(self) -> None:
        """Stops serving and frees the port."""
        self._close()
        self._started = False

    def __enter__(self) -> "RecordingProxy":
        return self.start()

    def __exit__(self, *exc) -> None:
        self.stop()
