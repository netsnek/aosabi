# SPDX-License-Identifier: MIT
"""tests/gen: family H, the wire, 101 cases of raw bytes.

A case of family H is the bytes a client writes on one connection, the order in
which they are split into writes and the pause before each write, and whether
the client shuts its write half after the last write. The cases are the 95 of
the draft `<v2>`, section The oracle, family H, in the order of its table, and
the six of the plan, section The oracle, H96 to H101. They are the one source of
the wire corpus: capture row C48 sends them to the reference, and
`30_http_tier1` sends them to the twins and to `aosabi-devtool http --requests`.

The rules each case names are the rules R1 to R21, N9 to N14 and A1 to A10 of
`<v2>`, and the quirks it pins are those of `docs/byte-equality.md` section
13.1. The bytes carry no customer value and reach no real host: the `Host` is
`h`, the probe header is `Origin` with the value `http://o.test`, whose echo
in the answer of Yoga's CORS shows that a field reached the application (the
draft, family H), and the document is `{__typename}`, which every fixture
answers.

The identity of a case is the SHA-256 of the bytes it sends when it sends them
in one write with no pause and keeps its write half open. Two cases send the
same bytes in different writes, H63 and H64, so a case of more than one write,
or one that shuts its write half, hashes its plan beside its bytes: the text
`aosabi-wire-1`, then per write its wait and its length, then the shut, then
the bytes. The digest of the bytes alone is kept beside it as `bytes_sha256`.

The command line prints the manifest of the family:

    python -m tests.gen --family wire
"""

from __future__ import annotations

import hashlib
import json
import struct
import zlib
from dataclasses import dataclass, field

#: The methods of `require('http').METHODS` of the pinned Node 24.13.0, in its
#: order.
NODE_METHODS = (
    "ACL", "BIND", "CHECKOUT", "CONNECT", "COPY", "DELETE", "GET", "HEAD", "LINK",
    "LOCK", "M-SEARCH", "MERGE", "MKACTIVITY", "MKCALENDAR", "MKCOL", "MOVE",
    "NOTIFY", "OPTIONS", "PATCH", "POST", "PROPFIND", "PROPPATCH", "PURGE", "PUT",
    "QUERY", "REBIND", "REPORT", "SEARCH", "SOURCE", "SUBSCRIBE", "TRACE", "UNBIND",
    "UNLINK", "UNLOCK", "UNSUBSCRIBE",
)

#: The bound of the counted bytes of a head, `http.maxHeaderSize`.
MAX_HEADER_SIZE = 16384

#: The number of cases of the family.
WIRE_CASE_COUNT = 101

#: The target of the query every fixture answers, `{__typename}` percent encoded.
TARGET = "/graphql?query=%7B__typename%7D"

#: The JSON body of the same document, 24 bytes.
BODY = b'{"query":"{__typename}"}'

#: The value of the probe header.
ORIGIN_VALUE = "http://o.test"

HOST = "Host: h\r\n"
ORIGIN = "Origin: " + ORIGIN_VALUE + "\r\n"
JSON_TYPE = "Content-Type: application/json\r\n"

#: A wait before a write: a pause in milliseconds, or until one more answer of
#: the connection has arrived.
MS = "ms"
ANSWER = "answer"


@dataclass(frozen=True)
class Write:
    """One write of a case, and what the client waits for before it."""

    data: bytes
    wait: str = MS
    amount: int = 0


