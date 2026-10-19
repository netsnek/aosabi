# SPDX-License-Identifier: MIT
"""tests/gen: the HTTP families T, K, D, U and R, and the operations over HTTP.

A case of these families is one request: the method, the target, the header
fields in the order they are sent and the body bytes, with the SHA-256 of that
canonical form as its id. Every case carries `Host: localhost`, so its bytes do
not depend on the port a side listens on, and an `x-request-id` of its own, so
no side generates a request id and the access lines of both sides carry the same
one. A body carries its `Content-Length`, which is part of the canonical form,
and the raw client of `tests/harness/http.py` writes the fields as they stand.

- **T, transport**, 40 cases per fixture over the one valid document
  `{__typename}`: the seven content types, the GET spelling with `variables`
  as a JSON string and as a broken one, the array body, `HEAD`, `OPTIONS` with
  and without `Origin`, `PUT`, `DELETE` and `PATCH`, one `Accept` per
  negotiated type and for `text/event-stream` and `multipart/mixed`, two
  charsets other than utf-8, five values of `Accept-Encoding` and nine corners
  of the URL. Those are 36 cases, and four more complete the 40 of the plan:
  the plain GET spelling, a GET with `operationName`, `Accept: */*` and a
  `POST` whose type names `charset=utf-8`.
- **K, the static routes**, seven cases per variant under each of the five
  variants of `docs/harness.md` section 8, 35 cases.
- **U, uploads**, the twelve cases of the multipart request specification over
  `upload` and `uploadNested` of `surface-app`.
- **R, the routes a fixture registers**, one request per route with its
  method, one with a wrong method, and a `HEAD` per `GET` route, from a list of
  routes a boot report or the authoring map gives.

The command line prints the counts:

    python -m tests.gen --family http
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from urllib.parse import quote

#: The document every case of family T sends.
DOCUMENT = "{__typename}"

#: The variants of the configuration matrix.
VARIANTS = ("default", "graphiql_false", "landing_false", "both", "logger_false")

DEFAULTS_VARIANTS = ("switches_on", "cors_star", "cors_list")

#: The counts of the plan.
TRANSPORT_COUNT = 40
STATIC_PER_VARIANT = 7
UPLOAD_COUNT = 12

#: The boundary of every multipart body the generator writes.
BOUNDARY = "aosabi-boundary"

#: The origin every case that needs one sends, a neutral test value.
ORIGIN = "https://example.invalid"


@dataclass(frozen=True)
class HttpCase:
    """One request of an HTTP family."""

    id: str
    family: str
    name: str
    method: str
    target: str
    headers: tuple[tuple[str, str], ...]
    body: bytes

    def raw(self) -> bytes:
        """The bytes of the request as the raw client writes them."""
        head = "%s %s HTTP/1.1\r\n" % (self.method, self.target)
        head += "".join("%s: %s\r\n" % pair for pair in self.headers) + "\r\n"
        return head.encode("latin-1") + self.body

    def request(self) -> dict:
        """The canonical request, the object `request.json` of a golden holds."""
        return {
            "method": self.method,
            "target": self.target,
            "headers": [list(pair) for pair in self.headers],
            "body_hex": self.body.hex(),
        }


def canonical(request: dict) -> bytes:
    """The bytes the id of a case is taken over: sorted keys, no spaces."""
    return json.dumps(request, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def make(family: str, name: str, method: str, target: str, headers, body: bytes = b"") -> HttpCase:
    """Builds a case with `Host` first, its `x-request-id` and its length.

    The request id names the family and the case, and the length is written
    last when the case has a body, so the fields a case names keep their order.
    """
    fields = [("Host", "localhost"), ("x-request-id", "%s-%s" % (family.lower(), name))]
    fields.extend(headers)
    if body and not any(key.lower() in ("content-length", "transfer-encoding") for key, _ in fields):
        fields.append(("Content-Length", str(len(body))))
    request = {
        "method": method,
        "target": target,
        "headers": [list(pair) for pair in fields],
        "body_hex": body.hex(),
    }
    ident = hashlib.sha256(canonical(request)).hexdigest()
    return HttpCase(ident, family, name, method, target, tuple(fields), body)


def json_body(document: str = DOCUMENT, **extra) -> bytes:
    """A JSON body of `query` and the keys given."""
    return json.dumps({"query": document, **extra}, separators=(",", ":")).encode()


def get_target(document: str = DOCUMENT, path: str = "/graphql", **params) -> str:
    """The GET spelling of a document, every parameter percent encoded."""
    pairs = [("query", document)] + list(params.items())
    return path + "?" + "&".join("%s=%s" % (key, quote(value, safe="")) for key, value in pairs)


def multipart(parts: list[tuple[str | None, str | None, str | None, bytes]], close: bool = True) -> bytes:
    """A multipart body: per part its name, file name, type and content."""
    out = b""
    for name, filename, content_type, content in parts:
        disposition = "form-data"
        if name is not None:
            disposition += '; name="%s"' % name
        if filename is not None:
            disposition += '; filename="%s"' % filename
        out += ("--%s\r\nContent-Disposition: %s\r\n" % (BOUNDARY, disposition)).encode()
        if content_type is not None:
            out += ("Content-Type: %s\r\n" % content_type).encode()
        out += b"\r\n" + content + b"\r\n"
    if close:
        out += ("--%s--\r\n" % BOUNDARY).encode()
    return out


def transport_cases() -> list[HttpCase]:
    """Family T, the 40 cases of the transport over `{__typename}`."""
    json_type = ("Content-Type", "application/json")
    body = json_body()
    form = ("query=" + quote(DOCUMENT, safe="")).encode()
    operations_only = multipart([("operations", None, None, body)])
    cases = [
        make("T", "t01", "POST", "/graphql", [json_type], body),
        make("T", "t02", "POST", "/graphql", [("Content-Type", "application/graphql-response+json")], body),
        make("T", "t03", "POST", "/graphql", [("Content-Type", "application/graphql")], DOCUMENT.encode()),
        make("T", "t04", "POST", "/graphql", [("Content-Type", "application/x-www-form-urlencoded")], form),
        make("T", "t05", "POST", "/graphql", [("Content-Type", "multipart/form-data; boundary=" + BOUNDARY)], operations_only),
        make("T", "t06", "POST", "/graphql", [], body),
        make("T", "t07", "POST", "/graphql", [("Content-Type", "text/plain")], body),
        make("T", "t08", "GET", get_target("query($x: String) { __typename }", variables='{"x":"1"}'), []),
        make("T", "t09", "GET", get_target(variables="{broken"), []),
        make("T", "t10", "POST", "/graphql", [json_type], b"[" + body + b"," + body + b"]"),
        make("T", "t11", "HEAD", get_target(), []),
        make("T", "t12", "OPTIONS", "/graphql", [("Origin", ORIGIN), ("Access-Control-Request-Method", "POST")]),
        make("T", "t13", "OPTIONS", "/graphql", []),
        make("T", "t14", "PUT", "/graphql", [json_type], body),
        make("T", "t15", "DELETE", "/graphql", [json_type], body),
        make("T", "t16", "PATCH", "/graphql", [json_type], body),
        make("T", "t17", "POST", "/graphql", [json_type, ("Accept", "application/json")], body),
        make("T", "t18", "POST", "/graphql", [json_type, ("Accept", "application/graphql-response+json")], body),
        make("T", "t19", "POST", "/graphql", [json_type, ("Accept", "text/event-stream")], body),
        make("T", "t20", "POST", "/graphql", [json_type, ("Accept", "multipart/mixed")], body),
        make("T", "t21", "POST", "/graphql", [json_type, ("Accept", "application/json; charset=iso-8859-1")], body),
        make("T", "t22", "POST", "/graphql", [json_type, ("Accept", "application/graphql-response+json; charset=utf-16")], body),
        make("T", "t23", "POST", "/graphql", [json_type, ("Accept-Encoding", "gzip")], body),
        make("T", "t24", "POST", "/graphql", [json_type, ("Accept-Encoding", "deflate")], body),
        make("T", "t25", "POST", "/graphql", [json_type, ("Accept-Encoding", "gzip, deflate")], body),
        make("T", "t26", "POST", "/graphql", [json_type, ("Accept-Encoding", "x-gzip")], body),
        make("T", "t27", "POST", "/graphql", [json_type, ("Accept-Encoding", "br")], body),
        make("T", "t28", "POST", "/graphql/", [json_type], body),
        make("T", "t29", "POST", "/a/graphql", [json_type], body),
        make("T", "t30", "POST", "/graphql?x=1", [json_type], body),
        make("T", "t31", "GET", "/health", []),
        make("T", "t32", "GET", "/health/", []),
        make("T", "t33", "GET", "/health?x", []),
        make("T", "t34", "GET", "/healthx", []),
        make("T", "t35", "GET", "/nested/health", []),
        make("T", "t36", "POST", "/%67raphql", [json_type], body),
        make("T", "t37", "GET", get_target(), []),
        make("T", "t38", "GET", get_target("query Q { __typename }", operationName="Q"), []),
        make("T", "t39", "POST", "/graphql", [json_type, ("Accept", "*/*")], body),
        make("T", "t40", "POST", "/graphql", [("Content-Type", "application/json; charset=utf-8")], body),
    ]
    if len(cases) != TRANSPORT_COUNT:
        raise AssertionError("family T holds %d cases, the plan asks %d" % (len(cases), TRANSPORT_COUNT))
    return cases


def static_cases(variant: str) -> list[HttpCase]:
    """Family K, the seven static cases of one variant."""
    if variant not in VARIANTS + DEFAULTS_VARIANTS:
        raise ValueError("no variant named %s" % variant)
    introspection = json_body("{__schema{queryType{name}}}")
    prefix = "k-" + variant
    return [
        make("K", prefix + "-landing", "GET", "/", [("Accept", "text/html")]),
        make("K", prefix + "-playground", "GET", "/graphql", [("Accept", "text/html")]),
        make("K", prefix + "-health", "GET", "/health", []),
        make("K", prefix + "-preflight", "OPTIONS", "/graphql", [("Origin", ORIGIN), ("Access-Control-Request-Method", "POST")]),
        make("K", prefix + "-head", "HEAD", "/graphql", []),
        make("K", prefix + "-introspection", "POST", "/graphql", [("Content-Type", "application/json")], introspection),
        make("K", prefix + "-unknown", "GET", "/nothing-here", [("Accept", "text/html")]),
    ]


def defaults_cases(variant: str) -> list[HttpCase]:
    """Each function below answers the cases of one row of the rated table under
    the variants that judge it, and none while its row waits.
    """
    if variant not in VARIANTS + DEFAULTS_VARIANTS:
        raise ValueError("no variant named %s" % variant)
    return qd055_cases(variant) + qd024_cases(variant) + qd275_cases(variant) + qd021_cases(variant) + qd228_cases(variant)


def qd055_cases(variant: str) -> list[HttpCase]:
    """QD-055, D46: under `cors_star`, a POST and a preflight from an origin the star would match."""
    if variant != "cors_star":
        return []
    origin = ("Origin", "https://a.example.invalid")
    return [
        make("D", "d-qd055-post-origin", "POST", "/graphql", [("Content-Type", "application/json"), origin], json_body()),
        make("D", "d-qd055-preflight-headers", "OPTIONS", "/graphql",
             [origin, ("Access-Control-Request-Method", "POST"), ("Access-Control-Request-Headers", "content-type")]),
    ]


def qd024_cases(variant: str) -> list[HttpCase]:
    """QD-024, D46: under `cors_list`, a POST from the origin `null`, from an origin outside the list and from one in it."""
    if variant != "cors_list":
        return []
    json_type = ("Content-Type", "application/json")
    return [
        make("D", "d-qd024-origin-null", "POST", "/graphql", [json_type, ("Origin", "null")], json_body()),
        make("D", "d-qd024-post-outside", "POST", "/graphql", [json_type, ("Origin", "https://c.example.invalid")], json_body()),
        make("D", "d-qd024-post-inside", "POST", "/graphql", [json_type, ("Origin", "https://a.example.invalid")], json_body()),
    ]


def qd275_cases(variant: str) -> list[HttpCase]:
    """QD-275, D46: an `OPTIONS` request to an unknown path and to the health check, the first coded."""
    return [
        make("D", "d-qd275-options-unknown-gzip", "OPTIONS", "/nothing-here",
             [("Accept", "text/html"), ("Accept-Encoding", "gzip")]),
        make("D", "d-qd275-options-health", "OPTIONS", "/health", [("Origin", ORIGIN)]),
    ]


def qd021_cases(variant: str) -> list[HttpCase]:
    """QD-021, D46: a POST, the page of GraphiQL and an unknown path, each from an origin."""
    origin = ("Origin", ORIGIN)
    return [
        make("D", "d-qd021-post-origin", "POST", "/graphql", [("Content-Type", "application/json"), origin], json_body()),
        make("D", "d-qd021-graphiql-origin", "GET", "/graphql", [("Accept", "text/html"), origin]),
        make("D", "d-qd021-unknown-origin", "GET", "/nothing-here", [("Accept", "text/html"), origin]),
    ]


def qd228_cases(variant: str) -> list[HttpCase]:
    """QD-228, D48: the viewer, by GET and by HEAD."""
    return [
        make("D", "d-qd228-viewer", "GET", "/viewer", [("Accept", "text/html")]),
        make("D", "d-qd228-viewer-head", "HEAD", "/viewer", []),
    ]


def upload_cases() -> list[HttpCase]:
    """Family U, the twelve cases of the multipart request specification."""
    kind = ("Content-Type", "multipart/form-data; boundary=" + BOUNDARY)
    one_file = json.dumps({"query": "mutation($file: File!) { upload(file: $file) }", "variables": {"file": None}}, separators=(",", ":"))
    two_files = json.dumps(
        {"query": "mutation($a: File!, $b: File!) { a: upload(file: $a) b: upload(file: $b) }", "variables": {"a": None, "b": None}},
        separators=(",", ":"),
    )
    nested = json.dumps(
        {"query": "mutation($input: UploadNestedInput!) { uploadNested(input: $input) }", "variables": {"input": {"file": None, "label": "l"}}},
        separators=(",", ":"),
    )
    map_one = b'{"0":["variables.file"]}'
    text = ("0", "a.txt", "text/plain", b"hello")
    bodies = [
        ("u01-one-part", multipart([("operations", None, None, one_file.encode()), ("map", None, None, map_one), text])),
        ("u02-two-parts", multipart([
            ("operations", None, None, two_files.encode()),
            ("map", None, None, b'{"0":["variables.a"],"1":["variables.b"]}'),
            ("0", "a.txt", "text/plain", b"first"),
            ("1", "b.txt", "text/plain", b"second"),
        ])),
        ("u03-empty-file", multipart([("operations", None, None, one_file.encode()), ("map", None, None, map_one), ("0", "e.txt", "text/plain", b"")])),
        ("u04-64kib", multipart([
            ("operations", None, None, one_file.encode()), ("map", None, None, map_one),
            ("0", "big.bin", "application/octet-stream", bytes(index % 251 for index in range(65536))),
        ])),
        ("u05-wrong-map", multipart([("operations", None, None, one_file.encode()), ("map", None, None, b'{"0":["variables.nope"]}'), text])),
        ("u06-missing-part", multipart([("operations", None, None, one_file.encode()), ("map", None, None, b'{"1":["variables.file"]}'), text])),
        ("u07-nested", multipart([("operations", None, None, nested.encode()), ("map", None, None, b'{"0":["variables.input.file"]}'), text])),
        ("u08-no-operations", multipart([("map", None, None, map_one), text])),
        ("u09-operations-not-json", multipart([("operations", None, None, b"{not json"), ("map", None, None, map_one), text])),
        ("u10-map-proto", multipart([("operations", None, None, one_file.encode()), ("map", None, None, b'{"0":["variables.__proto__.file"]}'), text])),
        ("u11-boundary-open", multipart([("operations", None, None, one_file.encode()), ("map", None, None, map_one), text], close=False)),
        ("u12-part-no-name", multipart([("operations", None, None, one_file.encode()), ("map", None, None, map_one), (None, "a.txt", "text/plain", b"hello")])),
    ]
    cases = [make("U", name, "POST", "/graphql", [kind], body) for name, body in bodies]
    if len(cases) != UPLOAD_COUNT:
        raise AssertionError("family U holds %d cases, the plan asks %d" % (len(cases), UPLOAD_COUNT))
    return cases


#: The method a wrong method case sends for a route of each method.
WRONG_METHOD = {"GET": "DELETE", "POST": "PUT", "PUT": "PATCH", "DELETE": "POST", "PATCH": "GET"}


def route_cases(routes: list[tuple[str, str]]) -> list[HttpCase]:
    """Family R over the routes of a boot report, each a method and a path.

    A route of the method `ALL` or `USE` or with a parameter in its path is
    sent as `GET` with the parameter named `x`, because the family asks one
    request per route and not a route's grammar.
    """
    cases = []
    for index, (method, path) in enumerate(routes, start=1):
        target = "/".join("x" if part.startswith(":") else part for part in path.split("/")).replace("*", "x")
        sent = method if method in WRONG_METHOD else "GET"
        name = "r%02d" % index
        cases.append(make("R", name + "-" + sent.lower(), sent, target, []))
        cases.append(make("R", name + "-wrong", WRONG_METHOD[sent], target, []))
        if sent == "GET":
            cases.append(make("R", name + "-head", "HEAD", target, []))
    return cases


def over_http(document: str, variables: dict | None, operation_name: str | None, name: str) -> list[HttpCase]:
    """An operation families as a `POST` and, for a query, as a GET."""
    extra = {}
    if variables is not None:
        extra["variables"] = variables
    if operation_name is not None:
        extra["operationName"] = operation_name
    cases = [make("O", name + "-post", "POST", "/graphql", [("Content-Type", "application/json")], json_body(document, **extra))]
    if not document.lstrip().startswith(("mutation", "subscription")):
        params = {}
        if variables is not None:
            params["variables"] = json.dumps(variables, separators=(",", ":"))
        if operation_name is not None:
            params["operationName"] = operation_name
        cases.append(make("O", name + "-get", "GET", get_target(document, **params), []))
    return cases


def http_manifest() -> dict:
    """The counts of the HTTP families, with the digest of every id in order."""
    transport = transport_cases()
    static = [case for variant in VARIANTS for case in static_cases(variant)]
    upload = upload_cases()
    everything = transport + static + upload
    return {
        "families": {"T": len(transport), "K": len(static), "U": len(upload)},
        "digest": hashlib.sha256(
            json.dumps([case.id for case in everything], separators=(",", ":")).encode()
        ).hexdigest(),
    }
