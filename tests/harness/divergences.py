# SPDX-License-Identifier: MIT
"""Aosabi harness: the named divergences of the side under test a comparison reads as the reference's answer.

A named divergence of docs/byte-equality.md section 3 that changes the bytes of an answer of the side under test
by a fixed rewrite is written here once, so a comparison asserts the side's own form and then reads the answer
as the reference writes it, every other byte held to the reference.

  D52  the title and the default query of GraphiQL, `Aosabi Playground` in the
       three places where the reference's page writes its own name
  D55  the variables of the uploads a delegation forwards, `_aosabiUpload_<n>`
       where the reference's federation writes its own prefix
  D57  an answer of the function below the threshold of compress(), which
       carries its Content-Length and goes out uncoded on every platform,
       where the reference on a Worker and on Bun codes it
  D58  a batch, which the function refuses as Yoga does with batching off,
       where an instance of the reference that enables batching runs it
  D59  no answer as multipart/mixed: the field accept of a 406 lists no
       multipart/mixed, a client that accepts it alone gets that 406, and
       one that accepts an event stream beside it and no JSON gets the
       event stream
  D72  the coding of the base pipeline of a Rust service, which codes no answer
       where the compress() of the reference codes one

  D56  an answer of Yoga's default plugins outside the GraphQL endpoint, the
       health check on a URL that ends in /health and the preflight of CORS
       on another path, which the hono of the app answers on the side ts,
       named only for a request target that `is_endpoint` rejects
  D47  the landing page of an unknown route, on the side ts with no
       `landingPage: true`
  D48  the viewer of `GET /viewer`, on the side ts with no `viewer: true`
  HV1  the field `Vary: Accept-Encoding` compress() of hono 4.13.14 sets on
       every answer of a type it codes, coded or not, where 4.11.9 sets none,
       which a middleware of an app that reads the fields of an answer after
       compress() has run sees as well, the memo of transit-app among them

  D80  the answer of the plugin short of plugins-app, 418 and short, where
       The reference loses it and answers 500 with {"error":"Internal Server Error"},
       row QD-003 of the rated table

and the changes of serveStatic of @hono/node-server 2.1.5, which an app on
Node installs itself since step 2, against the 1.19.9 of the reference's lockfile:

  HN1  a request of OPTIONS, which 2.1.5 hands to the next handler
  HN2  a range 2.1.5 cannot satisfy, answered 416, and a suffix range,
       answered with the last bytes of the file

The secure defaults of B2, D46 to D49 and D60, are the steps of
`served_defaults`, one step per row of the rated table, at the end of the module,
and the rules of the oracle of lane A for D46 to D49 and D57 to D60 close it in
`ORACLE`.
"""

# The module holds no `from __future__ import annotations`: the oracle of lane A loads it by its path under a
# name of its own that sys.modules does not hold, and dataclasses reads a string annotation of a field through
# the module of its class in sys.modules, which fails there, while Python 3.14 defers every annotation without
# it (PEP 649).
import dataclasses
import json
import re
import zlib
from pathlib import Path
from urllib.parse import parse_qs

#: The name of the playground the side under test writes, divergence D52.
PLAYGROUND = b"Aosabi Playground"

#: The name of the playground the reference writes.
PLAYGROUND_REFERENCE = b"Pylon Playground"


def playground_body(body: bytes) -> bytes:
    """Return the body `body` of the side under test with the playground of D52 named as the reference names it."""
    return body.replace(PLAYGROUND, PLAYGROUND_REFERENCE)


def playground_length(value: str, body: bytes) -> str:
    """Return the value `value` of content-length of the body `body` of the side under test as the reference's.

    Each name of D52 in the body is one byte longer than the reference's.
    """
    count = body.count(PLAYGROUND)
    if not count or not value.strip().isdigit():
        return value
    return str(int(value) - count * (len(PLAYGROUND) - len(PLAYGROUND_REFERENCE)))


def playground_answer(answer: dict | None) -> dict | None:
    """Return an answer of the side under test, `{status, headers, body}`, with the playground of D52 as the reference's.

    The headers map a name in lower case to its values, and content-length
    takes the length of the body of the reference.
    """
    if answer is None or not answer.get("body") or PLAYGROUND not in answer["body"]:
        return answer
    headers = dict(answer["headers"])
    if "content-length" in headers:
        headers["content-length"] = [playground_length(value, answer["body"]) for value in headers["content-length"]]
    return dict(answer, headers=headers, body=playground_body(answer["body"]))


def playground_fields(fields: list, body: bytes) -> list:
    """Return the fields `fields`, pairs of a name and a value, of an answer of the side under test as the reference's.

    The body `body` is the decoded body of the side under test, and
    content-length takes the length of the body of the reference.
    """
    return [(name, playground_length(value, body) if name.lower() == "content-length" else value)
            for name, value in fields]


def playground_message(raw: bytes) -> bytes:
    """Return the HTTP/1.1 message `raw` of the side under test with the playground of D52 named as the reference names it.

    The body follows the head whole, framed by content-length, which takes the
    length of the body of the reference. A message framed otherwise keeps its
    bytes, so a comparison reports it.
    """
    head, separator, body = raw.partition(b"\r\n\r\n")
    if not separator or PLAYGROUND not in body:
        return raw
    lines = head.split(b"\r\n")
    framed = False
    for index, line in enumerate(lines[1:], 1):
        name, colon, value = line.partition(b":")
        digits = value.strip()
        if colon and name.strip().lower() == b"content-length" and digits.isdigit():
            length = playground_length(digits.decode("ascii"), body).encode("ascii")
            lines[index] = name + colon + value.replace(digits, length, 1)
            framed = True
    if not framed:
        return raw
    return b"\r\n".join(lines) + separator + playground_body(body)


def playground_reasons(reference: bytes | None, side: bytes | None) -> list:
    """Return why the body `side` of the side under test breaks D52 beside the body `reference` of the reference.

    A page of the reference that names the reference's playground is answered on the
    side under test by a page that names Aosabi's.
    """
    if reference and PLAYGROUND_REFERENCE in reference and PLAYGROUND not in (side or b""):
        return ["the page of GraphiQL names no Aosabi Playground, divergence D52"]
    return []


#: The prefix of the variable of an upload a delegation forwards on the side under test, divergence D55.
UPLOAD = "_aosabiUpload_"

#: The prefix the federation of the reference writes.
UPLOAD_REFERENCE = "_pylonUpload_"


def upload_text(text: str) -> str:
    """Return the text `text` of the side under test with the variables of the uploads of D55 as the reference names them."""
    return text.replace(UPLOAD, UPLOAD_REFERENCE)


#: The codings the compress() of the reference writes, ENCODING_TYPES of hono 4.11.9, and x-gzip, the alias of
#: gzip (RFC 9110 section 8.4.1.3).
CODED = ("gzip", "x-gzip", "deflate")


def served_coding(reference: str | None) -> str | None:
    """Return the content-encoding the twin of a Rust service writes where the reference writes `reference`, divergence D72.

    The base pipeline of a Rust service codes no answer, so where the
    compress() of the reference coded the twin answers uncoded. Any other
    value, the `none` Yoga writes on an event stream among them, is the
    reference's.
    """
    if reference is not None and reference.strip().lower() in CODED:
        return None
    return reference


#: The body, the status and the type of the answer of hono to a request no route answers, its default not found
#: handler, `c.text("404 Not Found", 404)`.
HONO_NOT_FOUND = (404, b"404 Not Found", "text/plain; charset=UTF-8")

#: The field of the health check of Yoga, `x-yoga-id`, on its answer of status 200 and no body.
HEALTH_FIELD = "x-yoga-id"

#: The type of the landing page of the reference, by which an answer to HEAD, which holds no body, names it.
LANDING_TYPE = "text/html; charset=UTF-8"

#: The title of the viewer page of the reference's useViewer.
VIEWER_TITLE = b"<title>Pylon Viewer</title>"


class Answer:
    """An answer read in process, with the members of an answer of tests/harness/http.py this module reads."""

    def __init__(self, status: int, headers: list, decoded: bytes):
        self.status = status
        self.header_lines = [(name, value) for name, value in headers]
        self.decoded = decoded


def _values(answer, name: str) -> list:
    """Every value of the field `name` of `answer`, an answer of tests/harness/http.py, in order."""
    return [value for key, value in answer.header_lines if key.lower() == name]


def body_of(answer) -> bytes:
    """Return the body of `answer` with its coding undone, and a second gzip a host wrote over the first.

    workerd codes the body of an answer whose field Content-Encoding a Worker names, the default `encodeBody` of
    a Response there, so a body that compress() of hono coded leaves a Worker coded twice under one field,
    the reference's own build and the side ts alike, and the second gzip is undone as the first.
    """
    body = answer.decoded
    if [value.strip().lower() for value in _values(answer, "content-encoding")] == ["gzip"] and body[:2] == b"\x1f\x8b":
        try:
            return zlib.decompress(body, 16 + zlib.MAX_WBITS)
        except zlib.error:
            return body
    return body


def hono_not_found(answer) -> bool:
    """Return whether `answer` is the answer of hono's default not found handler.

    An answer to HEAD holds no body, and its length is the one of the body, none or 0, as the host writes it.
    """
    status, body, media = HONO_NOT_FOUND
    if answer is None or answer.status != status or _values(answer, "content-type") != [media]:
        return False
    held = body_of(answer)
    return held == body or (held == b"" and _values(answer, "content-length") in ([], ["0"], [str(len(body))]))


#: The two endings of the GraphQL endpoint of the reference's handler, `/graphql` with and without its slash.
ENDPOINT_ENDINGS = ("/graphql", "/graphql/")


def _path_of(target: str) -> str:
    """The path of the request target `target`, its query and its fragment cut and its dot segments removed.

    The path is the one `new URL(url).pathname` answers for the URL of the
    target in origin form, as the harness sends it, its dot segments, `%2e`
    for a dot among them, removed by the path state of the URL standard.
    """
    path = target.split("#", 1)[0].split("?", 1)[0]
    out, last = [], ""
    for segment in path.split("/")[1:]:
        last = segment.lower().replace("%2e", ".")
        if last == "..":
            if out:
                out.pop()
        elif last != ".":
            out.append(segment)
    joined = "/" + "/".join(out)
    return joined + "/" if last in (".", "..") and not joined.endswith("/") else joined


def is_endpoint(target: str) -> bool:
    """Return whether the request target `target` names the GraphQL endpoint, as `isEndpoint` tests its URL.

    A target that ends in /graphql or /graphql/, or whose path is one of the two, which the engine answers on
    the side ts with every method, while the hono of the app answers every other target.
    """
    return target.endswith(ENDPOINT_ENDINGS) or _path_of(target) in ENDPOINT_ENDINGS


def outside_endpoint(reference, viewer: bool = False, target: str | None = None) -> str | None:
    """`target` is the request target of the answer. A request with no target
    names none. A request of the endpoint names D46 for the preflight of
    CORS of Yoga, status 204 and `content-length: 0`, since the engine
    answers no `OPTIONS` request and hands it on as an unhandled route,
    and names nothing else, since the engine answers every other request of
    the endpoint on the side ts. For a target that `is_endpoint` rejects,
    D56 for the health check of Yoga, status 200, no body and the field
    x-yoga-id, and for the preflight of CORS of Yoga outside the endpoint,
    status 204 and `content-length: 0`, D47 for the landing page, read by its
    title or, for an answer to HEAD, by its status and its type, and D48 for
    the viewer, `viewer` true for a request of `GET /viewer`. The side ts
    answers each with the not found handler of hono, `hono_not_found`.
    """
    if reference is None or target is None:
        return None
    preflight = reference.status == 204 and _values(reference, "content-length") == ["0"]
    if is_endpoint(target):
        return "D46" if preflight else None
    body = body_of(reference)
    if reference.status == 200 and not body and _values(reference, HEALTH_FIELD):
        return "D56"
    if preflight:
        return "D56"
    if LANDING_REFERENCE_TITLE in body:
        return "D47"
    if reference.status == 404 and not body and _values(reference, "content-type") == [LANDING_TYPE]:
        return "D47"
    if viewer or VIEWER_TITLE in body:
        return "D48"
    return None