@dataclass(frozen=True)
class WireCase:
    """One case of family H."""

    number: int
    slug: str
    group: str
    rules: tuple[str, ...]
    pins: tuple[str, ...]
    writes: tuple[Write, ...]
    shut: bool = False
    note: str = ""
    requests: int = 1
    extra: dict = field(default_factory=dict)

    @property
    def name(self) -> str:
        """The name of the case, `H01` to `H101`."""
        return "H%02d" % self.number

    @property
    def data(self) -> bytes:
        """Every byte of the case, the writes joined."""
        return b"".join(write.data for write in self.writes)

    @property
    def bytes_sha256(self) -> str:
        """The SHA-256 of the bytes alone."""
        return hashlib.sha256(self.data).hexdigest()

    @property
    def plain(self) -> bool:
        """Whether the case is one write with no wait and an open write half."""
        return (
            len(self.writes) == 1
            and self.writes[0].amount == 0
            and not self.shut
        )

    @property
    def id(self) -> str:
        """The identity of the case, see the module documentation."""
        if self.plain:
            return self.bytes_sha256
        digest = hashlib.sha256(b"aosabi-wire-1\n")
        for write in self.writes:
            digest.update(("%s %d %d\n" % (write.wait, write.amount, len(write.data))).encode())
        digest.update(b"shut\n" if self.shut else b"open\n")
        digest.update(self.data)
        return digest.hexdigest()

    def plan(self) -> list[dict]:
        """The writes as a JSON list of their wait and their length."""
        return [
            {"wait": write.wait, "amount": write.amount, "length": len(write.data)}
            for write in self.writes
        ]

    def manifest(self) -> dict:
        """The case without its bytes, the form a golden and a report carry."""
        return {
            "name": self.name,
            "slug": self.slug,
            "id": self.id,
            "bytes_sha256": self.bytes_sha256,
            "length": len(self.data),
            "group": self.group,
            "rules": list(self.rules),
            "pins": list(self.pins),
            "writes": len(self.writes),
            "plan": self.plan() if len(self.writes) <= 64 else "%d writes" % len(self.writes),
            "shut": self.shut,
            "requests": self.requests,
            "note": self.note,
        }


def one(data) -> tuple[Write, ...]:
    """A case of one write with no wait."""
    return (Write(as_bytes(data)),)


def as_bytes(data) -> bytes:
    """Text is Latin-1, so a byte from 0x80 up is written as one byte."""
    return data if isinstance(data, bytes) else data.encode("latin-1")


def get(target: str = TARGET, version: str = "HTTP/1.1", fields: str = HOST) -> str:
    """A `GET` head with its fields and the empty line."""
    return "GET %s %s\r\n%s\r\n" % (target, version, fields)


def post(fields: str, body: bytes = BODY, version: str = "HTTP/1.1", target: str = "/graphql") -> bytes:
    """A `POST` of a JSON body with a length, its other fields first."""
    head = "POST %s %s\r\n%s%sContent-Length: %d\r\n\r\n" % (
        target, version, HOST, JSON_TYPE + fields, len(body))
    return head.encode("latin-1") + body


def chunked_post(fields: str = "", version: str = "HTTP/1.1") -> str:
    """The head of a chunked `POST` of JSON, its framing field last."""
    return "POST /graphql %s\r\n%s%s%sTransfer-Encoding: chunked\r\n\r\n" % (
        version, HOST, JSON_TYPE, fields)


def chunked(body: bytes = BODY) -> bytes:
    """One chunk of the whole body and the last chunk, the size in lower case hex."""
    return ("%x\r\n" % len(body)).encode() + body + b"\r\n0\r\n\r\n"


def fillers(count: int) -> str:
    """`count` filler fields `X0: a` to `X<count-1>: a`, which no rule reads."""
    return "".join("X%d: a\r\n" % index for index in range(count))


def gzip_stored(data: bytes) -> bytes:
    """A gzip member of one stored DEFLATE block, so its bytes depend on no zlib.

    The header carries no name and the time 0, the block is the stored type of
    RFC 1951 section 3.2.4, and the trailer is the CRC-32 and the length.
    """
    header = b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\xff"
    block = b"\x01" + struct.pack("<HH", len(data), 0xFFFF ^ len(data)) + data
    trailer = struct.pack("<II", zlib.crc32(data) & 0xFFFFFFFF, len(data))
    return header + block + trailer


def counted(target: str, fields: list[tuple[str, str]]) -> int:
    """The bytes rule R12 counts: the target, every name and every value."""
    return len(target) + sum(len(name) + len(value) for name, value in fields)


