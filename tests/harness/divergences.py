# SPDX-License-Identifier: MIT
"""Aosabi harness: the named divergences of the side under test a comparison reads as the reference's answer.

A named divergence of docs/byte-equality.md section 3 that changes the bytes of an answer of the side under test
by a fixed rewrite is written here once, so a comparison asserts the side's own form and then reads the answer
as the reference writes it, every other byte held to the reference.

  - The title and the default query of GraphiQL, `Aosabi Playground` in the three places where the reference's page
    writes its own name, which meets no answer, as the side serves no page of GraphiQL since 0.1.1.
  - The variables of the uploads a delegation forwards, `_aosabiUpload_<n>` where the reference's federation writes
    its own prefix.
  - An answer of the function below the threshold of compress(), which carries its Content-Length and goes out
    uncoded on every platform, where the reference on a Worker and on Bun codes it.
  - A batch, which the function refuses as Yoga does with batching off, where an instance of the reference that
    enables batching runs it.
  - No answer as multipart/mixed: the field accept of a 406 lists no multipart/mixed, a client that accepts it
    alone gets that 406, and one that accepts an event stream beside it and no JSON gets the event stream.
  - An unexpected error answered with its own message outside NODE_ENV=development, which the reference masks
    there, read off the side's own answer.
  - The coding of the base pipeline of a Rust service, which codes no answer where the compress() of the reference
    codes one.
  - The messages of the plugins of a Rust service, the dependency cycle and the failed setup, which name an
    `Aosabi plugin` where the reference's name its own.
  - The refusals of a build, the schema the glue refuses, the configuration that throws at its load, the entry it
    cannot find and the entry with no default export, which name Aosabi and `aosabi.config` where the reference's
    name Pylon and `pylon.config`.
  - The cookies of the login of Zitadel, `aosabi-auth` and `aosabi_code_verifier` where the reference names
    `pylon-auth` and `pylon_code_verifier`, the session cookie under the name the option `cookieName` of the app
    gives where it gives one, and the session cookie of `useAuth` of the names of Pylon's canary with them.
  - The typed client of aosabi-query, whose head names Aosabi and whose bundle names the regions of
    `.aosabi/client/` where the reference's names `@getcronit/pylon/query` and `.pylon/client/`.
  - The cap on the tokens of a request document, 120,008 or the value of the key `maxTokens` of the app, under
    which a served engine parses every request, so a step hands the reference the budget `maxTokens` of the cap and
    the reference answers what the side answers.
  - The place of the current schema a build writes, `schema.graphql` in the folder of the project beside the copy
    under the output folder, where the reference writes `.pylon/schema.graphql` alone.
  - The type `text/ping`, which the csrf of a Rust service checks as it checks `text/plain`, so a comparison sends
    the reference `text/plain` in its place.
  - The field `Vary` of an answer `compress()` of a Rust service does not leave alone, which carries
    `Accept-Encoding` after the names the route wrote, coded or not, where the reference's carries those names
    alone, as the compress() of hono 4.13.16 writes it on the side ts.
  - The preflight of `cors()` of a Rust service whose option `origin` is no `*`, which appends `Origin` to a `Vary`
    a middleware before it set, where the reference's answers `Vary: Origin` alone, as the cors() of hono 4.13.16
    does on the side ts.

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

  - `kept-plugin-answer`, the answer of the plugin short of plugins-app, 418 and short, where the reference loses
    it and answers 500 with {"error":"Internal Server Error"}.

and the changes of serveStatic of @hono/node-server 2.1.6, which an app on
Node installs itself, against the 1.19.9 of the reference's lockfile:

  - HN1, a request of OPTIONS, which 2.1.6 hands to the next handler.
  - HN2, a range 2.1.6 cannot satisfy, answered 416, and a suffix range,
    answered with the last bytes of the file.

The file server of a Rust twin answers GET and HEAD alone:

  - `static-method`, a request of another method, which serve_static of aosabi-http hands to the next handler, the
    not found handler of hono where no route answers, where the reference answers OPTIONS with the file.

The secure defaults and the divergences after them are the steps of `served_defaults`, one step each, at the end of
the module, and the rules of the oracle close it in `ORACLE`.
"""

# The module holds no `from __future__ import annotations`: the oracle loads it by its path under a name of its own
# that sys.modules does not hold, and dataclasses reads a string annotation of a field through the module of its
# class in sys.modules, which fails there, while Python 3.14 defers every annotation without it (PEP 649).
import dataclasses
import json
import re
import zlib
from pathlib import Path
from urllib.parse import parse_qs

#: The name of the playground the side under test wrote until 0.1.1.
PLAYGROUND = b"Aosabi Playground"

#: The name of the playground the reference writes.
PLAYGROUND_REFERENCE = b"Pylon Playground"


def playground_body(body: bytes) -> bytes:
    """Return the body `body` of the side under test with the playground named as the reference names it."""
    return body.replace(PLAYGROUND, PLAYGROUND_REFERENCE)


def playground_length(value: str, body: bytes) -> str:
    """Return the value `value` of content-length of the body `body` of the side under test as the reference's.

    Each name of the playground in the body is one byte longer than the reference's.
    """
    count = body.count(PLAYGROUND)
    if not count or not value.strip().isdigit():
        return value
    return str(int(value) - count * (len(PLAYGROUND) - len(PLAYGROUND_REFERENCE)))


def playground_answer(answer: dict | None) -> dict | None:
    """Return an answer of the side under test, `{status, headers, body}`, with the playground as the reference's.

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
    """Return the HTTP/1.1 message `raw` of the side under test with the playground named as the reference names it.

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
    """Return why the body `side` of the side under test breaks the name of the playground beside the body `reference` of the reference.

    A page of the reference that names the reference's playground is answered on the
    side under test by a page that names Aosabi's.
    """
    if reference and PLAYGROUND_REFERENCE in reference and PLAYGROUND not in (side or b""):
        return ["the page of GraphiQL names no Aosabi Playground"]
    return []


#: The prefix of the variable of an upload a delegation forwards on the side under test.
UPLOAD = "_aosabiUpload_"

#: The prefix the federation of the reference writes.
UPLOAD_REFERENCE = "_pylonUpload_"


def upload_text(text: str) -> str:
    """Return the text `text` of the side under test with the variables of the uploads as the reference names them."""
    return text.replace(UPLOAD, UPLOAD_REFERENCE)


#: The name the messages of the plugins of a Rust service give a plugin, its dependency cycle and its failed setup.
PLUGIN = "Aosabi plugin "

#: The name the reference's messages give it.
PLUGIN_REFERENCE = "Pylon plugin "


def _plugin_names(text):
    """Return the names of the plugins in the type of `text`, str or bytes, the side's first."""
    if isinstance(text, bytes):
        return PLUGIN.encode("ascii"), PLUGIN_REFERENCE.encode("ascii")
    return PLUGIN, PLUGIN_REFERENCE


def plugin_text(text):
    """Return the text `text` of the side under test, str or bytes, with the plugins named as the reference names them."""
    own, reference = _plugin_names(text)
    return text.replace(own, reference)


def plugin_reasons(reference, side) -> list:
    """Return why the text `side` of the side under test breaks the messages of the plugins beside the text `reference` of the reference.

    Each message in which the reference names a plugin after itself names it
    an Aosabi plugin on the side under test, so the two texts hold the two
    names as often, and the side holds no name of the reference.
    """
    if reference is None or side is None:
        return []
    own, theirs = _plugin_names(side)
    expected = reference.count(_plugin_names(reference)[1])
    if side.count(own) != expected or theirs in side:
        return ["the side writes Aosabi plugin %d times and Pylon plugin %d times where the reference writes "
                "Pylon plugin %d times" % (side.count(own), side.count(theirs), expected)]
    return []


#: The refusals of a build that name Aosabi since the release, each the text of the reference and the text of the
#: side: the schema the glue refuses, which aosabi-build writes for `cargo aosabi build` and for `aosabi build`
#: through infer.wasm, and the configuration that throws at its load, which `aosabi build` writes with a comma in
#: place of the dash, and the entry `aosabi build` cannot find and the entry with no default export, which name the
#: entrypoint and the class of Aosabi.
BUILD_REFUSALS = (("Pylon generated an invalid GraphQL schema:", "Aosabi generated an invalid GraphQL schema:"),
                  ("Failed to load pylon.config \u2014 aborting build", "Failed to load aosabi.config, aborting build"),
                  ("Could not find index.ts (pylon entrypoint)", "Could not find index.ts (aosabi entrypoint)"),
                  ("Pylon entry must `export default` the app (e.g. `export default new Pylon(...)`).",
                   "Aosabi entry must `export default` the app (e.g. `export default new App(...)`)."))


def build_refusal(text: str) -> str:
    """Return the refusal `text` of the side under test with the texts that name Aosabi as the reference writes them."""
    for reference, side in BUILD_REFUSALS:
        text = text.replace(side, reference)
    return text


def build_refusal_reasons(reference: str | None, side: str | None) -> list:
    """Return why the refusal `side` of the side under test breaks the refusals that name Aosabi beside the refusal `reference` of the reference.

    Each such text the reference writes, the side writes in its own form.
    """
    return ["the side writes no %r where the reference writes %r" % (own, theirs)
            for theirs, own in BUILD_REFUSALS if theirs in (reference or "") and own not in (side or "")]


#: The cookies of the login of Zitadel that name Aosabi since the release, each the name of the reference and the
#: name of the side: the session the identity reads, the callback sets and the logout deletes, and the verifier of
#: PKCE the login sets and the callback reads. The generators of tests/gen write the names of the side, one case in
#: the upper case form, while the goldens and the reference served live keep the names of the reference. Since
#: 0.1.1 the option `cookie_name` of both halves, `cookieName` on the TypeScript side and in the options of
#: `useAuth` of the names of Pylon's canary, names the session cookie of the side, which a run whose variant sets
#: it reads through `login_cookies`, while the verifier keeps its name.
LOGIN_COOKIES = (("pylon-auth", "aosabi-auth"), ("pylon_code_verifier", "aosabi_code_verifier"))