#: The answer of the reference where its dispatch of the middleware of the plugins loses the answer of a later
#: middleware, the error answer of its base pipeline to the error hono throws for a context with no answer.
LOST_ANSWER = (500, b'{"error":"Internal Server Error"}')

KEPT_ANSWER = (418, b"short", "text/plain; charset=UTF-8")


def kept_plugin_answer(reference, ours) -> str | None:
    """Return D80 where the reference answers `reference`, the 500 of a lost answer, and the side `ours` the answer it keeps.

    The side ts hands hono the answer of a plugin's middleware behind middlewares that await the rest of the
    chain and return nothing, which the reference's dispatch loses. Only the answer of the plugin short, its status, its
    body and its type, names the divergence, so any other answer in that place is held to the reference as every other
    answer.
    """
    if reference is None or ours is None or (reference.status, body_of(reference)) != LOST_ANSWER:
        return None
    status, body, media = KEPT_ANSWER
    if ours.status != status or body_of(ours) != body or _values(ours, "content-type") != [media]:
        return None
    return "D80"


#: The value hono 4.13.14 adds to the field Vary of an answer its compress() may code, change HV1.
VARY_ENCODING = "Accept-Encoding"


def hono_vary(side: list, reference: list) -> list:
    """Return the values `side` of the field Vary of the side ts read as hono 4.11.9 writes them, change HV1.

    compress() of hono 4.13.14 sets `Vary: Accept-Encoding` on an answer whose type it codes, coded or not,
    appending the name to a Vary that holds another, where compress() of 4.11.9 sets none. Where the values of
    the reference name no Accept-Encoding, the name is taken out of the values of the side, a field of that one
    name with it.
    """
    if any(VARY_ENCODING.lower() in value.lower() for value in reference):
        return side
    out = []
    for value in side:
        names = [part.strip() for part in value.split(",") if part.strip().lower() != VARY_ENCODING.lower()]
        if names:
            out.append(", ".join(names))
    return out


#: The field in which the memo of transit-app writes the names of the fields of the c.res it kept, after the
#: compress() of the base pipeline, which the module of the app stands before, has run.
MEMO_FIELD = "x-transit-memo"

#: The marker after the names of the fields in a value of `MEMO_FIELD`,
#: `<mode> <status> <type> <names> same=<bool> bytes=<length>`, the names joined by commas.
MEMO_SAME = " same="


def _memo_names(value: str) -> list:
    """The names of the fields a value of `MEMO_FIELD` lists, none for a value that lists no names."""
    head, same, _ = value.rpartition(MEMO_SAME)
    return head.rpartition(" ")[2].split(",") if same else []


def hono_memo(side: list, reference: list) -> list:
    """Return the values `side` of the field of the memo of transit-app read as hono 4.11.9 writes them, change HV1.

    The memo writes the names of the fields of the c.res it kept after the
    compress() of the base pipeline has run, which the module of the app
    stands before, so compress() of hono 4.13.14 adds vary to those names
    where 4.11.9 adds none. Where no value of the reference names vary, the
    name is taken out of the names of each value of the side, and every other
    byte of the value stays.
    """
    if any("vary" in _memo_names(value) for value in reference):
        return side
    out = []
    for value in side:
        head, same, tail = value.rpartition(MEMO_SAME)
        if not same:
            out.append(value)
            continue
        before, space, names = head.rpartition(" ")
        kept = [name for name in names.split(",") if name != "vary"]
        out.append(before + space + ",".join(kept) + same + tail)
    return out


def hono_headers(side: list, reference: list) -> tuple:
    """Return the header fields `side` of the side ts read as hono 4.11.9 writes them, and how many were read so.

    Each field is a pair of its name and its value, `reference` the fields
    of the answer of the reference. The values of Vary are read through
    `hono_vary`, change HV1, and so is the list of names of the field of the
    memo of transit-app, `MEMO_FIELD`, through `hono_memo`.
    """
    reference_vary = [value for name, value in reference if name.lower() == "vary"]
    reference_memo = [value for name, value in reference if name.lower() == MEMO_FIELD]
    out, read = [], 0
    for name, value in side:
        if name.lower() == "vary":
            kept = hono_vary([value], reference_vary)
            read += kept != [value]
            out.extend([name, item] for item in kept)
        elif name.lower() == MEMO_FIELD:
            kept = hono_memo([value], reference_memo)
            read += kept != [value]
            out.append([name, kept[0]])
        else:
            out.append([name, value])
    return out, read


#: The pattern of a suffix range, `bytes=-<n>`, and of the field Content-Range of a range answered 416.
SUFFIX_RANGE = re.compile(r"^bytes=-(\d+)$")
UNSATISFIED_RANGE = re.compile(r"^bytes \*/\d+$")


def node_server_change(method: str, headers, reference, side) -> str | None:
    """Return the change of serveStatic of @hono/node-server 2.1.5 against 1.19.9 that the answer `side` shows, or None.

    `method` and `headers`, pairs of a name and a value, are those of the request, `reference` the answer of
    the reference and `side` the answer of the side ts. HN1 is a request of OPTIONS, which 1.19.9 answers 200 with the
    type of the file and no body and 2.1.5 hands to the next handler, the not found handler of hono where no
    route answers. HN2 is a range: one 2.1.5 cannot satisfy it answers 416 with `Content-Range: bytes */<size>`
    and no body, where 1.19.9 fails with 500, and the suffix range `bytes=-<n>` it answers with the last n
    bytes, where 1.19.9 answers the bytes 0 to n.
    """
    if reference is None or side is None:
        return None
    if method == "OPTIONS" and reference.status == 200:
        return "HN1" if hono_not_found(side) else None
    asked = [value for name, value in headers if name.lower() == "range"]
    if not asked:
        return None
    ranges = _values(side, "content-range")
    if (reference.status == 500 and side.status == 416 and side.decoded == b"" and len(ranges) == 1
            and UNSATISFIED_RANGE.match(ranges[0])):
        return "HN2"
    suffix = SUFFIX_RANGE.match(asked[0])
    if suffix and side.status == 206 and len(ranges) == 1:
        length = int(suffix.group(1))
        size = int(ranges[0].rsplit("/", 1)[1])
        if (ranges[0] == "bytes %d-%d/%d" % (size - length, size - 1, size) and len(side.decoded) == length
                and _values(reference, "content-range") == ["bytes 0-%d/%d" % (length, size)]):
            return "HN2"
    return None


#: The threshold of compress(), the length below which an answer that carries its Content-Length stays uncoded.
COMPRESS_THRESHOLD = 1024

#: The media types of the answers of the regular processor of the function, whose body is a string.
GRAPHQL_TYPES = ("application/json", "application/graphql-response+json")


def d57_names(named: list, ours, host) -> list:
    """Return the names `named` of a served answer with those of divergence D57 named `D57`.

    `named` are the differences `ts1.judge_served` names where the side under
    test answers as the reference on Node and the reference on its host does not, each opening
    with its field, and `ours` and `host` are the answers of the side and of
    the reference's build on the host, answers of `http.py`. The function writes the
    Content-Length of every answer whose body is a string on every platform,
    so compress() leaves an answer below 1,024 bytes uncoded where the reference on a
    Worker and on Bun codes it, and the answer carries its length where
    the reference's there carries none. A name is D57 when the side's answer is one of
    the regular processor and the field is its coding, none on the side and
    one on the host with a body below the threshold, or its length, the length
    of its body on the side and none on the host, or on an answer of HEAD, which
    leaves its body out, a length on the side where the host sends none or 0.
    """
    if ours is None or host is None:
        return named
    media_type = (ours.header("content-type") or "").split(";")[0].strip().lower()
    if media_type not in GRAPHQL_TYPES:
        return named
    uncoded = ours.header("content-encoding") is None and host.header("content-encoding") is not None
    uncoded = uncoded and len(ours.decoded) < COMPRESS_THRESHOLD
    length = ours.header("content-length") == str(len(ours.decoded)) and host.header("content-length") is None
    # An answer of HEAD leaves its body out and keeps the length of the body.
    head = not ours.decoded and ours.header("content-length") not in (None, "0")
    length = length or (head and host.header("content-length") in (None, "0"))
    labelled = []
    for entry in named:
        field = entry.split(" ", 1)[0]
        if (field == "content-encoding" and uncoded) or (field == "content-length" and length):
            entry = "D57 " + entry
        labelled.append(entry)
    return labelled


#: The instances of the in process capture whose reference enables batching, divergence D58.
BATCHING_INSTANCES = ("batching", "secondBatchingLimit2", "secondBatchingTrue")

#: The body of the refusal of a batch, which Yoga answers with batching off and the function to every batch.
BATCH_REFUSAL = b'{"errors":[{"message":"Batching is not supported.","extensions":{"code":"BAD_REQUEST"}}]}'


def batch_refused(instance: str, body) -> bool:
    """Return whether the in process case of `instance` with the body `body` is a batch the function refuses, D58.

    The reference runs a JSON array body as a batch on an instance that enables
    batching, and the function refuses every batch.
    """
    return instance in BATCHING_INSTANCES and (body or "").lstrip().startswith("[")


def batch_refusal() -> dict:
    """Return the answer of the function to a batch of the in process capture, `{status, status_text, headers, body}`.

    It is the refusal Yoga answers with batching off, capture row C05, as the
    function answers it under `application/json`.
    """
    return {"status": 400, "status_text": "Bad Request",
            "headers": {"Content-Type": "application/json; charset=utf-8", "content-length": str(len(BATCH_REFUSAL))},
            "body": BATCH_REFUSAL}


#: The media type of the processor that leaves the function, divergence D59.
MULTIPART = "multipart/mixed"

#: The media type of the processor of an event stream, which answers in place of multipart/mixed where the request accepts it.
EVENT_STREAM = "text/event-stream"

#: The text a 406 joins the media types of its field accept with.
ACCEPT_JOIN = "; charset=utf-8, "

#: The media types of the processors of the function in the order of the choice, an event stream and JSON.
PROCESSOR_TYPES = (("text/event-stream",), ("application/graphql-response+json", "application/json"))

#: The content type the served function gives a 406 on Node, as the reference's adapter writes it.
NOT_ACCEPTABLE_TYPE = "text/plain; charset=UTF-8"

#: The fields Yoga's processor of an event stream sets, in its order.
EVENT_STREAM_FIELDS = (("Content-Type", "text/event-stream"), ("Connection", "keep-alive"),
                       ("Cache-Control", "no-cache"), ("Content-Encoding", "none"))

#: The fields Yoga's processor of multipart/mixed sets.
MULTIPART_FIELDS = ("Connection", "Content-Type", "Transfer-Encoding")

#: The codes of the errors Yoga marks `spec`, a parse error and a validation error.
SPEC_CODES = ("GRAPHQL_PARSE_FAILED", "GRAPHQL_VALIDATION_FAILED")

