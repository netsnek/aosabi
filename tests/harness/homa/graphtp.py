# SPDX-License-Identifier: MIT
"""Aosabi harness: the codec of GraphTP/1 in Python, written apart from the crate.

This module writes and reads every message of GraphTP/1 that the notebook
24_graphtp compares the crate graphtp against: the status byte and its mapping,
the registry of status and substatus with its names, the heads of a request and
of an answer, the checks of the credential, the first byte rule of a body, the
whole request with the 11 rules of refusal, the whole answer with its shapes, the
stream bodies, and the test vectors of section 9.2. It codes and decodes with
Python's zlib, which is C zlib, at wbits 12, a window of 4 KiB. It is written
from docs/graphtp.md, sections 2.2, 2.4, 2.5, 3.2 to 3.4, 5.1 to 5.5 and 9.2, and
from the IANA registry of HTTP status codes, and never from the source of the
crate, so the notebook never judges the crate against itself.

Every integer of a head is big endian. A refusal of a request raises
`Refused` with the code a server answers, and an answer a client cannot read
raises `Malformed` with its reason.

The transport half carries the messages over Homa through the Python Homa engine
tests/harness/homa/client.py of the homa track: `Client` sends each exchange as
one Homa RPC, a PULL as a patient RPC, an OPERATION with the report of
RPC_UNKNOWN unless its caller marks it repeatable, applies the stream answers in
the order of `seq`, holds one that is ahead, and keeps the start rule, all from
docs/graphtp.md sections 2.4, 2.7 and 4.1. The notebooks that judge the GraphTP
listener run their server rows through it.
"""

from __future__ import annotations

import json
import re
import time
import zlib
from dataclasses import dataclass

from . import client as homa

# The version GraphTP/1 writes in bits 7 and 6 of the first byte of a request.
# The bits 00 are never a version, 10 is GraphTP/2 and 11 is the escape a later
# version defines.
VERSION_1 = 1

# The kinds of bits 5 to 3 of the first byte. The values 3 to 7 are unassigned.
OPERATION = 0
PULL = 1
CANCEL = 2

# The longest content or body a sender sends plain. A sender codes exactly the
# contents and bodies of more than this many bytes, >128 B.
THRESHOLD = 128

# The longest credential the length field of a request head carries.
MAX_CREDENTIAL = 65535

# The base 2 logarithm of the window of every zlib stream of GraphTP/1, a window
# of 4,096 bytes.
WBITS = 12

# The IANA registry of HTTP status codes, the rows of the classes 2 to 5 that are
# assigned, with the description column verbatim. The file read is
# https://www.iana.org/assignments/http-status-codes/http-status-codes-1.csv,
# whose SHA-256 was
# 4a9550d4b4ae49cf41cf9050cf9b56b0d6082ad1edfc0e2b09b07f251a36d7a4. Rows marked
# Unassigned and ranges are left out, as docs/graphtp.md section 5.2 counts them.
IANA_REASONS = {
    200: "OK",
    201: "Created",
    202: "Accepted",
    203: "Non-Authoritative Information",
    204: "No Content",
    205: "Reset Content",
    206: "Partial Content",
    207: "Multi-Status",
    208: "Already Reported",
    226: "IM Used",
    300: "Multiple Choices",
    301: "Moved Permanently",
    302: "Found",
    303: "See Other",
    304: "Not Modified",
    305: "Use Proxy",
    306: "(Unused)",
    307: "Temporary Redirect",
    308: "Permanent Redirect",
    400: "Bad Request",
    401: "Unauthorized",
    402: "Payment Required",
    403: "Forbidden",
    404: "Not Found",
    405: "Method Not Allowed",
    406: "Not Acceptable",
    407: "Proxy Authentication Required",
    408: "Request Timeout",
    409: "Conflict",
    410: "Gone",
    411: "Length Required",
    412: "Precondition Failed",
    413: "Content Too Large",
    414: "URI Too Long",
    415: "Unsupported Media Type",
    416: "Range Not Satisfiable",
    417: "Expectation Failed",
    418: "(Unused)",
    421: "Misdirected Request",
    422: "Unprocessable Content",
    423: "Locked",
    424: "Failed Dependency",
    425: "Too Early",
    426: "Upgrade Required",
    428: "Precondition Required",
    429: "Too Many Requests",
    431: "Request Header Fields Too Large",
    451: "Unavailable For Legal Reasons",
    500: "Internal Server Error",
    501: "Not Implemented",
    502: "Bad Gateway",
    503: "Service Unavailable",
    504: "Gateway Timeout",
    505: "HTTP Version Not Supported",
    506: "Variant Also Negotiates",
    507: "Insufficient Storage",
    508: "Loop Detected",
    510: "Not Extended (OBSOLETED)",
    511: "Network Authentication Required",
}