#: A field of a head that sets a cookie, with the name of the cookie it sets.
SET_COOKIE = re.compile(rb"(?im)^(set-cookie:[ \t]*)([^=\r\n;]*)(?==)")


def login_cookies(session: str | None = None) -> tuple:
    """Return the pairs of LOGIN_COOKIES of a run whose side names its session cookie `session`, the name the
    option `cookieName` of its variant gives, or LOGIN_COOKIES for None and the name of Aosabi."""
    if session is None or session == LOGIN_COOKIES[0][1]:
        return LOGIN_COOKIES
    return ((LOGIN_COOKIES[0][0], session),) + LOGIN_COOKIES[1:]


def login_cookie(name: str, cookies: tuple = LOGIN_COOKIES) -> str:
    """Return the name the reference gives the cookie `name` of the side under test, with the
    pairs `cookies` of the run."""
    for reference, side in cookies:
        if name == side:
            return reference
    return name


def login_cookie_request(raw: bytes, cookies: tuple = LOGIN_COOKIES) -> bytes:
    """Return the request `raw` written for the side under test with the cookies of the login named as the reference
    names them, in the lower and the upper case form and in the head alone, so a reference served live reads the
    cookies it sets itself and the body keeps its length, with the pairs `cookies` of the run."""
    head, separator, body = raw.partition(b"\r\n\r\n")
    for reference, side in cookies:
        for theirs, own in ((reference, side), (reference.upper(), side.upper())):
            head = head.replace(own.encode("ascii"), theirs.encode("ascii"))
    return head + separator + body


def login_cookie_answer(data: bytes, cookies: tuple = LOGIN_COOKIES) -> bytes:
    """Return the answer `data` of the side under test with each cookie of the login its head sets named as the reference
    names it, every other byte as the side wrote it, with the pairs `cookies` of the run."""
    head, separator, body = data.partition(b"\r\n\r\n")
    names = {side.encode("ascii"): reference.encode("ascii") for reference, side in cookies}
    head = SET_COOKIE.sub(lambda match: match.group(1) + names.get(match.group(2), match.group(2)), head)
    return head + separator + body


def login_cookie_reasons(data: bytes, cookies: tuple = LOGIN_COOKIES) -> list:
    """Return why the answer `data` of the side under test breaks the names of the cookies, a cookie of the login it sets under the name
    the reference gives it, where the side names each of them after Aosabi or as the pairs `cookies` of the run
    name it."""
    head = data.partition(b"\r\n\r\n")[0]
    references = {reference.encode("ascii") for reference, side in cookies if reference != side}
    return ["the side sets the cookie %s under the name of the reference" % name.decode("ascii")
            for name in (match.group(2) for match in SET_COOKIE.finditer(head)) if name in references]


def login_cookie_field(value: str, cookies: tuple = LOGIN_COOKIES) -> str:
    """Return the value `value` of a `Set-Cookie` field of the side under test with the cookie of the login it sets named
    as the reference names it, any other value as it is, with the pairs `cookies` of the run."""
    name, mark, rest = value.partition("=")
    return login_cookie(name, cookies) + mark + rest if mark else value


def login_cookie_field_reasons(values: list, cookies: tuple = LOGIN_COOKIES) -> list:
    """Return why the `Set-Cookie` values `values` of the side under test break the names of the cookies, as `login_cookie_reasons` reads
    a whole answer."""
    references = {reference for reference, side in cookies if reference != side}
    return ["the side sets the cookie %s under the name of the reference" % value.split("=", 1)[0]
            for value in values if value.split("=", 1)[0] in references]


def login_cookie_text(text: str) -> str:
    """Return the text `text` written for the side under test with the cookies of the login named as the reference names
    them, in the lower and the upper case form, such as a request shape of tests/gen/zitadel.py, whose cookie the
    reference reads under its own name."""
    for reference, side in LOGIN_COOKIES:
        for theirs, own in ((reference, side), (reference.upper(), side.upper())):
            text = text.replace(own, theirs)
    return text


def login_cookie_outbound(records: list, cookies: tuple = LOGIN_COOKIES) -> list:
    """Return the requests the side under test sent the issuer, each with the cookies of the login its body names
    named as the reference names them, in the lower and the upper case form. The cases `z-40` to `z-45` send the
    cookies of a request in two `Cookie` fields, and on some hosts the token that a side then sends the issuer
    holds the name of a cookie of the login, so a side and the reference send there each its own name, every other
    byte as the side sent it, with the pairs `cookies` of the run."""
    out = []
    for record in records:
        body = record.get("body")
        if body:
            for reference, side in cookies:
                for theirs, own in ((reference, side), (reference.upper(), side.upper())):
                    body = body.replace(own, theirs)
            record = dict(record, body=body)
        out.append(record)
    return out


#: The texts of the typed client that name Aosabi since the release, each the text of the reference and the text of
#: the side: the head of `index.ts` and of the bundled `index.js`, which names the generator, and the regions of
#: the bundle, which name the client folder under the artifact directory.
CLIENT_TEXTS = (("Generated by @getcronit/pylon/query.", "Generated by Aosabi."),
                ("//#region .pylon/client/", "//#region .aosabi/client/"))


def client_text(text: str) -> str:
    """Return the client text `text` of the side under test with the texts that name Aosabi as the reference writes them."""
    for reference, side in CLIENT_TEXTS:
        text = text.replace(side, reference)
    return text


def client_text_reasons(reference: str | None, side: str | None) -> list:
    """Return why the client text `side` of the side under test breaks the texts that name Aosabi beside the text `reference`.

    Each such text the reference writes, the side writes in its own form as often.
    """
    return ["the side writes %r %d times where the reference writes %r %d times"
            % (own, (side or "").count(own), theirs, (reference or "").count(theirs))
            for theirs, own in CLIENT_TEXTS if (reference or "").count(theirs) != (side or "").count(own)]


#: The cap on the tokens of a request document a served engine parses where the app sets none, `REQUEST_TOKENS` of
#: crates/aosabi-lang/src/parser.rs.
REQUEST_TOKENS = 120_008


def request_parse_options(options: dict | None = None, max_tokens: int = REQUEST_TOKENS) -> dict:
    """Return the parser options `options` with the budget under which the reference parses a request
    document as a served engine of the side under test parses it under the cap `max_tokens`.

    A document of more tokens than the cap answers on the side the syntax error the parser of the reference answers
    under that budget, at the first token past it.
    The cap is `REQUEST_TOKENS` where the app sets none and the value of its key `maxTokens`, `.max_tokens(..)` of the
    chain of a Rust app, where it sets one.
    """
    return dict(options or {}, maxTokens=max_tokens)


#: The tokens of the lexical grammar of GraphQL, the ignored ones first: the byte order mark, white space, line
#: terminators, commas and comments, then a block string, a string, the spread, the other punctuators, a number and
#: a name.
TOKEN = re.compile(
    r'''
  (?P<ignored>[\ufeff\t \n\r,]+|\#[^\n\r]*)
| (?P<block>"""(?:\\"""|[^"]|"(?!""))*""")
| (?P<string>"(?:\\.|[^"\\\n\r])*")
| (?P<spread>\.\.\.)
| (?P<punctuator>[!$&()\:=@\[\]{|}])
| (?P<number>-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)
| (?P<name>[_A-Za-z][_0-9A-Za-z]*)
''',
    re.X | re.S,
)


def count_tokens(text: str) -> int | None:
    """Return the tokens of `text` the parser of graphql-js counts against its option `maxTokens`, or None for a
    text that holds a character outside the lexical grammar.

    Every token the lexer hands the parser counts, the end of the source left out, while white space, commas and
    comments are no token the lexer hands it.
    """
    position, count = 0, 0
    while position < len(text):
        found = TOKEN.match(text, position)
        if found is None:
            return None
        if found.lastgroup != "ignored":
            count += 1
        position = found.end()
    return count


def listed_document(tokens: int, head: str = "query", directive: str = "") -> str:
    """Return a request document of `tokens` tokens whose parse is linear and which validation rejects.

    The operation `head` declares a variable it never uses, whose default value is a list of empty lists of two
    tokens each, with one list `[1]` of three in front where the items hold an odd count, and `directive` stands on
    the operation when given. Posted at the cap of a request and past it, such documents test that cap.
    """
    before = head + "($v: [[Int]] = ["
    after = "])" + (" " + directive if directive else "") + " { __typename }"
    pairs, odd = divmod(tokens - count_tokens(before + after), 2)
    document = before + "[1]" * odd + "[]" * (pairs - odd) + after
    assert count_tokens(document) == tokens, (tokens, count_tokens(document))
    return document


#: The place of the current schema a build writes since 0.1.1, each relative to the folder of the project: the
#: reference writes the schema under its output folder alone, which the ignore file of its scaffold keeps out of
#: git, and the side writes it to the folder of the project as well, the folder of Cargo.toml for
#: `cargo aosabi build` and the folder of package.json for `aosabi build`, with the bytes of the copy under its
#: output folder, which stays. No answer changes, so no comparison reads the file.
SCHEMA_PLACE_REFERENCE = ".pylon/schema.graphql"
SCHEMA_PLACE = "schema.graphql"


def schema_place_reasons(project, copy) -> list:
    """Return why the build of the project in the folder `project` does not write its schema there as it writes the copy `copy`.

    A build that writes the copy under its output folder writes SCHEMA_PLACE in the folder of the
    project with the same bytes.
    """
    place = Path(project) / SCHEMA_PLACE
    if not place.is_file():
        return ["the build writes no %s in the folder of the project" % SCHEMA_PLACE]
    if not Path(copy).is_file():
        return ["the build writes %s in the folder of the project and no copy %s"
                % (SCHEMA_PLACE, Path(copy).name)]
    if place.read_bytes() != Path(copy).read_bytes():
        return ["%s in the folder of the project differs from the copy under the output folder"
                % SCHEMA_PLACE]
    return []


#: The type of the ping of a link, which the csrf of a Rust service, an app's own `csrf(options)` and the default
#: of the base pipeline, checks as it checks the types a form element sends, a named divergence.
PING_TYPE = "text/ping"