#: The refusals of a request Yoga marks `spec` under the code BAD_REQUEST.
SPEC_REFUSALS = ("POST body sent invalid JSON.", 'Invalid "params" in the request body', "Must provide query string.")


def accept_without_multipart(value: str) -> str:
    """Return the field accept of a 406 of the reference as the function writes it, without multipart/mixed, D59."""
    return ACCEPT_JOIN.join(item for item in value.split(ACCEPT_JOIN) if item != MULTIPART)


def answers_multipart(content_type) -> bool:
    """Return whether an answer of the type `content_type` is the answer of the processor of multipart/mixed."""
    return (content_type or "").split(";")[0].strip().lower() == MULTIPART


def media_types_in_order(accept) -> list:
    """Return the media types of the Accept `accept`, the last listed first, as Yoga's accept.js negotiates them.

    Every whitespace character leaves, the text is lowercased and split on
    commas, an entry whose charset is not utf-8 is dropped, and the list is
    reversed.
    """
    text = "".join(c for c in (accept or "*/*") if not c.isspace()).lower()
    types = []
    for entry in text.split(","):
        parts = entry.split(";")
        charset = next((part for part in parts[1:] if "charset=" in part), "charset=utf-8")
        if charset == "charset=utf-8":
            types.append(parts[0])
    return types[::-1]


def accepts_event_stream(accept) -> bool:
    """Return whether the Accept `accept` holds a media type that matches text/event-stream, D59.

    Yoga matches a media type of the request against the type of a processor when each half of the request's is
    `*` or equal to the processor's.
    """
    for media in media_types_in_order(accept):
        halves = media.split("/")
        minor = halves[1] if len(halves) > 1 else None
        if halves[0] in ("*", "text") and minor in ("*", "event-stream"):
            return True
    return False


def not_acceptable(accept) -> str:
    """Return the field accept of the 406 the function answers a query asked for under the Accept `accept`, D59.

    Each processor offers its media types once per media type of the request,
    an event stream first and JSON after it, joined as Yoga joins them.
    """
    count = len(media_types_in_order(accept))
    return ACCEPT_JOIN.join(media for types in PROCESSOR_TYPES for _ in range(count) for media in types)


def multipart_chunks(body: bytes) -> list:
    """Return the JSON texts of the parts of a body of Yoga's processor of multipart/mixed, in order, D59.

    The body opens with a line break and `---`, each part is a line break, its fields Content-Type and
    Content-Length, a blank line, the text of that length, a line break and `---`, and `--` with a line break
    ends it.
    """
    if not body.startswith(b"\r\n---"):
        raise ValueError("no body of the processor of multipart/mixed: %r" % body[:40])
    chunks = []
    at = len(b"\r\n---")
    while body.startswith(b"\r\n", at):
        fields_end = body.index(b"\r\n\r\n", at)
        fields = body[at + 2:fields_end].split(b"\r\n")
        length = next(int(field.split(b":", 1)[1]) for field in fields if field.lower().startswith(b"content-length:"))
        start = fields_end + 4
        chunks.append(body[start:start + length])
        at = start + length + len(b"\r\n---")
    if body[at:] != b"--\r\n":
        raise ValueError("the body of the processor of multipart/mixed ends in %r" % body[at:])
    return chunks


def event_stream_body(chunks) -> bytes:
    """Return the body Yoga's processor of an event stream writes for the results `chunks`, D59.

    It opens with the ping `:` and a blank line, writes each result as the event `next` with its text as data,
    and ends with the event `complete`.
    """
    return b":\n\n" + b"".join(b"event: next\ndata: " + chunk + b"\n\n" for chunk in chunks) + b"event: complete\ndata:\n\n"


def spec_error(error) -> bool:
    """Return whether the error `error` of a result, as JSON, is one Yoga marks `spec`, by its code or its refusal."""
    if not isinstance(error, dict):
        return False
    code = (error.get("extensions") or {}).get("code")
    return code in SPEC_CODES or (code == "BAD_REQUEST" and error.get("message") in SPEC_REFUSALS)


def event_stream_status(status: int, chunks) -> int:
    """Return the status of the event stream of the results `chunks` whose answer as multipart/mixed has `status`, D59.

    Yoga's processor of an event stream leaves the errors marked `spec` out of the status, which the processor
    of multipart/mixed counts. So a result whose every error is marked `spec` answers 200, and every other
    result keeps the status of its answer as multipart/mixed.
    """
    errors = []
    for chunk in chunks:
        try:
            errors += json.loads(chunk).get("errors") or []
        except (ValueError, AttributeError):
            return status
    if errors and all(spec_error(error) for error in errors):
        return 200
    return status


def accept_406(answer):
    """Return the answer `answer` of the reference, an answer of `http.py`, with the field accept of a 406 without multipart/mixed, D59."""
    if answer is None or answer.status != 406:
        return answer
    lines = [(name, accept_without_multipart(value) if name.lower() == "accept" else value)
             for name, value in answer.header_lines]
    if lines == list(answer.header_lines):
        return answer
    return dataclasses.replace(answer, header_lines=lines)


def served_event_stream(answer):
    """Return the answer `answer` of the processor of multipart/mixed, an answer of `http.py`, as the event stream the function answers in its place, D59.

    The status follows `event_stream_status`, the fields of Yoga's processor of
    multipart/mixed give way to those of its processor of an event stream,
    every other field of the reference is kept, and the body is the event
    stream of the parts of the reference's body.
    """
    chunks = multipart_chunks(answer.decoded)
    body = event_stream_body(chunks)
    replaced = ("content-type", "connection", "cache-control", "content-encoding")
    lines = [(name.lower(), value) for name, value in EVENT_STREAM_FIELDS]
    lines += [(name, value) for name, value in answer.header_lines if name.lower() not in replaced]
    return dataclasses.replace(answer, status=event_stream_status(answer.status, chunks), header_lines=lines,
                               wire_body=body, payload=body, decoded=body)


def multipart_reasons(reference, ours, accept) -> list | None:
    """Return why the answer `ours` breaks D59 where the reference answered `reference` as multipart/mixed, or None.

    None is a reference answer that is no multipart answer, which the caller
    compares as before. Where the reference answered multipart/mixed and the
    Accept `accept` of the request holds an event stream, the function answers
    the event stream of `served_event_stream`, whose status, type and body the
    answer holds. Where it holds none, the function answers the 406 of a type
    it does not serve: the status 406, the field accept of `not_acceptable` for
    the Accept and an empty body.
    """
    if reference is None or not answers_multipart(reference.header("content-type")):
        return None
    if accepts_event_stream(accept):
        expected = served_event_stream(reference)
        if ours is None:
            return ["no answer where the function answers an event stream, D59"]
        reasons = []
        if ours.status != expected.status:
            reasons.append("status %d where the event stream of D59 answers %d" % (ours.status, expected.status))
        if (ours.header("content-type") or "").split(";")[0].strip().lower() != EVENT_STREAM:
            reasons.append("content-type %r where D59 answers %s" % (ours.header("content-type"), EVENT_STREAM))
        if ours.decoded != expected.decoded:
            reasons.append("a body of %d bytes where the event stream of D59 holds %d: %r | %r"
                           % (len(ours.decoded), len(expected.decoded), ours.decoded[:120], expected.decoded[:120]))
        return reasons
    if ours is None:
        return ["no answer where the function answers 406, D59"]
    reasons = []
    if ours.status != 406:
        reasons.append("status %d where the function answers 406, D59" % ours.status)
    if ours.header("accept") != not_acceptable(accept):
        reasons.append("accept %r where D59 lists %r" % (ours.header("accept"), not_acceptable(accept)))
    if ours.decoded:
        reasons.append("a body of %d bytes where the 406 of D59 has none" % len(ours.decoded))
    return reasons


def served_d59(answer, accept):
    """Return the answer `answer` of the reference served on Node, an answer of `http.py`, as the function answers it, D59.

    A 406 lists no multipart/mixed in its field accept. An answer of the
    processor of multipart/mixed becomes the event stream of
    `served_event_stream` where the Accept `accept` of the request holds an
    event stream, and otherwise the 406 of a type the function does not serve,
    with the field accept of `not_acceptable` for the Accept, the content type
    the reference's adapter gives a 406 on Node and no body, every other field of the
    reference kept.
    """
    if answer is None:
        return answer
    if answers_multipart(answer.header("content-type")):
        if accepts_event_stream(accept):
            return served_event_stream(answer)
        dropped = ("accept", "content-type", "connection", "transfer-encoding", "content-length")
        lines = [("accept", not_acceptable(accept)), ("content-type", NOT_ACCEPTABLE_TYPE)]
        lines += [(name, value) for name, value in answer.header_lines if name.lower() not in dropped]
        return dataclasses.replace(answer, status=406, header_lines=lines, wire_body=b"", payload=b"", decoded=b"")
    return accept_406(answer)


def in_process_event_stream(expected: dict) -> dict:
    """Return the recorded answer `expected` of the processor of multipart/mixed, `{status, status_text, headers, body}`, as the event stream the function answers in its place, D59.

    The fields of Yoga's processor of an event stream come first in its order,
    and every field of the recorded answer that the processor of
    multipart/mixed did not set follows as Yoga assigns it, the status follows
    `event_stream_status` and the body is the event stream of the parts of the
    recorded body.
    """
    chunks = multipart_chunks(expected["body"])
    status = event_stream_status(expected["status"], chunks)
    headers = dict(EVENT_STREAM_FIELDS)
    for name, value in expected["headers"].items():
        if name not in MULTIPART_FIELDS:
            headers[name] = value
    # The status changes to 200 alone, whose text the Response of the capture writes as OK.
    status_text = expected["status_text"] if status == expected["status"] else "OK"
    return {"status": status, "status_text": status_text, "headers": headers, "body": event_stream_body(chunks)}


def in_process_d59(request_headers: dict, expected: dict) -> dict:
    """Return the recorded answer `expected` of an in process case, `{status, status_text, headers, body}`, as the function answers it, D59.

    A 406 lists no multipart/mixed in its field accept. An answer of the
    processor of multipart/mixed becomes the event stream of
    `in_process_event_stream` where the Accept of `request_headers` holds an
    event stream, and otherwise the 406 of a type the function does not serve,
    its field accept the list of `not_acceptable` for that Accept.
    """
    headers = expected["headers"]
    content_type = next((value for name, value in headers.items() if name.lower() == "content-type"), None)
    if answers_multipart(content_type):
        accept = next((value for name, value in request_headers.items() if name.lower() == "accept"), None)
        if accepts_event_stream(accept):
            return in_process_event_stream(expected)
        return {"status": 406, "status_text": "Not Acceptable", "headers": {"accept": not_acceptable(accept)},
                "body": b""}
    if expected["status"] == 406:
        rewritten = {name: accept_without_multipart(value) if name.lower() == "accept" else value
                     for name, value in headers.items()}
        if rewritten != headers:
            return dict(expected, headers=rewritten)
    return expected


# A side under test answers only what its configuration turns on, and each row
# of the rated table that brings one of those defaults is one step below, which
# rewrites an answer of the reference into the answer of the side under test.
# `served_defaults`, `golden_defaults` and `in_process_defaults` run the steps
# in their order over the three forms of an answer the notebooks hold: an answer
# of `http.py`, a recorded golden `{status, headers, body}` and a recorded answer
# of the in process capture `{status, status_text, headers, body}`.

#: The root of the tree whose landing template the steps read.
ROOT = Path(__file__).resolve().parents[2]