# The registry of status and substatus of docs/graphtp.md section 2.5, one row
# per code, as its name, who answers it and the state of the row. S is the
# service, G the GraphTP server and L an intermediary. The row "Any other
# status.0" is no code of its own and stands in `code_name` instead.
REGISTRY = {
    (200, 0): ("OK", "S G", "HTTP"),
    (200, 1): ("Partial", "S", "Decided"),
    (200, 2): ("No Data", "S", "Decided"),
    (200, 3): ("Stream Opened", "G", "Proposed"),
    (400, 1): ("Syntax Error", "S", "Decided"),
    (400, 2): ("Validation Error", "S", "Decided"),
    (400, 3): ("Invalid Variables", "S", "Decided"),
    (400, 4): ("Operation Not Found", "S", "Decided"),
    (400, 5): ("Query Too Complex", "S G", "Decided"),
    (400, 6): ("Invalid Request Body", "S", "Proposed"),
    (400, 7): ("Malformed Message", "G", "Proposed"),
    (400, 8): ("Bad Coding", "G", "Proposed"),
    (403, 1): ("Stream Not Allowed", "G", "Proposed"),
    (404, 1): ("Unknown Stream", "G", "Proposed"),
    (409, 1): ("Pull Already Parked", "G", "Proposed"),
    (413, 0): ("Content Too Large", "S G L", "HTTP"),
    (415, 1): ("Unsupported Coding", "G", "Proposed"),
    (422, 2): ("Validation Error", "S", "Proposed"),
    (422, 3): ("Invalid Variables", "S", "Proposed"),
    (422, 4): ("Operation Not Found", "S", "Proposed"),
    (422, 5): ("Query Too Complex", "S", "Proposed"),
    (422, 6): ("Invalid Request Body", "S", "Proposed"),
    (429, 1): ("Too Many Streams", "G", "Proposed"),
    (431, 0): ("Request Header Fields Too Large", "S G", "HTTP"),
    (500, 0): ("Internal Server Error", "S G", "HTTP"),
    (500, 1): ("Answer Too Large", "G", "Proposed"),
    (500, 2): ("Status Not Representable", "G", "Proposed"),
    (500, 3): ("Unsupported Answer Type", "G", "Proposed"),
    (501, 0): ("Not Implemented", "S G", "HTTP"),
    (502, 0): ("Bad Gateway", "S G L", "HTTP"),
    (502, 1): ("Unknown Outcome", "L", "Proposed"),
    (502, 2): ("Link Down", "L", "Proposed"),
    (503, 1): ("Insufficient Memory", "G", "Proposed"),
    (504, 0): ("Gateway Timeout", "S G L", "HTTP"),
    (505, 0): ("HTTP Version Not Supported", "S G", "HTTP"),
}


@dataclass(frozen=True)
class Code:
    """A status with its substatus, written status.substatus."""

    status: int
    substatus: int

    def __str__(self) -> str:
        return "%d.%d" % (self.status, self.substatus)

    @property
    def name(self) -> str | None:
        """The name of the code, which `code_name` gives."""
        return code_name(self.status, self.substatus)


# The codes a server refuses a request with by the rules of section 5.3.
MALFORMED_MESSAGE = Code(400, 7)
HTTP_VERSION_NOT_SUPPORTED = Code(505, 0)
NOT_IMPLEMENTED = Code(501, 0)
REQUEST_HEADER_FIELDS_TOO_LARGE = Code(431, 0)
BAD_CODING = Code(400, 8)
UNSUPPORTED_CODING = Code(415, 1)
CONTENT_TOO_LARGE = Code(413, 0)

# The two codes whose answer carries a stream body.
OK = Code(200, 0)
STREAM_OPENED = Code(200, 3)


class Refused(Exception):
    """A request a server refuses, with the code it answers."""

    def __init__(self, code: Code) -> None:
        super().__init__(str(code))
        self.code = code


class Malformed(Exception):
    """An answer a client reads as malformed, with the reason.

    The reason is `short` for a message of fewer than 2 bytes and `coding` for a
    body that opens with no coding of the first byte rule.
    """

    def __init__(self, reason: str) -> None:
        super().__init__(reason)
        self.reason = reason


def status_byte(status: int) -> int | None:
    """The status byte of `status`, or None for a status that has none.

    The byte is 64 times the first digit less 2, plus the last two digits, and
    only the classes 2 to 5 with last two digits of at most 63 have one.
    """
    first, last = divmod(status, 100)
    if first not in (2, 3, 4, 5) or last > 63:
        return None
    return 64 * (first - 2) + last