#: The type a comparison sends the reference in the place of `PING_TYPE`, which the check of the reference reads
#: as a type a form element sends, so both sides check the request alike.
PING_TYPE_REFERENCE = "text/plain"


def ping_type(value: str) -> str:
    """Return the `Content-Type` value `value` of a request as a comparison sends it the reference, the type
    `PING_TYPE` at its start, in any case, written as `PING_TYPE_REFERENCE`, any other value as it is."""
    if value[:len(PING_TYPE)].lower() == PING_TYPE:
        return PING_TYPE_REFERENCE + value[len(PING_TYPE):]
    return value


def ping_type_case(case: dict) -> dict:
    """Return a case `case` of the csrf middleware as a comparison sends it the reference, its `Content-Type` field
    through `ping_type`, every other field as it is."""
    headers = [[name, ping_type(value) if name.lower() == "content-type" else value]
               for name, value in case["headers"]]
    return dict(case, headers=headers)


#: The codings the compress() of the reference writes, ENCODING_TYPES of hono 4.11.9, and x-gzip, the alias of gzip
#: (RFC 9110 section 8.4.1.3).
CODED = ("gzip", "x-gzip", "deflate")


def served_coding(reference: str | None) -> str | None:
    """Return the content-encoding the twin of a Rust service writes where the reference writes `reference`.

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


#: The path of the GraphQL endpoint of the reference's handler.
ENDPOINT_PATH = "/graphql"

#: The two endings of the GraphQL endpoint of the reference's handler, `/graphql` with and without its slash.
ENDPOINT_ENDINGS = (ENDPOINT_PATH, ENDPOINT_PATH + "/")


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
    return _path_of(target) == ENDPOINT_PATH


def outside_endpoint(reference, viewer: bool = False, target: str | None = None) -> str | None:
    """Return the name of the divergence the answer `reference` of Pylon stands for on the side ts, or None.

    `target` is the request target of the answer. A request with no target
    names none. A request of the endpoint names `cors-plugin` for the preflight of
    CORS of Yoga, status 204 and `content-length: 0`, since the engine
    answers no `OPTIONS` request and hands it on as an unhandled route,
    and names nothing else, since the engine answers every other request of
    the endpoint on the side ts. For a target that `is_endpoint` rejects,
    `yoga-plugins` for the health check of Yoga, status 200, no body and the field
    x-yoga-id, and for the preflight of CORS of Yoga outside the endpoint,
    status 204 and `content-length: 0`, `endpoint-slash` for the path of the
    endpoint and a slash and `endpoint-ending` for a target that ends in the
    endpoint, each of which Yoga answered as a request of the endpoint,
    `landing-plugin` for the landing page, read by its title or, for an answer to
    HEAD, by its status and its type, and `graphiql-viewer` for the viewer,
    `viewer` true for a request of `GET /viewer`. The side ts answers each with
    the not found handler of hono, `hono_not_found`.
    """
    if reference is None or target is None:
        return None
    preflight = reference.status == 204 and _values(reference, "content-length") == ["0"]
    if is_endpoint(target):
        return "cors-plugin" if preflight else None
    body = body_of(reference)
    if reference.status == 200 and not body and _values(reference, HEALTH_FIELD):
        return "yoga-plugins"
    if preflight:
        return "yoga-plugins"
    if _path_of(target) == ENDPOINT_PATH + "/":
        return "endpoint-slash"
    if target.endswith(ENDPOINT_ENDINGS):
        return "endpoint-ending"
    if LANDING_REFERENCE_TITLE in body:
        return "landing-plugin"
    if reference.status == 404 and not body and _values(reference, "content-type") == [LANDING_TYPE]:
        return "landing-plugin"
    if viewer or VIEWER_TITLE in body:
        return "graphiql-viewer"
    return None


#: The answer of the reference where its dispatch of the middleware of the plugins loses the answer of a later
#: middleware, the error answer of its base pipeline to the error hono throws for a context with no answer.
LOST_ANSWER = (500, b'{"error":"Internal Server Error"}')

KEPT_ANSWER = (418, b"short", "text/plain; charset=UTF-8")


def kept_plugin_answer(reference, ours) -> str | None:
    """Return `kept-plugin-answer` where the reference answers `reference`, the 500 of a lost answer, and the side `ours` the answer it keeps.

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
    return "kept-plugin-answer"


#: The value hono 4.13.16 adds to the field Vary of an answer its compress() may code.
VARY_ENCODING = "Accept-Encoding"


def hono_vary(side: list, reference: list) -> list:
    """Return the values `side` of the field Vary of the side ts read as hono 4.11.9 writes them.

    compress() of hono 4.13.16 sets `Vary: Accept-Encoding` on an answer whose type it codes, coded or not,
    appending the name to a Vary that holds another, where compress() of 4.11.9 sets none. Where the values of the
    reference name no Accept-Encoding, the name is taken out of the values of the side, a field of that one name
    with it.
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
    """Return the values `side` of the field of the memo of transit-app read as hono 4.11.9 writes them.

    The memo writes the names of the fields of the c.res it kept after the compress() of the base pipeline has run,
    which the module of the app stands before, so compress() of hono 4.13.16 adds vary to those names where 4.11.9
    adds none. Where no value of the reference names vary, the name is taken out of the names of each value of the
    side, and every other byte of the value stays.
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

    Each field is a pair of its name and its value, `reference` the fields of the answer of the reference. The
    values of Vary are read through `hono_vary`, and so is the list of names of the field of the memo of
    transit-app, `MEMO_FIELD`, through `hono_memo`.
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
    """Return the change of serveStatic of @hono/node-server 2.1.6 against 1.19.9 that the answer `side` shows, or None.

    `method` and `headers`, pairs of a name and a value, are those of the request, `reference` the answer of the
    reference and `side` the answer of the side ts. HN1 is a request of OPTIONS, which 1.19.9 answers 200 with the
    type of the file and no body and 2.1.6 hands to the next handler, the not found handler of hono where no route
    answers. HN2 is a range: one 2.1.6 cannot satisfy it answers 416 with `Content-Range: bytes */<size>` and no
    body, where 1.19.9 fails with 500, and the suffix range `bytes=-<n>` it answers with the last n bytes, where
    1.19.9 answers the bytes 0 to n.
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


def serve_static_method(method: str, reference, side) -> str | None:
    """Return `static-method` where the answer `side` of a Rust twin to a request of `method` is hono's not found, or None.

    `reference` is the answer of the reference to the same request.
    """
    if reference is None or side is None or method in ("GET", "HEAD"):
        return None
    return "static-method" if reference.status == 200 and hono_not_found(side) else None


#: The threshold of compress(), the length below which an answer that carries its Content-Length stays uncoded.
COMPRESS_THRESHOLD = 1024

#: The media types of the answers of the regular processor of the function, whose body is a string.
GRAPHQL_TYPES = ("application/json", "application/graphql-response+json")


def uncoded_names(named: list, ours, host) -> list:
    """Return the names `named` of a served answer with those of an uncoded small answer named `uncoded-small`.

    `named` are the differences the judge of a served answer names where the side under
    test answers as the reference on Node and the reference on its host does not, each opening
    with its field, and `ours` and `host` are the answers of the side and of
    the reference's build on the host, answers of `http.py`. The function writes the
    Content-Length of every answer whose body is a string on every platform,
    so compress() leaves an answer below 1,024 bytes uncoded where the reference on a
    Worker and on Bun codes it, and the answer carries its length where
    the reference's there carries none. A name is `uncoded-small` when the side's answer is one of
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
            entry = "uncoded-small " + entry
        labelled.append(entry)
    return labelled


#: The instances of the in process capture whose reference enables batching.
BATCHING_INSTANCES = ("batching", "secondBatchingLimit2", "secondBatchingTrue")

#: The body of the refusal of a batch, which Yoga answers with batching off and the function to every batch.
BATCH_REFUSAL = b'{"errors":[{"message":"Batching is not supported.","extensions":{"code":"BAD_REQUEST"}}]}'


def batch_refused(instance: str, body) -> bool:
    """Return whether the in process case of `instance` with the body `body` is a batch the function refuses.

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


#: The media type of the processor that leaves the function.
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
    """Return the field accept of a 406 of the reference as the function writes it, without multipart/mixed."""
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
    """Return whether the Accept `accept` holds a media type that matches text/event-stream.

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
    """Return the field accept of the 406 the function answers a query asked for under the Accept `accept`.

    Each processor offers its media types once per media type of the request,
    an event stream first and JSON after it, joined as Yoga joins them.
    """
    count = len(media_types_in_order(accept))
    return ACCEPT_JOIN.join(media for types in PROCESSOR_TYPES for _ in range(count) for media in types)


def multipart_chunks(body: bytes) -> list:
    """Return the JSON texts of the parts of a body of Yoga's processor of multipart/mixed, in order.

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
    """Return the body Yoga's processor of an event stream writes for the results `chunks`.

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
    """Return the status of the event stream of the results `chunks` whose answer as multipart/mixed has `status`.

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
    """Return the answer `answer` of the reference, an answer of `http.py`, with the field accept of a 406 without multipart/mixed."""
    if answer is None or answer.status != 406:
        return answer
    lines = [(name, accept_without_multipart(value) if name.lower() == "accept" else value)
             for name, value in answer.header_lines]
    if lines == list(answer.header_lines):
        return answer
    return dataclasses.replace(answer, header_lines=lines)


def served_event_stream(answer):
    """Return the answer `answer` of the processor of multipart/mixed, an answer of `http.py`, as the event stream the function answers in its place.

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
    """Return why the answer `ours` is not the one of the function where the reference answered `reference` as multipart/mixed, or None.

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
            return ["no answer where the function answers an event stream"]
        reasons = []
        if ours.status != expected.status:
            reasons.append("status %d where the event stream of the function answers %d" % (ours.status, expected.status))
        if (ours.header("content-type") or "").split(";")[0].strip().lower() != EVENT_STREAM:
            reasons.append("content-type %r where the function answers %s" % (ours.header("content-type"), EVENT_STREAM))
        if ours.decoded != expected.decoded:
            reasons.append("a body of %d bytes where the event stream of the function holds %d: %r | %r"
                           % (len(ours.decoded), len(expected.decoded), ours.decoded[:120], expected.decoded[:120]))
        return reasons
    if ours is None:
        return ["no answer where the function answers 406"]
    reasons = []
    if ours.status != 406:
        reasons.append("status %d where the function answers 406" % ours.status)
    if ours.header("accept") != not_acceptable(accept):
        reasons.append("accept %r where the function lists %r" % (ours.header("accept"), not_acceptable(accept)))
    if ours.decoded:
        reasons.append("a body of %d bytes where the 406 of the function has none" % len(ours.decoded))
    return reasons


def served_without_multipart(answer, accept):
    """Return the answer `answer` of the reference served on Node, an answer of `http.py`, as the function answers it.

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
    """Return the recorded answer `expected` of the processor of multipart/mixed, `{status, status_text, headers, body}`, as the event stream the function answers in its place.

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


def in_process_without_multipart(request_headers: dict, expected: dict) -> dict:
    """Return the recorded answer `expected` of an in process case, `{status, status_text, headers, body}`, as the function answers it.

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