#: The keys of the variants of docs/harness.md section 8 that tests/sidecar/config-overlay.mjs spreads over
#: the reference, with the twins of the fixtures that select them through AOSABI_CONFIG setting the same.
OVERLAY_KEYS = {
    "graphiql_false": {"graphiql": False},
    "landing_false": {"landingPage": False},
    "both": {"graphiql": False, "landingPage": False},
    "switches_on": {"landingPage": True, "graphiql": True, "multipart": True, "viewer": True},
    "cors_star": {"cors": {"origin": "https://*.example.invalid"}},
    "cors_list": {"cors": {"origin": ["https://a.example.invalid", "https://b.example.invalid"]}},
}

#: The keys the own configuration of a fixture sets on both sides.
FIXTURE_KEYS = {
    "blog-app": {"graphiql": False, "landingPage": False},
}

#: The keys the twin of a fixture sets beyond the reference's configuration, by fixture.
TWIN_KEYS = {
    # The twins that take uploads set `multipart: true`, D49.
    "surface-app": {"multipart": True},
    "federation-catalogue-app": {"multipart": True},
    "federation-boundary-app": {"multipart": True},
    "docs-app": {"multipart": True},
}

#: The keys that reach the module of the side ts, the bytes of the boot record of crates/aosabi-js/src/boot.rs.
TS_KEYS = ("graphiql", "landingPage")

#: The keys of the instances of the in process capture, which set the option graphiql themselves and leave
#: multipart and cors unset.
IN_PROCESS_KEYS = {"graphiql": True}

#: The keys a boot in process of the twin carries in step 7 of 60_plugins, one entry per row: the switches
#: of the secure defaults that turn on a plugin the reference adds under every configuration, so the twin boots the
#: plugins of the reference's boot.
BOOT_KEYS = {
    # D47: the landing page, which the reference adds unless the key is `false`.
    "landing_page": True,

    # D48: the viewer, which the reference adds under every configuration.
    "viewer": True,
}


#: The key under which `keys_of` hands the steps the middleware cors() of the app of a fixture, its entries of
#: `APP_CORS`, which is source of the app and no key of a configuration.
APP_CORS_KEY = "appCors"


def keys_of(fixture: str, variant: str, side: str = "rs") -> dict:
    """Return the keys of the configuration the side under test serves `fixture` under `variant` with.

    The keys of the fixture's own configuration and of the variant hold on both
    sides. The twin of the side rs adds the keys of `TWIN_KEYS`, and the module
    of the side ts reads the keys of `TS_KEYS` alone. The middleware cors() of
    hono that the app of the fixture puts before the routes of the function
    holds on both sides, under `APP_CORS_KEY`.
    """
    keys = dict(FIXTURE_KEYS.get(fixture, {}))
    keys.update(OVERLAY_KEYS.get(variant, {}))
    if side == "ts":
        keys = {name: value for name, value in keys.items() if name in TS_KEYS}
    else:
        keys.update(TWIN_KEYS.get(fixture, {}))
    if fixture in APP_CORS:
        keys[APP_CORS_KEY] = APP_CORS[fixture]
    return keys


@dataclasses.dataclass
class Ask:
    """A request as the steps read it: its method, its target, its fields in order and where it was answered.

    `served` holds where a web server answered it around the function, as
    the reference's hono does, and fails for the in process capture. `node` holds where
    the adapter of Node wrote the answer, which gives an answer of no body and
    no type `text/plain; charset=UTF-8`, and fails for Bun and workerd, which
    give it none.
    """

    method: str
    target: str
    headers: list
    served: bool = True
    node: bool = True

    def header(self, name: str):
        """The first value of the field `name`, compared case folded, or `None`."""
        for key, value in self.headers:
            if key.lower() == name.lower():
                return value
        return None

    @property
    def path(self) -> str:
        """The path of the target without its query."""
        return self.target.split("?")[0]


@dataclasses.dataclass
class Reply:
    """An answer as the steps read and write it: its status, its reason, its fields in order and its body."""

    status: int
    reason: str
    lines: list
    body: bytes

    def header(self, name: str):
        """The first value of the field `name`, compared case folded, or `None`."""
        for key, value in self.lines:
            if key.lower() == name.lower():
                return value
        return None


@dataclasses.dataclass(frozen=True)
class AppCors:
    """The middleware cors() of hono that an app puts on paths of its own before the routes of the function.

    It answers a preflight on those paths itself, sets its fields before the route on every other answer there,
    and appends `Origin` to `vary` after the route where its option origin is no star. `paths` holds the paths
    it runs on, one that ends in `/*` a prefix, `origin` the rule of its option origin, which takes the origin a
    request sends and the request and gives the origin it allows or `None`, `credentials` whether it sends
    `Access-Control-Allow-Credentials`, and `seen` the fields of the app whose values list the names of the
    fields of the answer of a route, as a middleware after it reads them after `next()`.
    """

    paths: tuple
    origin: object
    credentials: bool = False
    seen: tuple = ()

    def guards(self, path: str) -> bool:
        """Return whether the middleware runs on a request for `path`."""
        for guarded in self.paths:
            if guarded.endswith("/*") and (path == guarded[:-2] or path.startswith(guarded[:-1])):
                return True
            if path == guarded:
                return True
        return False


def listed(*origins):
    """Return the rule of an option origin of hono's cors() that lists `origins`, which allows an origin among them."""
    return lambda origin, ask: origin if origin in origins else None


def transit_origin(origin: str, ask: Ask):
    """Return the origin the rule allowOrigin of transit-app allows for `origin` and `ask`, or `None`."""
    if origin.endswith(".example.invalid"):
        return origin
    return "*" if ask.header("x-transit-origin") == "any" else None


#: The middleware cors() of hono that the app of a fixture puts before the routes of the function, by fixture.
#: The middleware of /graphql of transit-app keeps a memo whose field X-Transit-Memo lists the names of the
#: fields of the answer it read.
APP_CORS = {
    "transit-app": (
        AppCors(("/graphql",), transit_origin, credentials=True, seen=("x-transit-memo",)),
        AppCors(("/api/*",), listed("https://a.example.invalid", "https://b.example.invalid")),
        AppCors(("/login/*",), listed("https://sso.example.invalid", "https://app.example.invalid")),
    ),
    "lines-catalogue-app": (
        AppCors(("/graphql", "/catalogue-api/graphql"),
                listed("https://catalogue.example.invalid", "https://desk.catalogue.example.invalid"),
                credentials=True),
    ),
}


def app_cors(ask: Ask, keys: dict):
    """Return the middleware cors() of the app among `keys` that runs on `ask`, or `None`."""
    return next((entry for entry in keys.get(APP_CORS_KEY, ()) if entry.guards(ask.path)), None)


#: The fields of an answer its body sets, which an answer of another body replaces.
BODY_FIELDS = ("content-type", "content-length", "content-encoding")


#: The fields Yoga's plugin of CORS sets, in lower case.
CORS_FIELDS = ("access-control-allow-origin", "access-control-allow-credentials", "access-control-allow-methods",
               "access-control-allow-headers", "access-control-expose-headers", "access-control-max-age")

#: The names the plugin of CORS writes into `vary`.
CORS_VARY = ("Origin", "Access-Control-Request-Headers")

#: The content type and the body of the not found answer of hono, `c.text('404 Not Found', 404)`.
TEXT_TYPE = "text/plain; charset=UTF-8"
NOT_FOUND_BODY = b"404 Not Found"

#: The version the landing page of the reference names, quirk Q04.
LANDING_VERSION = "2.9.5"


def landing_page() -> bytes:
    """Return the landing page of the reference, the template of aosabi-landing with its version, and Q04."""
    template = (ROOT / "plugins" / "aosabi-landing" / "src" / "landing.html.tmpl").read_text(encoding="utf-8")
    return template.replace("${version}", LANDING_VERSION, 1).encode("utf-8")


def is_cors_line(name: str, value: str) -> bool:
    """Return whether the field `name` with `value` is one the plugin of CORS sets."""
    if name.lower() in CORS_FIELDS:
        return True
    return name.lower() == "vary" and all(part.strip() in CORS_VARY for part in value.split(","))


def cors_lines(reply: Reply) -> list:
    """Return the fields of CORS of `reply` in their order."""
    return [(name, value) for name, value in reply.lines if is_cors_line(name, value)]


def without_cors(reply: Reply) -> Reply:
    """Return `reply` without its fields of CORS."""
    lines = [(name, value) for name, value in reply.lines if not is_cors_line(name, value)]
    return reply if lines == reply.lines else dataclasses.replace(reply, lines=lines)


def compress_coding(ask: Ask):
    """Return the coding the reference's compress() gives an answer that carries no length, or None.

    hono's compress() takes the first of gzip and deflate that the field Accept-Encoding holds as a substring,
    and codes no answer of HEAD.
    """
    if ask.method == "HEAD":
        return None
    accepted = ask.header("accept-encoding") or ""
    return next((coding for coding in ("gzip", "deflate") if coding in accepted), None)


def fallthrough(ask: Ask, landing: bool) -> Reply:
    """Return the answer of the reference to `ask` on a route no handler answers.

    It is the landing page where `landing` holds and the not found answer of hono otherwise, status 404 with its
    type, coded as compress() codes it, with its length where it goes out uncoded, and with no body for HEAD.
    """
    body = landing_page() if landing else NOT_FOUND_BODY
    lines = [("content-type", LANDING_TYPE if landing else TEXT_TYPE)]
    coding = compress_coding(ask)
    if ask.method == "HEAD":
        body = b""
    elif coding:
        lines.insert(0, ("content-encoding", coding))
    else:
        lines.append(("content-length" if landing else "Content-Length", str(len(body))))
    return Reply(404, "Not Found", lines, body)


def is_landing(reply: Reply) -> bool:
    """Return whether `reply` is the landing page, a 404 of the type of `c.html`."""
    return reply.status == 404 and reply.header("content-type") == LANDING_TYPE


def is_multipart(ask: Ask) -> bool:
    """Return whether `ask` is a request Yoga's multipart parser reads, a POST of multipart/form-data."""
    return ask.method == "POST" and (ask.header("content-type") or "").startswith("multipart/form-data")


def steps(reply: Reply, ask: Ask, keys: dict) -> Reply:
    """Return the answer `reply` of the reference to `ask` as the side under test with `keys` answers it, D46 to D49 and D60."""
    for step, _ in NAMED_STEPS:
        reply = step(reply, ask, keys)
    return reply


def qd055(reply, ask, keys):
    """QD-055, D46: a string option `origin` that holds a star is compared as text.

    Yoga's `origin.includes('*')` echoes every origin for it with `Vary: Origin`, and the function as the row
    leaves it answers the option itself in `Access-Control-Allow-Origin` and leaves `Origin` out of `vary`. In
    the series `qd275` then gives a preflight the answer of a route no handler answers, and `qd021` takes every
    field this step writes out of every other answer, since the engine answers no CORS.
    """
    cors = keys.get("cors")
    origin = cors.get("origin") if isinstance(cors, dict) else None
    if not isinstance(origin, str) or "*" not in origin or origin == "*":
        return reply
    if reply.header("access-control-allow-origin") is None:
        return reply
    lines = []
    for name, value in reply.lines:
        if name.lower() == "access-control-allow-origin":
            lines.append((name, origin))
        elif name.lower() == "vary" and is_cors_line(name, value):
            rest = ", ".join(part.strip() for part in value.split(",") if part.strip() != "Origin")
            if rest:
                lines.append((name, rest))
        else:
            lines.append((name, value))
    return dataclasses.replace(reply, lines=lines)