def request_line_cases() -> list[WireCase]:
    """The 21 cases of the request line, R1 to R4."""
    sequential = tuple(
        Write(("%s /graphql HTTP/1.1\r\n%s\r\n" % (method, HOST)).encode(), ANSWER if index else MS, 1 if index else 0)
        for index, method in enumerate(m for m in NODE_METHODS if m != "CONNECT")
    )
    group = "request line"
    return [
        WireCase(1, "methods_sequential", group, ("R1", "R17"), ("Q18",), sequential,
                 note="The 34 methods of Node but CONNECT on one kept connection, each written after the answer before it arrived", requests=34),
        WireCase(2, "lower_method", group, ("R1",), ("Q18",), one("get %s HTTP/1.1\r\n%s\r\n" % (TARGET, HOST))),
        WireCase(3, "brew", group, ("R1",), ("Q18",), one("BREW %s HTTP/1.1\r\n%s\r\n" % (TARGET, HOST))),
        WireCase(4, "pri", group, ("R1",), ("Q18",), one("PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")),
        WireCase(5, "options_star", group, ("R2", "A6"), ("Q19", "Q54"), one("OPTIONS * HTTP/1.1\r\n%s\r\n" % HOST)),
        WireCase(6, "absolute_form", group, ("R2", "A6"), ("Q19", "Q54"), one(get("http://h" + TARGET))),
        WireCase(7, "target_hash", group, ("R2",), ("Q19",), one(get(TARGET + "#f"))),
        WireCase(8, "target_obs_text", group, ("R2",), ("Q19",), one(get(TARGET + "&x=\xe9"))),
        WireCase(9, "target_ctl", group, ("R2",), ("Q19",), one(get(TARGET + "&x=\x01"))),
        WireCase(10, "target_tab", group, ("R2",), ("Q19",), one(get("/gra\tphql"))),
        WireCase(11, "target_space_inside", group, ("R2", "R4"), ("Q19", "Q21"), one(get("/gra phql"))),
        WireCase(12, "http10", group, ("R3", "R15"), ("Q20", "Q27"), one(get(version="HTTP/1.0", fields=HOST + ORIGIN))),
        WireCase(13, "http20", group, ("R3",), ("Q20",), one(get(version="HTTP/2.0"))),
        WireCase(14, "no_version", group, ("R3",), ("Q20",), one("GET %s\r\n%s\r\n" % (TARGET, HOST))),
        WireCase(15, "two_empty_lines", group, ("R4",), ("Q21",), one("\r\n\r\n" + get())),
        WireCase(16, "http09_host", group, ("R3",), ("Q20",), one(get(version="HTTP/0.9"))),
        WireCase(17, "http12", group, ("R3",), ("Q20",), one(get(version="HTTP/1.2"))),
        WireCase(18, "two_sp", group, ("R4",), ("Q21",), one("GET  %s  HTTP/1.1\r\n%s\r\n" % (TARGET, HOST))),
        WireCase(19, "lead_bare_lf", group, ("R4",), ("Q21",), one("\n" + get())),
        WireCase(20, "tab_after_method", group, ("R4",), ("Q21",), one("GET\t%s HTTP/1.1\r\n%s\r\n" % (TARGET, HOST))),
        WireCase(21, "sp_after_version", group, ("R4",), ("Q21",), one("GET %s HTTP/1.1 \r\n%s\r\n" % (TARGET, HOST))),
    ]