#: The root of the tree whose sources the steps read, the landing page of the reference among them.
ROOT = Path(__file__).resolve().parents[2]

#: The keys of the variants that the configuration overlay spreads over the reference, with the twins of the
#: fixtures that select them through AOSABI_CONFIG setting the same but `cors`, `graphiql`, `viewer` and
#: `landingPage`, keys Aosabi does not have, which the steps read as keys of the reference.
OVERLAY_KEYS = {
    "graphiql_false": {"graphiql": False},
    "landing_false": {"landingPage": False},
    "both": {"graphiql": False, "landingPage": False},
    "switches_on": {"landingPage": True, "graphiql": True, "multipart": True, "viewer": True},
    "cors_star": {"cors": {"origin": "https://*.example.invalid"}},
    "cors_list": {"cors": {"origin": ["https://a.example.invalid", "https://b.example.invalid"]}},
    "tokens_low": {"maxTokens": 1000},
    "tokens_high": {"maxTokens": 240_016},
    "introspection_false": {"introspection": False},
    "masked_errors_true": {"maskedErrors": True},
}

#: The keys the own configuration of a fixture sets on the reference, which the twin does not have, and the steps
#: read as keys of the reference.
FIXTURE_KEYS = {
    "blog-app": {"graphiql": False, "landingPage": False},
}

#: The keys the twin of a fixture sets beyond the reference's configuration, by fixture.
TWIN_KEYS = {
    # The twins that take uploads set `multipart: true`.
    "surface-app": {"multipart": True},
    "federation-catalogue-app": {"multipart": True},
    "federation-boundary-app": {"multipart": True},
    "docs-app": {"multipart": True},
}

#: The keys the steps read on the side ts: `introspection`, `maskedErrors` and `maxTokens`, the bytes of the boot
#: record of crates/aosabi-js/src/boot.rs, and `graphiql` and `landingPage`, which no side reads and the steps read
#: as keys of the reference.
TS_KEYS = ("graphiql", "landingPage", "maxTokens", "introspection", "maskedErrors")

#: The keys of the instances of the in process capture, which leave multipart and cors unset.
IN_PROCESS_KEYS = {}

#: The built in plugins of the reference that the side under test does not have: the viewer and the landing page.
EMPTY_SLOTS = ("viewer", "landing")

#: The label of an unnamed plugin in the message of a failed setup, its index in the sorted list of the pass.
UNNAMED = re.compile(r"(Pylon plugin #)(\d+)( \(unnamed\) failed during setup: )")

#: The route of the viewer of the reference, which its setup registers.
VIEWER_ROUTE = "/viewer"


def slot_label(text: str, shift: int = 1) -> str:
    """Return the message `text` of a failed setup with the index of its unnamed plugin lowered by `shift`.

    The viewer opens the sorted list of the first pass of the reference,
    so an unnamed plugin of that pass stands one place lower on the side under
    test, which has no viewer.
    """
    return UNNAMED.sub(lambda m: "%s%d%s" % (m.group(1), int(m.group(2)) - shift, m.group(3)), text)


def boot_slots(line: dict, description: dict, slots: tuple = EMPTY_SLOTS) -> dict:
    """Return the answer `line` of the reference to a boot `description` of tests/gen/plugins.py as the side
    under test answers it, with the built in plugins of `slots` out of the arrays, the sorted and the loaded
    labels of its passes.

    A failed setup of an unnamed plugin of the first pass names an index one
    lower where the viewer is among `slots`, and one of the strategy `last`
    fails in the second pass, whose sorted list holds no viewer.
    """
    def kept(label):
        return not (isinstance(label, dict) and label.get("builtin") in slots)

    if "passes" in line:
        passes = [dict(one, array=[entry for entry in one["array"] if kept(entry[0])],
                       sorted=[label for label in one["sorted"] if kept(label)],
                       loaded=[label for label in one["loaded"] if kept(label)]) for one in line["passes"]]
        return dict(line, passes=passes)
    if "thrown" in line and "viewer" in slots:
        thrower = next((row for row in description.get("plugins", [])
                        if str(row.get("setup", "")).startswith("throw")), None)
        if thrower is not None and thrower.get("strategy") != "last":
            return dict(line, thrown=slot_label(line["thrown"]))
    return line


def boot_stream(stream: bytes, descriptions: list, slots: tuple = EMPTY_SLOTS) -> bytes:
    """Return the answers `stream` of the reference to the boots `descriptions`, one JSON line each, with
    `boot_slots` applied, a line it leaves as it is kept as its bytes."""
    by_id = {description["id"]: description for description in descriptions}
    lines = []
    for text in stream.decode("utf-8").splitlines():
        line = json.loads(text)
        held = boot_slots(line, by_id.get(line.get("id"), {}), slots)
        lines.append(text if held == line else json.dumps(held, ensure_ascii=False, separators=(",", ":")))
    return "".join(text + "\n" for text in lines).encode("utf-8")


def report_slots(report: dict, slots: tuple = EMPTY_SLOTS) -> dict:
    """Return the boot report `report` of the reference as the side under test writes it, with the built in
    plugins of `slots` out of the array written back and the route of the viewer out of its routes."""
    after = [entry for entry in report.get("config_plugins_after_boot") or []
             if not (isinstance(entry.get("label"), dict) and entry["label"].get("builtin") in slots)]
    routes = report.get("routes") or []
    if "viewer" in slots:
        routes = [route for route in routes if not (route.get("method") == "GET" and route.get("path") == VIEWER_ROUTE)]
    return dict(report, config_plugins_after_boot=after, routes=routes)


def boot_keys(boot: dict) -> dict:
    """Return a boot description `boot` of tests/gen/plugins.py as a boot of the twin takes it.

    It carries `introspection: false` where it sets `graphiql: false`, the
    key that adds on the twin the refusal the reference adds on
    `graphiql: false`. The keys `graphiql` and `landing_page` leave it,
    since the twin has neither.
    """
    keys = dict(boot)
    if boot.get("graphiql") is False:
        keys.setdefault("introspection", False)
    keys.pop("graphiql", None)
    keys.pop("landing_page", None)
    return keys


#: The key under which `keys_of` hands the steps the `NODE_ENV` the side under test serves under, which no
#: configuration sets and which the variants pair with each variant.
NODE_ENV_KEY = "nodeEnv"

#: The key under which `keys_of` hands the steps the middleware cors() of the app of a fixture, its entries of
#: `APP_CORS`, which is source of the app and no key of a configuration.
APP_CORS_KEY = "appCors"

#: The key under which `keys_of` hands the steps the prefixes the app of a fixture strips before it dispatches a
#: request through the app again, its entries of `APP_PREFIXES`, which are source of the app and no key of a
#: configuration.
APP_PREFIX_KEY = "appPrefixes"


def keys_of(fixture: str, variant: str, side: str = "rs", node_env: str | None = None) -> dict:
    """Return the keys of the configuration the side under test serves `fixture` under `variant` with.

    The keys of the fixture's own configuration and of the variant hold on both
    sides. The twin of the side rs adds the keys of `TWIN_KEYS`, and the module
    of the side ts reads the keys of `TS_KEYS` alone. The middleware cors() of
    hono that the app of the fixture puts before the routes of the function
    holds on both sides, under `APP_CORS_KEY`, and so do the prefixes the app
    strips before it dispatches a request through the app again, under
    `APP_PREFIX_KEY`, and the `NODE_ENV` `node_env` the side serves under,
    under `NODE_ENV_KEY`, where the caller names one other than `unset`.
    """
    keys = dict(FIXTURE_KEYS.get(fixture, {}))
    keys.update(OVERLAY_KEYS.get(variant, {}))
    if side == "ts":
        keys = {name: value for name, value in keys.items() if name in TS_KEYS}
    else:
        keys.update(TWIN_KEYS.get(fixture, {}))
    if fixture in APP_CORS:
        keys[APP_CORS_KEY] = APP_CORS[fixture]
    if fixture in APP_PREFIXES:
        keys[APP_PREFIX_KEY] = APP_PREFIXES[fixture]
    if node_env not in (None, "unset"):
        keys[NODE_ENV_KEY] = node_env
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


#: The prefixes the app of a fixture strips from a path before it dispatches the request through the app again, by
#: fixture.
APP_PREFIXES = {
    "lines-catalogue-app": ("/catalogue-api",),
}


def app_target(ask: Ask, keys: dict) -> str:
    """Return the target of `ask` as the app hands it to the function, without the prefix among `keys` it strips."""
    for prefix in keys.get(APP_PREFIX_KEY, ()):
        if ask.path.startswith(prefix + "/"):
            return ask.target[len(prefix):]
    return ask.target


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

#: The version the landing page of the reference names.
LANDING_VERSION = "2.9.5"
#: The source of the landing page of the reference and the text that opens its template.
LANDING_SOURCE = Path("vendor") / "pylon" / "packages" / "pylon" / "src" / "plugins" / "use-unhandled-route.ts"
LANDING_ANCHOR = "html`<!doctype html>"