def qd024(reply, ask, keys):
    """QD-024, D46: an origin outside a list of two or more origins gets no `Access-Control-Allow-Origin`
    and no credentials.

    Yoga answers it `null` with `Access-Control-Allow-Credentials: true`, the origin a sandboxed frame sends,
    and an origin in the list keeps the answer the row leaves. In the series `qd021` then takes the fields of
    CORS out of every answer, since the engine answers no CORS.
    """
    cors = keys.get("cors")
    origin = cors.get("origin") if isinstance(cors, dict) else None
    if not isinstance(origin, list) or len(origin) < 2 or "*" in origin:
        return reply
    if reply.header("access-control-allow-origin") != "null" or ask.header("origin") in origin:
        return reply
    dropped = ("access-control-allow-origin", "access-control-allow-credentials")
    return dataclasses.replace(reply, lines=[(name, value) for name, value in reply.lines if name.lower() not in dropped])


def qd275(reply, ask, keys):
    """QD-275, D46: the engine answers no `OPTIONS` request, which falls through to the web server.

    Yoga's CORS answers every one with 204 and `Content-Length: 0` on every path, and the function hands it on
    as an unhandled route, to the not found answer of the web server where it is served, the landing page where
    the reference serves it, and to the empty 404 of the unhandled route in process. A preflight on a path the
    app's own cors() of hono guards is the answer of that middleware, which the engine never sees, and stays.
    """
    if ask.method.upper() != "OPTIONS" or reply.status != 204 or app_cors(ask, keys) is not None:
        return reply
    if not ask.served:
        return Reply(404, "Not Found", [("content-type", "text/plain;charset=UTF-8")], b"")
    return fallthrough(ask, keys.get("landingPage") is not False)


def qd228(reply, ask, keys):
    """QD-228, D48: the viewer only on `viewer: true`, a key the reference does not read.

    The reference serves its viewer at `/viewer` under every configuration, and the
    function hands the route on as one no handler answers unless the key is
    `true`.
    """
    if keys.get("viewer") is True or ask.path != "/viewer" or ask.method not in ("GET", "HEAD"):
        return reply
    if reply.status != 200:
        return reply
    return fallthrough(ask, keys.get("landingPage") is not False)


def qd122(reply, ask, keys):
    """QD-122, D48: the page of GraphiQL only on `graphiql: true`.

    Yoga serves it unless the key is `false`, and the function answers a request for it as one for a type it
    does not serve, the 406 of `not_acceptable` with no body, with the content type the adapter of Node gives it
    where that adapter writes it.
    """
    if keys.get("graphiql") is True or reply.status != 200 or PLAYGROUND_REFERENCE not in reply.body:
        return reply
    lines = [("accept", not_acceptable(ask.header("accept")))] + cors_lines(reply)
    if ask.served and ask.node:
        lines.append(("content-type", NOT_ACCEPTABLE_TYPE))
    return Reply(406, "Not Acceptable", lines, b"")


def qd224(reply, ask, keys):
    """QD-224, D49: a multipart body is read only on `multipart: true`.

    Yoga reads one unless the key is `false`, and the function answers a multipart request that reaches its
    parsers as a body of a type no parser reads, 415 with no body, with the content type the adapter of Node
    gives it where that adapter writes it. A route no handler answers and the health check answer before the
    parsers.
    """
    if keys.get("multipart") is True or not is_multipart(ask):
        return reply
    if reply.status == 404 or reply.header("x-yoga-id") is not None:
        return reply
    lines = cors_lines(reply)
    if ask.served and ask.node:
        lines.append(("content-type", TEXT_TYPE))
    return Reply(415, "Unsupported Media Type", lines, b"")


def engine_cors_names(ask: Ask, entry: AppCors) -> tuple:
    """Return the names of the fields Yoga's CORS sets on its answer to `ask`, a request with `Origin`, that the
    app's own cors() `entry` does not set before the route.

    Yoga's default echoes the origin with `Vary: Origin` and credentials, and the method and the headers a
    request asks for. The app's middleware sets `Access-Control-Allow-Origin` where its rule allows the origin
    and `Access-Control-Allow-Credentials` where it sends credentials, and its field wins where both set one.
    """
    names = ["vary"]
    if entry.origin(ask.header("origin"), ask) is None:
        names.append("access-control-allow-origin")
    if not entry.credentials:
        names.append("access-control-allow-credentials")
    if ask.header("access-control-request-method") is not None:
        names.append("access-control-allow-methods")
    if ask.header("access-control-request-headers") is not None:
        names.append("access-control-allow-headers")
    return tuple(names)


def seen_without(value: str, names: tuple) -> str:
    """Return `value`, the value of a field of the app that lists names of fields joined by commas among its
    words, without `names`."""
    return " ".join(",".join(part for part in word.split(",") if part not in names) for word in value.split(" "))


def without_engine_cors(reply: Reply, ask: Ask, entry: AppCors) -> Reply:
    """Return `reply`, an answer on a path the app's own cors() `entry` guards, without the fields Yoga's CORS
    added to it.

    Yoga adds them only to an answer the engine gives to a request with `Origin`, whose `vary` then names
    `Origin` twice, Yoga's value before the one the app's middleware appends after the route. So `vary` loses
    Yoga's names and the answer the fields of `engine_cors_names`, and an answer of a route or of a memo of the
    app keeps every field. A field of `entry.seen` loses those names from its list wherever the request carries
    `Origin`, since a warm read of the memo lists the fields of the fill of its key, which the cases send from
    the same origin.
    """
    if ask.header("origin") is None:
        return reply
    names = engine_cors_names(ask, entry)
    parts = [part.strip() for part in (reply.header("vary") or "").split(",") if part.strip()]
    engine = parts.count("Origin") > 1
    if engine:
        parts.remove("Origin")
        if "access-control-allow-headers" in names and "Access-Control-Request-Headers" in parts:
            parts.remove("Access-Control-Request-Headers")
    lines = []
    for name, value in reply.lines:
        if engine and name.lower() == "vary":
            value = ", ".join(parts)
        elif engine and name.lower() in names:
            continue
        elif name.lower() in entry.seen:
            value = seen_without(value, names)
        lines.append((name, value))
    return reply if lines == reply.lines else dataclasses.replace(reply, lines=lines)


def qd021(reply, ask, keys):
    """QD-021, D46: the answer without its fields of CORS, since the engine answers no CORS.

    Every field `CORS_FIELDS` names leaves, and `vary` with them where it holds
    the names of CORS alone. On a path the app's own cors() of hono guards,
    whose fields the web server sets on both sides, only the fields Yoga's CORS
    added leave, `without_engine_cors`.
    """
    entry = app_cors(ask, keys)
    if entry is None:
        return without_cors(reply)
    return without_engine_cors(reply, ask, entry)


#: The title of the landing page of the reference and of the function, and the link of the favicon of the
#: reference.
LANDING_REFERENCE_TITLE = b"<title>Welcome to Pylon</title>"
LANDING_TITLE = b"<title>Welcome to Aosabi</title>"
LANDING_FAVICON = b'<link\nrel="icon"\nhref="https://pylon.cronit.io/favicon/favicon.ico"\n/>\n'


def qd830(reply, ask, keys):
    """QD-830, D60: the landing page names Aosabi and loads no favicon of a third party.

    The title names Aosabi where the reference's names its own, the link of the
    favicon of the reference's host leaves, and a length the answer names follows
    the body.
    """
    if not is_landing(reply) or LANDING_REFERENCE_TITLE not in reply.body:
        return reply
    body = reply.body.replace(LANDING_REFERENCE_TITLE, LANDING_TITLE, 1).replace(LANDING_FAVICON, b"", 1)
    lines = [(name, str(len(body)) if name.lower() == "content-length" else value) for name, value in reply.lines]
    return dataclasses.replace(reply, lines=lines, body=body)


def qd680(reply, ask, keys):
    """QD-680, D47: the landing page answers a route no handler answers only on `landingPage: true`.

    The reference serves it unless the key is `false`, and the function hands the
    route to the not found answer of hono otherwise, which carries the fields
    the middleware of the app set around the route, as the landing page does,
    every field but those of its body and of the connection.
    """
    if keys.get("landingPage") is True or not is_landing(reply):
        return reply
    done = fallthrough(ask, False)
    kept = [(name, value) for name, value in reply.lines
            if name.lower() not in BODY_FIELDS + CONNECTION_FIELDS]
    return dataclasses.replace(done, lines=kept + done.lines) if kept else done


#: The steps `steps` runs, in the order they rewrite an answer, each with the divergence its row names.
NAMED_STEPS = ((qd055, "D46"), (qd024, "D46"), (qd275, "D46"), (qd228, "D48"), (qd122, "D48"),
               (qd224, "D49"), (qd021, "D46"), (qd830, "D60"), (qd680, "D47"))


def raw_message(status_line: str, lines: list, body: bytes, chunked: bool) -> bytes:
    """Return the HTTP/1.1 message of an answer, its body chunked where `chunked` holds."""
    head = status_line + "\r\n" + "".join("%s: %s\r\n" % (name, value) for name, value in lines)
    if chunked:
        head += "Transfer-Encoding: chunked\r\n\r\n"
        wire = (b"%x\r\n" % len(body) + body + b"\r\n" if body else b"") + b"0\r\n\r\n"
        return head.encode("latin-1") + wire
    return (head + "\r\n").encode("latin-1") + body


def ask_of_raw(request: bytes) -> Ask:
    """Return the request of the HTTP/1.1 message `request` as the steps read it."""
    head = request.split(b"\r\n\r\n", 1)[0].decode("latin-1").split("\r\n")
    method, target = head[0].split(" ")[:2]
    fields = [tuple(part.strip() for part in line.split(":", 1)) for line in head[1:] if ":" in line]
    return Ask(method, target, fields)


#: The fields of the connection of an answer, which a rewritten answer keeps from the reference's.
CONNECTION_FIELDS = ("date", "connection", "keep-alive", "transfer-encoding")


def plain_body(answer) -> bytes:
    """Return the body of the answer `answer` of `http.py` with its codings undone.

    A Worker's runtime gzips the gzip of compress() again, divergence D43, so
    a body that is still a gzip member after the coding its field names is
    undone once more.
    """
    body = answer.decoded
    coding = next((value for name, value in answer.header_lines if name.lower() == "content-encoding"), "")
    if coding.strip().lower() == "gzip" and body[:2] == b"\x1f\x8b":
        try:
            return zlib.decompress(body, 16 + zlib.MAX_WBITS)
        except zlib.error:
            return body
    return body


def host_length(lines: list, done: Reply, ask: Ask, answer) -> list:
    """Return the fields `lines` of the answer `done` to `ask` with the length Bun and workerd frame it with.

    Both write `Content-Length: 0` on an answer of no body that carries no length and no coding, and on an
    answer of HEAD Bun writes `content-length: 0` whatever body the route has where workerd writes no length, so
    an answer of HEAD keeps the length of the answer `answer` of the reference on the same host.
    """
    if ask.method == "HEAD":
        kept = [(name, value) for name, value in lines if name.lower() != "content-length"]
        return kept + [(name, value) for name, value in answer.header_lines
                       if name.lower() == "content-length" and value == "0"]
    if done.body or done.status in (204, 304):
        return lines
    if any(name.lower() in ("content-length", "content-encoding") for name, _ in lines):
        return lines
    return lines + [("Content-Length", "0")]