def field_cases() -> list[WireCase]:
    """The 23 cases of the fields, R5 to R7 and R13."""
    group = "fields"
    post_origin = post("Origin: http://o\xe9.test\r\n")
    preflight = "OPTIONS /graphql HTTP/1.1\r\n%sOrigin: http://o\xe9.test\r\nAccess-Control-Request-Method: POST\r\n\r\n" % HOST
    cl_1024 = ("POST /graphql HTTP/1.1\r\n%s%s%sContent-Length: %d\r\n\r\n" % (
        HOST, JSON_TYPE, fillers(1021), len(BODY))).encode() + BODY + get().encode()
    return [
        WireCase(22, "field_bare_lf", group, ("R5",), ("Q22",), one(get(fields="Host: h\n" + ORIGIN))),
        WireCase(23, "space_before_colon", group, ("R6",), ("Q23",), one(get(fields=HOST + "Origin : " + ORIGIN_VALUE + "\r\n"))),
        WireCase(24, "empty_name", group, ("R6",), ("Q23",), one(get(fields=HOST + ": v\r\n"))),
        WireCase(25, "obs_fold", group, ("R6",), ("Q23",), one(get(fields=HOST + ORIGIN + " fold\r\n"))),
        WireCase(26, "value_tab", group, ("R7",), ("Q23",), one(get(fields=HOST + "Origin: http://o\t.test\r\n"))),
        WireCase(27, "value_nul", group, ("R7",), ("Q23",), one(get(fields=HOST + "Origin: http://o\x00.test\r\n"))),
        WireCase(28, "value_del", group, ("R7",), ("Q23",), one(get(fields=HOST + "Origin: http://o\x7f.test\r\n"))),
        WireCase(29, "origin_obs_text_post", group, ("R7", "N13"), ("Q23", "Q58"), (Write(post_origin),)),
        WireCase(30, "origin_obs_text_preflight", group, ("R7", "N13"), ("Q23", "Q58"), one(preflight)),
        WireCase(31, "value_trailing_ws", group, ("R7",), ("Q23",), one(get(fields=HOST + "Origin: " + ORIGIN_VALUE + "   \r\n"))),
        WireCase(32, "two_origins", group, ("R7", "A7"), ("Q55",), one(get(fields=HOST + "Origin: http://a.test\r\nOrigin: http://b.test\r\n"))),
        WireCase(33, "two_hosts", group, ("R13", "A6"), ("Q55", "Q54"), one(get(fields=HOST + "Host: h2\r\n"))),
        WireCase(34, "fields_1023", group, ("R13", "A7"), ("Q26", "Q55"), one(get(fields=HOST + fillers(1021) + ORIGIN))),
        WireCase(35, "fields_1024", group, ("R13", "A7"), ("Q26", "Q55"), one(get(fields=HOST + fillers(1022) + ORIGIN))),
        WireCase(36, "host_field_1000", group, ("R13", "R14"), ("Q26", "Q37"), one(get(fields=fillers(999) + HOST))),
        WireCase(37, "host_field_1001", group, ("R13", "R14"), ("Q26", "Q37"), one(get(fields=fillers(1000) + HOST))),
        WireCase(38, "expect_foo_1000", group, ("R13", "N10"), ("Q26", "Q37"), one(get(fields=HOST + fillers(998) + "Expect: foo\r\n"))),
        WireCase(39, "expect_foo_1001", group, ("R13", "N10"), ("Q26", "Q37"), one(get(fields=HOST + fillers(999) + "Expect: foo\r\n"))),
        WireCase(40, "te_chunked_1000", group, ("R13", "N6"), ("Q26", "Q29"), one(get("/health", "HTTP/1.0", HOST + fillers(998) + "TE: chunked\r\n"))),
        WireCase(41, "te_chunked_1001", group, ("R13", "N6"), ("Q26", "Q29"), one(get("/health", "HTTP/1.0", HOST + fillers(999) + "TE: chunked\r\n"))),
        WireCase(42, "cl_field_1024", group, ("R13", "R8"), ("Q26",), (Write(cl_1024),), requests=2),
        WireCase(43, "connection_close_1024", group, ("R13", "R15"), ("Q26",), one(get(fields=HOST + fillers(1022) + "Connection: close\r\n"))),
        WireCase(44, "upgrade_1024_1025", group, ("R13", "R18"), ("Q26", "Q40"),
                 one(get(fields=HOST + fillers(1022) + "Connection: upgrade\r\nUpgrade: websocket\r\n") + get()), requests=2),
    ]