def landing_page() -> bytes:
    """Return the landing page of the reference, the template of its source with its version.

    The template is the raw slice between the backticks its anchor opens.
    """
    text = (ROOT / LANDING_SOURCE).read_text(encoding="utf-8")
    opened = text.index("`", text.index(LANDING_ANCHOR)) + 1
    closed = opened
    while text[closed] != "`":
        closed += 2 if text[closed] == "\\" else 1
    return text[opened:closed].replace("${version}", LANDING_VERSION, 1).encode("utf-8")


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
    """Return the answer `reply` of the reference to `ask` as the side under test with `keys` answers it."""
    for step, _ in NAMED_STEPS:
        reply = step(reply, ask, keys)
    return reply


def star_origin_as_text(reply, ask, keys):
    """A string option `origin` that holds a star is compared as text.

    Yoga's `origin.includes('*')` echoes every origin for it with `Vary: Origin`, and the function as the row
    leaves it answers the option itself in `Access-Control-Allow-Origin` and leaves `Origin` out of `vary`. In the
    series `no_options_answer` then gives a preflight the answer of a route no handler answers, and
    `no_cors_fields` takes every field this step writes out of every other answer, since the engine answers no
    CORS.
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


def origin_outside_list(reply, ask, keys):
    """An origin outside a list of two or more origins gets no `Access-Control-Allow-Origin` and no credentials.

    Yoga answers it `null` with `Access-Control-Allow-Credentials: true`, the origin a sandboxed frame sends, and
    an origin in the list keeps the answer the row leaves. In the series `no_cors_fields` then takes the fields of
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


def no_options_answer(reply, ask, keys):
    """The engine answers no `OPTIONS` request, which falls through to the web server.

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


def no_viewer(reply, ask, keys):
    """No viewer under any key.

    The reference serves its viewer at `/viewer` under every configuration, and the
    function hands the route on as one no handler answers.
    """
    if ask.path != "/viewer" or ask.method not in ("GET", "HEAD"):
        return reply
    if reply.status != 200:
        return reply
    return fallthrough(ask, keys.get("landingPage") is not False)


def no_graphiql_page(reply, ask, keys):
    """The function serves no page of GraphiQL under any key.

    Yoga serves it unless the key is `false`, and the function answers a request for it as one for a type it
    does not serve, the 406 of `not_acceptable` with no body, with the content type the adapter of Node gives it
    where that adapter writes it.
    """
    if reply.status != 200 or PLAYGROUND_REFERENCE not in reply.body:
        return reply
    lines = [("accept", not_acceptable(ask.header("accept")))] + cors_lines(reply)
    if ask.served and ask.node:
        lines.append(("content-type", NOT_ACCEPTABLE_TYPE))
    return Reply(406, "Not Acceptable", lines, b"")


def multipart_on_key(reply, ask, keys):
    """A multipart body is read only on `multipart: true`.

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


def no_cors_fields(reply, ask, keys):
    """The answer without its fields of CORS, since the engine answers no CORS.

    Every field `CORS_FIELDS` names leaves, and `vary` with them where it holds
    the names of CORS alone. On a path the app's own cors() of hono guards,
    whose fields the web server sets on both sides, only the fields Yoga's CORS
    added leave, `without_engine_cors`.
    """
    entry = app_cors(ask, keys)
    if entry is None:
        return without_cors(reply)
    return without_engine_cors(reply, ask, entry)


#: The title of the landing page of the reference.
LANDING_REFERENCE_TITLE = b"<title>Welcome to Pylon</title>"


def no_landing_page(reply, ask, keys):
    """The function serves no landing page under any key.

    The reference serves it on a route no handler answers unless the key is
    `false`, and the function hands the route to the not found answer of hono,
    which carries the fields the middleware of the app set around the route,
    as the landing page does, every field but those of its body and of the
    connection.
    """
    if not is_landing(reply):
        return reply
    done = fallthrough(ask, False)
    kept = [(name, value) for name, value in reply.lines
            if name.lower() not in BODY_FIELDS + CONNECTION_FIELDS]
    return dataclasses.replace(done, lines=kept + done.lines) if kept else done


def no_health_check(reply, ask, keys):
    """The function answers no health check, which falls through to the web server.

    Yoga's health check answers every method on a URL that ends in `/health` with 200, no body and `x-yoga-id`
    before Pylon's routes run, and the function hands such a request on as an unhandled route, to the not found
    answer of the web server where it is served and to the empty 404 of the unhandled route in process. An
    `OPTIONS` request takes this line as well, since the health check answers it before Yoga's CORS.
    """
    if reply.status != 200 or reply.header(HEALTH_FIELD) is None:
        return reply
    if not ask.served:
        return Reply(404, "Not Found", [("content-type", "text/plain;charset=UTF-8")], b"")
    return fallthrough(ask, False)


def endpoint_by_path(reply, ask, keys):
    """The function matches its endpoint by its whole path.

    Yoga runs a request whose whole URL ends in the endpoint or in the endpoint and a slash as one to the endpoint,
    and the function hands such a request on as an unhandled route, to the not found answer of the web server where
    it is served and to the empty 404 of the unhandled route in process. A request under a prefix the app strips
    before it dispatches the request through the app again reaches the function with the target after the prefix,
    which the step reads.
    """
    endpoint = keys.get("graphqlEndpoint") or "/graphql"
    target = app_target(ask, keys)
    if target.split("?")[0] in (endpoint, endpoint + "/") or not target.endswith((endpoint, endpoint + "/")):
        return reply
    if reply.status == 404:
        return reply
    if not ask.served:
        return Reply(404, "Not Found", [("content-type", "text/plain;charset=UTF-8")], b"")
    return fallthrough(ask, False)


def endpoint_path_alone(reply: Reply, ask: Ask, keys: dict) -> Reply:
    """The function takes the path of its endpoint alone for its endpoint.

    Yoga runs a request to the endpoint and a slash as one to the endpoint, and the function hands it on as an
    unhandled route, as the step `endpoint_by_path` hands on a URL that merely ends in the endpoint.
    """
    endpoint = keys.get("graphqlEndpoint") or "/graphql"
    if app_target(ask, keys).split("?")[0] != endpoint + "/" or reply.status == 404:
        return reply
    if not ask.served:
        return Reply(404, "Not Found", [("content-type", "text/plain;charset=UTF-8")], b"")
    return fallthrough(ask, False)


#: The refusal of the plugin `useDisableIntrospection`, which the handler of the reference adds on
#: `graphiql: false`.
INTROSPECTION_REFUSAL = b"GraphQL introspection has been disabled"

#: The answer of the document `{__schema{queryType{name}}}` of family K, which the
#: reference refuses with one error per field of introspection the document holds.
INTROSPECTION_ANSWER = b'{"data":{"__schema":{"queryType":{"name":"Query"}}}}'


def introspection_on_key(reply: Reply, ask: Ask, keys: dict) -> Reply:
    """The function refuses introspection on `introspection: false` alone.

    The handler of the reference adds the refusal on `graphiql: false`, and the function answers the document under
    that key. The step holds the one document of the cases, which it knows by the two fields its refusal names, so
    the refusal of any other document stays a difference of the comparison.
    """
    if not introspection_refused(keys):
        return reply
    if reply.body.count(INTROSPECTION_REFUSAL) != 2:
        return reply
    if b'\\"__schema\\"' not in reply.body or b'\\"queryType\\"' not in reply.body:
        return reply
    lines = [(name, str(len(INTROSPECTION_ANSWER)) if name.lower() == "content-length" else value)
             for name, value in reply.lines]
    return Reply(200, reply.reason if reply.status == 200 else "OK", lines, INTROSPECTION_ANSWER)


#: The steps `steps` runs, in the order they rewrite an answer, each with the divergence its row names.
NAMED_STEPS = ((star_origin_as_text, "cors-plugin"), (origin_outside_list, "cors-plugin"),
               (no_options_answer, "cors-plugin"), (no_health_check, "no-health-check"),
               (endpoint_by_path, "endpoint-ending"), (endpoint_path_alone, "endpoint-slash"),
               (no_viewer, "graphiql-viewer"), (no_graphiql_page, "graphiql-viewer"),
               (introspection_on_key, "introspection-key"), (multipart_on_key, "multipart"),
               (no_cors_fields, "cors-plugin"), (no_landing_page, "landing-plugin"))


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

    A Worker's runtime gzips the gzip of compress() again, so a body that is still a gzip member after the coding
    its field names is undone once more.
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
    `gen/http.py` or an `Ask`, as the side under test with the keys `keys` answers it.

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
    function with the keys `keys` answers the request `request`, `{method, url, headers}`.

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


# The mask of the reference outside `NODE_ENV=development`.
#
# Pylon puts `maskedErrors: false` before the spread of its configuration under
# `NODE_ENV=development` alone, so outside development Yoga masks each
# unexpected error of a result, and the side masks one only where the key
# `maskedErrors` of its configuration turns the mask on. An answer of the
# reference outside development so holds the masked form of each error the
# side answers with its own message. The comparisons under development hold
# that message to the reference's, so outside development the answer of the
# side guides the rewrite: a masked error of the reference reads as the error
# at the same place of the side's answer where that error is no masked error
# and the reference's error is its masked form, and every other byte of the
# reference's answer stays. Where the key turns the mask on, the mask of the
# reference reads `NODE_ENV` and writes the original error with its stack
# under `extensions.originalError` under development, and the side's mask
# reads no `NODE_ENV` and writes none, so there a masked error of the reference
# reads as the error at its place of the side's answer where that error is the
# reference's without its original error.

#: The message and the code of a masked error, whose `unexpected` the result processor leaves out.
MASKED_MESSAGE = "Unexpected error."
MASKED_CODE = "INTERNAL_SERVER_ERROR"


def mask_differs(keys: dict) -> bool:
    """Return whether an answer of the reference reads off the answer of the side under test with the keys `keys`:
    outside `NODE_ENV=development` with no key `maskedErrors`, where the reference masks an error the side
    answers with its own message, and under development with the key on, where the mask of the reference writes the
    original error that the side's mask leaves out."""
    masks = keys.get("maskedErrors")
    if keys.get(NODE_ENV_KEY) == "development":
        return masks not in (None, False)
    return masks is None