def served_defaults(answer, case, keys: dict, node: bool = True):
    """Return the answer `answer` of the reference to the case `case`, an answer of `http.py` and a case of
    `gen/http.py` or an `Ask`, as the side under test with the keys `keys` answers it, D46 to D49 and D60.

    `node` fails for an answer of the reference served on Bun or on workerd,
    whose length `host_length` writes. An answer no step changes is returned
    as it is. A changed one keeps the fields of the connection of the
    reference, its framing follows its length, and its body is the body the
    steps give, uncoded.
    """
    if answer is None or case is None:
        return answer
    ask = Ask(case.method, case.target, list(case.headers), node=node)
    reason = answer.status_line.split(" ", 2)[2] if answer.status_line.count(" ") >= 2 else ""
    reply = Reply(answer.status, reason, list(answer.header_lines), plain_body(answer))
    done = steps(reply, ask, keys)
    if done == reply:
        return answer
    status_line = "HTTP/1.1 %d %s" % (done.status, done.reason)
    lines = [(name, value) for name, value in done.lines if name.lower() not in CONNECTION_FIELDS]
    if not node:
        lines = host_length(lines, done, ask, answer)
    connection = [(name, value) for name, value in answer.header_lines
                  if name.lower() in CONNECTION_FIELDS and name.lower() != "transfer-encoding"]
    length = next((value for name, value in lines if name.lower() == "content-length"), None)
    chunked = length is None and ask.method != "HEAD" and done.status not in (204, 304)
    framing = "none" if ask.method == "HEAD" else "content-length" if length is not None else (
        "chunked" if chunked else "none")
    raw = raw_message(status_line, lines + connection, done.body, chunked)
    if chunked:
        connection.append(("Transfer-Encoding", "chunked"))
    return dataclasses.replace(answer, status=done.status, status_line=status_line, header_lines=lines + connection,
                               framing=framing, wire_body=done.body, payload=done.body, decoded=done.body, raw=raw)


def golden_defaults(answer: dict | None, request: dict, keys: dict) -> dict | None:
    """Return a recorded golden of the reference, `{status, headers, body}` with its fields by lower case name, as
    the side under test with the keys `keys` answers the request `request`, `{method, target, headers}`.

    An answer no step changes is returned as it is.
    """
    if answer is None:
        return answer
    ask = Ask(request["method"], request["target"], [tuple(pair) for pair in request.get("headers", [])])
    lines = [(name, value) for name, values in answer["headers"].items() for value in values]
    reply = Reply(answer["status"], "", lines, answer.get("body") or b"")
    done = steps(reply, ask, keys)
    if done == reply:
        return answer
    headers = {}
    for name, value in done.lines:
        headers.setdefault(name.lower(), []).append(value)
    return dict(answer, status=done.status, headers=headers, body=done.body)


def view_defaults(view: tuple, request: bytes, keys: dict) -> tuple:
    """Return the view `(status, {name: value}, body)` of an answer of the reference to the HTTP/1.1 request
    `request` as the side under test with the keys `keys` answers it, with the fields the view names.

    A view no step changes is returned as it is.
    """
    status, fields, body = view
    reply = Reply(status, "", [(name, value) for name, value in fields.items() if value is not None], body)
    done = steps(reply, ask_of_raw(request), keys)
    if done == reply:
        return view
    return (done.status, {name: done.header(name) for name in fields}, done.body)


def in_process_defaults(request: dict, expected: dict, keys: dict) -> dict:
    """Return the recorded answer `expected` of an in process case, `{status, status_text, headers, body}`, as the
    function with the keys `keys` answers the request `request`, `{method, url, headers}`, D46 to D49.

    An answer no step changes is returned as it is.
    """
    target = request.get("url") or request.get("target") or "/graphql"
    if "://" in target:
        rest = target.split("://", 1)[1]
        target = "/" + rest.split("/", 1)[1] if "/" in rest else "/"
    ask = Ask(request.get("method", "GET"), target, list((request.get("headers") or {}).items()), served=False)
    reply = Reply(expected["status"], expected.get("status_text", ""), list(expected["headers"].items()),
                  expected.get("body") or b"")
    done = steps(reply, ask, keys)
    if done == reply:
        return expected
    return dict(expected, status=done.status, status_text=done.reason, headers=dict(done.lines), body=done.body)


def message_body(fields: list, wire: bytes) -> bytes:
    """Return the body of an HTTP/1.1 message with the fields `fields` and the bytes `wire` after its head, its
    chunks joined and its coding undone."""
    if any(name.lower() == "transfer-encoding" and "chunked" in value.lower() for name, value in fields):
        joined, at = bytearray(), 0
        while True:
            end = wire.find(b"\r\n", at)
            size = int(wire[at:end].split(b";")[0], 16) if end > at else 0
            if size == 0:
                break
            joined += wire[end + 2:end + 2 + size]
            at = end + 2 + size + 2
        wire = bytes(joined)
    coding = next((value.strip().lower() for name, value in fields if name.lower() == "content-encoding"), "")
    if coding in ("gzip", "x-gzip"):
        return zlib.decompress(wire, 16 + zlib.MAX_WBITS)
    if coding == "deflate":
        return zlib.decompress(wire)
    return wire


def coded_body(body: bytes, coding) -> bytes:
    """Return `body` coded by the content coding `coding`, gzip or deflate, and as it is for any other."""
    coding = (coding or "").strip().lower()
    if coding in ("gzip", "x-gzip"):
        packer = zlib.compressobj(wbits=16 + zlib.MAX_WBITS)
        return packer.compress(body) + packer.flush()
    if coding == "deflate":
        return zlib.compress(body)
    return body


def message_defaults(answer: bytes, request: bytes, keys: dict) -> bytes:
    """Return the HTTP/1.1 answer `answer` of the reference to the HTTP/1.1 request `request`, both as bytes, as
    the side under test with the keys `keys` answers it.

    A message no step changes is returned as it is. A changed one holds the
    fields the steps give and then the fields of the connection of the
    reference, its body chunked where it carries no length and coded by the
    coding its field `content-encoding` names.
    """
    head, _, wire = answer.partition(b"\r\n\r\n")
    lines = head.decode("latin-1").split("\r\n")
    fields = [tuple(part.strip() for part in line.split(":", 1)) for line in lines[1:] if ":" in line]
    parts = lines[0].split(" ", 2)
    reply = Reply(int(parts[1]), parts[2] if len(parts) > 2 else "", fields, message_body(fields, wire))
    ask = ask_of_raw(request)
    done = steps(reply, ask, keys)
    if done == reply:
        return answer
    own = [(name, value) for name, value in done.lines if name.lower() not in CONNECTION_FIELDS]
    connection = [(name, value) for name, value in fields
                  if name.lower() in CONNECTION_FIELDS and name.lower() != "transfer-encoding"]
    chunked = done.header("content-length") is None and ask.method != "HEAD" and done.status not in (204, 304)
    body = coded_body(done.body, done.header("content-encoding"))
    return raw_message("HTTP/1.1 %d %s" % (done.status, done.reason), own + connection, body, chunked)


def pairs_defaults(status: int, fields: list, body: bytes, ask: Ask, keys: dict) -> tuple:
    """Return the answer `(status, fields, body)` of the reference to `ask`, its fields pairs and its body
    uncoded, as the side under test with the keys `keys` answers it, the field `date` of the reference kept.

    An answer of HTTP/2 carries no field of the connection beside `date`. An
    answer no step changes is returned as it is.
    """
    reply = Reply(status, "", [tuple(pair) for pair in fields], body)
    done = steps(reply, ask, keys)
    if done == reply:
        return status, fields, body
    dated = [(name, value) for name, value in reply.lines if name.lower() == "date"]
    return done.status, [(name, value) for name, value in done.lines if name.lower() != "date"] + dated, done.body


# The rules of the oracle of lane A, D46 to D49 and D57 to D60.
#
# The oracle of tests/notebooks/34_http_oracle.ipynb sends each request of its corpus to the pipeline of
# 27d713a0 and to the tree under test, and lets a difference pass only through the rule of its name in `ORACLE`,
# the rules applied in the order of their names, each a function of the request, the old answer and the new
# answer as the rules before it left it, which answers the new answer written as the old pipeline writes it, or
# None. The rules share one model. The old answer, read as a `Reply`, goes through the rewrites of the
# divergences in the order the tree writes them, D58 and D59 of the function, the steps of the rows of B2 and
# the coding of the host, which D57 changes on Bun and on workerd. The rule of a name holds where the new answer
# is the old one through the rewrites of that name and of every name after it, and it answers the old one
# through the rewrites of the names after it alone, so the rule of the last name a difference needs answers the
# old answer itself. Each kind of request of the oracle holds its answer in a form of its own, and the rewrites
# read and write each form as the oracle builds it: the answers of HTTP/1.1 of the kinds `http` and `yoga`, the
# bytes of the kind `wire`, the three forms of the kind `h2`, the answers of the requests of a log, the front of
# the federation and the answers of GraphTP.

#: The names whose rules `ORACLE` holds, in the order the oracle applies them.
ORACLE_NAMES = ("D46", "D47", "D48", "D49", "D57", "D58", "D59", "D60")

#: The fields of a head the oracle never compares, `connection` aside, which only a stream keeps.
ORACLE_IGNORED = ("date", "keep-alive", "server", "transfer-encoding")

#: The body of the 405 the engine answers a request of HEAD with, whose length that answer carries on Bun, D57.
METHOD_REFUSAL = (b'{"errors":[{"message":"GraphQL only supports GET and POST requests.",'
                  b'"extensions":{"code":"BAD_REQUEST"}}]}')

#: The answer the glue of Bun gives where the app's fetch rejects, here on a field value Bun refuses.
BUN_REJECTED = re.compile(rb"the fetch of the app rejected: TypeError: Header '[0-9]+' has invalid value: '(.*)'",
                          re.S)


def oracle_ask(request: dict):
    """Return the request `request` of the oracle as the steps read it, or None for bytes no step reads.

    A request of the kind `yoga` goes to an instance in process, every other
    one is served, by the twin on the side rs and by the module on Bun and on
    workerd, whose host writes an answer of no body as Node does not.
    """
    if request.get("kind") == "yoga":
        target = request.get("target") or "/graphql"
        if "://" in target:
            rest = target.split("://", 1)[1]
            target = "/" + rest.split("/", 1)[1] if "/" in rest else "/"
        headers = [tuple(pair) for pair in request.get("headers") or []]
        return Ask(request.get("method") or "GET", target, headers, served=False)
    raw = request.get("raw") or b""
    try:
        ask = ask_of_raw(raw) if raw else Ask(request.get("method") or "GET", request.get("target") or "/",
                                              [tuple(pair) for pair in request.get("headers") or []])
    except (ValueError, IndexError):
        return None
    return dataclasses.replace(ask, node=request.get("side") == "rs")


def oracle_keys(request: dict) -> dict:
    """Return the keys of the configuration the tree serves the request `request` under."""
    if request.get("kind") == "yoga":
        return dict(IN_PROCESS_KEYS)
    side = "rs" if request.get("side") == "rs" else "ts"
    return keys_of(request.get("fixture") or "", request.get("variant") or "default", side)


def oracle_plain(form: dict) -> bytes:
    """Return the body of the compared form `form` with the second gzip of a Worker undone, D43."""
    body = form.get("body") or b""
    coding = (form.get("headers", {}).get("content-encoding") or [""])[0].strip().lower()
    if coding == "gzip" and body[:2] == b"\x1f\x8b":
        try:
            return zlib.decompress(body, 16 + zlib.MAX_WBITS)
        except zlib.error:
            return body
    return body