def framing_cases() -> list[WireCase]:
    """The 20 cases of the framing, R8 to R11, R16, R19 and R20."""
    group = "framing"
    head = chunked_post()
    body = BODY
    size = b"%x" % len(body)
    return [
        WireCase(45, "cl_plus", group, ("R8",), ("Q24",), one(("POST /graphql HTTP/1.1\r\n%s%sContent-Length: +24\r\n\r\n" % (HOST, JSON_TYPE)).encode() + body)),
        WireCase(46, "cl_space_inside", group, ("R8",), ("Q24",), one(("POST /graphql HTTP/1.1\r\n%s%sContent-Length: 2 4\r\n\r\n" % (HOST, JSON_TYPE)).encode() + body)),
        WireCase(47, "cl_list", group, ("R8",), ("Q24",), one(("POST /graphql HTTP/1.1\r\n%s%sContent-Length: 24, 24\r\n\r\n" % (HOST, JSON_TYPE)).encode() + body)),
        WireCase(48, "dup_cl_same", group, ("R8",), ("Q24",), one(("POST /graphql HTTP/1.1\r\n%s%sContent-Length: 24\r\nContent-Length: 24\r\n\r\n" % (HOST, JSON_TYPE)).encode() + body)),
        WireCase(49, "cl_and_te", group, ("R10",), ("Q24",), one(("POST /graphql HTTP/1.1\r\n%s%sContent-Length: 24\r\nTransfer-Encoding: chunked\r\n\r\n" % (HOST, JSON_TYPE)).encode() + chunked())),
        WireCase(50, "te_gzip_only", group, ("R9",), ("Q24",), one("POST /graphql HTTP/1.1\r\n%s%sTransfer-Encoding: gzip\r\n\r\n" % (HOST, JSON_TYPE))),
        WireCase(51, "te_gzip_chunked", group, ("R9",), ("Q24",), one(("POST /graphql HTTP/1.1\r\n%s%sTransfer-Encoding: gzip, chunked\r\n\r\n" % (HOST, JSON_TYPE)).encode() + chunked())),
        WireCase(52, "te_chunked_upper", group, ("R9",), ("Q24",), one(("POST /graphql HTTP/1.1\r\n%s%sTransfer-Encoding: CHUNKED\r\n\r\n" % (HOST, JSON_TYPE)).encode() + chunked())),
        WireCase(53, "chunk_size_leading_ws", group, ("R11",), ("Q34",), one(head.encode() + b" " + size + b"\r\n" + body + b"\r\n0\r\n\r\n")),
        WireCase(54, "chunk_lf_only", group, ("R5", "R11"), ("Q22", "Q34"), one(head.encode() + size + b"\n" + body + b"\r\n0\r\n\r\n")),
        WireCase(55, "chunk_ext_16384", group, ("R19",), ("Q33",), one(head.encode() + size + b";e=" + b"v" * 16383 + b"\r\n" + body + b"\r\n0\r\n\r\n"),
                 note="The extension counts its name and value, 1 and 16,383 bytes, 16,384 in all"),
        WireCase(56, "chunk_ext_16385", group, ("R19",), ("Q33",), one(head.encode() + size + b";e=" + b"v" * 16384 + b"\r\n" + body + b"\r\n0\r\n\r\n"),
                 note="The extension counts its name and value, 1 and 16,384 bytes, 16,385 in all"),
        WireCase(57, "trailers", group, ("R11",), ("Q32",), one(head.encode() + size + b"\r\n" + body + b"\r\n0\r\nT: 1\r\n\r\n")),
        WireCase(58, "trailer_counted_16383", group, ("R11", "R12"), ("Q32",), one(head.encode() + size + b"\r\n" + body + b"\r\n0\r\nT: " + b"v" * 16382 + b"\r\n\r\n"),
                 note="The trailer counts its name and value, 1 and 16,382 bytes"),
        WireCase(59, "trailer_counted_16384", group, ("R11", "R12"), ("Q32",), one(head.encode() + size + b"\r\n" + body + b"\r\n0\r\nT: " + b"v" * 16383 + b"\r\n\r\n"),
                 note="The trailer counts its name and value, 1 and 16,383 bytes"),
        WireCase(60, "chunk_size_leading_zeros_1e6", group, ("R11",), ("Q34",), one(head.encode() + b"0" * 1000000 + size + b"\r\n" + body + b"\r\n0\r\n\r\n")),
        WireCase(61, "get_with_body", group, ("R16",), ("Q47",), one(get(fields=HOST + "Content-Length: 4\r\n") + "abcd" + get()), requests=2),
        WireCase(62, "body_short_close", group, ("R20",), ("Q42",),
                 (Write(("POST /graphql HTTP/1.1\r\n%s%sContent-Length: 24\r\n\r\n" % (HOST, JSON_TYPE)).encode() + body[:10]),), shut=True,
                 note="Ten of the 24 bytes of the body, then the client shuts its write half"),
        WireCase(63, "bad_chunk_same_write", group, ("R20",), ("Q42",), one(head.encode() + b"zz\r\nab\r\n0\r\n\r\n")),
        WireCase(64, "bad_chunk_after_100ms", group, ("R20",), ("Q42",),
                 (Write(head.encode()), Write(b"zz\r\nab\r\n0\r\n\r\n", MS, 100))),
    ]