def masked_error(error: dict) -> dict:
    """Return the error `error` of a result as the mask writes it: the message of a masked error, the `locations`
    and the `path` of `error`, and the code of a masked error alone among its extensions."""
    masked = {"message": MASKED_MESSAGE}
    for name in ("locations", "path"):
        if name in error:
            masked[name] = error[name]
    masked["extensions"] = {"code": MASKED_CODE}
    return masked


def json_space(text: str, at: int) -> int:
    """Return the index of the first character at or after `at` of `text` that is no white space of JSON."""
    while at < len(text) and text[at] in " \t\n\r":
        at += 1
    return at


def json_end(text: str, at: int) -> int:
    """Return the index after the JSON value that starts at `at` of `text`, with no recursion."""
    depth = 0
    while True:
        char = text[at]
        if char == '"':
            at += 1
            while text[at] != '"':
                at += 2 if text[at] == "\\" else 1
            at += 1
        elif char in "{[":
            depth += 1
            at += 1
            continue
        elif char in "}]":
            depth -= 1
            at += 1
        elif char in ",: \t\n\r":
            at += 1
            continue
        else:
            while at < len(text) and text[at] not in ",:]} \t\n\r":
                at += 1
        if depth == 0:
            return at


def error_spans(text: str):
    """Return the spans `(start, end)` of the entries of the member `errors` of the JSON object `text`, or None."""
    at = json_space(text, 0)
    if text[at:at + 1] != "{":
        return None
    at = json_space(text, at + 1)
    while at < len(text) and text[at] == '"':
        key_end = json_end(text, at)
        key = json.loads(text[at:key_end])
        at = json_space(text, json_space(text, key_end) + 1)
        end = json_end(text, at)
        if key == "errors":
            if text[at] != "[":
                return None
            spans, inner = [], json_space(text, at + 1)
            while text[inner] != "]":
                item_end = json_end(text, inner)
                spans.append((inner, item_end))
                inner = json_space(text, item_end)
                if text[inner] == ",":
                    inner = json_space(text, inner + 1)
            return spans
        at = json_space(text, end)
        if text[at:at + 1] == ",":
            at = json_space(text, at + 1)
    return None


#: The members of an error the writer of a result writes, `toJSON` of `GraphQLError`, which hold no stack.
ERROR_MEMBERS = ("message", "locations", "path", "extensions")


def masked_error_reads(error: dict, side: dict) -> bool:
    """Return whether the error `error` of a result of the reference reads as the error `side` at its place of the
    side's result: a masked error where the side answers its own message, or a masked error that carries the
    original error under `extensions.originalError` where the side's masked error carries none. The error of the
    side holds no member but those of `ERROR_MEMBERS`, so no stack."""
    if any(name not in ERROR_MEMBERS for name in side):
        return False
    if error == masked_error(error):
        return side != masked_error(side) and masked_error(side) == error
    extensions = error.get("extensions")
    if not isinstance(extensions, dict) or "originalError" not in extensions:
        return False
    bare = dict(error, extensions={name: value for name, value in extensions.items() if name != "originalError"})
    return bare == masked_error(bare) and side == bare


def unmasked_result(result: bytes, guide: bytes) -> bytes:
    """Return the GraphQL result `result` of the reference with each masked error read as the error at its place of
    the result `guide` of the side under test.

    An error reads so where `masked_error_reads` holds for it and the error at the
    same place of `guide`, whose errors are as many. The error of the side is
    written as the side wrote it, and every other byte of `result` stays.
    """
    try:
        text = result.decode("utf-8", "surrogateescape")
        spans = error_spans(text)
        theirs = guide.decode("utf-8", "surrogateescape")
        guides = error_spans(theirs)
    except (ValueError, IndexError):
        return result
    if not spans or not guides or len(guides) != len(spans):
        return result
    out, last = [], 0
    for (start, end), (side_start, side_end) in zip(spans, guides):
        try:
            error = json.loads(text[start:end])
            side = json.loads(theirs[side_start:side_end])
        except ValueError:
            continue
        if not isinstance(error, dict) or not isinstance(side, dict) or not masked_error_reads(error, side):
            continue
        out += [text[last:start], theirs[side_start:side_end]]
        last = end
    if not out:
        return result
    return ("".join(out) + text[last:]).encode("utf-8", "surrogateescape")


#: The start of a line of an event stream that holds a result.
DATA_LINE = b"data: {"


def unmasked_event_stream(body: bytes, guide: bytes) -> bytes:
    """Return the event stream `body` of the reference with each result read through `unmasked_result` beside the
    result at the same place of the event stream `guide` of the side under test, where both hold as many."""
    lines = body.split(b"\n")
    places = [index for index, line in enumerate(lines) if line.startswith(DATA_LINE)]
    ours = [line for line in guide.split(b"\n") if line.startswith(DATA_LINE)]
    if not places or len(places) != len(ours):
        return body
    for index, side in zip(places, ours):
        lines[index] = b"data: " + unmasked_result(lines[index][6:], side[6:])
    return b"\n".join(lines)


def unmasked_body(body: bytes, guide: bytes, content_type) -> bytes:
    """Return the body `body` of an answer of the reference of the type `content_type` with each masked error read
    as the error at its place of the body `guide` of the side under test, a GraphQL result or an event stream
    of results."""
    media = (content_type or "").split(";")[0].strip().lower()
    if media == EVENT_STREAM:
        return unmasked_event_stream(body, guide)
    if media in GRAPHQL_TYPES:
        return unmasked_result(body, guide)
    return body


def body_fields(lines: list, before: bytes, body: bytes, ask: Ask) -> list:
    """Return the fields `lines` of an answer of the reference to `ask` whose body `before` becomes `body`.

    A length the answer names moves by the bytes the body gains, so a length
    a mask of the comparison left behind its body stays as far behind. The
    coding follows compress() of the reference, which codes an answer of
    1,024 bytes or more in the coding the request accepts and leaves a
    shorter one uncoded with its length.
    """
    length = next((value for name, value in lines if name.lower() == "content-length"), None)
    coding = next((value for name, value in lines if name.lower() == "content-encoding"), None)
    grown = len(body) - len(before)
    if length is not None and length.isdigit():
        size = int(length) + grown
        if size >= COMPRESS_THRESHOLD and compress_coding(ask) is not None:
            kept = [(name, value) for name, value in lines if name.lower() != "content-length"]
            return sorted_in(kept, ("content-encoding", compress_coding(ask)))
        return [(name, str(size) if name.lower() == "content-length" else value) for name, value in lines]
    if coding is not None and len(body) < COMPRESS_THRESHOLD:
        kept = [(name, value) for name, value in lines if name.lower() != "content-encoding"]
        return sorted_in(kept, ("content-length", str(len(body))))
    return lines


def sorted_in(lines: list, field: tuple) -> list:
    """Return the fields `lines` with `field` at its place in the order of names the reference writes its fields in,
    before the first field whose name sorts after its own."""
    at = next((index for index, (name, _) in enumerate(lines) if name.lower() > field[0]), len(lines))
    return lines[:at] + [field] + lines[at:]


def unmasked_served(answer, guide, case, keys: dict):
    """Return the answer `answer` of the reference, an answer of `http.py`, to the case `case` with each masked
    error read as the error at its place of the answer `guide` of the side under test, under the keys `keys` of
    the side.

    An answer no error of which changes is returned as it is. A changed one
    keeps every field of the reference but its length and its coding, which
    `body_fields` gives, and its framing follows its length.
    """
    if answer is None or guide is None or case is None or not mask_differs(keys):
        return answer
    before = plain_body(answer)
    body = unmasked_body(before, plain_body(guide), answer.header("content-type"))
    if body == before:
        return answer
    return with_body(answer, Ask(case.method, case.target, list(case.headers)), before, body)


def with_body(answer, ask: Ask, before: bytes, body: bytes):
    """Return the answer `answer` of `http.py` to `ask` whose body `before` becomes `body`, its fields read by
    `body_fields` and its framing following its length."""
    lines = body_fields(list(answer.header_lines), before, body, ask)
    own = [(name, value) for name, value in lines if name.lower() != "transfer-encoding"]
    chunked = not any(name.lower() == "content-length" for name, _ in own) and ask.method != "HEAD"
    coded = coded_body(body, next((value for name, value in own if name.lower() == "content-encoding"), None))
    raw = raw_message(answer.status_line, own, coded, chunked)
    header_lines = own + ([("Transfer-Encoding", "chunked")] if chunked else [])
    return dataclasses.replace(answer, header_lines=header_lines, framing="chunked" if chunked else "content-length",
                               wire_body=raw[raw.index(b"\r\n\r\n") + 4:], payload=coded, decoded=body, raw=raw)


def unmasked_stream(stream, guide, ask: Ask, keys: dict):
    """Return the read `stream` of the reference, an answer of `sse.py`, to `ask` with each masked error read as the
    error at its place of the read `guide` of the side under test, under the keys `keys` of the side.

    A read no error of which changes is returned as it is. An event stream
    reads its frames through `unmasked_event_stream` and keeps its fields and its
    framing, but for a length it names, as the answers of Bun to a request
    that names identity do, which moves by the bytes the frames gain. An
    answer that is no stream reads its body through `unmasked_body` and its fields
    through `body_fields`.
    """
    if stream is None or guide is None or stream.status is None or not mask_differs(keys):
        return stream
    if stream.is_stream:
        before = stream.text.encode("utf-8", "surrogateescape")
        text = unmasked_event_stream(before, guide.text.encode("utf-8", "surrogateescape"))
        if text == before:
            return stream
        frames, _ = split_event_frames(text.decode("utf-8", "surrogateescape"))
        body = text + stream.body[len(before):] if stream.body.startswith(before) else stream.body
        grown = len(text) - len(before)
        lines = [(name, str(int(value) + grown) if name.lower() == "content-length" and value.isdigit() else value)
                 for name, value in stream.header_lines]
        return dataclasses.replace(stream, frames=frames, body=body, header_lines=lines,
                                   headers={name.lower(): value for name, value in lines})
    body = unmasked_body(stream.body, guide.body, stream.headers.get("content-type"))
    if body == stream.body:
        return stream
    own = [(name, value) for name, value in body_fields(list(stream.header_lines), stream.body, body, ask)
           if name.lower() != "transfer-encoding"]
    chunked = not any(name.lower() == "content-length" for name, _ in own) and ask.method != "HEAD"
    coded = coded_body(body, next((value for name, value in own if name.lower() == "content-encoding"), None))
    raw = raw_message(stream.status_line, own, coded, chunked)
    lines = own + ([("Transfer-Encoding", "chunked")] if chunked else [])
    return dataclasses.replace(stream, header_lines=lines, headers={name.lower(): value for name, value in lines},
                               framing="chunked" if chunked else "content-length", body=body,
                               wire_body=raw[raw.index(b"\r\n\r\n") + 4:])


