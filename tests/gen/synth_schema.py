# SPDX-License-Identifier: MIT
"""tests/gen: the seeded synthetic schemas of the documentation notebooks.

The generator writes synthetic-176, which has the counts of the largest measured
schema of the estate, synthetic-5000 for the 5000 type gate, and the schemas of
2500 and 10,000 types for the linear shape of the pass, each from seed 20260923.
The same seed writes the same bytes, and the notebook compares their SHA-256
with the digest recorded beside the goldens before any tree is compared. Every
type and field name is generic and copied from no application.

The counts of synthetic-176 are those of the row `zitadel-gql`: 176 types, 93
objects, 40 inputs, 21 interfaces, 10 enums, 12 scalars, 771 fields, 72
arguments, 18 connection and 18 edge types, with 23 fields on the query root and
25 on the mutation root. A field is a field of an object or an interface or an
input field, and an enum value is no field, which is how the counts add up to 771.
The larger schemas scale every count of that row by the number of types beside
the twelve scalars, so the three sizes of the linear shape share one shape, and
the types stand in domains of about 14 types that reference each other mostly
inside the domain. The depth one neighbourhood of every type but the two roots
and `PageInfo` therefore keeps its size from 2500 to 10,000 types, and the
neighbours of all types together grow with the type count alone, which is the
shape the linear pass is judged on.

The output is an introspection result in the shape the operation introspect of
`tests/sidecar/graphql-js.mjs` answers, `{"data":{"__schema":..}}`, with the
keys in the order of `getIntrospectionQuery` of graphql-js 16.12.0 under its
four options and written the way `JSON.stringify` writes it. It carries the
declared types alone, in the order a schema of the compatibility layer prints them, so the eight
introspection types and the five specified directives a server adds are left
out, and the five specified scalars carry no description. The transforms of the
core fold the introspection types and the twelve branded scalars, the five
specified ones among them, into the field `builtin` of `Transformed`. The
specified directives are no types and never enter that field. All three parts
are left out to keep text that graphql-js wrote out of this repository.
[`to_sdl`] writes the same schema as SDL, and graphql-js answers that SDL with
the same types, fields, arguments, values, order and descriptions but for those
three parts.

    python -m tests.gen.synth_schema
    python -m tests.gen.synth_schema --name synthetic-176 --out target/viewer/synth
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path

from .rng import Rng
from .sdl import print_string

#: The seed every synthetic schema is written from.
SEED = 20260923

#: The schemas the generator writes, by name, with their type counts. The plan
#: names synthetic-176 and synthetic-5000 and gives the two other sizes of the
#: linear shape no name, so they take the same form.
SCHEMAS = {
    "synthetic-176": 176,
    "synthetic-2500": 2500,
    "synthetic-5000": 5000,
    "synthetic-10000": 10000,
}

#: The golden rule of each schema, the whole tree or `manifest.json` alone.
GOLDENS = {
    "synthetic-176": "tree",
    "synthetic-2500": "manifest",
    "synthetic-5000": "manifest",
    "synthetic-10000": "manifest",
}

#: Where the command line writes a schema when asked to, beside the trees of the
#: notebooks under `target/viewer/`.
DEFAULT_OUT = Path("target/viewer/synth")

#: The twelve branded scalars every build of the compatibility layer prints, in its order.
BRANDED_SCALARS = (
    "String", "ID", "Int", "Float", "Boolean", "Number", "Any", "Void",
    "JSONObject", "File", "Date", "JSON",
)

#: The five scalars the specification builds in, whose description graphql-js
#: supplies whatever the SDL says, because a standard scalar the SDL declares is
#: replaced by the built in one.
SPECIFIED_SCALARS = ("String", "Int", "Float", "Boolean", "ID")


@dataclass(frozen=True)
class Row:
    """Holds the counts one synthetic schema is built to."""

    types: int
    objects: int
    inputs: int
    interfaces: int
    enums: int
    scalars: int
    fields: int
    arguments: int
    connections: int
    query_fields: int
    mutation_fields: int


#: The row of the largest measured schema.
LARGEST = Row(176, 93, 40, 21, 10, 12, 771, 72, 18, 23, 25)

#: The number of types a domain holds on average, which is 164 declared types
#: beside the scalars over 12 domains in synthetic-176.
DOMAIN_TYPES = 14

#: The first words of the domain names. A stem cluster groups by the first word
#: of a name, so every domain up to this many has a cluster of its own.
WORDS = (
    "Alder", "Aspen", "Birch", "Cedar", "Cypress", "Elm", "Hazel", "Hemlock",
    "Larch", "Linden", "Maple", "Oak", "Pine", "Poplar", "Rowan", "Spruce",
    "Willow", "Yew", "Agate", "Basalt", "Beryl", "Cobalt", "Flint", "Garnet",
    "Granite", "Jasper", "Marble", "Onyx", "Quartz", "Slate", "Topaz", "Zircon",
    "Bay", "Creek", "Canyon", "Cliff", "Dune", "Fjord", "Glacier", "Grove",
    "Lagoon", "Meadow", "Mesa", "Heath", "Prairie", "Ravine", "Reef", "Ridge",
    "Tundra", "Valley", "Egret", "Finch", "Heron", "Ibis", "Kestrel", "Lark",
    "Magpie", "Osprey", "Plover", "Tern", "Wren", "Sparrow", "Thrush", "Starling",
)

#: The second words of the domain names past the first 64 domains, which share
#: a first word and so a stem cluster with an earlier domain.
QUALIFIERS = (
    "", "North", "South", "East", "West", "Upper", "Lower", "Inner", "Outer",
    "Early", "Late", "Middle",
)

#: The nouns of the plain objects of a domain.
NOUNS = (
    "Ledger", "Record", "Entry", "Batch", "Bundle", "Parcel", "Survey", "Sample",
    "Report", "Notice", "Ticket", "Voucher", "Journal", "Roster", "Schedule",
    "Budget", "Quota", "Tally", "Folder", "Sheet", "Badge", "Permit", "Summary",
    "Profile",
)

#: The traits of the interfaces, each with the fields it declares. A domain
#: holds each trait at most once, and no two traits share a field name, so an
#: object that implements several interfaces never meets two fields of one name.
TRAITS = (
    ("Tracked", (("trackedAt", "Date!"), ("trackedBy", "String"))),
    ("Named", (("displayName", "String!"), ("shortName", "String"))),
    ("Owned", (("ownerId", "ID!"), ("ownerRole", "String"))),
    ("Ranked", (("rank", "Int!"), ("rankNote", "String"))),
    ("Tagged", (("tags", "[String!]!"), ("tagCount", "Int"))),
    ("Versioned", (("version", "Int!"), ("versionNote", "String"))),
    ("Located", (("area", "String"), ("zone", "String"))),
    ("Scheduled", (("startsAt", "Date"), ("endsAt", "Date"))),
)

#: The leaf fields a plain object or an input draws from, with their scalar.
LEAVES = (
    ("label", "String"), ("code", "String"), ("title", "String"),
    ("summary", "String"), ("notes", "String"), ("comment", "String"),
    ("source", "String"), ("origin", "String"), ("color", "String"),
    ("shade", "String"), ("checksum", "String"), ("rating", "Int"),
    ("count", "Int"), ("total", "Int"), ("size", "Int"), ("width", "Int"),
    ("height", "Int"), ("depth", "Int"), ("level", "Int"), ("sequence", "Int"),
    ("score", "Float"), ("ratio", "Float"), ("weight", "Float"),
    ("amount", "Number"), ("balance", "Number"), ("active", "Boolean"),
    ("visible", "Boolean"), ("locked", "Boolean"), ("archived", "Boolean"),
    ("pinned", "Boolean"), ("createdAt", "Date"), ("updatedAt", "Date"),
    ("closedAt", "Date"), ("dueAt", "Date"), ("payload", "JSON"),
    ("settings", "JSONObject"), ("extra", "Any"), ("attachment", "File"),
    ("reference", "ID"), ("externalId", "ID"),
)

#: The roles of the inputs and of the enums named after a plain object.
INPUT_ROLES = ("Input", "Filter", "Patch", "Order")
ENUM_ROLES = ("State", "Kind", "Level")

#: The values an enum draws from, kept in this order.
VALUES = (
    "ACTIVE", "PENDING", "CLOSED", "DRAFT", "OPEN", "PAUSED", "LOW", "MEDIUM",
    "HIGH", "NONE", "FIRST", "SECOND", "THIRD", "OTHER", "UNKNOWN", "READY",
)

#: The sentences of the descriptions. `{noun}` is the type in words, `{area}`
#: the domain and `{field}` the member. Some carry the constructs of the
#: CommonMark subset, a code span, emphasis and a second paragraph.
TYPE_SENTENCES = (
    "A {noun} of the {area} area.",
    "Holds one {noun} and the facts that belong to it.",
    "The {noun} as the {area} area keeps it.",
    "A *{noun}* that the {area} area reads and writes.",
    "One {noun}.\n\nThe {area} area keeps it until it is closed.",
)
MEMBER_SENTENCES = (
    "The {field} of this {noun}.",
    "Returns the {field}.",
    "The `{field}` value of the {noun}.",
    "The {field}, or nothing when the {noun} has none.",
    "The {field} in *plain* text.\n\nIt stays the same once the {noun} is closed.",
)

#: The number of steps a probability below counts in. An event of
#: probability `P` happens when a draw of `randrange(CHANCE_STEPS)` is below
#: `P`, so every constant says exactly how many of the steps it takes.
CHANCE_STEPS = 10_000

# The draws below are choices of the generator that give the shape of the row,
# measured and printed by `counts`, each in steps of one in `CHANCE_STEPS`. A
# quarter of the fields point at a composite type, above the 18.5 percent of the
# corpus, because the root, connection and edge fields the row fixes are
# composite whatever the draws say.
P_INTERFACE_PARENT = 4000
P_INTERFACE_ORIGIN = 4000
P_INTERFACE_LABEL = 3000
P_IMPLEMENTS = 8500
P_HIDDEN = 3500
P_TOTAL_COUNT = 4000
P_NULLABLE_EDGES = 3000
P_NON_NULL_NODE = 5000
P_CONNECTION_ON_QUERY = 4000
P_INPUT_FIELD = 4500
P_COMPOSITE = 1500
P_ENUM = 1200
P_NESTED_INPUT = 800
P_INPUT_ENUM = 1200
P_DEFAULT = 1500
P_NON_NULL = 4000
P_SAME_DOMAIN = 7000
P_NEAR_DOMAIN = 2000
P_TYPE_DESCRIPTION = 4000
P_MEMBER_DESCRIPTION = 1200
P_TAG_DEPRECATED = 150
P_DIRECTIVE_DEPRECATED = 100
P_VALUE_DEPRECATED = 300

#: The bound on the draws of one free field name, past which the builder
#: refuses the schema with a `ValueError`.
MAX_ATTEMPTS = 64

#: The composite kinds, object, interface and union, whose types the walks of
#: `docs/aoviewer.md` section 2.3 count.
COMPOSITE_KINDS = ("OBJECT", "INTERFACE", "UNION")


@dataclass
class InputValue:
    """Holds an argument or an input field.

    `type` is the type in SDL notation, such as `[Foo!]!`, and `default` the
    GraphQL literal introspection prints for the default value.
    """

    name: str
    type: str
    description: str | None = None
    default: str | None = None
    deprecation: str | None = None


@dataclass
class Field:
    """Holds a field of an object or an interface.

    `deprecation` is the reason of a `@deprecated` directive, and `None` when
    the field carries none.
    """

    name: str
    type: str
    description: str | None = None
    args: list[InputValue] = field(default_factory=list)
    deprecation: str | None = None


@dataclass
class EnumValue:
    """Holds one value of an enum."""

    name: str
    description: str | None = None
    deprecation: str | None = None


@dataclass
class TypeDef:
    """Holds one named type.

    `kind` is the introspection kind, `OBJECT`, `INTERFACE`, `UNION`, `ENUM`,
    `INPUT_OBJECT` or `SCALAR`, and each list holds what its kind carries.
    """

    kind: str
    name: str
    description: str | None = None
    fields: list[Field] = field(default_factory=list)
    input_fields: list[InputValue] = field(default_factory=list)
    interfaces: list[str] = field(default_factory=list)
    values: list[EnumValue] = field(default_factory=list)
    members: list[str] = field(default_factory=list)
    specified_by: str | None = None


@dataclass
class Directive:
    """Holds one directive definition."""

    name: str
    locations: list[str]
    description: str | None = None
    args: list[InputValue] = field(default_factory=list)
    repeatable: bool = False


@dataclass
class Schema:
    """Holds a schema in the order its types print."""

    name: str
    types: list[TypeDef]
    query: str | None = "Query"
    mutation: str | None = None
    subscription: str | None = None
    directives: list[Directive] = field(default_factory=list)


@dataclass(frozen=True)
class Written:
    """Names the file one schema was written to and its digest."""

    name: str
    path: Path
    size: int
    sha256: str


def row(types: int) -> Row:
    """Returns the counts of a synthetic schema of `types` types.

    Every count of [`LARGEST`] but the twelve scalars scales by the number of
    types beside the scalars, rounded half up, and the four kinds share those
    types by the largest remainder, so `row(176)` is the measured row itself.
    """
    if types < LARGEST.types:
        raise ValueError(f"a synthetic schema has {LARGEST.types} types or more, not {types}")
    rest = types - LARGEST.scalars
    objects, inputs, interfaces, enums = _apportion(
        rest, (LARGEST.objects, LARGEST.inputs, LARGEST.interfaces, LARGEST.enums)
    )
    return Row(
        types=types,
        objects=objects,
        inputs=inputs,
        interfaces=interfaces,
        enums=enums,
        scalars=LARGEST.scalars,
        fields=_scaled(LARGEST.fields, rest),
        arguments=_scaled(LARGEST.arguments, rest),
        connections=_scaled(LARGEST.connections, rest),
        query_fields=_scaled(LARGEST.query_fields, rest),
        mutation_fields=_scaled(LARGEST.mutation_fields, rest),
    )


def build(name: str, seed: int = SEED) -> Schema:
    """Returns the synthetic schema called `name`, drawn from `seed`.

    `name` is a key of [`SCHEMAS`]. The same name and seed give the same
    schema on every run, because every draw comes from a stream of
    [`Rng`] named after the schema and its purpose.
    """
    if name not in SCHEMAS:
        raise ValueError(f"no synthetic schema named {name}")
    return _Builder(name, row(SCHEMAS[name]), Rng(seed)).run()


def introspection(schema: Schema) -> dict:
    """Returns the introspection result of `schema` as a JSON object.

    The keys stand in the order of `getIntrospectionQuery` of graphql-js
    16.12.0 with `descriptions`, `specifiedByUrl`, `directiveIsRepeatable` and
    `inputValueDeprecation` on, and a key a kind does not carry is `null`, as
    graphql-js answers it. A name no type declares is written with the kind
    `OBJECT`, so a hostile document can name a type it lacks.
    """
    kinds = {t.name: t.kind for t in schema.types}
    types = []
    for t in schema.types:
        composite = t.kind in ("OBJECT", "INTERFACE")
        possible = None
        if t.kind == "INTERFACE":
            possible = [
                _ref(o.name, kinds)
                for o in schema.types
                if o.kind == "OBJECT" and t.name in o.interfaces
            ]
        elif t.kind == "UNION":
            possible = [_ref(m, kinds) for m in t.members]
        types.append({
            "kind": t.kind,
            "name": t.name,
            "description": t.description,
            "specifiedByURL": t.specified_by,
            "fields": [_field(f, kinds) for f in t.fields] if composite else None,
            "inputFields": (
                [_input_value(v, kinds) for v in t.input_fields]
                if t.kind == "INPUT_OBJECT" else None
            ),
            "interfaces": [_ref(i, kinds) for i in t.interfaces] if composite else None,
            "enumValues": [_enum_value(v) for v in t.values] if t.kind == "ENUM" else None,
            "possibleTypes": possible,
        })
    return {
        "data": {
            "__schema": {
                "queryType": _root(schema.query),
                "mutationType": _root(schema.mutation),
                "subscriptionType": _root(schema.subscription),
                "types": types,
                "directives": [
                    {
                        "name": d.name,
                        "description": d.description,
                        "isRepeatable": d.repeatable,
                        "locations": list(d.locations),
                        "args": [_input_value(a, kinds) for a in d.args],
                    }
                    for d in schema.directives
                ],
            }
        }
    }


def dumps(document: object) -> bytes:
    """Returns `document` as the UTF-8 bytes `JSON.stringify` writes for it.

    No space follows a separator and no line ends the text. A character under
    U+0020 becomes its short escape or `\\u00xx` in lower case, the quote and
    the backslash take their short escapes, and every other character stands as
    itself, which is what both writers do for text without a lone surrogate.
    """
    return json.dumps(document, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def to_sdl(schema: Schema) -> str:
    """Returns `schema` as SDL text that graphql-js builds.

    Every description is a quoted string, which graphql-js reads back as it
    stands, where a block string would lose its indent. A schema definition is
    written only when a root name differs from its default.
    """
    out: list[str] = []
    roots = (("query", schema.query, "Query"), ("mutation", schema.mutation, "Mutation"),
             ("subscription", schema.subscription, "Subscription"))
    if any(value is not None and value != default for _, value, default in roots):
        out.append("schema {")
        out.extend(f"  {key}: {value}" for key, value, _ in roots if value is not None)
        out.append("}")
        out.append("")
    for t in schema.types:
        if t.description is not None:
            out.append(print_string(t.description))
        if t.kind == "SCALAR":
            line = f"scalar {t.name}"
            if t.specified_by is not None:
                line += f" @specifiedBy(url: {print_string(t.specified_by)})"
            out.append(line)
        elif t.kind == "UNION":
            out.append(f"union {t.name} = " + " | ".join(t.members))
        elif t.kind == "ENUM":
            out.append(f"enum {t.name} {{")
            for v in t.values:
                if v.description is not None:
                    out.append("  " + print_string(v.description))
                out.append(f"  {v.name}{_deprecated(v.deprecation)}")
            out.append("}")
        elif t.kind == "INPUT_OBJECT":
            out.append(f"input {t.name} {{")
            for v in t.input_fields:
                if v.description is not None:
                    out.append("  " + print_string(v.description))
                out.append("  " + _sdl_input_value(v))
            out.append("}")
        else:
            keyword = "type" if t.kind == "OBJECT" else "interface"
            implements = " implements " + " & ".join(t.interfaces) if t.interfaces else ""
            out.append(f"{keyword} {t.name}{implements} {{")
            for f in t.fields:
                if f.description is not None:
                    out.append("  " + print_string(f.description))
                args = ""
                if f.args:
                    args = "(" + ", ".join(_sdl_arg(a) for a in f.args) + ")"
                out.append(f"  {f.name}{args}: {f.type}{_deprecated(f.deprecation)}")
            out.append("}")
        out.append("")
    for d in schema.directives:
        if d.description is not None:
            out.append(print_string(d.description))
        args = "(" + ", ".join(_sdl_arg(a) for a in d.args) + ")" if d.args else ""
        repeatable = " repeatable" if d.repeatable else ""
        out.append(f"directive @{d.name}{args}{repeatable} on " + " | ".join(d.locations))
        out.append("")
    return "\n".join(out)


def generate(name: str, seed: int = SEED) -> bytes:
    """Returns the bytes of the introspection result of the schema `name`."""
    return dumps(introspection(build(name, seed)))


def write(name: str, directory: Path = DEFAULT_OUT, seed: int = SEED) -> Written:
    """Writes the schema `name` to `<directory>/<name>.json` and names it.

    The directory is created when it is missing. The result carries the size
    and the SHA-256 of the bytes written, which the notebook compares with the
    digest recorded beside the goldens.
    """
    data = generate(name, seed)
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"{name}.json"
    path.write_bytes(data)
    return Written(name, path, len(data), hashlib.sha256(data).hexdigest())


def walk(schema: Schema, strict: bool = False) -> set[str]:
    """Returns the names of the types a walk from the roots of `schema` reaches.

    The walk of aoviewer starts at all three roots and follows field types,
    union members, the implementations of an interface, the interfaces a type
    implements, argument types and input field types. The strict walk follows
    field types and the interfaces a type implements alone. A name no type
    declares is not followed.
    """
    by_name = {t.name: t for t in schema.types}
    implementations: dict[str, list[str]] = {}
    for t in schema.types:
        for name in t.interfaces:
            implementations.setdefault(name, []).append(t.name)
    roots = [r for r in (schema.query, schema.mutation, schema.subscription) if r in by_name]
    seen = set(roots)
    todo = list(roots)
    while todo:
        t = by_name[todo.pop()]
        names = [_named(f.type) for f in t.fields] + t.interfaces
        if not strict:
            names += [_named(a.type) for f in t.fields for a in f.args]
            names += [_named(v.type) for v in t.input_fields]
            names += t.members + implementations.get(t.name, [])
        for name in names:
            if name in by_name and name not in seen:
                seen.add(name)
                todo.append(name)
    return seen


def counts(schema: Schema) -> dict:
    """Returns the counts of `schema` the notebook prints beside the row.

    A connection is an object whose name ends in `Connection` and which has an
    `edges` field. `reached` counts the composite types the walk of aoviewer
    reaches, and `reached_only_as_implementation` those it reaches and the
    strict walk does not, which is the lint of that name.
    """
    kinds = {t.name: t.kind for t in schema.types}
    by_kind = {kind: 0 for kind in ("OBJECT", "INPUT_OBJECT", "INTERFACE", "UNION", "ENUM", "SCALAR")}
    fields = arguments = values = implements = composite = described = 0
    tagged = deprecated = 0
    described_bytes = 0
    for t in schema.types:
        by_kind[t.kind] = by_kind.get(t.kind, 0) + 1
        implements += len(t.interfaces)
        values += len(t.values)
        texts = [t.description]
        for f in t.fields:
            fields += 1
            arguments += len(f.args)
            if kinds.get(_named(f.type)) in COMPOSITE_KINDS:
                composite += 1
            if f.deprecation is not None:
                deprecated += 1
            if f.description is not None and f.description.startswith("@deprecated"):
                tagged += 1
            texts.append(f.description)
            texts.extend(a.description for a in f.args)
        for v in t.input_fields:
            fields += 1
            texts.append(v.description)
        for v in t.values:
            if v.deprecation is not None:
                deprecated += 1
            texts.append(v.description)
        for text in texts:
            if text is not None:
                described += 1
                described_bytes += len(text.encode("utf-8"))
    connections = sum(
        1 for t in schema.types
        if t.kind == "OBJECT" and t.name.endswith("Connection")
        and any(f.name == "edges" for f in t.fields)
    )
    edges = sum(1 for t in schema.types if t.kind == "OBJECT" and t.name.endswith("Edge"))
    composites = [t.name for t in schema.types if t.kind in COMPOSITE_KINDS]
    aoviewer = walk(schema)
    strict = walk(schema, strict=True)
    return {
        "types": len(schema.types),
        "objects": by_kind["OBJECT"],
        "inputs": by_kind["INPUT_OBJECT"],
        "interfaces": by_kind["INTERFACE"],
        "unions": by_kind["UNION"],
        "enums": by_kind["ENUM"],
        "scalars": by_kind["SCALAR"],
        "fields": fields,
        "arguments": arguments,
        "enum_values": values,
        "connections": connections,
        "edges": edges,
        "implements": implements,
        "composite_fields": composite,
        "composite_types": len(composites),
        "reached": sum(1 for name in composites if name in aoviewer),
        "reached_only_as_implementation": sum(
            1 for name in composites if name in aoviewer and name not in strict),
        "deprecated_by_directive": deprecated,
        "deprecated_by_tag": tagged,
        "descriptions": described,
        "description_bytes": described_bytes,
    }


def _scaled(value: int, rest: int) -> int:
    """Scales a count of the measured row to `rest` types, rounded half up."""
    base = LARGEST.types - LARGEST.scalars
    return (2 * value * rest + base) // (2 * base)


def _apportion(total: int, weights: tuple[int, ...]) -> list[int]:
    """Shares `total` by `weights` with the largest remainder, ties in order."""
    whole = sum(weights)
    shares = [total * w // whole for w in weights]
    remainders = [total * w % whole for w in weights]
    order = sorted(range(len(weights)), key=lambda i: (-remainders[i], i))
    for i in order[: total - sum(shares)]:
        shares[i] += 1
    return shares


def _spread(count: int, parts: int) -> list[int]:
    """Shares `count` over `parts` as evenly as integers allow, in order."""
    return [count * (i + 1) // parts - count * i // parts for i in range(parts)]


def _lower(name: str) -> str:
    """Returns `name` with its first letter in lower case."""
    return name[:1].lower() + name[1:]


def _plural(word: str) -> str:
    """Returns the English plural of a generic noun of this module."""
    if word.endswith(("ch", "sh", "s", "x")):
        return word + "es"
    if word.endswith("y") and word[-2:-1] not in "aeiou":
        return word[:-1] + "ies"
    return word + "s"


def _words(name: str) -> str:
    """Returns an upper camel case name as lower case words."""
    out = []
    for char in name:
        if char.isupper() and out:
            out.append(" ")
        out.append(char.lower())
    return "".join(out)


def _named(expr: str) -> str:
    """Returns the named type inside a type in SDL notation."""
    return expr.strip("[]!")


def _root(name: str | None) -> dict | None:
    """Returns the root entry of `__schema`, `{name kind}` or `null`."""
    return None if name is None else {"name": name, "kind": "OBJECT"}


def _ref(expr: str, kinds: dict[str, str]) -> dict:
    """Returns the introspection type reference of a type in SDL notation."""
    if expr.endswith("!"):
        return {"kind": "NON_NULL", "name": None, "ofType": _ref(expr[:-1], kinds)}
    if expr.startswith("[") and expr.endswith("]"):
        return {"kind": "LIST", "name": None, "ofType": _ref(expr[1:-1], kinds)}
    return {"kind": kinds.get(expr, "OBJECT"), "name": expr, "ofType": None}


def _field(f: Field, kinds: dict[str, str]) -> dict:
    """Returns the introspection entry of a field."""
    return {
        "name": f.name,
        "description": f.description,
        "args": [_input_value(a, kinds) for a in f.args],
        "type": _ref(f.type, kinds),
        "isDeprecated": f.deprecation is not None,
        "deprecationReason": f.deprecation,
    }


def _input_value(v: InputValue, kinds: dict[str, str]) -> dict:
    """Returns the introspection entry of an argument or an input field."""
    return {
        "name": v.name,
        "description": v.description,
        "type": _ref(v.type, kinds),
        "defaultValue": v.default,
        "isDeprecated": v.deprecation is not None,
        "deprecationReason": v.deprecation,
    }


def _enum_value(v: EnumValue) -> dict:
    """Returns the introspection entry of an enum value."""
    return {
        "name": v.name,
        "description": v.description,
        "isDeprecated": v.deprecation is not None,
        "deprecationReason": v.deprecation,
    }


def _deprecated(reason: str | None) -> str:
    """Returns the `@deprecated` directive of a reason, or nothing."""
    return "" if reason is None else f" @deprecated(reason: {print_string(reason)})"


def _sdl_input_value(v: InputValue) -> str:
    """Returns an argument or an input field without its description."""
    default = "" if v.default is None else f" = {v.default}"
    return f"{v.name}: {v.type}{default}{_deprecated(v.deprecation)}"


def _sdl_arg(a: InputValue) -> str:
    """Returns an argument with its description in front of it."""
    text = _sdl_input_value(a)
    return text if a.description is None else f"{print_string(a.description)} {text}"


class _Domain:
    """Holds the types of one domain while the builder fills them."""

    def __init__(self, index: int, name: str) -> None:
        self.index = index
        self.name = name
        self.lower = _lower(name)
        self.objects: list[TypeDef] = []
        self.interfaces: list[TypeDef] = []
        self.inputs: list[TypeDef] = []
        self.enums: list[TypeDef] = []
        self.connections: list[tuple[TypeDef, TypeDef]] = []
        self.nodes = 0


class _Builder:
    """Builds one synthetic schema to the counts of a [`Row`]."""

    def __init__(self, name: str, target: Row, rng: Rng) -> None:
        self.name = name
        self.row = target
        self.rng = rng
        self.domains: list[_Domain] = []
        self.domain_of: dict[str, _Domain] = {}
        self.hidden: dict[str, bool] = {}
        self.query: list[tuple[int, int, Field]] = []
        self.mutation: list[tuple[int, int, Field]] = []
        self.connection_fields: list[Field] = []
        self.fetch_fields: list[Field] = []
        self.find_fields: list[tuple[_Domain, Field]] = []
        self.list_fields: list[Field] = []
        self.own_fields: list[tuple[TypeDef, Field]] = []
        self.query_type = TypeDef("OBJECT", "Query")
        self.mutation_type = TypeDef("OBJECT", "Mutation")
        self.page_info = TypeDef("OBJECT", "PageInfo", fields=[
            Field("hasNextPage", "Boolean!"),
            Field("hasPreviousPage", "Boolean!"),
            Field("startCursor", "String"),
            Field("endCursor", "String"),
        ])

    def stream(self, purpose: str):
        """Returns the random stream of `purpose` within this schema."""
        return self.rng.stream(f"{self.name}:{purpose}")

    def chance(self, stream, p: int) -> bool:
        """Draws one event that takes `p` of the [`CHANCE_STEPS`] steps."""
        return stream.randrange(CHANCE_STEPS) < p

    def run(self) -> Schema:
        """Builds the schema in the order the counts depend on each other."""
        self._domains()
        self._interfaces()
        self._implements()
        self._connections()
        self._inputs_and_enums()
        self._roots()
        self._seal_roots()
        self._mandatory_references()
        self._link_islands()
        self._fill_fields()
        self._arguments()
        self._text()
        return self._assemble()

    def _domains(self) -> None:
        """Names the domains and their plain objects."""
        rest = self.row.types - self.row.scalars
        count = max(3, (rest + DOMAIN_TYPES // 2) // DOMAIN_TYPES)
        if count > len(WORDS) * len(QUALIFIERS):
            raise ValueError(f"{self.row.types} types need more domain names than exist")
        plain = self.row.objects - 3 - 2 * self.row.connections
        nodes = _spread(self.row.connections, count)
        for index, share in enumerate(_spread(plain, count)):
            name = WORDS[index % len(WORDS)] + QUALIFIERS[index // len(WORDS)]
            domain = _Domain(index, name)
            domain.nodes = nodes[index]
            if share > len(NOUNS) or share < max(1, domain.nodes):
                raise ValueError(f"a domain of {share} plain objects is outside the nouns")
            nouns = list(NOUNS)
            self.stream(f"nouns:{index}").shuffle(nouns)
            for noun in nouns[:share]:
                obj = TypeDef("OBJECT", name + noun, fields=[Field("id", "ID!")])
                domain.objects.append(obj)
                self.domain_of[obj.name] = domain
                self.hidden[obj.name] = False
            self.domains.append(domain)

    def _interfaces(self) -> None:
        """Declares the interfaces of every domain with their trait fields."""
        spread = _spread(self.row.interfaces, len(self.domains))
        for domain, share in zip(self.domains, spread):
            if share > len(TRAITS):
                raise ValueError(f"a domain of {share} interfaces is outside the traits")
            draw = self.stream(f"interfaces:{domain.index}")
            traits = list(TRAITS)
            draw.shuffle(traits)
            for position, (trait, declared) in enumerate(traits[:share]):
                iface = TypeDef("INTERFACE", domain.name + trait)
                if position > 0 and self.chance(draw, P_INTERFACE_PARENT):
                    parent = domain.interfaces[-1]
                    iface.interfaces = parent.interfaces + [parent.name]
                    iface.fields = [Field(f.name, f.type) for f in parent.fields]
                prefix = _lower(trait)
                iface.fields.extend(Field(n, t) for n, t in declared)
                if self.chance(draw, P_INTERFACE_ORIGIN):
                    iface.fields.append(Field(prefix + "Origin", domain.objects[0].name))
                if self.chance(draw, P_INTERFACE_LABEL):
                    iface.fields.append(Field(prefix + "Label", "String"))
                domain.interfaces.append(iface)
                self.domain_of[iface.name] = domain

    def _implements(self) -> None:
        """Lets the plain objects implement interfaces of their domain.

        The list is closed over the interfaces an interface implements, which
        graphql-js asks for, and the object declares every field of every
        interface it lists after its `id`. The first objects of a domain, which
        are the nodes of its connections, are never hidden, and neither is the
        first when the domain has no connection, because interface fields and
        the fallback references point at it.
        """
        for domain in self.domains:
            draw = self.stream(f"implements:{domain.index}")
            for position, obj in enumerate(domain.objects):
                chosen: list[str] = []
                for iface in domain.interfaces:
                    if self.chance(draw, P_IMPLEMENTS):
                        for name in iface.interfaces + [iface.name]:
                            if name not in chosen:
                                chosen.append(name)
                order = [i.name for i in domain.interfaces]
                obj.interfaces = sorted(chosen, key=order.index)
                seen = {"id"}
                for name in obj.interfaces:
                    iface = domain.interfaces[order.index(name)]
                    for f in iface.fields:
                        if f.name not in seen:
                            seen.add(f.name)
                            obj.fields.append(Field(f.name, f.type))
                shown = max(1, domain.nodes)
                if obj.interfaces and position >= shown and self.chance(draw, P_HIDDEN):
                    self.hidden[obj.name] = True

    def _connections(self) -> None:
        """Declares the connection and edge types and the field of each.

        The node of a connection is a plain object of its domain that never
        hides, and the field that returns the connection stands on the query
        root or on another plain object of the same domain.
        """
        on_query = 0
        for domain in self.domains:
            draw = self.stream(f"connections:{domain.index}")
            for position, node in enumerate(domain.objects[:domain.nodes]):
                conn = TypeDef("OBJECT", node.name + "Connection")
                edge = TypeDef("OBJECT", node.name + "Edge")
                edges = f"[{edge.name}]" if self.chance(draw, P_NULLABLE_EDGES) else f"[{edge.name}!]!"
                conn.fields = [Field("edges", edges), Field("pageInfo", "PageInfo!")]
                if self.chance(draw, P_TOTAL_COUNT):
                    conn.fields.append(Field("totalCount", "Int!"))
                node_type = node.name + "!" if self.chance(draw, P_NON_NULL_NODE) else node.name
                edge.fields = [Field("cursor", "String!"), Field("node", node_type)]
                domain.connections.append((conn, edge))
                self.domain_of[conn.name] = domain
                self.domain_of[edge.name] = domain
                noun = node.name[len(domain.name):]
                parents = [o for o in domain.objects if o is not node]
                query = self.chance(draw, P_CONNECTION_ON_QUERY)
                if not parents or (query and on_query < self.row.query_fields // 2):
                    f = Field(domain.lower + _plural(noun), conn.name + "!")
                    self.query.append((domain.index, position, f))
                    on_query += 1
                else:
                    parent = parents[draw.randrange(len(parents))]
                    f = Field(_lower(_plural(noun)), conn.name + "!")
                    parent.fields.append(f)
                self.connection_fields.append(f)

    def _inputs_and_enums(self) -> None:
        """Declares the inputs and the enums, each named after a plain object."""
        inputs = _spread(self.row.inputs, len(self.domains))
        enums = _spread(self.row.enums, len(self.domains))
        for domain, n_inputs, n_enums in zip(self.domains, inputs, enums):
            draw = self.stream(f"inputs:{domain.index}")
            objects = domain.objects
            if n_inputs > len(objects) * len(INPUT_ROLES) or n_enums > len(objects) * len(ENUM_ROLES):
                raise ValueError("a domain holds more inputs or enums than its names allow")
            for j in range(n_inputs):
                obj = objects[j % len(objects)]
                role = INPUT_ROLES[j // len(objects)]
                t = TypeDef("INPUT_OBJECT", obj.name + role)
                leaf, scalar = LEAVES[draw.randrange(len(LEAVES))]
                t.input_fields.append(self._input_leaf(draw, leaf, scalar))
                domain.inputs.append(t)
                self.domain_of[t.name] = domain
            for j in range(n_enums):
                obj = objects[j % len(objects)]
                t = TypeDef("ENUM", obj.name + ENUM_ROLES[j // len(objects)])
                picked = list(range(len(VALUES)))
                draw.shuffle(picked)
                size = 2 + draw.randrange(5)
                t.values = [EnumValue(VALUES[i]) for i in sorted(picked[:size])]
                domain.enums.append(t)
                self.domain_of[t.name] = domain

    def _input_leaf(self, draw, name: str, scalar: str) -> InputValue:
        """Returns a leaf input field, sometimes with a default value."""
        value = InputValue(name, scalar + ("!" if self.chance(draw, P_NON_NULL) else ""))
        if self.chance(draw, P_DEFAULT):
            value.default = {"Int": "10", "Boolean": "false", "String": '"none"'}.get(scalar)
        return value

    def _roots(self) -> None:
        """Fills the two roots to their counts, domain by domain in turn.

        The query root holds the connection fields placed on it and takes
        fetch fields of the plain objects that show and find fields of the
        interfaces. The mutation root takes a create, a remove and an update
        field per plain object that shows, the create only where the object
        has its `Input` and the update only where it has its `Patch`, each
        with the one argument it cannot do without. Both take one item per
        domain in turn until their count is reached.
        """
        wanted = self.row.query_fields - len(self.query)
        per_domain = []
        for domain in self.domains:
            fetch = [o for o in domain.objects if not self.hidden[o.name]]
            items = []
            for k in range(max(len(fetch), len(domain.interfaces))):
                if k < len(fetch):
                    noun = fetch[k].name[len(domain.name):]
                    items.append(("fetch", Field(domain.lower + noun, fetch[k].name)))
                if k < len(domain.interfaces):
                    iface = domain.interfaces[k]
                    items.append(("find", Field("find" + iface.name, f"[{iface.name}!]!")))
            per_domain.append(items)
        for domain_index, kind, position, f in _round_robin(per_domain, wanted):
            self.query.append((domain_index, 100 + position, f))
            if kind == "fetch":
                self.fetch_fields.append(f)
            else:
                self.find_fields.append((self.domains[domain_index], f))
        per_domain = []
        for domain in self.domains:
            names = {t.name for t in domain.inputs}
            items = []
            for obj in domain.objects:
                if self.hidden[obj.name]:
                    continue
                if obj.name + "Input" in names:
                    items.append(("create", Field(
                        "create" + obj.name, obj.name + "!",
                        args=[InputValue("input", obj.name + "Input!")])))
                items.append(("remove", Field(
                    "remove" + obj.name, "Boolean!", args=[InputValue("id", "ID!")])))
                if obj.name + "Patch" in names:
                    items.append(("update", Field(
                        "update" + obj.name, obj.name,
                        args=[InputValue("patch", obj.name + "Patch!")])))
            per_domain.append(items)
        for domain_index, _, position, f in _round_robin(per_domain, self.row.mutation_fields):
            self.mutation.append((domain_index, position, f))

    def _seal_roots(self) -> None:
        """Orders the root fields by domain and by their place in it."""
        for root, fields in ((self.query_type, self.query), (self.mutation_type, self.mutation)):
            root.fields = [f for _, _, f in sorted(fields, key=lambda item: (item[0], item[1]))]

    def _references(self) -> dict[str, int]:
        """Counts the fields that name each type so far."""
        found: dict[str, int] = {}
        fields = [f for _, _, f in self.query + self.mutation]
        for domain in self.domains:
            for t in domain.objects + domain.interfaces:
                fields.extend(t.fields)
            for conn, edge in domain.connections:
                fields.extend(conn.fields + edge.fields)
        for f in fields:
            named = _named(f.type)
            found[named] = found.get(named, 0) + 1
        return found

    def _mandatory_references(self) -> None:
        """Lets a field name every interface and every plain object that shows.

        An interface nobody names gets a list field on a plain object of its
        domain, and a plain object that is not hidden and that nobody names
        gets a field on another plain object. Every interface and every plain
        object that shows is then named by a field, and a hidden one
        implements a named interface. A name alone does not bring a type into
        the walk of aoviewer, because a domain can name its types from inside
        itself only, which [`_Builder._link_islands`] mends.
        """
        found = self._references()
        for domain in self.domains:
            draw = self.stream(f"references:{domain.index}")
            for iface in domain.interfaces:
                if iface.name in found:
                    continue
                trait = iface.name[len(domain.name):]
                holders = [o for o in domain.objects if not self.hidden[o.name]]
                holder = holders[draw.randrange(len(holders))]
                f = Field(_lower(trait) + "Items", f"[{iface.name}!]!")
                holder.fields.append(f)
                self.own_fields.append((holder, f))
                self.list_fields.append(f)
                found[iface.name] = 1
            for obj in domain.objects:
                if self.hidden[obj.name] or obj.name in found:
                    continue
                others = [o for o in domain.objects if o is not obj]
                if others:
                    holder = others[draw.randrange(len(others))]
                    name = _lower(obj.name[len(domain.name):])
                else:
                    holder = self.domains[(domain.index + 1) % len(self.domains)].objects[0]
                    name = _lower(obj.name)
                if any(f.name == name for f in holder.fields):
                    name = _lower(obj.name)
                f = Field(name, obj.name)
                holder.fields.append(f)
                self.own_fields.append((holder, f))
                found[obj.name] = 1

    def _link_islands(self) -> None:
        """Links every island to the walk of aoviewer by its interfaces and shown plain objects.

        Every composite type of the largest measured schema is reached by the
        walk of aoviewer, and a synthetic schema keeps that property of the row.
        The references above leave a domain whose names stay inside it as an
        island no root reaches, 27 composite types in 4 domains of
        synthetic-5000 and 36 in 5 domains of synthetic-10000, and none in the
        two smaller schemas. Each interface and then each plain object that
        shows and is still not reached gets a field on a plain object that shows
        and is reached, of its own domain where one exists and else of the
        nearest domain before it, and the walk is taken again. The fill only
        adds fields, so no type leaves the walk after this step, and
        [`_Builder._assemble`] refuses a schema where one does.
        """
        reached = walk(self._draft())
        for domain in self.domains:
            shown = [o for o in domain.objects if not self.hidden[o.name]]
            targets = domain.interfaces + shown
            if all(t.name in reached for t in targets):
                continue
            draw = self.stream(f"links:{domain.index}")
            for target in targets:
                if target.name in reached:
                    continue
                holder, foreign = self._reached_holder(draw, domain, reached)
                local = target.name if foreign else target.name[len(domain.name):]
                if target.kind == "INTERFACE":
                    name, expr, fallback = (_lower(local) + "Items", f"[{target.name}!]!",
                                            _lower(target.name) + "Items")
                else:
                    name, expr, fallback = _lower(local), target.name, _lower(target.name)
                taken = {f.name for f in holder.fields}
                if name in taken:
                    name = fallback
                if name in taken:
                    raise ValueError(
                        f"{holder.name} of {self.name} has no free name for {target.name}")
                f = Field(name, expr)
                holder.fields.append(f)
                self.own_fields.append((holder, f))
                if expr.startswith("["):
                    self.list_fields.append(f)
                reached = walk(self._draft())

    def _reached_holder(self, draw, domain: _Domain, reached: set[str]) -> tuple[TypeDef, bool]:
        """Draws a plain object that shows and is reached, and whether it is foreign.

        The object stands in `domain` where one there is reached, and else in
        the nearest domain before it that holds one.
        """
        count = len(self.domains)
        for step in range(count):
            other = self.domains[(domain.index - step) % count]
            pool = [o for o in other.objects if not self.hidden[o.name] and o.name in reached]
            if pool:
                return pool[draw.randrange(len(pool))], other is not domain
        raise ValueError(f"no plain object of {self.name} is reached from a root")

    def _count_fields(self) -> int:
        """Counts the fields and input fields declared so far."""
        total = len(self.query) + len(self.mutation) + len(self.page_info.fields)
        for domain in self.domains:
            for t in domain.objects + domain.interfaces:
                total += len(t.fields)
            for conn, edge in domain.connections:
                total += len(conn.fields) + len(edge.fields)
            for t in domain.inputs:
                total += len(t.input_fields)
        return total

    def _fill_fields(self) -> None:
        """Spends the rest of the field budget on plain objects and inputs."""
        budget = self.row.fields - self._count_fields()
        if budget < 0:
            raise ValueError(f"the shape of {self.name} leaves {budget} fields")
        objects = [o for d in self.domains for o in d.objects]
        inputs = [t for d in self.domains for t in d.inputs]
        draw = self.stream("fill")
        for _ in range(budget):
            for _ in range(MAX_ATTEMPTS):
                if inputs and self.chance(draw, P_INPUT_FIELD):
                    if self._add_input_field(draw, inputs[draw.randrange(len(inputs))]):
                        break
                elif self._add_object_field(draw, objects[draw.randrange(len(objects))]):
                    break
            else:
                raise ValueError(f"no free field name is left in {self.name}")

    def _add_object_field(self, draw, obj: TypeDef) -> bool:
        """Adds one own field to a plain object, or answers that none fits."""
        domain = self.domain_of[obj.name]
        taken = {f.name for f in obj.fields}
        if self.chance(draw, P_COMPOSITE):
            target, foreign = self._target(draw, domain)
            local = target.name if foreign else target.name[len(domain.name):]
            if target.kind == "INTERFACE":
                local += "Item"
            roll = draw.randrange(10)
            expr = target.name if roll < 5 else target.name + "!" if roll < 7 else (
                f"[{target.name}!]!" if roll < 9 else f"[{target.name}]")
            name = _lower(_plural(local) if expr.startswith("[") else local)
            if name in taken:
                return False
            f = Field(name, expr)
            if expr.startswith("["):
                self.list_fields.append(f)
        elif domain.enums and self.chance(draw, P_ENUM):
            enum = domain.enums[draw.randrange(len(domain.enums))]
            name = _lower(enum.name[len(domain.name):])
            if name in taken:
                return False
            f = Field(name, enum.name + ("!" if self.chance(draw, P_NON_NULL) else ""))
        else:
            name, scalar = LEAVES[draw.randrange(len(LEAVES))]
            if name in taken:
                return False
            f = Field(name, scalar + ("!" if self.chance(draw, P_NON_NULL) else ""))
        obj.fields.append(f)
        self.own_fields.append((obj, f))
        return True

    def _add_input_field(self, draw, t: TypeDef) -> bool:
        """Adds one field to an input, or answers that none fits."""
        domain = self.domain_of[t.name]
        taken = {v.name for v in t.input_fields}
        others = [i for i in domain.inputs if i is not t]
        if others and self.chance(draw, P_NESTED_INPUT):
            nested = others[draw.randrange(len(others))]
            value = InputValue(_lower(nested.name[len(domain.name):]), nested.name)
        elif domain.enums and self.chance(draw, P_INPUT_ENUM):
            enum = domain.enums[draw.randrange(len(domain.enums))]
            value = InputValue(_lower(enum.name[len(domain.name):]), enum.name)
            if self.chance(draw, P_DEFAULT):
                value.default = enum.values[0].name
        else:
            name, scalar = LEAVES[draw.randrange(len(LEAVES))]
            value = self._input_leaf(draw, name, scalar)
        if value.name in taken:
            return False
        t.input_fields.append(value)
        return True

    def _target(self, draw, domain: _Domain) -> tuple[TypeDef, bool]:
        """Draws the composite type a field points at, and whether it is foreign.

        Most targets stand in the same domain, some in one of the next two and
        a few anywhere, and a hidden object is never a target.
        """
        roll = draw.randrange(CHANCE_STEPS)
        count = len(self.domains)
        if roll < P_SAME_DOMAIN:
            other = domain
        elif roll < P_SAME_DOMAIN + P_NEAR_DOMAIN:
            other = self.domains[(domain.index + 1 + draw.randrange(2)) % count]
        else:
            other = self.domains[draw.randrange(count)]
        pool = [o for o in other.objects if not self.hidden[o.name]] + other.interfaces
        return pool[draw.randrange(len(pool))], other is not domain

    def _arguments(self) -> None:
        """Brings the arguments to their count.

        Every mutation field carries its one argument and every connection
        field its `first`. Then fetch fields take `id`, connection fields
        `after`, then `last` and `before`, list fields `limit` and find fields
        `filter`, each tier in a drawn order, until the count is reached.
        """
        for f in self.connection_fields:
            f.args.append(InputValue("first", "Int"))
        have = sum(len(f.args) for _, _, f in self.mutation) + len(self.connection_fields)
        wanted = self.row.arguments - have
        if wanted < 0:
            raise ValueError(f"the roots of {self.name} carry {have} arguments")
        draw = self.stream("arguments")
        find = []
        for domain, f in self.find_fields:
            filters = [t for t in domain.inputs if t.name.endswith("Filter")]
            arg = InputValue("filter", filters[0].name) if filters else InputValue(
                "limit", "Int", default="10")
            find.append([(f, arg)])
        tiers: list[list[list[tuple[Field, InputValue]]]] = [
            [[(f, InputValue("id", "ID!"))] for f in self.fetch_fields],
            [[(f, InputValue("after", "String"))] for f in self.connection_fields],
            [[(f, InputValue("last", "Int")), (f, InputValue("before", "String"))]
             for f in self.connection_fields],
            [[(f, InputValue("limit", "Int", default="10"))] for f in self.list_fields],
            find,
        ]
        for tier in tiers:
            groups = list(tier)
            draw.shuffle(groups)
            for f, arg in (unit for group in groups for unit in group):
                if wanted == 0:
                    return
                f.args.append(arg)
                wanted -= 1
        if wanted:
            raise ValueError(f"{self.name} lacks {wanted} places for arguments")

    def _text(self) -> None:
        """Writes the descriptions and the deprecations.

        A deprecation stands on an own field of a plain object as the
        `@deprecated` tag that opens its description, the form the reference writes, or
        as the directive, and on an enum value as the directive. The reference writes no
        directive, so the directive form is a choice of the generator that feeds
        the path of the core that reads a deprecation by directive.
        """
        draw = self.stream("text")
        for t in self._ordered():
            words = _words(t.name)
            domain = self.domain_of.get(t.name)
            area = _words(domain.name) if domain else "shared"
            if t.kind != "SCALAR" and self.chance(draw, P_TYPE_DESCRIPTION):
                sentence = TYPE_SENTENCES[draw.randrange(len(TYPE_SENTENCES))]
                t.description = sentence.format(noun=words, area=area)
            members = [(m.name, m) for m in t.fields] + [(m.name, m) for m in t.input_fields]
            members += [(m.name, m) for m in t.values]
            for name, member in members:
                if self.chance(draw, P_MEMBER_DESCRIPTION):
                    sentence = MEMBER_SENTENCES[draw.randrange(len(MEMBER_SENTENCES))]
                    member.description = sentence.format(field=_words(name), noun=words)
                if isinstance(member, EnumValue) and self.chance(draw, P_VALUE_DEPRECATED):
                    member.deprecation = f"{name.capitalize()} is no longer written."
            for f in t.fields:
                for a in f.args:
                    if self.chance(draw, P_MEMBER_DESCRIPTION):
                        sentence = MEMBER_SENTENCES[draw.randrange(len(MEMBER_SENTENCES))]
                        a.description = sentence.format(field=_words(a.name), noun=_words(f.name))
        for obj, f in self.own_fields:
            roll = draw.randrange(CHANCE_STEPS)
            if roll < P_TAG_DEPRECATED:
                f.description = f"@deprecated Use `id` of the {_words(obj.name)} instead."
            elif roll < P_TAG_DEPRECATED + P_DIRECTIVE_DEPRECATED:
                f.deprecation = f"Use id of the {_words(obj.name)} instead."

    def _ordered(self) -> list[TypeDef]:
        """Returns every type in the order a schema of the compatibility layer prints them.

        That order is the roots, the objects, the inputs, the interfaces, the
        twelve scalars and the enums.
        """
        types = [self.query_type, self.mutation_type]
        for domain in self.domains:
            types.extend(domain.objects)
            for conn, edge in domain.connections:
                types.extend((conn, edge))
        types.append(self.page_info)
        types.extend(t for d in self.domains for t in d.inputs)
        types.extend(t for d in self.domains for t in d.interfaces)
        types.extend(TypeDef("SCALAR", name) for name in BRANDED_SCALARS)
        types.extend(t for d in self.domains for t in d.enums)
        return types

    def _draft(self) -> Schema:
        """Returns the schema as it stands, in the order its types print."""
        return Schema(self.name, self._ordered(), query="Query", mutation="Mutation")

    def _assemble(self) -> Schema:
        """Returns the schema, checked against its row and against the walk.

        The walk of aoviewer must reach every composite type, so the builder
        refuses a schema where it does not.
        """
        schema = self._draft()
        measured = counts(schema)
        expected = {
            "types": self.row.types, "objects": self.row.objects, "inputs": self.row.inputs,
            "interfaces": self.row.interfaces, "enums": self.row.enums,
            "scalars": self.row.scalars, "fields": self.row.fields,
            "arguments": self.row.arguments, "connections": self.row.connections,
            "edges": self.row.connections,
        }
        for key, value in expected.items():
            if measured[key] != value:
                raise ValueError(f"{self.name} has {measured[key]} {key}, not {value}")
        if measured["reached"] != measured["composite_types"]:
            aoviewer = walk(schema)
            missing = [t.name for t in schema.types
                       if t.kind in COMPOSITE_KINDS and t.name not in aoviewer]
            raise ValueError(f"the walk of aoviewer misses {len(missing)} composite types "
                             f"of {self.name}, the first {missing[0]}")
        return schema


def _round_robin(per_domain: list[list[tuple[str, Field]]], wanted: int):
    """Takes `wanted` items, one per domain in turn, in the order each lists.

    Each item comes back with its domain, its kind and its place in the list
    of its domain.
    """
    taken = []
    depth = max((len(items) for items in per_domain), default=0)
    for position in range(depth):
        for index, items in enumerate(per_domain):
            if len(taken) == wanted:
                return taken
            if position < len(items):
                kind, f = items[position]
                taken.append((index, kind, position, f))
    if len(taken) < wanted:
        raise ValueError(f"the roots offer {len(taken)} fields, not {wanted}")
    return taken


def parse_arguments(argv: list[str]) -> argparse.Namespace:
    """Reads the command line of the generator."""
    parser = argparse.ArgumentParser(prog="python -m tests.gen.synth_schema", description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--name", action="append", choices=list(SCHEMAS))
    parser.add_argument("--seed", type=int, default=SEED)
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--sdl", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    """Builds each schema asked for, writes it when asked and prints its line.

    The line is one JSON object with the name, the golden rule, the size, the
    SHA-256 of the introspection bytes and the counts. With `--sdl` the SDL
    text is written beside the JSON file.
    """
    arguments = parse_arguments(argv)
    for name in arguments.name or list(SCHEMAS):
        schema = build(name, arguments.seed)
        data = dumps(introspection(schema))
        if arguments.out is not None:
            arguments.out.mkdir(parents=True, exist_ok=True)
            (arguments.out / f"{name}.json").write_bytes(data)
            if arguments.sdl:
                (arguments.out / f"{name}.graphql").write_text(to_sdl(schema), encoding="utf-8")
        line = {
            "name": name,
            "seed": arguments.seed,
            "golden": GOLDENS[name],
            "bytes": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
            "counts": counts(schema),
        }
        print(json.dumps(line, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