def size_cases() -> list[WireCase]:
    """The 6 cases of the size of a head, R12."""
    group = "size"
    base = counted(TARGET, [("Host", "h"), ("X", "")])
    value_16383 = get(fields=HOST + "X: " + "a" * (16383 - base) + "\r\n")
    value_16384 = get(fields=HOST + "X: " + "a" * (16384 - base) + "\r\n")
    pad = TARGET + "&p="
    target_base = counted(pad, [("Host", "h")])
    target_16383 = get(pad + "a" * (16383 - target_base))
    target_16384 = get(pad + "a" * (16384 - target_base))
    frame = get(fields=HOST + "X: \r\n")
    kib = get(fields=HOST + "X: " + "a" * (16384 - len(frame)) + "\r\n")
    skipped = (
        "\r\n" * 500000 + "GET" + " " * 1000000 + TARGET + " " * 1000000 + "HTTP/1.1\r\n"
        + HOST + "Origin:" + " " * 1000000 + ORIGIN_VALUE + "\r\n\r\n" + get()
    )
    return [
        WireCase(65, "value_16383", group, ("R12",), ("Q25",), one(value_16383), note="16,383 counted bytes"),
        WireCase(66, "value_16384", group, ("R12",), ("Q25",), one(value_16384), note="16,384 counted bytes"),
        WireCase(67, "target_16383", group, ("R12",), ("Q25",), one(target_16383), note="16,383 counted bytes"),
        WireCase(68, "target_16384", group, ("R12",), ("Q25",), one(target_16384), note="16,384 counted bytes"),
        WireCase(69, "head_16k_byte_per_write", group, ("R12",), ("Q25",),
                 tuple(Write(bytes([byte])) for byte in kib.encode()), note="A head of 16,384 bytes on the wire, one byte per write"),
        WireCase(70, "skipped_bytes", group, ("R4", "R12"), ("Q21", "Q25"), one(skipped), requests=2,
                 note="500,000 CRLF, 1,000,000 SP after the method, before the version and before the value of Origin, and a second request"),
    ]