def introspection_refused(keys: dict) -> bool:
    """Return whether the reference refuses introspection that the side under test answers under the keys `keys`:
    under `graphiql: false` with no key `introspection` of the value false."""
    return keys.get("graphiql") is False and keys.get("introspection") is not False


def introspection_refusal_result(result: bytes, guide: bytes, keys: dict) -> bytes:
    """Return the GraphQL result `result` of the reference with its refusal of introspection read off the result
    `guide` of the side under test, under the keys `keys` of the side.

    The side under test answers a document of introspection under
    `graphiql: false` as under a configuration without that key, so `guide`
    stands for the answer where the reference refused every field of the
    document and `guide` holds data and no error. Every other pair returns
    `result` as it is, which stays a difference of the comparison.
    """
    if not introspection_refused(keys) or INTROSPECTION_REFUSAL not in result:
        return result
    try:
        refused, ours = json.loads(result), json.loads(guide)
    except ValueError:
        return result
    errors = refused.get("errors") if isinstance(refused, dict) else None
    if not errors or "data" in refused or any(
            INTROSPECTION_REFUSAL.decode() not in str(error.get("message")) for error in errors):
        return result
    if not isinstance(ours, dict) or "errors" in ours or not isinstance(ours.get("data"), dict):
        return result
    return guide


def introspection_refusal_served(answer, guide, case, keys: dict):
    """Return the answer `answer` of the reference, an answer of `http.py`, to the case `case` with its refusal of
    introspection read off the answer `guide` of the side under test, under the keys `keys` of the side.

    The step `introspection_on_key` knows the answer of one document, and the
    operations of family O send other documents of introspection, whose answer
    the refusal of the reference does not carry. Their bodies read through
    `introspection_refusal_result`, and an answer whose body it reads takes the body of the
    guide through `with_body`. Every other pair is returned as it is and stays
    a difference of the comparison.
    """
    if answer is None or guide is None or case is None or not introspection_refused(keys):
        return answer
    before = plain_body(answer)
    if INTROSPECTION_REFUSAL not in before:
        return answer
    body = introspection_refusal_result(before, plain_body(guide), keys)
    if body == before:
        return answer
    return with_body(answer, Ask(case.method, case.target, list(case.headers)), before, body)


def unmasked_message(answer: bytes, guide: bytes, request: bytes, keys: dict) -> bytes:
    """Return the HTTP/1.1 answer `answer` of the reference to the HTTP/1.1 request `request` with each masked error
    read as the error at its place of the HTTP/1.1 answer `guide` of the side under test, under the keys `keys`
    of the side.

    A message no error of which changes is returned as it is. A changed one
    keeps every field of the reference in its order but its length and its
    coding, which `body_fields` gives, and its body is chunked where it names
    no length.
    """
    if not answer or not guide or not mask_differs(keys):
        return answer
    head, _, wire = answer.partition(b"\r\n\r\n")
    lines = head.decode("latin-1").split("\r\n")
    fields = [tuple(part.strip() for part in line.split(":", 1)) for line in lines[1:] if ":" in line]
    guide_head, _, guide_wire = guide.partition(b"\r\n\r\n")
    guide_fields = [tuple(part.strip() for part in line.split(":", 1))
                    for line in guide_head.decode("latin-1").split("\r\n")[1:] if ":" in line]
    try:
        before = message_body(fields, wire)
        ours = message_body(guide_fields, guide_wire)
    except (ValueError, zlib.error):
        return answer
    content_type = next((value for name, value in fields if name.lower() == "content-type"), None)
    body = unmasked_body(before, ours, content_type)
    if body == before:
        return answer
    ask = ask_of_raw(request)
    own = [(name, value) for name, value in body_fields(fields, before, body, ask)
           if name.lower() != "transfer-encoding"]
    chunked = not any(name.lower() == "content-length" for name, _ in own) and ask.method != "HEAD"
    coded = coded_body(body, next((value for name, value in own if name.lower() == "content-encoding"), None))
    return raw_message(lines[0], own, coded, chunked)


#: A Sentry event id of an error as the masks of the comparison write it, and the characters they take off the id
#: of 32 it stands for.
EVENT_ID_MASKED = b'"sentryEventId":"<sentry-event-id>"'
EVENT_ID_TAKEN = 32 - len(b"<sentry-event-id>")


def named_length(body: bytes) -> int:
    """Return the length the answer named for the body `body` the masks of the comparison wrote, each Sentry event
    id read at the 32 characters it stands for."""
    return len(body) + body.count(EVENT_ID_MASKED) * EVENT_ID_TAKEN


def unmasked_form(form: dict, guide: dict) -> dict:
    """Return the compared form `form` of an answer of the reference, `{status, headers, body}` with its fields by
    lower case name and an event stream read as `frames` where it holds them, with each masked error read as the
    error at its place of the compared form `guide` of the side under test, and a length it names following its
    body.

    The body comes back plain, as `oracle_plain` reads a coded one, since the
    oracle compares a body after its coding is undone. The length moves by
    the bytes the body gains as the answer wrote them, so an event id the
    masks wrote shorter counts at its own length.
    """
    frames, ours = form.get("frames"), guide.get("frames")
    if isinstance(frames, list) and isinstance(ours, list):
        joined = unmasked_event_stream("".join(frames).encode("utf-8", "surrogateescape"),
                                  "".join(ours).encode("utf-8", "surrogateescape"))
        done, _ = split_event_frames(joined.decode("utf-8", "surrogateescape"))
        return form if done == frames else dict(form, frames=done)
    content_type = (form.get("headers", {}).get("content-type") or [None])[0]
    before = oracle_plain(form)
    body = unmasked_body(before, oracle_plain(guide), content_type)
    if body == before:
        return form
    headers = dict(form.get("headers", {}))
    if headers.get("content-length"):
        grown = named_length(body) - named_length(before)
        headers["content-length"] = [str(int(headers["content-length"][0]) + grown)]
    return dict(form, headers=headers, body=body)


def masked_count(body: bytes) -> int:
    """Return how many errors of the GraphQL result `body` are masked errors, none for a body that is no result."""
    try:
        errors = json.loads(body.decode("utf-8", "surrogateescape")).get("errors")
    except (ValueError, AttributeError):
        return 0
    if not isinstance(errors, list):
        return 0
    return sum(1 for error in errors if isinstance(error, dict) and error == masked_error(error))


def unmasked_log(form: dict, guide: dict) -> dict:
    """Return the compared form `form` of a log of the reference, its standard output, the answers of its requests
    as `follow` and the first words of the lines of its standard error as `verdict`, with each masked error read as
    the error at its place of the log `guide` of the side under test.

    Each answer reads as `unmasked_form` reads it beside the answer at the same place of `guide`. The verdict loses
    its last word ERR for each error so read, since the mask writes the original of an error it masks on standard
    error in a line that opens with ERR, after the lines of the execution of the request, and the side masks none.
    """
    follow, ours = form.get("follow"), guide.get("follow")
    if not isinstance(follow, list) or not isinstance(ours, list) or len(follow) != len(ours):
        return form
    out, read = [], 0
    for entry, side in zip(follow, ours):
        pair = isinstance(entry, list) and isinstance(side, list) and len(entry) == len(side) == 3
        if not pair or entry[0] is None or side[0] is None:
            out.append(entry)
            continue
        before = {"status": entry[0], "headers": entry[1], "body": entry[2]}
        done = unmasked_form(before, {"status": side[0], "headers": side[1], "body": side[2]})
        if done is before:
            out.append(entry)
            continue
        read += max(0, masked_count(oracle_plain(before)) - masked_count(done["body"]))
        out.append([done["status"], done["headers"], done["body"]])
    if out == follow:
        return form
    verdict = form.get("verdict") or ""
    words = verdict.split(" ") if verdict else []
    for _ in range(read):
        at = max((index for index, word in enumerate(words) if word == "ERR"), default=None)
        if at is not None:
            del words[at]
    return dict(form, follow=out, verdict=" ".join(words))


# The rules of the oracle.
#
# The oracle sends each request of its corpus to the old pipeline and to the tree under test, and lets a difference
# pass only through the rule of its name in `ORACLE`, the rules applied in the order of their names, each a
# function of the request, the old answer and the new answer as the rules before it left it, which answers the new
# answer written as the old pipeline writes it, or None. The rules share one model. The old answer, read as a
# `Reply`, goes through the rewrites of the divergences in the order the tree writes them, the batch the function
# refuses and its answer without multipart/mixed, the steps of the rows of the secure defaults and the coding of
# the host, which the uncoded small answer changes on Bun and on workerd. The rule of a name holds where the new
# answer is the old one through the rewrites of that name and of every name after it, and it answers the old one
# through the rewrites of the names after it alone, so the rule of the last name a difference needs answers the old
# answer itself. Each kind of request of the oracle holds its answer in a form of its own, and the rewrites read
# and write each form as the oracle builds it: the answers of HTTP/1.1 of the kinds `http` and `yoga`, the bytes of
# the kind `wire`, the three forms of the kind `h2`, the answers of the requests of a log, the front of the
# federation and the answers of GraphTP, and the rewrite `log_without_excerpts` reads the verdict of a log, the
# first words of the lines of its standard error.

#: The divergence of the log line, a thrown error on standard error as the first line of its stack with no
#: excerpt of the source, whose rewrite `log_without_excerpts` reads the verdict of a log.
STACK_LINE = "log-excerpts"