def status_of(byte: int) -> int:
    """The status the byte `byte` names, the byte divided by 64 plus 2 in the
    hundreds and the byte modulo 64 below them."""
    if not 0 <= byte <= 255:
        raise ValueError("a status byte is 0 to 255, not %r" % byte)
    return (byte // 64 + 2) * 100 + byte % 64


def status_class(status: int) -> int:
    """The class of `status`, its first digit."""
    return status // 100


def reason(status: int) -> str | None:
    """The reason phrase the IANA registry gives `status`, or None."""
    return IANA_REASONS.get(status)


def code_name(status: int, substatus: int) -> str | None:
    """The name of the code `status`.`substatus`, or None for an unregistered one.

    Every status.0 is named by the reason phrase of its status, the row "Any
    other status.0" of the registry among them, and every other code by the name
    of its row.
    """
    if substatus == 0:
        return reason(status)
    row = REGISTRY.get((status, substatus))
    return row[0] if row else None


@dataclass(frozen=True)
class RequestHead:
    """The four fields of the 3 bytes that open a request."""

    version: int
    kind: int
    compressed: bool
    credential_len: int


def write_request_head(version: int, kind: int, compressed: bool, credential_len: int) -> bytes | None:
    """The 3 bytes of a request head, or None for a version or kind with no bits.

    The first byte holds the version in bits 7 and 6, the kind in bits 5 to 3,
    the compressed bit in bit 2 and the two reserved bits as 0, and the next two
    bytes the credential length. The version 0 is never written, since 00 is
    never valid.
    """
    if version not in (1, 2, 3) or not 0 <= kind <= 7:
        return None
    if not 0 <= credential_len <= MAX_CREDENTIAL:
        raise ValueError("a credential length is 0 to 65535, not %r" % credential_len)
    first = (version << 6) | (kind << 3) | (4 if compressed else 0)
    return bytes([first]) + credential_len.to_bytes(2, "big")


def read_request_head(message: bytes) -> RequestHead:
    """The head at the start of `message`, by rules 1 to 4 of section 5.3.

    The rules are checked in their order: at least 3 bytes, else 400.7, the
    version 01, else 505.0, a kind of 0 to 2, else 501.0, and the reserved bits
    0, else 400.7.
    """
    if len(message) < 3:
        raise Refused(MALFORMED_MESSAGE)
    first = message[0]
    if first >> 6 != VERSION_1:
        raise Refused(HTTP_VERSION_NOT_SUPPORTED)
    kind = (first >> 3) & 7
    if kind not in (OPERATION, PULL, CANCEL):
        raise Refused(NOT_IMPLEMENTED)
    if first & 3:
        raise Refused(MALFORMED_MESSAGE)
    return RequestHead(
        version=VERSION_1,
        kind=kind,
        compressed=bool(first & 4),
        credential_len=int.from_bytes(message[1:3], "big"),
    )


def write_answer_head(status: int, substatus: int) -> bytes | None:
    """The 2 bytes of an answer head, or None for a status with no byte."""
    byte = status_byte(status)
    if byte is None:
        return None
    if not 0 <= substatus <= 255:
        raise ValueError("a substatus is 0 to 255, not %r" % substatus)
    return bytes([byte, substatus])


def read_answer_head(message: bytes) -> Code:
    """The code of the head at the start of the answer `message`.

    Every pair of bytes is a head, and a message of fewer than 2 bytes is
    malformed.
    """
    if len(message) < 2:
        raise Malformed("short")
    return Code(status_of(message[0]), message[1])


def zlib_header(cmf: int, flg: int) -> bool:
    """Whether `cmf` and `flg` are a zlib header of the profile of section 3.2.

    CM, the low four bits of CMF, is 8, CINFO, the high four, is 0 to 4, CMF
    times 256 plus FLG is a multiple of 31, and FDICT, bit 5 of FLG, is 0.
    """
    return cmf % 16 == 8 and cmf // 16 <= 4 and (cmf * 256 + flg) % 31 == 0 and not flg & 32


def coding_of(body: bytes) -> str:
    """The coding of an answer body by its first byte, `empty`, `plain` or `zlib`.

    The brace and the bracket open plain JSON, and a zlib header of the profile
    opens a zlib stream. Any other opening is malformed.
    """
    if not body:
        return "empty"
    if body[0] in (0x7B, 0x5B):
        return "plain"
    if len(body) >= 2 and zlib_header(body[0], body[1]):
        return "zlib"
    raise Malformed("coding")


def check_credential(credential: bytes, bound: int | None = None) -> bytes:
    """The credential when a server accepts it, by rules 6 and 7 of section 5.3.

    A byte outside 0x20 to 0x7E refuses it with 400.7 first, since a credential
    is a JWT or another token in the form HTTP carries, then a length above
    65535, the most a head carries, or above the server's `bound` refuses it with
    431.0.
    """
    if any(byte < 0x20 or byte > 0x7E for byte in credential):
        raise Refused(MALFORMED_MESSAGE)
    if len(credential) > MAX_CREDENTIAL:
        raise Refused(REQUEST_HEADER_FIELDS_TOO_LARGE)
    if bound is not None and len(credential) > bound:
        raise Refused(REQUEST_HEADER_FIELDS_TOO_LARGE)
    return credential


def server_only(code: Code) -> bool:
    """Whether the registry of section 2.5 gives `code` to the server alone.

    Such a code has G alone in the column of who answers and a nonzero substatus,
    and no answer of it but 200.3 carries a body.
    """
    row = REGISTRY.get((code.status, code.substatus))
    return code.substatus != 0 and row is not None and row[1] == "G"


def deflate(plain: bytes, level: int = 6) -> bytes:
    """One zlib stream of `plain` at a window of 4 KiB, which C zlib writes."""
    encoder = zlib.compressobj(level, zlib.DEFLATED, WBITS)
    return encoder.compress(plain) + encoder.flush()


def coded(plain: bytes) -> bytes:
    """The bytes a sender puts on the wire for the content or body `plain`.

    A sender codes exactly the contents and bodies of more than 128 bytes, with
    no size check, and sends every one of at most 128 bytes plain.
    """
    return deflate(plain) if len(plain) > THRESHOLD else plain


def _inflated(stream: bytes, room: int) -> tuple:
    """What C zlib at wbits 12 writes for `stream` with room for `room` bytes.

    The result is the bytes, the decoder and None, or None, the decoder and the
    message of the zlib error the decoder meets. C zlib reads on past a full
    output buffer up to the next symbol that needs room, so an error that
    stands right behind the last byte of the room surfaces as well.
    """
    decoder = zlib.decompressobj(WBITS)
    try:
        return decoder.decompress(stream, room), decoder, None
    except zlib.error as error:
        return None, decoder, str(error)


def inflate(stream: bytes, bound: int) -> bytes:
    """The plain bytes of the zlib stream `stream` under the decoded `bound`.

    A stream that expands past the bound stops with 413.0 as soon as its plain
    bytes pass it, and a stream that does not decode before that, ends early,
    fails its Adler-32 or has bytes after it refuses with 400.8. The header is
    checked before this call, by rule 9, and the bound is at least 1.

    C zlib decodes with room for one byte past the bound. When it meets an error
    there, the error stands before or behind the byte that passes the bound, and
    a second decoding with room for the bound alone tells which: an error there
    stands before that byte. With room for that byte, the one error that can
    stand at it is a distance too far back, which C zlib checks before it copies
    a match and never once its room is full.
    """
    if bound < 1:
        raise ValueError("the bound is at least 1, not %r" % bound)
    plain, decoder, error = _inflated(stream, bound + 1)
    if error is None:
        if len(plain) > bound:
            raise Refused(CONTENT_TOO_LARGE)
        if not decoder.eof or decoder.unused_data:
            raise Refused(BAD_CODING)
        return plain
    _, _, before = _inflated(stream, bound)
    if before is not None or "too far back" in error:
        raise Refused(BAD_CODING)
    raise Refused(CONTENT_TOO_LARGE)


def check_header(cmf: int, flg: int) -> None:
    """Rule 9 of section 5.3 for the zlib header `cmf` and `flg`.

    FCHECK is read first, 400.8 when CMF times 256 plus FLG is no multiple of 31,
    then the fields, 415.1 for CM other than 8, CINFO above 4 or FDICT set.
    """
    if (cmf * 256 + flg) % 31:
        raise Refused(BAD_CODING)
    if cmf % 16 != 8 or cmf // 16 > 4 or flg & 32:
        raise Refused(UNSUPPORTED_CODING)


def write_operation(credential: bytes | None, content: bytes) -> bytes:
    """The whole OPERATION with `credential`, None for none, and `content`.

    The first byte is 0x40 plain and 0x44 coded, the credential length and the
    credential follow the head, and the content travels coded when it holds more
    than 128 bytes.
    """
    credential = check_credential(credential or b"")
    compressed = len(content) > THRESHOLD
    head = write_request_head(VERSION_1, OPERATION, compressed, len(credential))
    return head + credential + coded(content)


def write_pull() -> bytes:
    """The whole PULL, 48 00 00."""
    return write_request_head(VERSION_1, PULL, False, 0)


def write_cancel(handle: int) -> bytes:
    """The whole CANCEL of the stream `handle`, 0 for every stream of the peer."""
    return write_request_head(VERSION_1, CANCEL, False, 0) + handle.to_bytes(2, "big")


def write_answer(status: int, substatus: int, body: bytes) -> bytes | None:
    """The whole answer of `status`.`substatus` with `body`, coded when it holds
    more than 128 bytes, or None for a status that has no byte."""
    head = write_answer_head(status, substatus)
    return None if head is None else head + coded(body)


def read_request(message: bytes, max_credential: int = MAX_CREDENTIAL, max_content: int = 1 << 20) -> dict:
    """The request `message` as a server accepts it, by the 11 rules of section 5.3.

    The rules are checked in their order, and the first one the message breaks
    raises `Refused` with its code: rules 1 to 4 on the head, 5 the credential
    within the message, 6 every byte of the credential 0x20 to 0x7E and none on
    PULL and CANCEL, 7 the credential within `max_credential`, 8 the shape of
    PULL and CANCEL, 9 the zlib header of a coded content, 10 the decoded content
    within `max_content` and 11 the zlib stream whole.

    The request is a dict with `kind`, and for an OPERATION its `credential`,
    None for none, its `coding` and its decoded `content`, and for a CANCEL its
    `handle`.
    """
    head = read_request_head(message)
    end = 3 + head.credential_len
    if len(message) < end:
        raise Refused(MALFORMED_MESSAGE)
    credential, rest = message[3:end], message[end:]
    if any(byte < 0x20 or byte > 0x7E for byte in credential):
        raise Refused(MALFORMED_MESSAGE)
    if head.kind != OPERATION and credential:
        raise Refused(MALFORMED_MESSAGE)
    if len(credential) > max_credential:
        raise Refused(REQUEST_HEADER_FIELDS_TOO_LARGE)
    if head.kind == PULL:
        if head.compressed or rest:
            raise Refused(MALFORMED_MESSAGE)
        return {"kind": PULL}
    if head.kind == CANCEL:
        if head.compressed or len(rest) != 2:
            raise Refused(MALFORMED_MESSAGE)
        return {"kind": CANCEL, "handle": int.from_bytes(rest, "big")}
    if head.compressed:
        if len(rest) < 2:
            raise Refused(BAD_CODING)
        check_header(rest[0], rest[1])
        content = inflate(rest, max_content)
    else:
        if len(rest) > max_content:
            raise Refused(CONTENT_TOO_LARGE)
        content = rest
    return {
        "kind": OPERATION,
        "credential": credential or None,
        "coding": "zlib" if head.compressed else "plain",
        "content": content,
    }


def read_answer(kind: int, message: bytes, bound: int = 1 << 24) -> dict:
    """The answer `message` to a request of `kind` as a client reads it.

    The head comes first, then the coding of the body by its first byte, then the
    shape the kind and the code give it. An empty body is a refusal whatever its
    code, but for 200.3 and the 200.0 of a PULL or a CANCEL, which carry a stream
    head or a batch. A body under a code the registry gives to the server alone,
    200.3 apart, under a code other than 200.0 on a PULL or a CANCEL, or on a
    kind of 3 to 7 is malformed, as is a coded body that does not decode and a
    stream body that is no JSON of section 2.4.

    The answer is a dict with `answer`, one of `result`, `opened`, `batch` and
    `refused`, the `code`, and for a body its `coding` and the decoded `body`,
    with `seq` and `entries` for a stream body and `stream` for the head of a
    stream. A reason it cannot be read raises `Malformed`: `short`, `coding`,
    `empty`, `body`, `decode`, `bound` or `stream`.
    """
    code = read_answer_head(message)
    body = message[2:]
    coding = coding_of(body)
    if coding == "empty":
        if code == STREAM_OPENED or (code == OK and kind in (PULL, CANCEL)):
            raise Malformed("empty")
        return {"answer": "refused", "code": code}
    if kind == OPERATION and code == STREAM_OPENED:
        shape = "opened"
    elif kind == OPERATION and not server_only(code):
        shape = "result"
    elif kind in (PULL, CANCEL) and code == OK:
        shape = "batch"
    else:
        raise Malformed("body")
    if coding == "zlib":
        try:
            plain = inflate(body, bound)
        except Refused as refusal:
            raise Malformed("bound" if refusal.code == CONTENT_TOO_LARGE else "decode") from None
    elif len(body) > bound:
        raise Malformed("bound")
    else:
        plain = body
    answer = {"answer": shape, "code": code, "coding": coding, "body": plain}
    if shape == "result":
        return answer
    handle, seq, entries = read_stream(plain)
    if shape == "opened":
        if handle is None:
            raise Malformed("stream")
        answer["stream"] = handle
    answer["seq"] = seq
    answer["entries"] = entries
    return answer


def write_stream(entries: list, seq: int, handle: int | None = None) -> bytes:
    """The plain stream body of `entries`, the head of stream `handle` or a batch.

    An entry is a tuple, ("next", handle, the bytes of a JSON value) or one of
    ("complete", handle), ("cancelled", handle) and ("aborted", handle). The keys
    stand in the order of section 2.4 with no other key and no whitespace, and a
    `next` value is embedded verbatim.
    """
    parts = []
    for entry in entries:
        mark, stream = entry[0], entry[1]
        if mark == "next":
            parts.append(b'{"stream":%d,"next":%s}' % (stream, entry[2]))
        else:
            parts.append(b'{"stream":%d,"%s":true}' % (stream, mark.encode("ascii")))
    events = b'"events":[' + b",".join(parts) + b"]}"
    if handle is None:
        return b'{"seq":%d,' % seq + events
    return b'{"stream":%d,"seq":%d,' % (handle, seq) + events


# A number of a stream body as a sender writes it, digits with no sign, no
# leading zero, no fraction and no exponent.
_INTEGER = re.compile(r"0|[1-9][0-9]*")
_SPACE = re.compile(r"[ \t\n\r]*")
_MARKS = ("complete", "cancelled", "aborted")


def _refuse_constant(name):
    """Refuse NaN and Infinity, which Python's json reads and JSON has not."""
    raise ValueError(name)


_DECODER = json.JSONDecoder(parse_constant=_refuse_constant)


def _decode(text: str, at: int):
    """The JSON value at `at` of `text` and the index after it.

    Python's json reads nested values by recursion, so a value nested deeper than
    the interpreter's recursion limit is refused as well.
    """
    try:
        return _DECODER.raw_decode(text, at)
    except (ValueError, RecursionError):
        raise Malformed("stream") from None


def _members(text: str, at: int, known: tuple) -> tuple:
    """The members of the JSON object at `at` of `text` and the index after it.

    The members are a dict from each key of `known` to its value and the span of
    its bytes. A key of `known` that stands twice is malformed, and every other
    key is read and left out. The value of `events` is read by `_events`.
    """
    at = _SPACE.match(text, at).end()
    if text[at : at + 1] != "{":
        raise Malformed("stream")
    at = _SPACE.match(text, at + 1).end()
    members = {}
    if text[at : at + 1] == "}":
        return members, at + 1
    while True:
        if text[at : at + 1] != '"':
            raise Malformed("stream")
        key, at = _decode(text, at)
        at = _SPACE.match(text, at).end()
        if text[at : at + 1] != ":":
            raise Malformed("stream")
        start = _SPACE.match(text, at + 1).end()
        if key == "events" and "events" in known:
            value, at = _events(text, start)
        else:
            value, at = _decode(text, start)
        if key in known:
            if key in members:
                raise Malformed("stream")
            members[key] = (value, start, at)
        at = _SPACE.match(text, at).end()
        mark = text[at : at + 1]
        at += 1
        if mark == "}":
            return members, at
        if mark != ",":
            raise Malformed("stream")
        at = _SPACE.match(text, at).end()


def _events(text: str, at: int) -> tuple:
    """The entries of the array `events` at `at` of `text` and the index after it."""
    if text[at : at + 1] != "[":
        raise Malformed("stream")
    at = _SPACE.match(text, at + 1).end()
    entries = []
    if text[at : at + 1] == "]":
        return entries, at + 1
    while True:
        members, at = _members(text, at, ("stream", "next") + _MARKS)
        entries.append(_entry(text, members))
        at = _SPACE.match(text, at).end()
        mark = text[at : at + 1]
        at += 1
        if mark == "]":
            return entries, at
        if mark != ",":
            raise Malformed("stream")


def _integer(text: str, member: tuple, largest: int) -> int:
    """The integer of the member `member`, 0 to `largest`, as a sender writes it."""
    _, start, end = member
    digits = text[start:end]
    if not _INTEGER.fullmatch(digits) or int(digits) > largest:
        raise Malformed("stream")
    return int(digits)


def _entry(text: str, members: dict) -> tuple:
    """The entry of the members of one object of `events`."""
    if "stream" not in members:
        raise Malformed("stream")
    handle = _integer(text, members["stream"], 65535)
    marks = [mark for mark in ("next",) + _MARKS if mark in members]
    if handle == 0 or len(marks) != 1:
        raise Malformed("stream")
    mark = marks[0]
    _, start, end = members[mark]
    if mark == "next":
        return ("next", handle, text[start:end].encode("latin-1"))
    if text[start:end] != "true":
        raise Malformed("stream")
    return (mark, handle)


def read_stream(body: bytes) -> tuple:
    """The handle, the batch number and the entries of the plain stream body `body`.

    The body is read with Python's json, which C reads under the hood, over the
    bytes as Latin-1, so an index of the text is an index of the bytes and a
    `next` value comes back as the bytes the service wrote. Any key order and any
    whitespace of JSON are read, and a key the body does not know is read and
    left out. The handle is None for a batch. A body that is no object, has no
    `seq` or `events`, carries a known key twice, a handle outside 1 to 65535, a
    batch number outside 32 bits, a number not written as a sender writes it, an
    entry with no `stream` or with none or more than one of `next`, `complete`,
    `cancelled` and `aborted`, a mark other than `true`, or bytes after the
    object raises `Malformed("stream")`.
    """
    text = body.decode("latin-1")
    members, at = _members(text, 0, ("stream", "seq", "events"))
    if _SPACE.match(text, at).end() != len(text):
        raise Malformed("stream")
    if "seq" not in members or "events" not in members:
        raise Malformed("stream")
    handle = None
    if "stream" in members:
        handle = _integer(text, members["stream"], 65535)
        if handle == 0:
            raise Malformed("stream")
    seq = _integer(text, members["seq"], 2**32 - 1)
    return handle, seq, members["events"][0]


# The credential of 71 bytes and the exchange {__typename} of the test vectors
# of docs/graphtp.md section 9.2.
VECTOR_TOKEN = b"Bearer x7Kq2mP9vR4tY8wZ1aB3cD5eF6gH0iJ2kL4mN6oP8qR0sT2uV4wX6yZ8aB0cD2eF"
VECTOR_QUERY = b'{"query":"{__typename}"}'
VECTOR_RESULT = b'{"data":{"__typename":"Query"}}'
VECTOR_CODED = (
    b'{"data":{"createPost":{"id":"post_7f3a9c21","title":"Hello GraphTP","status":"DRAFT",'
    b'"tags":["homa","graphql"],"createdAt":"2026-09-25T20:00:00.000Z"}}}'
)


def test_vectors() -> list:
    """The test vectors of section 9.2, written by this module.

    Each vector is a dict with its `name`, its `kind` of request or answer, the
    `message` this module writes for it and what reading it gives, `request` or
    `answer` for a message a peer accepts and `refused` for a malformed request
    with the code and the 2 bytes a server answers. The coded answer is this
    module's own zlib stream of the 152 bytes the section codes, since another
    encoder may write other bytes for the same body.
    """
    head = write_stream([], 0, handle=1)
    vectors = [
        ("the exchange {__typename}, request", "request", write_operation(None, VECTOR_QUERY)),
        ("the exchange {__typename}, answer", "answer", write_answer(200, 0, VECTOR_RESULT)),
        ("a credential of 71 bytes", "request", write_operation(VECTOR_TOKEN, VECTOR_QUERY)),
        ("a coded answer", "answer", write_answer(200, 0, VECTOR_CODED)),
        ("200.1 Partial", "answer", write_answer(200, 1, VECTOR_RESULT)),
        ("200.2 No Data", "answer", write_answer(200, 2, VECTOR_RESULT)),
        ("400.1 Syntax Error", "answer", write_answer(400, 1, VECTOR_RESULT)),
        ("200.3 Stream Opened, the head of stream 1", "answer", write_answer(200, 3, head)),
        ("PULL", "request", write_pull()),
        ("CANCEL of stream 1", "request", write_cancel(1)),
        ("CANCEL of every stream of the peer", "request", write_cancel(0)),
        ("505.0, a refusal", "answer", write_answer(505, 0, b"")),
        ("415.1, a refusal", "answer", write_answer(415, 1, b"")),
    ]
    malformed = [
        ("rule 2, version 00", bytes.fromhex("000000")),
        ("rule 3, kind 3", bytes.fromhex("580000")),
        ("rule 4, a reserved bit set", bytes.fromhex("4100007b7d")),
        ("rule 5, the credential passes the end", bytes.fromhex("40000541")),
        ("rule 6, a credential on a PULL", bytes.fromhex("48000141")),
        ("rule 8, a CANCEL of 1 byte", bytes.fromhex("50000001")),
        ("rule 9, a coded content of 1 byte", bytes.fromhex("44000048")),
        ("rule 9, CINFO 7 and a broken FCHECK", bytes.fromhex("4400007800") + deflate(b"{}")[2:]),
        ("rule 9, CINFO 7, a window of 32 KiB", bytes.fromhex("440000789c") + deflate(b"{}")[2:]),
    ]
    out = []
    for name, kind, message in vectors:
        if kind == "request":
            out.append({"name": name, "kind": kind, "message": message, "request": read_request(message)})
        else:
            out.append({"name": name, "kind": kind, "message": message, "answer": read_answer(OPERATION, message)})
    for name, message in malformed:
        try:
            read_request(message)
        except Refused as refusal:
            answer = write_answer(refusal.code.status, refusal.code.substatus, b"")
            out.append({"name": name, "kind": "request", "message": message, "refused": refusal.code, "reply": answer})
            continue
        raise AssertionError("the vector %s is accepted" % name)
    return out


# The transport half.

# The codes a client answers its own caller with when the transport failed or the
# answer could not be read, the rows L of the registry.
BAD_GATEWAY = Code(502, 0)
UNKNOWN_OUTCOME = Code(502, 1)
LINK_DOWN = Code(502, 2)
UNKNOWN_STREAM = Code(404, 1)

# The failures of the Homa engine after which the client cannot know whether the
# server ran the request.
_UNKNOWN_AFTER = {"RpcUnknown": UNKNOWN_OUTCOME, "LinkDown": LINK_DOWN, "PeerTimeout": LINK_DOWN}


class Exchange:
    """One exchange of a `Client`: its kind, its Homa RPC and its mark."""

    def __init__(self, kind: int, rpc, repeatable: bool) -> None:
        self.kind = kind
        self.rpc = rpc
        self.repeatable = repeatable

    @property
    def done(self) -> bool:
        """Whether the RPC has ended."""
        return self.rpc.state not in ("out", "in")

    def outcome(self) -> dict:
        """What the exchange gave its caller.

        An answer is read by `read_answer`, a malformed one is 502.0, a request
        the engine refused before it left is `refused` with the engine's reason,
        and a failure of the transport is `unknown` with 502.1 after RPC_UNKNOWN
        and 502.2 after a link down or a dead peer, since the client cannot know
        whether the server ran the request.
        """
        rpc = self.rpc
        if rpc.state == "ok":
            try:
                return read_answer(self.kind, rpc.response)
            except Malformed as error:
                return {"answer": "malformed", "code": BAD_GATEWAY, "reason": error.reason}
        if rpc.state == "failed" and rpc.why in _UNKNOWN_AFTER:
            return {"answer": "unknown", "code": _UNKNOWN_AFTER[rpc.why], "why": rpc.why}
        if rpc.state in ("failed", "refused"):
            return {"answer": "failed", "code": BAD_GATEWAY, "why": rpc.why}
        return {"answer": "pending"}


class Client:
    """A GraphTP client over one engine of tests/harness/homa/client.py.

    The methods `operation`, `pull` and `cancel` send an exchange, `wait` moves
    the engine until exchanges end, and `take` reads an exchange and applies a
    stream answer. The client applies stream answers in the order of `seq`: one
    that is ahead waits until the gap fills, and one that is behind is dropped
    and counted. It starts with the count 0 on a link whose end the server sees,
    and with no count after `forget`, where `start` sends the CANCEL of handle 0
    of the start rule and takes the count from its answer. A stream is `open`,
    `cancelling` once a CANCEL names it, and `complete`, `cancelled`, `aborted`
    or `lost`.
    """

    def __init__(self, engine, count_known: bool = True) -> None:
        self.engine = engine
        self.expected = 0 if count_known else None
        self.held = {}
        self.dropped = 0
        self.applied = []
        self.streams = {}

    # Sending.

    def operation(self, content: bytes, credential: bytes | None = None, repeatable: bool = False) -> Exchange:
        """Send an OPERATION, which reports RPC_UNKNOWN unless it is repeatable."""
        message = write_operation(credential, content)
        rpc = self.engine.request(message, patient=False, report_unknown=not repeatable)
        return Exchange(OPERATION, rpc, repeatable)

    def pull(self) -> Exchange:
        """Send a PULL as a patient RPC, which is always repeatable."""
        return Exchange(PULL, self.engine.request(write_pull(), patient=True, report_unknown=False), True)

    def cancel(self, handle: int) -> Exchange:
        """Send a CANCEL of `handle`, 0 for every stream, always repeatable.

        Each stream the CANCEL names is cancelling from here on, so a PULL that
        meets 404.1 after it is no sign that the server forgot the peer.
        """
        for known, stream in self.streams.items():
            if stream["state"] == "open" and handle in (0, known):
                stream["state"] = "cancelling"
        return Exchange(CANCEL, self.engine.request(write_cancel(handle), report_unknown=False), True)

    # Driving.

    def wait(self, exchanges, timeout: float = 30.0, step: float = 0.002) -> bool:
        """Move the engine until every exchange has ended or `timeout` passed."""
        deadline = time.monotonic() + timeout
        while not all(exchange.done for exchange in exchanges):
            if time.monotonic() > deadline:
                return False
            self.engine.poll(step)
        return True

    def run(self, exchange: Exchange, timeout: float = 30.0) -> dict:
        """Wait for one exchange and take it."""
        self.wait([exchange], timeout)
        return self.take(exchange)

    # Reading.

    def take(self, exchange: Exchange, apply: bool = True) -> dict:
        """Read the outcome of an ended exchange, and apply it when it is a
        stream answer and `apply` holds.

        A PULL refused with 404.1 while the client holds a stream it has not
        cancelled makes the count unknown, since the server forgot the peer,
        and every stream it held is lost with the answers held for them.
        """
        outcome = exchange.outcome()
        if outcome.get("answer") in ("opened", "batch") and apply:
            self.accept(outcome)
        elif (exchange.kind == PULL and outcome.get("answer") == "refused"
              and outcome["code"] == UNKNOWN_STREAM and self.open_streams()):
            self.forget()
        return outcome

    def accept(self, answer: dict) -> None:
        """Apply the stream answer `answer` in the order of `seq`."""
        seq = answer["seq"]
        if self.expected is None:
            self.held[seq] = answer
            return
        if (seq - self.expected) % 2**32 >= 2**31:
            self.dropped += 1
            return
        self.held[seq] = answer
        while self.expected in self.held:
            ready = self.held.pop(self.expected)
            self._apply(ready)
            self.expected = (self.expected + 1) % 2**32

    def forget(self) -> None:
        """Lose the count, the answers held and every stream not ended, as
        after a server that forgot the peer."""
        self.expected = None
        self.held.clear()
        for stream in self.streams.values():
            if stream["state"] in ("open", "cancelling"):
                stream["state"] = "lost"

    def start(self, timeout: float = 30.0) -> dict:
        """Send the CANCEL of handle 0 of the start rule, wait for its answer
        and take the count from it, so the next answer expected is one more."""
        cancel = self.cancel(0)
        self.wait([cancel], timeout)
        outcome = cancel.outcome()
        if outcome.get("answer") == "batch":
            self.held.clear()
            for _, handle in [entry for entry in outcome["entries"] if entry[0] == "cancelled"]:
                self.streams.setdefault(handle, {"next": [], "state": "open", "seq": None})["state"] = "cancelled"
            self.expected = (outcome["seq"] + 1) % 2**32
            self.applied.append((outcome["seq"], None, list(outcome["entries"])))
        return outcome

    def open_streams(self) -> list:
        """The handles of the streams the client holds open."""
        return [handle for handle, stream in self.streams.items() if stream["state"] == "open"]

    def _apply(self, answer: dict) -> None:
        handle = answer.get("stream")
        if handle is not None:
            self.streams[handle] = {"next": [], "state": "open", "seq": answer["seq"]}
        self.applied.append((answer["seq"], handle, list(answer["entries"])))
        for entry in answer["entries"]:
            stream = self.streams.setdefault(entry[1], {"next": [], "state": "open", "seq": None})
            if entry[0] == "next":
                stream["next"].append(entry[2])
            else:
                stream["state"] = {"complete": "complete", "cancelled": "cancelled", "aborted": "aborted"}[entry[0]]


def harness_content(spec: dict | None = None, query: str = "{ok}", pad: int = 0) -> bytes:
    """The content of a request to the service of the card test-graphtp.

    The service reads `extensions.harness`, and `pad` adds a member of that many
    bytes, so a content reaches an exact length.
    """
    extensions = {}
    if spec is not None:
        extensions["harness"] = spec
    if pad:
        extensions["pad"] = "x" * pad
    body = {"query": query}
    if extensions:
        body["extensions"] = extensions
    return json.dumps(body, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def harness_bytes(length: int, seed: int) -> bytes:
    """The `length` bytes the service of the card test-graphtp draws from
    `seed`, the top byte of each step of xorshift64*."""
    mask = 2**64 - 1
    state = max(seed, 1) & mask
    out = bytearray()
    for _ in range(length):
        state ^= state >> 12
        state ^= (state << 25) & mask
        state ^= state >> 27
        out.append(((state * 0x2545F4914F6CDD1D) & mask) >> 56)
    return bytes(out)


def connect(form: str, path, profile=None, max_message: int = 1 << 20, first_id: int = 0x30000002, hook=None,
            count_known: bool = True) -> Client:
    """A client over a new engine connected to the socket at `path`."""
    profile = profile or homa.unix_socket(max_message)
    transport = homa.open_transport(form, path, profile.max_packet, hook)
    local = homa.limits(profile, max_message)
    engine = homa.Engine(transport, profile, local, first_id=first_id)
    return Client(engine, count_known=count_known)