def connection_cases() -> list[WireCase]:
    """The 25 cases of the connection, R14 to R18, R21, N10 to N12 and A10."""
    group = "connection"
    continue_head = "POST /graphql HTTP/1.1\r\n%s%sExpect: 100-continue\r\nContent-Length: 24\r\n\r\n" % (HOST, JSON_TYPE)
    continue_10 = "POST /graphql HTTP/1.0\r\n%s%sExpect: 100-continue\r\nContent-Length: 24\r\n\r\n" % (HOST, JSON_TYPE)
    no_host_continue = "POST /graphql HTTP/1.1\r\n%sExpect: 100-continue\r\nContent-Length: 24\r\n\r\n" % JSON_TYPE
    connect = "CONNECT h:443 HTTP/1.1\r\nHost: h:443\r\n\r\n"
    unread = "POST /nothing-here HTTP/1.1\r\n%sContent-Length: 4\r\n\r\n" % HOST
    upgrade = "Connection: upgrade\r\nUpgrade: websocket\r\n"
    return [
        WireCase(71, "no_host_11", group, ("R14", "N10"), ("Q37",), one("GET %s HTTP/1.1\r\n\r\n" % TARGET)),
        WireCase(72, "no_host_10", group, ("R14", "A6"), ("Q54",), one("GET %s HTTP/1.0\r\n\r\n" % TARGET)),
        WireCase(73, "host_bad_port", group, ("A6",), ("Q54",), one(get(fields="Host: x:abc\r\n"))),
        WireCase(74, "http10_keepalive", group, ("R15",), ("Q27",), one(get(version="HTTP/1.0", fields=HOST + "Connection: keep-alive\r\n"))),
        WireCase(75, "http10_no_connection", group, ("R15",), ("Q27",), one(post("", version="HTTP/1.0"))),
        WireCase(76, "connection_close", group, ("R15",), ("Q44",), one(get(fields=HOST + "Connection: close\r\n"))),
        WireCase(77, "expect_continue", group, ("N11",), ("Q38",), one(continue_head.encode() + BODY)),
        WireCase(78, "expect_foo", group, ("N10",), ("Q37",), one(get(fields=HOST + "Expect: foo\r\n"))),
        WireCase(79, "http10_expect_foo", group, ("N10",), ("Q37",), one(get(version="HTTP/1.0", fields=HOST + "Expect: foo\r\n"))),
        WireCase(80, "http10_expect_continue_late", group, ("N11",), ("Q38",),
                 (Write(continue_10.encode()), Write(BODY, MS, 100))),
        WireCase(81, "http20_no_host_expect_foo", group, ("R14", "N10"), ("Q37", "Q54"), one("GET %s HTTP/2.0\r\nExpect: foo\r\n\r\n" % TARGET)),
        WireCase(82, "three_pipelined", group, ("R17",), ("Q35",), one(post("") + get().encode() + get().encode()), requests=3),
        WireCase(83, "close_then_get", group, ("R15", "R17"), ("Q41",), one(get(fields=HOST + "Connection: close\r\n") + get()), requests=2),
        WireCase(84, "connect", group, ("R18",), ("Q39",), one(connect)),
        WireCase(85, "get_then_connect", group, ("R17", "R18"), ("Q39",), one(get() + connect), requests=2),
        WireCase(86, "upgrade", group, ("R18",), ("Q40",), one(get(fields=HOST + upgrade))),
        WireCase(87, "post_gzip_body", group, ("A8",), (), one(post("Content-Encoding: gzip\r\n", gzip_stored(BODY))),
                 note="The body is a gzip member of one stored block, so its bytes depend on no zlib"),
        WireCase(88, "unread_body_one_write", group, ("R21", "A10"), ("Q47",), one(unread + "abcd" + get()), requests=2),
        WireCase(89, "unread_body_late", group, ("R21", "A10"), ("Q47",),
                 (Write((unread + "ab").encode()), Write(("cd" + get()).encode(), MS, 200)), requests=2),
        WireCase(90, "http20_keepalive_followup", group, ("R3", "R15"), ("Q20", "Q27"),
                 (Write(get(version="HTTP/2.0", fields=HOST + "Connection: keep-alive\r\n").encode()), Write(get().encode(), MS, 100)), requests=2),
        WireCase(91, "upgrade_then_get_same_write", group, ("R18",), ("Q40",), one(get(fields=HOST + upgrade) + get()), requests=2),
        WireCase(92, "get_then_post_expect_continue", group, ("N11", "R17"), ("Q38",), one(get().encode() + continue_head.encode() + BODY), requests=2),
        WireCase(93, "no_host_expect_continue_late", group, ("N10", "R14"), ("Q37", "Q38"),
                 (Write(no_host_continue.encode()), Write(BODY, MS, 100))),
        WireCase(94, "expect_foo_close_then_get", group, ("N10", "R15"), ("Q37", "Q41"),
                 one(get(fields=HOST + "Expect: foo\r\nConnection: close\r\n") + get()), requests=2),
        WireCase(95, "no_host_then_connect", group, ("N10", "R18"), ("Q37", "Q39"),
                 one("GET %s HTTP/1.1\r\n\r\n" % TARGET + connect), requests=2),
    ]