#: The names whose rules `ORACLE` holds, in the order the oracle applies them, the order of the numbers of their
#: divergences.
ORACLE_NAMES = ("cors-plugin", "landing-plugin", "graphiql-viewer", "multipart", "uncoded-small", "batching",
                "no-multipart-mixed", "no-health-check", "endpoint-ending", "endpoint-slash", "unmasked-error",
                "introspection-key", STACK_LINE)

#: The fields of a head the oracle never compares, `connection` aside, which only a stream keeps.
ORACLE_IGNORED = ("date", "keep-alive", "server", "transfer-encoding")

#: The body of the 405 the engine answers a request of HEAD with, whose length that answer carries on Bun.
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
    return keys_of(request.get("fixture") or "", request.get("variant") or "default", side,
                   request.get("node_env"))


def oracle_plain(form: dict) -> bytes:
    """Return the body of the compared form `form` with the second gzip of a Worker undone."""
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
    """Return `done`, an answer a step of the secure defaults wrote as the reference's compress() codes it, as the host of `side` codes it.

    The twin codes no gzip, so its compress() codes deflate where the request names it and nothing otherwise, and
    writes the length of an answer it does not code, while Bun and workerd code as the reference.
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


def uncoded_host(done: Reply, ask: Ask, side: str, written: bool) -> Reply:
    """Return the answer `done` of the function on Bun or on workerd with its Content-Length.

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


def refused_batch(reply: Reply, request: dict, ask: Ask) -> Reply:
    """Return the answer `reply` of an instance in process that runs batching to a batch as the function's refusal,
    under the type the answer of the instance negotiated."""
    if request.get("kind") != "yoga" or not batch_refused(request.get("instance") or "",
                                                          (request.get("body") or b"").decode("utf-8", "replace")):
        return reply
    lines = [(name, value) for name, value in reply.lines if name.lower() == "content-type"]
    if not lines:
        lines = [("content-type", "application/json; charset=utf-8")]
    if reply.header("content-length") is not None:
        lines.append(("content-length", str(len(BATCH_REFUSAL))))
    return Reply(400, "Bad Request", lines, BATCH_REFUSAL)


def without_multipart(reply: Reply, ask: Ask) -> Reply:
    """Return the answer `reply` with no answer as multipart/mixed.

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


def oracle_no_graphiql_page(reply, ask, keys):
    """`no_graphiql_page` for an answer of the old pipeline, whose page of GraphiQL names Aosabi already.

    The 406 of a subscription lists the media types of an event stream alone, the one processor a subscription has,
    where `no_graphiql_page` writes the 406 of a query.
    """
    probe = reply
    if PLAYGROUND in reply.body:
        probe = dataclasses.replace(reply, body=reply.body.replace(PLAYGROUND, PLAYGROUND_REFERENCE))
    done = no_graphiql_page(probe, ask, keys)
    if done == probe:
        return reply
    if oracle_subscription(ask):
        count = len(media_types_in_order(ask.header("accept")))
        offered = ACCEPT_JOIN.join(media for _ in range(count) for media in PROCESSOR_TYPES[0])
        done = dataclasses.replace(done, lines=[(name, offered if name.lower() == "accept" else value)
                                                for name, value in done.lines])
    return done


#: The steps of `NAMED_STEPS` as the rules run them on an answer of the old pipeline.
ORACLE_STEPS = tuple((oracle_no_graphiql_page if step is no_graphiql_page else step, name) for step, name in NAMED_STEPS)


@dataclasses.dataclass
class OracleAnswer:
    """The fields of an old answer, as `host_length` reads the answer of the reference on the same host."""

    header_lines: list


def oracle_chain(reply: Reply, request: dict, ask: Ask, keys: dict, side: str, names) -> Reply:
    """Return the answer `reply` to `ask` on `side` through the rewrites of the divergences `names` in the order
    the tree writes them: the batch the function refuses, its answer without multipart/mixed, the steps of the
    secure defaults each coded by the host, the length Bun and workerd frame a rewritten answer with, and the
    uncoded small answer."""
    before = reply
    if "batching" in names:
        reply = refused_batch(reply, request, ask)
    if "no-multipart-mixed" in names:
        reply = without_multipart(reply, ask)
    for step, name in ORACLE_STEPS:
        if name in names:
            done = step(reply, ask, keys)
            if done != reply:
                reply = host_coding(done, ask, side)
    written = reply != before
    if written and side in ("bun", "workerd") and ask.served:
        reply = dataclasses.replace(reply, lines=host_length(reply.lines, reply, ask, OracleAnswer(before.lines)))
    if "uncoded-small" in names:
        reply = uncoded_host(reply, ask, side, written)
    return reply


def oracle_served(old: dict, request: dict, names, guide: dict | None = None) -> dict | None:
    """The rewrites of an answer of the kinds `http` and `yoga` and of an answer of HTTP/1.1 inside TLS.

    `unmasked_form` reads each masked error of the old answer as the error at its place of
    the new answer `guide` before the rewrites of the function, since the
    engine answers the error.
    """
    ask = oracle_ask(request)
    if ask is None or old.get("status") is None:
        return None
    if "unmasked-error" in names and guide is not None and mask_differs(oracle_keys(request)):
        old = unmasked_form(old, guide)
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
        if head_only and status == 200 and reply.header(HEALTH_FIELD) is not None:
            # The health check of the reference answers every method alike,
            # with no body and no length, so its answer reads as one to GET.
            head_only = False
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
    """The rewrites of a case of the frames of HTTP/2: the fields and the body of each stream the server
    answered, in the record the oracle writes as JSON with its keys sorted, and the entry of `unmet` that
    names the bodies."""
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


#: The first word of a line the logger of the old pipeline wrote below the message of a located error, its excerpt
#: of the source per location: the name of the source, `GraphQL request`, a line number or a prefix padded with
#: blanks.
EXCERPT_WORD = re.compile(r"GraphQL|[0-9]+|")


def log_without_excerpts(form: dict) -> dict:
    """Return the compared form `form` of a log of the old pipeline with the excerpts of its errors gone.

    The logger of the tree writes a thrown error as the first line of its
    stack, one line that opens with ERR, where the logger of the old pipeline wrote
    an excerpt of the source per location below a located error, so each
    word of an excerpt line after a word ERR of the verdict leaves.
    """
    verdict = form.get("verdict")
    if not verdict:
        return form
    out, inside = [], False
    for word in verdict.split(" "):
        if word == "ERR":
            inside = True
        elif inside and EXCERPT_WORD.fullmatch(word):
            continue
        else:
            inside = False
        out.append(word)
    done = " ".join(out)
    return form if done == verdict else dict(form, verdict=done)


def oracle_log(old: dict, request: dict, names) -> dict:
    """The rewrites of a log: its verdict through `log_without_excerpts`, and the answers of its requests, each a
    request of GET the answer names as `wire_ask` reads it."""
    if STACK_LINE in names:
        old = log_without_excerpts(old)
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
    """The rewrites of an answer of a front of the federation to an upload: where the old front forwarded a multipart
    body, the function of the new front refuses it and forwards nothing."""
    upstream = old.get("upstream")
    if "multipart" not in names or not isinstance(upstream, dict) or old.get("status") is None:
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
    """The rewrites of an answer of GraphTP to a batch: the refusal of the function, status 400 with the substatus 6,
    uncoded."""
    body = (request.get("body") or b"").decode("utf-8", "replace")
    if "batching" not in names or not batch_refused(request.get("instance") or "", body):
        return old
    headers = dict(old.get("headers", {}), substatus=["6"], coding=["plain"])
    return dict(old, status=400, headers=headers, body=BATCH_REFUSAL)


def oracle_rewrite(old: dict, request: dict, names, guide: dict | None = None) -> dict | None:
    """Return the old answer `old` to the request `request` through the rewrites of the divergences `names`.

    None stands for a request or an answer no rewrite reads. The new answer
    `guide` guides the reading of a masked error, and an answer of the kind `ingest`, an answer whose
    run counts the envelopes of Sentry, reads as a served answer.
    """
    kind = request.get("kind")
    rewrite = {"http": oracle_served, "yoga": oracle_served, "ingest": oracle_served, "h2": oracle_h2,
               "wire": oracle_wire, "log": oracle_log, "front": oracle_front,
               "graphtp": oracle_graphtp}.get(kind)
    if rewrite is None:
        return None
    try:
        if rewrite is oracle_served:
            return oracle_served(old, request, names, guide)
        if kind in ("front", "log") and "unmasked-error" in names and guide is not None and mask_differs(oracle_keys(request)):
            old = unmasked_log(old, guide) if kind == "log" else unmasked_form(old, guide)
        return rewrite(old, request, names)
    except (ValueError, KeyError, IndexError, StopIteration, TypeError, AttributeError, zlib.error):
        return None


def bun_refused_origin(request: dict, old: dict, new: dict) -> bool:
    """Return whether the old answer is the 500 of the glue of Bun to an `Origin` outside ASCII.

    The engine of the old pipeline echoes the origin into a field, which Bun
    refuses as a value outside ASCII, so the app's fetch rejects. The engine of
    the tree answers no CORS, and the new answer carries no field of CORS.
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
    later = ORACLE_NAMES[ORACLE_NAMES.index(name):]

    def rule(request: dict, old: dict, new: dict):
        if name == "cors-plugin" and bun_refused_origin(request, old, new):
            return old
        want = oracle_rewrite(old, request, later, new)
        back = oracle_rewrite(old, request, later[1:], new)
        if want is None or back is None:
            return None
        wanted, left = oracle_view(want, old), oracle_view(back, old)
        if wanted == left or oracle_view(new, old) != wanted:
            return None
        return old if left == oracle_view(old, old) else back

    rule.__name__ = rule.__qualname__ = "oracle_" + name.replace("-", "_")
    rule.__doc__ = "The rule of the oracle for %s, `oracle_rule`." % name
    return rule


#: The rules of the oracle for the divergences of this module, by name.
ORACLE = {name: oracle_rule(name) for name in ORACLE_NAMES}
