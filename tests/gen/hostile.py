# SPDX-License-Identifier: MIT
"""tests/gen: the hostile inputs of the documentation notebooks.

The module writes the hostile introspection corpus and the 24 hostile JSON
documents that the readers of the core must refuse or read with 0 panics,
among them a nesting of depth 10,000, a string of 64 MiB, invalid UTF-8, a
lone surrogate escape, a duplicate key, a number of 400 digits and trailing
bytes. The same step feeds 10,000 documents mutated from the fixtures by
truncation, byte flips and depth, which [`mutations`] draws from the seed.

A document names what the plan lets a reader answer. `read` is a document the
reader must take, such as a lone surrogate escape, which the pull parser
replaces by U+FFFD, `refuse` is one it must answer with a `ReadError`, such as
a nesting past `max_depth`, and `either` is one where the plan leaves the
answer to the reader, such as a duplicate key, which the parser passes on. The
gate of step 8 is the same for all three, a `ReadError` or a model with 0
panics. The bounds are those of `ReadLimits::DEFAULT`.

An attack the reader meets only by pulling tokens through it stands under the
key `extensions` in front of `data`, because a GraphQL response may carry that
key beside `data` and a reader that refuses at its first unexpected token
would otherwise never reach the attack. Every string of the corpus is
invented.

The hostile introspection corpus carries the strings of the row `hostile` of
the plan through the introspection input, and what the SDL of
`tests/fixtures/viewer/hostile.graphql` cannot carry: the lone surrogate
escapes, which the lexer of graphql-js refuses in an SDL string, and the
description of 1 MiB. It adds the names at the bounds of the name rule and the
documents whose shape the builder refuses. Its golden rule is assertions only,
and [`Document.strings`] names what a tree of a `read` document must carry as
written.

    python -m tests.gen.hostile
    python -m tests.gen.hostile --out target/viewer/hostile --mutations 10000
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections.abc import Iterator, Sequence
from dataclasses import dataclass, field
from pathlib import Path

from .rng import Rng
from .synth_schema import (
    Directive,
    EnumValue,
    Field,
    InputValue,
    Schema,
    TypeDef,
    dumps,
    introspection,
)

#: The seed of the mutated documents.
SEED = 20260923

#: The number of hostile JSON documents.
JSON_DOCUMENT_COUNT = 24

#: The number of documents mutated from the fixtures.
MUTATION_COUNT = 10000

#: The answers a document allows.
READ = "read"
REFUSE = "refuse"
EITHER = "either"

#: The bounds of `ReadLimits::DEFAULT`.
MAX_INPUT_BYTES = 64 << 20
MAX_DEPTH = 128
MAX_TYPES = 20_000
MAX_MEMBERS = 400_000
MAX_NAME_BYTES = 1024
MAX_DESCRIPTION_BYTES = 1 << 20

#: The depths a depth mutation nests a value in, around the bound and far
#: past it.
MUTATION_DEPTHS = (MAX_DEPTH - 1, MAX_DEPTH, MAX_DEPTH + 1, 1000, 10000)

#: The number of bytes a flip mutation changes at most.
MAX_FLIPS = 8

#: Where the command line writes the documents when asked to.
DEFAULT_OUT = Path("target/viewer/hostile")

#: The four presets, the committed fixtures a mutation starts from by default.
PRESETS = ("star-wars", "yelp", "shopify-storefront", "github")

#: The strings of the row `hostile` that attack the Typst, Markdown, HTML and
#: XML writers.
ATTACKS = (
    '#import "@preview/x:0.1.0": *',
    '#eval("1")',
    "#let x = 1",
    "$x^2$",
    "= a line that starts with an equals sign",
    'A backslash \\ and a quote " and a lone backtick ` in one line.',
    "<script>alert(1)</script>",
    "The end of a character data section ]]> inside text.",
    "A cell with a | pipe and a second | pipe.",
    "[a link with a script scheme](javascript:alert(1))",
    "# A heading the writer demotes",
)

#: The characters of the row that are no markup: U+0001, U+001F, U+2028 and
#: U+FEFF, and U+007F, U+0085, U+2029 and U+202E beside them.
CONTROLS = "A U+0001 \x01, a U+001F \x1f, a U+007F \x7f, a U+0085 \x85, " \
    "a U+2028 \u2028, a U+2029 \u2029, a U+202E \u202e and a U+FEFF \ufeff."

#: The name of 200 characters of the row, the one `hostile.graphql` declares.
NAME_200 = "Name" + "Segment" * 28


@dataclass(frozen=True)
class Document:
    """Holds one hostile document and what a reader may answer to it.

    `expect` is [`READ`], [`REFUSE`] or [`EITHER`], `reason` says which rule
    decides it, and `strings` are the texts a model of a `read` document
    carries, in the form the reader decodes them to.
    """

    name: str
    data: bytes
    expect: str
    reason: str
    strings: tuple[str, ...] = field(default=())

    @property
    def sha256(self) -> str:
        """The SHA-256 of the bytes of the document."""
        return hashlib.sha256(self.data).hexdigest()


def json_documents() -> list[Document]:
    """Returns the 24 hostile JSON documents of step 8 of `51_viewer_core`.

    Each is a complete input of the introspection reader, and each but the
    malformed ones wraps the attack in an otherwise valid result of one type,
    so the attack is the only defect a reader meets.
    """
    base = _minimal()
    deep = MAX_DEPTH - 1
    digits = b"1" + b"0" * 399
    docs = [
        Document("J01-depth-10000-arrays", _extensions(base, b"[" * 10000 + b"]" * 10000),
                 REFUSE, "10,000 nested arrays, past max_depth"),
        Document("J02-depth-10000-objects",
                 _extensions(base, b'{"a":' * 10000 + b"null" + b"}" * 10000),
                 REFUSE, "10,000 nested objects, past max_depth"),
        Document("J03-depth-129", _extensions(base, b"[" * (deep + 1) + b"]" * (deep + 1)),
                 REFUSE, "129 open containers, one past max_depth"),
        Document("J04-depth-128", _extensions(base, b"[" * deep + b"]" * deep),
                 EITHER, "128 open containers, max_depth itself, under a key the reader may skip"),
        Document("J05-string-64MiB", _description(base, b"a" * MAX_INPUT_BYTES),
                 REFUSE, "a string of 64 MiB, a document past max_input_bytes"),
        Document("J06-invalid-utf8", _description(base, b"ab\xffcd"),
                 REFUSE, "the byte 0xFF, which UTF-8 never holds"),
        Document("J07-overlong-utf8", _description(base, b"ab\xc0\xafcd"),
                 REFUSE, "an overlong encoding of U+002F"),
        Document("J08-encoded-surrogate", _description(base, b"ab\xed\xa0\x80cd"),
                 REFUSE, "U+D800 encoded as three bytes, which UTF-8 forbids"),
        Document("J09-truncated-utf8", _description(base, b"ab\xe2\x82"),
                 REFUSE, "a sequence of three bytes cut by the closing quote"),
        Document("J10-lone-surrogate-escapes",
                 _description(base, rb"\ud800 \udc00 \ud800x \ud800\ud800 \ud800\u0041 \udbff\udfff"),
                 READ, "every lone surrogate escape becomes U+FFFD and the pair stays",
                 ("\ufffd \ufffd \ufffdx \ufffd\ufffd \ufffdA \U0010ffff",)),
        Document("J11-escape-unknown", _description(base, rb"ab\x41cd"),
                 REFUSE, "the escape \\x, which RFC 8259 does not define"),
        Document("J12-escape-short", _description(base, rb"ab\u12"),
                 REFUSE, "a \\u escape of two hexadecimal digits"),
        Document("J13-control-byte", _description(base, b"ab\x01cd"),
                 REFUSE, "an unescaped U+0001 inside a string"),
        Document("J14-duplicate-key",
                 base.replace(b'"name":"Query","description"',
                              b'"name":"Query","name":"Shadow","description"', 1),
                 EITHER, "the key name twice in one type, which the parser passes on"),
        Document("J15-number-400-digits", _extensions(base, b'{"cost":' + digits + b"}"),
                 EITHER, "a number of 400 digits, kept as its text"),
        Document("J16-trailing-bytes", base + b"\n{}",
                 REFUSE, "a second document after the first"),
        Document("J17-empty", b"", REFUSE, "no byte at all"),
        Document("J18-truncated", base[: len(base) // 2],
                 REFUSE, "the first half of a valid document"),
        Document("J19-byte-order-mark", b"\xef\xbb\xbf" + base,
                 EITHER, "a byte order mark, which RFC 8259 section 8.1 lets a parser ignore"),
        Document("J20-number-grammar", _extensions(base, b"-01.e+"),
                 REFUSE, "a leading zero, a fraction and an exponent without digits"),
        Document("J21-not-a-number", _extensions(base, b"NaN"),
                 REFUSE, "NaN, which JSON has no token for"),
        Document("J22-trailing-comma", base.replace(b'"directives":[]', b'"directives":[],', 1),
                 REFUSE, "a comma before the closing brace"),
        Document("J23-comment", b"/* c */" + base,
                 REFUSE, "a comment, which JSON has no token for"),
        Document("J24-wide-array", _extensions(base, b"[" + b"0," * 999_999 + b"0]"),
                 EITHER, "an array of 1,000,000 numbers under a key the reader may skip"),
    ]
    if len(docs) != JSON_DOCUMENT_COUNT:
        raise AssertionError(f"{len(docs)} hostile JSON documents, not {JSON_DOCUMENT_COUNT}")
    return docs


def introspection_documents() -> list[Document]:
    """Returns the hostile introspection corpus.

    The documents are valid JSON. Those marked `read` carry hostile text in
    every place a schema holds text, or names at the bounds of the name rule,
    and those marked `refuse` carry a shape, a name or a count the builder
    refuses.
    """
    docs = [
        _strings_document(),
        Document("H02-description-1MiB", _one_description(_sized(MAX_DESCRIPTION_BYTES)),
                 READ, "a description of max_description_bytes"),
        Document("H03-description-over", _one_description(_sized(MAX_DESCRIPTION_BYTES + 1)),
                 REFUSE, "a description one byte past max_description_bytes"),
        _surrogates_document(),
        _document("H05-names-case", _schema([
            TypeDef("OBJECT", "Query", fields=[
                Field("foo", "Foo"), Field("lower", "foo"), Field("upper", "FOO"),
                Field("a", "String"), Field("A", "String")]),
            TypeDef("OBJECT", "Foo", fields=[Field("value", "Case")]),
            TypeDef("OBJECT", "foo", fields=[Field("value", "Case")]),
            TypeDef("OBJECT", "FOO", fields=[Field("value", "Case")]),
            TypeDef("ENUM", "Case", values=[EnumValue("ONE"), EnumValue("one"), EnumValue("One")]),
        ]), READ, "names that only case separates, whose anchors take -2 and -3",
            ("Foo", "foo", "FOO")),
        _document("H06-name-1024", _named("N" * MAX_NAME_BYTES), READ,
                  "a name of max_name_bytes", ("N" * MAX_NAME_BYTES,)),
        _document("H07-name-1025", _named("N" * (MAX_NAME_BYTES + 1)), REFUSE,
                  "a name one byte past max_name_bytes"),
        _document("H08-name-digit", _named("1Foo"), REFUSE,
                  "a name that starts with a digit"),
        _document("H09-name-non-ascii", _named("F\u00f6o"), REFUSE,
                  "a name with a letter outside ASCII"),
        _document("H10-name-empty", _named(""), REFUSE, "an empty name"),
        _document("H11-name-markup", _named("Foo<script>"), REFUSE,
                  "a name with markup in it"),
        _document("H12-names-javascript", _schema([
            TypeDef("OBJECT", "Query", fields=[
                Field("constructor", "constructor"), Field("toString", "String"),
                Field("hasOwnProperty", "prototype")]),
            TypeDef("OBJECT", "constructor", fields=[Field("valueOf", "String")]),
            TypeDef("ENUM", "prototype", values=[EnumValue("constructor"), EnumValue("toString")]),
        ]), READ, "names a JavaScript object inherits, which the name rule allows",
            ("constructor", "prototype", "toString", "hasOwnProperty", "valueOf")),
        _document("H13-name-double-underscore", _schema([
            TypeDef("OBJECT", "Query", fields=[Field("proto", "__proto__")]),
            TypeDef("OBJECT", "__proto__", fields=[Field("value", "String")]),
        ]), EITHER, "a name the specification reserves for introspection"),
        _document("H14-duplicate-type", _schema([
            TypeDef("OBJECT", "Query", fields=[Field("a", "Twice")]),
            TypeDef("OBJECT", "Twice", fields=[Field("a", "String")]),
            TypeDef("OBJECT", "Twice", fields=[Field("b", "String")]),
        ]), REFUSE, "two types of one name"),
        _document("H15-dangling-field", _schema([
            TypeDef("OBJECT", "Query", fields=[Field("a", "Missing")]),
        ]), REFUSE, "a field whose type no type declares"),
        _document("H16-dangling-root", _schema([
            TypeDef("OBJECT", "Query", fields=[Field("a", "String")]),
        ], query="Nowhere"), REFUSE, "a query root no type declares"),
        _document("H17-union-of-scalar", _schema([
            TypeDef("OBJECT", "Query", fields=[Field("a", "Mixed")]),
            TypeDef("UNION", "Mixed", members=["String"]),
        ]), REFUSE, "a union whose member is a scalar"),
        _document("H18-implements-object", _schema([
            TypeDef("OBJECT", "Query", fields=[Field("a", "Child")]),
            TypeDef("OBJECT", "Parent", fields=[Field("a", "String")]),
            TypeDef("OBJECT", "Child", fields=[Field("a", "String")], interfaces=["Parent"]),
        ]), REFUSE, "an object that implements an object"),
        _document("H19-kind-unknown", _schema([
            TypeDef("OBJECT", "Query", fields=[Field("a", "String")]),
            TypeDef("WIDGET", "Gadget"),
        ]), REFUSE, "a kind introspection does not define"),
        Document("H20-types-not-a-list", _reshaped(lambda s: s.__setitem__("types", {})),
                 REFUSE, "types as an object in place of a list"),
        Document("H21-refused-introspection",
                 dumps({"errors": [{"message": "Introspection is disabled."}], "data": None}),
                 REFUSE, "the answer of a server that refuses introspection"),
        Document("H22-bare-schema", dumps(introspection(_schema([
            TypeDef("OBJECT", "Query", fields=[Field("a", "String")]),
        ]))["data"]), READ, "the form {\"__schema\":..} without data, which the reader takes"),
        Document("H23-type-reference-200", _wrapped(["LIST"] * 200),
                 REFUSE, "200 wrappers around one type, past max_depth"),
        Document("H24-type-reference-9", _wrapped(["NON_NULL", "LIST"] * 4 + ["NON_NULL"]),
                 READ, "9 wrappers, the most the introspection query of graphql-js asks for"),
        Document("H25-non-null-twice", _wrapped(["NON_NULL", "NON_NULL"]),
                 EITHER, "a non null wrapper around a non null wrapper"),
        _document("H26-interface-cycle", _schema([
            TypeDef("OBJECT", "Query", fields=[Field("a", "Loop")]),
            TypeDef("INTERFACE", "Loop", fields=[Field("a", "String")], interfaces=["Loop"]),
        ]), EITHER, "an interface that implements itself"),
        _document("H27-empty-types", _schema([
            TypeDef("OBJECT", "Query", fields=[
                Field("a", "Empty"), Field("b", "Nothing"), Field("c", "None"),
                Field("d", "String", args=[InputValue("e", "Blank")])]),
            TypeDef("OBJECT", "Empty"),
            TypeDef("ENUM", "Nothing"),
            TypeDef("UNION", "None"),
            TypeDef("INPUT_OBJECT", "Blank"),
        ]), EITHER, "an object, an enum, a union and an input with no member"),
        Document("H28-types-over", _many_types(MAX_TYPES + 1),
                 REFUSE, "one type past max_types"),
        Document("H29-members-over", _many_values(MAX_MEMBERS + 1),
                 REFUSE, "one member past max_members"),
    ]
    return docs


def mutations(sources: Sequence[tuple[str, bytes]], count: int = MUTATION_COUNT,
              seed: int = SEED) -> Iterator[Document]:
    """Yields `count` documents mutated from `sources`, drawn from `seed`.

    `sources` are pairs of a name and the bytes of a fixture. Document `i`
    takes source `i` modulo their number and the operation `i` modulo three: a
    truncation at a drawn offset, a flip of one to eight drawn bytes, or a
    nesting in 127 to 10,000 arrays. Of the nestings, counted from zero, the
    even ones nest a drawn `null` value of the fixture, or the whole document
    when it has no `null` value, where the reader mostly meets a place that
    takes no array and refuses the shape before the depth bound. The odd ones
    put the arrays with a `null` inside under the key `extensions` in front of
    the first key, which the reader pulls through its depth bound as for J01
    to J04 of [`json_documents`], and a fixture with no object takes the even
    form. Over the four presets, 1658 of the 1667 even nestings end in a shape
    error and 9 at the depth bound, and 1331 of the 1666 odd ones end at the
    depth bound while the 335 of 127 arrays read. The documents come one at a
    time, because 10,000 of them from the largest preset would hold gigabytes.
    """
    if not sources:
        raise ValueError("a mutation needs at least one source")
    draw = Rng(seed).stream("hostile:mutations")
    values: dict[str, list[int]] = {}
    operations = ("truncate", "flip", "depth")
    for i in range(count):
        name, data = sources[i % len(sources)]
        operation = operations[i % len(operations)]
        reason = f"a {operation} of {name}"
        if operation == "truncate":
            mutated = data[: draw.randrange(len(data))] if data else b""
        elif operation == "flip":
            buffer = bytearray(data or b" ")
            for _ in range(1 + draw.randrange(MAX_FLIPS)):
                buffer[draw.randrange(len(buffer))] ^= 1 + draw.randrange(255)
            mutated = bytes(buffer)
        else:
            depth = MUTATION_DEPTHS[draw.randrange(len(MUTATION_DEPTHS))]
            nested = b"[" * depth + b"null" + b"]" * depth
            start = data.find(b"{")
            if (i // len(operations)) % 2 and start >= 0:
                reason = f"a nesting in {depth} arrays under extensions of {name}"
                mutated = data[:start + 1] + b'"extensions":' + nested + b"," + data[start + 1:]
            else:
                reason = f"a nesting in {depth} arrays at a null value of {name}"
                if name not in values:
                    values[name] = [m.end() - 4 for m in re.finditer(rb":\s*null", data)]
                offsets = values[name]
                if offsets:
                    at = offsets[draw.randrange(len(offsets))]
                    mutated = data[:at] + nested + data[at + 4:]
                else:
                    mutated = b"[" * depth + data + b"]" * depth
        yield Document(f"M{i + 1:05d}-{operation}-{name}", mutated, EITHER, reason)


def preset_sources() -> list[tuple[str, bytes]]:
    """Returns the four committed presets as the sources of [`mutations`]."""
    root = Path(__file__).resolve().parents[2] / "tests" / "fixtures" / "viewer" / "presets"
    return [(name, (root / f"{name}.json").read_bytes()) for name in PRESETS]


def write(directory: Path = DEFAULT_OUT) -> list[dict]:
    """Writes every hostile JSON and introspection document and a manifest.

    Each document goes to `<directory>/<name>.json`, and `manifest.json`
    beside them lists the name, the file, the answer allowed, the reason, the
    size and the SHA-256 of each. The manifest entries are returned.
    """
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    entries = []
    for doc in json_documents() + introspection_documents():
        path = directory / f"{doc.name}.json"
        path.write_bytes(doc.data)
        entries.append(_entry(doc) | {"file": path.name})
    (directory / "manifest.json").write_text(
        json.dumps(entries, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return entries


def _entry(doc: Document) -> dict:
    """Returns the manifest entry of one document."""
    return {"name": doc.name, "expect": doc.expect, "reason": doc.reason,
            "bytes": len(doc.data), "sha256": doc.sha256}


def _schema(types: list[TypeDef], query: str | None = "Query",
            directives: list[Directive] | None = None) -> Schema:
    """Returns a hostile schema with `String` declared after `types`."""
    if not any(t.name == "String" for t in types):
        types = types + [TypeDef("SCALAR", "String")]
    return Schema("hostile", types, query=query, directives=directives or [])


def _document(name: str, schema: Schema, expect: str, reason: str,
              strings: tuple[str, ...] = ()) -> Document:
    """Returns the introspection result of `schema` as one document."""
    return Document(name, dumps(introspection(schema)), expect, reason, strings)


def _minimal() -> bytes:
    """Returns a valid result of one object with one field, the base of a JSON attack."""
    return dumps(introspection(_schema([
        TypeDef("OBJECT", "Query", fields=[Field("a", "String")]),
    ])))


def _extensions(base: bytes, value: bytes) -> bytes:
    """Puts `value` under the key `extensions` in front of `data`."""
    return b'{"extensions":' + value + b"," + base[1:]


def _description(base: bytes, raw: bytes) -> bytes:
    """Puts `raw` between the quotes of the description of the query type."""
    marker = b'"name":"Query","description":null'
    if base.count(marker) != 1:
        raise AssertionError("the base document lost its query type")
    return base.replace(marker, b'"name":"Query","description":"' + raw + b'"')


def _splice(data: bytes, raw: dict[str, bytes]) -> bytes:
    """Replaces each marker string of `raw` by its raw string body."""
    for marker, body in raw.items():
        quoted = json.dumps(marker).encode("ascii")
        if data.count(quoted) != 1:
            raise AssertionError(f"the marker {marker} stands {data.count(quoted)} times")
        data = data.replace(quoted, b'"' + body + b'"')
    return data


def _named(name: str) -> Schema:
    """Returns a schema whose one object type carries `name`."""
    return _schema([
        TypeDef("OBJECT", "Query", fields=[Field("a", name)]),
        TypeDef("OBJECT", name, fields=[Field("value", "String")]),
    ])


def _sized(size: int) -> str:
    """Returns a description of exactly `size` bytes of UTF-8.

    It repeats the attacks of the row with characters of two, three and four
    bytes between them, cut at a character border and filled up with `a`.
    """
    unit = "\n".join(ATTACKS) + "\n\u00e9 \u20ac \U0001d11e\n"
    text = unit * (size // len(unit.encode("utf-8")) + 1)
    data = text.encode("utf-8")[:size]
    while data:
        try:
            text = data.decode("utf-8")
            break
        except UnicodeDecodeError:
            data = data[:-1]
    return text + "a" * (size - len(text.encode("utf-8")))


def _one_description(text: str) -> bytes:
    """Returns a result whose query type carries `text` as its description."""
    return dumps(introspection(_schema([
        TypeDef("OBJECT", "Query", description=text, fields=[Field("a", "String")]),
    ])))


def _strings_document() -> Document:
    """Returns the document that puts the hostile strings of the row in every text place.

    The strings stand in the descriptions of a type, a field, an argument, an
    enum value, an input field and a directive, in a deprecation reason, in a
    default value and in the URL of a scalar, each place holding some of
    them, beside the names `Foo`, `foo` and the name of 200 characters and a
    description of 10,000 bytes.
    """
    block = "\n".join(ATTACKS)
    big = _sized(10_000)
    default = '"</script> | `x` $x$ ]]>"'
    reason = "Gone <script>alert(1)</script> ]]> | `x`"
    types = [
        TypeDef("OBJECT", "Query", description=CONTROLS, fields=[
            Field("foo", "Foo", description=ATTACKS[0]),
            Field("lower", "foo", description=ATTACKS[1]),
            Field("long", NAME_200, description=ATTACKS[2]),
            Field("big", "Big", description=ATTACKS[3]),
            Field("probe", "String", description=ATTACKS[4], args=[
                InputValue("input", "Probe", description=ATTACKS[5]),
                InputValue("mark", "Mark", description=ATTACKS[6], default="ONE"),
            ]),
            Field("link", "Link", description=ATTACKS[7]),
            Field("gone", "String", description=ATTACKS[8], deprecation=reason),
        ]),
        TypeDef("OBJECT", "Foo", description=block, fields=[
            Field(f"attack{i}", "String", description=text) for i, text in enumerate(ATTACKS)
        ] + [Field("controls", "String", description=CONTROLS)]),
        TypeDef("OBJECT", "foo", description="The lowercase twin of Foo.",
                fields=[Field("value", "String")]),
        TypeDef("OBJECT", NAME_200, description="A type whose name is 200 characters long.",
                fields=[Field("value", "String")]),
        TypeDef("OBJECT", "Big", description=big, fields=[Field("value", "String")]),
        TypeDef("INPUT_OBJECT", "Probe", description=ATTACKS[9], input_fields=[
            InputValue("text", "String", description=ATTACKS[10], default=default),
        ]),
        TypeDef("ENUM", "Mark", description=ATTACKS[6], values=[
            EnumValue("ONE", description=ATTACKS[7]),
            EnumValue("TWO", description=ATTACKS[8], deprecation=reason),
        ]),
        TypeDef("SCALAR", "Link", description=ATTACKS[9],
                specified_by="javascript:alert(1)"),
    ]
    directives = [Directive("mark", ["FIELD_DEFINITION"], description=block, args=[
        InputValue("note", "String", description=ATTACKS[10], default=default)])]
    schema = _schema(types, directives=directives)
    strings = ATTACKS + (CONTROLS, reason, default, "javascript:alert(1)", NAME_200, "Foo", "foo")
    return Document("H01-strings", dumps(introspection(schema)), READ,
                    "the strings of the row in every place a schema holds text, "
                    f"with a description of {len(big.encode('utf-8'))} bytes", strings)


def _surrogates_document() -> Document:
    """Returns the document of lone surrogate escapes in every text place."""
    raw = {
        "@@type@@": rb"a \ud800 b",
        "@@field@@": rb"\udc00 at the start",
        "@@arg@@": rb"at the end \udbff",
        "@@value@@": rb"\ud800\ud800 twice",
        "@@reason@@": rb"high \ud800 then low \udc00 apart",
    }
    types = [
        TypeDef("OBJECT", "Query", description="@@type@@", fields=[
            Field("a", "String", description="@@field@@", deprecation="@@reason@@",
                  args=[InputValue("b", "Pick", description="@@arg@@")]),
        ]),
        TypeDef("ENUM", "Pick", values=[EnumValue("ONE", description="@@value@@")]),
    ]
    data = _splice(dumps(introspection(_schema(types))), raw)
    strings = ("a \ufffd b", "\ufffd at the start", "at the end \ufffd",
               "\ufffd\ufffd twice", "high \ufffd then low \ufffd apart")
    return Document("H04-lone-surrogates", data, READ,
                    "a lone surrogate escape in every text place becomes U+FFFD", strings)


def _reshaped(change) -> bytes:
    """Returns the minimal result with `change` applied to its `__schema`."""
    document = json.loads(_minimal())
    change(document["data"]["__schema"])
    return dumps(document)


def _wrapped(kinds: list[str]) -> bytes:
    """Returns a result whose one field wraps `String` in `kinds`, outermost first."""
    document = json.loads(_minimal())
    ref: dict = {"kind": "SCALAR", "name": "String", "ofType": None}
    for kind in reversed(kinds):
        ref = {"kind": kind, "name": None, "ofType": ref}
    document["data"]["__schema"]["types"][0]["fields"][0]["type"] = ref
    return dumps(document)


def _many_types(count: int) -> bytes:
    """Returns a result of `count` types, the query type and scalars."""
    types = [TypeDef("OBJECT", "Query", fields=[Field("a", "String")]),
             TypeDef("SCALAR", "String")]
    types.extend(TypeDef("SCALAR", f"S{i}") for i in range(count - len(types)))
    return dumps(introspection(Schema("hostile", types)))


def _many_values(count: int) -> bytes:
    """Returns a result of `count` members, one field and the rest enum values."""
    return dumps(introspection(_schema([
        TypeDef("OBJECT", "Query", fields=[Field("a", "Wide")]),
        TypeDef("ENUM", "Wide", values=[EnumValue(f"V{i}") for i in range(count - 1)]),
    ])))


def parse_arguments(argv: list[str]) -> argparse.Namespace:
    """Reads the command line of the hostile corpus."""
    parser = argparse.ArgumentParser(prog="python -m tests.gen.hostile", description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--mutations", type=int, default=MUTATION_COUNT)
    parser.add_argument("--seed", type=int, default=SEED)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    """Prints one line per document and the digest of the mutated documents.

    With `--out` the documents and their manifest are written as well. The
    mutations run over the four presets, and their line carries the count per
    operation and the SHA-256 of the digests of all of them in order, so two
    runs with one seed print one line.
    """
    arguments = parse_arguments(argv)
    if arguments.out is not None:
        entries = write(arguments.out)
    else:
        entries = [_entry(doc) for doc in json_documents() + introspection_documents()]
    for entry in entries:
        print(json.dumps(entry, ensure_ascii=False))
    total = hashlib.sha256()
    per_operation: dict[str, int] = {}
    size = 0
    for doc in mutations(preset_sources(), arguments.mutations, arguments.seed):
        operation = doc.name.split("-")[1]
        per_operation[operation] = per_operation.get(operation, 0) + 1
        size += len(doc.data)
        total.update(doc.sha256.encode("ascii"))
    print(json.dumps({"mutations": arguments.mutations, "seed": arguments.seed,
                      "sources": list(PRESETS), "per_operation": per_operation,
                      "bytes": size, "sha256": total.hexdigest()}))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