def added_cases() -> list[WireCase]:
    """The six cases of the plan, H96 to H101."""
    group = "added"
    head = chunked_post()
    extension = b";e=" + b"v" * 9999
    return [
        WireCase(96, "target_raw_classes", group, ("R2", "A6"), ("Q19",), one(get("/a<b>`?x<y", fields=HOST + ORIGIN)),
                 note="The target /a<b>, a backtick and ?x<y, with Host and Origin"),
        WireCase(97, "head_end_lf_crlf", group, ("R5",), ("Q22",), one(get(fields=HOST + "Origin: " + ORIGIN_VALUE + "\n")),
                 note="The head ends in \\n\\r\\n after its last field"),
        WireCase(98, "http10_chunked", group, ("R9", "R3"), ("Q24",), one(chunked_post(version="HTTP/1.0").encode() + chunked()),
                 note="Decides the part of Q24 on HTTP/1.0"),
        WireCase(99, "chunk_size_trailing_sp", group, ("R11",), ("Q34",),
                 one(head.encode() + b"2 \r\n" + BODY[:2] + b"\r\n" + (b"%x" % (len(BODY) - 2)) + b"\r\n" + BODY[2:] + b"\r\n0\r\n\r\n"),
                 note="Decides the space after the size of Q34"),
        WireCase(100, "half_closed_client", group, ("R15",), ("Q48",), (Write(get().encode()),), shut=True,
                 note="Decides Q48: the client shuts its write half after the request"),
        WireCase(101, "chunk_extensions_per_chunk", group, ("R19",), ("Q33",),
                 one(head.encode() + b"2" + extension + b"\r\n" + BODY[:2] + b"\r\n" + (b"%x" % (len(BODY) - 2)) + extension + b"\r\n" + BODY[2:] + b"\r\n0\r\n\r\n"),
                 note="Two chunks, each with 10,000 counted bytes of extensions"),
    ]


def wire_cases() -> list[WireCase]:
    """Every case of family H, H01 to H101, in the order of their numbers."""
    cases = (
        request_line_cases() + field_cases() + framing_cases() + size_cases()
        + connection_cases() + added_cases()
    )
    numbers = [case.number for case in cases]
    if numbers != list(range(1, WIRE_CASE_COUNT + 1)):
        raise AssertionError("family H is numbered %s, the plan asks 1 to %d" % (numbers, WIRE_CASE_COUNT))
    ids = [case.id for case in cases]
    if len(set(ids)) != len(ids):
        raise AssertionError("two cases of family H share an id")
    return cases


def wire_manifest() -> dict:
    """The manifest of the family: the count per group and every case without its bytes."""
    cases = wire_cases()
    groups: dict[str, int] = {}
    for case in cases:
        groups[case.group] = groups.get(case.group, 0) + 1
    return {
        "family": "H",
        "count": len(cases),
        "groups": groups,
        "bytes": sum(len(case.data) for case in cases),
        "digest": hashlib.sha256(
            json.dumps([case.id for case in cases], separators=(",", ":")).encode()
        ).hexdigest(),
        "cases": [case.manifest() for case in cases],
    }