def oracle_canon(value) -> str:
    """Return `value` as the oracle compares it, a mapping by its keys whatever their order."""
    return json.dumps(value, sort_keys=True, default=repr)


def oracle_view(form: dict, old: dict) -> tuple:
    """Return what the rules compare of the compared form `form` of an answer whose old answer is `old`.

    The oracle compares `content-length` only where the old answer holds it,
    the body of a Worker after its second gzip is undone, and no frames read
    as an empty list of them.
    """
    headers = {name: list(values) for name, values in form.get("headers", {}).items()}
    if "content-length" not in old.get("headers", {}):
        headers.pop("content-length", None)
    rest = tuple((name, oracle_canon(list(form.get(name) or []) if name == "frames" else form.get(name)))
                 for name in sorted(form) if name not in ("status", "headers", "body", "framing", "gaps"))
    return form.get("status"), tuple(sorted((name, tuple(values)) for name, values in headers.items())), \
        oracle_plain(form), rest


def oracle_reply(form: dict) -> Reply:
    """Return the compared form `form` of an answer as the steps read it."""
    lines = [(name, value) for name, values in form.get("headers", {}).items() for value in values]
    return Reply(form.get("status") or 0, "", lines, oracle_plain(form))


def oracle_head(lines: list, stream: bool = False) -> dict:
    """Return the fields `lines` of an answer as the oracle compares a head: the names in lower case, the values in
    order, the ignored fields out, `connection` out of an answer that is no stream and `content-length` out of
    one with a coding."""
    coding = next((value for name, value in lines if name.lower() == "content-encoding"), "").strip().lower()
    headers = {}
    for name, value in lines:
        folded = name.lower()
        if folded in ORACLE_IGNORED or (folded == "connection" and not stream):
            continue
        if folded == "content-length" and coding not in ("", "identity"):
            continue
        headers.setdefault(folded, []).append(value)
    return headers


def oracle_form(done: Reply, old: dict, request: dict, ask: Ask) -> dict:
    """Return the answer `done` the rewrites give for the old answer `old` as the oracle compares it.

    A served request of the kind `http` whose Accept names an event stream
    reads an event stream as its frames.
    """
    stream = (done.header("content-type") or "").startswith(EVENT_STREAM)
    form = dict(old, status=done.status, headers=oracle_head(done.lines, stream), body=done.body)
    reads_stream = request.get("kind") == "http" and EVENT_STREAM in (ask.header("accept") or "")
    if stream and reads_stream and old.get("frames") is None:
        frames, _ = split_event_frames(done.body.decode("utf-8", "surrogateescape"))
        form.update(body=b"", frames=frames, ended="complete" if frames and frames[-1] == TERMINATOR else "body")
    return form


#: The last frame of a finite event stream.
TERMINATOR = "event: complete\ndata:\n\n"


def split_event_frames(text: str) -> tuple:
    """Return the whole frames of the event stream `text`, each with its blank line, and the rest."""
    frames = []
    while True:
        index = text.find("\n\n")
        if index < 0:
            return frames, text
        frames.append(text[:index + 2])
        text = text[index + 2:]


def host_coding(done: Reply, ask: Ask, side: str) -> Reply:
    """Return `done`, an answer a step of B2 wrote as the reference's compress() codes it, as the host of `side` codes it.

    The twin codes no gzip, so its compress() codes deflate where the request
    names it and nothing otherwise, and writes the length of an answer it does
    not code, D43, while Bun and workerd code as the reference.
    """
    coding = (done.header("content-encoding") or "").strip().lower()
    if side != "rs" or coding not in ("gzip", "deflate"):
        return done
    accepted = ask.header("accept-encoding") or ""
    if "deflate" in accepted:
        lines = [(name, "deflate" if name.lower() == "content-encoding" else value) for name, value in done.lines]
        return dataclasses.replace(done, lines=lines)
    lines = [(name, value) for name, value in done.lines if name.lower() != "content-encoding"]
    return dataclasses.replace(done, lines=lines + [("content-length", str(len(done.body)))])


def d57_host(done: Reply, ask: Ask, side: str, written: bool) -> Reply:
    """Return the answer `done` of the function on Bun or on workerd with its Content-Length, D57.

    The function writes the length of every answer whose body is a string, so compress() leaves an answer below
    1,024 bytes uncoded with that length where the reference's build codes it: a GraphQL answer, and every answer a
    rewrite of the function wrote, `written`. On Bun an answer of HEAD keeps the length of the body it leaves
    out where the reference's build writes 0, the 405 of the engine.
    """
    if side not in ("bun", "workerd"):
        return done
    media_type = (done.header("content-type") or "").split(";")[0].strip().lower()
    if ask.method == "HEAD":
        if side != "bun" or done.status != 405 or media_type not in GRAPHQL_TYPES:
            return done
        if done.header("content-length") != "0":
            return done
        lines = [(name, str(len(METHOD_REFUSAL)) if name.lower() == "content-length" else value)
                 for name, value in done.lines]
        return dataclasses.replace(done, lines=lines)
    coding = (done.header("content-encoding") or "").strip().lower()
    if coding not in ("gzip", "deflate") or len(done.body) >= COMPRESS_THRESHOLD:
        return done
    if media_type not in GRAPHQL_TYPES and not written:
        return done
    lines = [(name, value) for name, value in done.lines if name.lower() != "content-encoding"]
    return dataclasses.replace(done, lines=lines + [("content-length", str(len(done.body)))])


def d58_batch(reply: Reply, request: dict, ask: Ask) -> Reply:
    """Return the answer `reply` of an instance in process that runs batching to a batch as the function's refusal,
    D58, under the type the answer of the instance negotiated."""
    if request.get("kind") != "yoga" or not batch_refused(request.get("instance") or "",
                                                          (request.get("body") or b"").decode("utf-8", "replace")):
        return reply
    lines = [(name, value) for name, value in reply.lines if name.lower() == "content-type"]
    if not lines:
        lines = [("content-type", "application/json; charset=utf-8")]
    if reply.header("content-length") is not None:
        lines.append(("content-length", str(len(BATCH_REFUSAL))))
    return Reply(400, "Bad Request", lines, BATCH_REFUSAL)


def d59_reply(reply: Reply, ask: Ask) -> Reply:
    """Return the answer `reply` with no answer as multipart/mixed, D59.

    An answer of the processor of multipart/mixed becomes the event stream of
    its parts where the request accepts an event stream and otherwise the 406
    of a type the function does not serve, which keeps the fields of CORS
    alone, and the field accept of a 406 lists no multipart/mixed.
    """
    accept = ask.header("accept")
    if answers_multipart(reply.header("content-type")):
        if accepts_event_stream(accept):
            chunks = multipart_chunks(reply.body)
            kept = [(name, value) for name, value in reply.lines
                    if name.lower() not in ("accept", "content-type", "connection", "transfer-encoding",
                                            "content-length", "cache-control", "content-encoding")]
            lines = [(name.lower(), value) for name, value in EVENT_STREAM_FIELDS] + kept
            return Reply(event_stream_status(reply.status, chunks), "", lines, event_stream_body(chunks))
        lines = [("accept", not_acceptable(accept))] + cors_lines(reply)
        if ask.served and ask.node:
            lines.append(("content-type", NOT_ACCEPTABLE_TYPE))
        return Reply(406, "Not Acceptable", lines, b"")
    if reply.status == 406:
        lines = [(name, accept_without_multipart(value) if name.lower() == "accept" else value)
                 for name, value in reply.lines]
        return dataclasses.replace(reply, lines=lines)
    return reply


def oracle_subscription(ask: Ask) -> bool:
    """Return whether `ask` is a request of GET for a subscription the engine runs.

    A subscription whose root field is one of introspection fails validation and is answered as a query.
    """
    if ask.method != "GET" or "?" not in ask.target:
        return False
    query = parse_qs(ask.target.split("?", 1)[1]).get("query", [""])[0]
    root = re.match(r"\s*subscription\b[^{]*\{\s*([_A-Za-z][_0-9A-Za-z]*)", query)
    return root is not None and not root.group(1).startswith("__")


def oracle_qd122(reply, ask, keys):
    """`qd122` for an answer of 27d713a0, whose page of GraphiQL names Aosabi already, D52.

    The 406 of a subscription lists the media types of an event stream alone, the one processor a subscription
    has, where `qd122` writes the 406 of a query.
    """
    probe = reply
    if PLAYGROUND in reply.body:
        probe = dataclasses.replace(reply, body=reply.body.replace(PLAYGROUND, PLAYGROUND_REFERENCE))
    done = qd122(probe, ask, keys)
    if done == probe:
        return reply
    if oracle_subscription(ask):
        count = len(media_types_in_order(ask.header("accept")))
        offered = ACCEPT_JOIN.join(media for _ in range(count) for media in PROCESSOR_TYPES[0])
        done = dataclasses.replace(done, lines=[(name, offered if name.lower() == "accept" else value)
                                                for name, value in done.lines])
    return done


#: The steps of `NAMED_STEPS` as the rules run them on an answer of 27d713a0.
ORACLE_STEPS = tuple((oracle_qd122 if step is qd122 else step, name) for step, name in NAMED_STEPS)


@dataclasses.dataclass
class OracleAnswer:
    """The fields of an old answer, as `host_length` reads the answer of the reference on the same host."""

    header_lines: list


def oracle_chain(reply: Reply, request: dict, ask: Ask, keys: dict, side: str, names) -> Reply:
    """Return the answer `reply` to `ask` on `side` through the rewrites of the divergences `names` in the order
    the tree writes them: D58 and D59 of the function, the steps of B2 each coded by the host, the length Bun and
    workerd frame a rewritten answer with, and D57."""
    before = reply
    if "D58" in names:
        reply = d58_batch(reply, request, ask)
    if "D59" in names:
        reply = d59_reply(reply, ask)
    for step, name in ORACLE_STEPS:
        if name in names:
            done = step(reply, ask, keys)
            if done != reply:
                reply = host_coding(done, ask, side)
    written = reply != before
    if written and side in ("bun", "workerd") and ask.served:
        reply = dataclasses.replace(reply, lines=host_length(reply.lines, reply, ask, OracleAnswer(before.lines)))
    if "D57" in names:
        reply = d57_host(reply, ask, side, written)
    return reply


def oracle_served(old: dict, request: dict, names) -> dict | None:
    """The rewrites of an answer of the kinds `http` and `yoga` and of an answer of HTTP/1.1 inside TLS."""
    ask = oracle_ask(request)
    if ask is None or old.get("status") is None:
        return None
    side = request.get("side") if request.get("kind") != "h2" else "rs"
    before = oracle_reply(old)
    done = oracle_chain(before, request, ask, oracle_keys(request), side, names)
    return old if done == before else oracle_form(done, old, request, ask)


def wire_message(data: bytes, at: int):
    """Return the whole answer of HTTP/1.1 that starts at `at` of the bytes `data`, as `(status, reason, fields,
    body, head_only, end)`, or None where no whole answer starts there.

    An answer that names a length whose body is not there, the next answer
    or the end following its head, answers a request of HEAD, and so does an
    answer of a type that names neither a length nor chunks and ends with its
    head, as the twin answers HEAD.
    """
    if not data.startswith(b"HTTP/1.", at):
        return None
    split = data.find(b"\r\n\r\n", at)
    if split < 0:
        return None
    lines = data[at:split].decode("latin-1").split("\r\n")
    parts = lines[0].split(" ", 2)
    if len(parts) < 2 or not parts[1].isdigit():
        return None
    status, reason = int(parts[1]), parts[2] if len(parts) > 2 else ""
    fields = [tuple(part.strip() for part in line.split(":", 1)) for line in lines[1:] if ":" in line]
    start = split + 4
    field = {name.lower(): value for name, value in reversed(fields)}
    if 100 <= status < 200 or status in (204, 304):
        return status, reason, fields, b"", False, start
    if "chunked" in field.get("transfer-encoding", "").lower():
        position = start
        while True:
            line_end = data.find(b"\r\n", position)
            if line_end < 0:
                return None
            size = int(data[position:line_end].split(b";")[0], 16)
            if size == 0:
                end = data.find(b"\r\n\r\n", line_end)
                if end < 0:
                    return None
                return status, reason, fields, data[start:end + 4], False, end + 4
            position = line_end + 2 + size + 2
    if "content-length" in field:
        length = int(field["content-length"])
        if length and (start == len(data) or data.startswith(b"HTTP/1.", start)):
            return status, reason, fields, b"", True, start
        if start + length > len(data):
            return None
        return status, reason, fields, data[start:start + length], False, start + length
    if "content-type" in field and start == len(data):
        return status, reason, fields, b"", True, start
    return status, reason, fields, data[start:], False, len(data)


def wire_ask(reply: Reply, head_only: bool) -> Ask:
    """Return the request an answer of the twin to bytes the rules do not read answers, as the steps read it.

    The oracle gives a rule no bytes of a case of raw bytes, so its answer
    names the request: a 204 answers the `OPTIONS` of Yoga's CORS, an answer
    whose body is not there a request of HEAD, a coded one a request that
    accepts its coding, and every other a request of GET.
    """
    method = "OPTIONS" if reply.status == 204 else "HEAD" if head_only else "GET"
    coding = (reply.header("content-encoding") or "").strip().lower()
    return Ask(method, "/", [("accept-encoding", coding)] if coding in ("gzip", "deflate") else [])


def wire_bytes(data: bytes, request: dict, names, keys: dict) -> bytes:
    """Return the bytes `data` the twin wrote, each whole answer through the rewrites of `names`, written as
    `message_defaults` writes a rewritten answer, and the bytes after the last whole answer as they are."""
    out, at = bytearray(), 0
    while True:
        message = wire_message(data, at)
        if message is None:
            break
        status, reason, fields, wire, head_only, end = message
        reply = Reply(status, reason, fields, message_body(fields, wire) if wire else b"")
        ask = wire_ask(reply, head_only)
        done = oracle_chain(reply, request, ask, keys, "rs", names)
        if done == reply:
            out += data[at:end]
        else:
            own = [(name, value) for name, value in done.lines if name.lower() not in CONNECTION_FIELDS]
            connection = [(name, value) for name, value in fields
                          if name.lower() in CONNECTION_FIELDS and name.lower() != "transfer-encoding"]
            chunked = done.header("content-length") is None and not head_only and done.status not in (204, 304)
            body = b"" if head_only else coded_body(done.body, done.header("content-encoding"))
            out += raw_message("HTTP/1.1 %d %s" % (done.status, done.reason), own + connection, body, chunked)
        at = end
    return bytes(out + data[at:])


def oracle_wire(old: dict, request: dict, names) -> dict:
    """The rewrites of a case of raw bytes, its answers and those of its follow up."""
    keys = oracle_keys(request)
    out = dict(old, body=wire_bytes(old.get("body") or b"", request, names, keys))
    follow = old.get("follow")
    if isinstance(follow, list) and follow and isinstance(follow[0], (bytes, bytearray)):
        out["follow"] = [wire_bytes(bytes(follow[0]), request, names, keys)] + list(follow[1:])
    return old if out == old else out


def h2_requests_line(streams: dict) -> str:
    """Return the entry of `unmet` of a case of frames that expects requests, which names the first three lines of
    the body of each answered stream."""
    got = sorted(tuple(re.sub(r"^[a-z]+://[^/]*", "", line) if at == 1 else line
                       for at, line in enumerate(stream.get("body", "").split("\n")[:3]))
                 for stream in streams.values())
    return "requests %s" % got[:3]


def oracle_h2_case(old: dict, request: dict, names) -> dict:
    """The rewrites of a case of the frames of HTTP/2 of 33_http_hardening: the fields and the body of each
    stream the server answered, in the record the oracle writes as JSON with its keys sorted, and the entry of
    `unmet` that names the bodies."""
    judged = json.loads((old.get("body") or b"{}").decode("utf-8", "surrogateescape"))
    streams = judged.get("streams") if isinstance(judged, dict) else None
    if not isinstance(streams, dict):
        return old
    unmet = judged.get("unmet") or []
    line = h2_requests_line(streams)
    keys, changed = oracle_keys(request), False
    for stream in streams.values():
        blocks = stream.get("fields") or []
        if not blocks:
            continue
        head = blocks[0]
        status = int(next(value for name, value in head if name == ":status"))
        lines = [(name, value) for name, value in head if not name.startswith(":") and name != "date"]
        dated = [[name, value] for name, value in head if name == "date"]
        wire = stream.get("body", "").encode("latin-1")
        reply = Reply(status, "", lines, message_body(lines, wire) if wire else b"")
        ask = wire_ask(reply, False)
        done = oracle_chain(reply, request, ask, keys, "rs", names)
        if done == reply:
            continue
        changed = True
        fields = [[":status", str(done.status)]] + [[name.lower(), value] for name, value in done.lines]
        stream["fields"] = [fields + dated] + blocks[1:]
        stream["body"] = coded_body(done.body, done.header("content-encoding")).decode("latin-1")
    if not changed:
        return old
    judged["unmet"] = [h2_requests_line(streams) if entry == line else entry for entry in unmet]
    text = json.dumps(judged, sort_keys=True, ensure_ascii=False, default=str)
    return dict(old, body=text.encode("utf-8", "surrogateescape"))


def oracle_h2(old: dict, request: dict, names) -> dict | None:
    """The rewrites of the three forms of the kind `h2`: a case of frames, an answer of HTTP/1.1 inside TLS and
    an answer curl read over HTTP/2, whose request the oracle does not hand a rule, so its answer names it as
    `wire_ask` reads it, under the Accept of the cases of rows C51, C52 and C54 that ask for GraphiQL."""
    if old.get("status") is None:
        return oracle_h2_case(old, request, names)
    if request.get("raw"):
        return oracle_served(old, request, names)
    before = oracle_reply(old)
    guessed = wire_ask(before, False)
    ask = Ask(request.get("method") or guessed.method, "/", guessed.headers + [("accept", "text/html")])
    done = oracle_chain(before, request, ask, oracle_keys(request), "rs", names)
    return old if done == before else dict(old, status=done.status, headers=oracle_head(done.lines),
                                           body=coded_body(done.body, done.header("content-encoding")))


def oracle_log(old: dict, request: dict, names) -> dict:
    """The rewrites of the answers of the requests of a log, each a request of GET the answer names as
    `wire_ask` reads it."""
    follow = old.get("follow")
    if not isinstance(follow, list):
        return old
    keys, side, out = oracle_keys(request), request.get("side"), []
    for entry in follow:
        if not isinstance(entry, list) or len(entry) != 3 or entry[0] is None:
            out.append(entry)
            continue
        status, headers, body = entry
        reply = Reply(status, "", [(name, value) for name, values in headers.items() for value in values], body)
        ask = dataclasses.replace(wire_ask(reply, False), node=side == "rs")
        done = oracle_chain(reply, request, ask, keys, side, names)
        out.append(entry if done == reply else [done.status, oracle_head(done.lines), done.body])
    return old if out == follow else dict(old, follow=out)


def oracle_front(old: dict, request: dict, names) -> dict:
    """The rewrites of an answer of a front of the federation to an upload, D49: where the old front forwarded a
    multipart body, the function of the new front refuses it and forwards nothing."""
    upstream = old.get("upstream")
    if "D49" not in names or not isinstance(upstream, dict) or old.get("status") is None:
        return old
    sent = [call for calls in upstream.values() for call in (calls or [])]
    if not any((value or "").startswith("multipart/form-data") for call in sent
               for name, value in call.get("headers") or [] if name.lower() == "content-type"):
        return old
    side = request.get("side")
    ask = Ask(request.get("method") or "POST", "/graphql", [("content-type", "multipart/form-data")],
              node=side == "rs")
    before = oracle_reply(old)
    done = oracle_chain(before, request, ask, oracle_keys(request), side, names)
    if done == before:
        return old
    return dict(old, status=done.status, headers=oracle_head(done.lines), body=done.body,
                upstream={name: [] for name in upstream})


def oracle_graphtp(old: dict, request: dict, names) -> dict:
    """The rewrites of an answer of GraphTP to a batch, D58: the refusal of the function, status 400 with the
    substatus 6, uncoded."""
    body = (request.get("body") or b"").decode("utf-8", "replace")
    if "D58" not in names or not batch_refused(request.get("instance") or "", body):
        return old
    headers = dict(old.get("headers", {}), substatus=["6"], coding=["plain"])
    return dict(old, status=400, headers=headers, body=BATCH_REFUSAL)


def oracle_rewrite(old: dict, request: dict, names) -> dict | None:
    """Return the old answer `old` to the request `request` through the rewrites of the divergences `names`.

    None stands for a request or an answer no rewrite reads.
    """
    kind = request.get("kind")
    rewrite = {"http": oracle_served, "yoga": oracle_served, "h2": oracle_h2, "wire": oracle_wire,
               "log": oracle_log, "front": oracle_front, "graphtp": oracle_graphtp}.get(kind)
    if rewrite is None:
        return None
    try:
        return rewrite(old, request, names)
    except (ValueError, KeyError, IndexError, StopIteration, TypeError, AttributeError, zlib.error):
        return None


def bun_refused_origin(request: dict, old: dict, new: dict) -> bool:
    """Return whether the old answer is the 500 of the glue of Bun to an `Origin` outside ASCII, D46.

    The engine of 27d713a0 echoes the origin into a field, which Bun refuses
    as a value outside ASCII, so the app's fetch rejects. The engine of the
    tree answers no CORS, and the new answer carries no field of CORS.
    """
    if request.get("side") != "bun" or old.get("status") != 500:
        return False
    ask = oracle_ask(request)
    origin = ask.header("origin") if ask is not None else None
    if origin is None or origin.isascii():
        return False
    match = BUN_REJECTED.fullmatch(old.get("body") or b"")
    if match is None or match.group(1) != origin.encode("utf-8"):
        return False
    return not any(is_cors_line(name, value) for name, values in new.get("headers", {}).items() for value in values)


def oracle_rule(name: str):
    """Return the rule of `ORACLE` for the divergence `name`."""
    later = tuple(entry for entry in ORACLE_NAMES if entry >= name)

    def rule(request: dict, old: dict, new: dict):
        if name == "D46" and bun_refused_origin(request, old, new):
            return old
        want = oracle_rewrite(old, request, later)
        back = oracle_rewrite(old, request, later[1:])
        if want is None or back is None:
            return None
        wanted, left = oracle_view(want, old), oracle_view(back, old)
        if wanted == left or oracle_view(new, old) != wanted:
            return None
        return old if left == oracle_view(old, old) else back

    rule.__name__ = rule.__qualname__ = "oracle_" + name
    rule.__doc__ = "The rule of the oracle for %s, `oracle_rule`." % name
    return rule


#: The rules of the oracle of lane A for the divergences of this module, by name.
ORACLE = {name: oracle_rule(name) for name in ORACLE_NAMES}
