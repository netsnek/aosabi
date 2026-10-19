# SPDX-License-Identifier: MIT
"""tests/gen: the generated schemas of the huge introspection answers.

The map of `aosabi-json` builds, looks up and writes every object of an
answer, and an introspection answer is the heaviest work it meets, thousands
of small objects with `name`, `kind`, `fields`, `type` and `ofType`. So the
section The cost of the insertion ordered map of `01_json_writer` measures the
map on the introspection answers of schemas of 100, 1,000 and 5,000 object
types that this module writes, and on one schema whose descriptions are
Japanese.

[`scale_schema`] writes a schema text of `types` object types of ten fields
each, with interfaces, enums, input objects, arguments with and without
default values, lists, non null wrappers, descriptions, deprecated fields,
enum values, arguments and input fields, a schema description and a root type
`Query` of ten fields. The same seed and the same count write the same bytes,
because every draw comes from the stream of [`Rng`] that the count and the
language name. Every name is generic and copied from no application.

The module decides nothing about correctness. The notebook runs graphql-js
16.12.0 over the text for the reference bytes and Aosabi's executor over the
same text, and compares the two answers byte for byte.
"""

from __future__ import annotations

from .rng import Rng
from .sdl import print_string

#: The scalars the specification builds into every schema.
SCALARS = ("String", "Int", "Float", "Boolean", "ID")

#: The wrappers of a named type and how often each is drawn, in percent.
WRAPPERS = (("%s", 30), ("%s!", 30), ("[%s]", 10), ("[%s!]", 10), ("[%s]!", 10), ("[%s!]!", 10))

#: The fields of a type, the interface fields among them.
FIELDS = 10

#: The words of a Japanese description, so the answer carries UTF-8 of three
#: bytes per character beside its ASCII.
JAPANESE_WORDS = ("名前", "住所", "注文", "商品", "価格", "在庫", "顧客", "配送", "日付", "状態", "説明", "番号")


def _wrap(draw, name: str) -> str:
    """Returns `name` inside one of the wrappers of [`WRAPPERS`]."""
    roll = draw.randrange(100)
    for form, weight in WRAPPERS:
        if roll < weight:
            return form % name
        roll -= weight
    return name


def _text(draw, japanese: bool, what: str, index: int) -> str:
    """Returns a description of the element `what` at `index`."""
    if japanese:
        words = "、".join(draw.choice(JAPANESE_WORDS) for _ in range(draw.randint(1, 4)))
        return "%s %d の%s。" % (what, index, words)
    return "The %s %d of the generated schema." % (what, index)


def _described(draw, japanese: bool, what: str, index: int, indent: str = "") -> str:
    """Returns the description line of an element, drawn for half of the elements."""
    if draw.random() < 0.5:
        return ""
    return indent + print_string(_text(draw, japanese, what, index)) + "\n"


def scale_schema(types: int, seed: int, japanese: bool = False) -> str:
    """Returns the schema text of `types` object types drawn from `seed`.

    Beside the object types the text holds one interface for every ten of
    them, one enum and one input object for every twenty, each at least one,
    and the root type `Query`. An object type implements none, one or two of
    the interfaces and carries their fields first, then its own fields up to
    ten. With `japanese` every description is Japanese.
    """
    draw = Rng(seed).stream("introspection scale %d%s" % (types, " japanese" if japanese else ""))
    interfaces = max(1, types // 10)
    enums = max(1, types // 20)
    inputs = max(1, types // 20)
    parts = [print_string(_text(draw, japanese, "schema", types)) + "\nschema {\n  query: Query\n}\n"]

    for enum in range(enums):
        lines = [_described(draw, japanese, "enum", enum) + "enum Kind%d {" % enum]
        for value in range(draw.randint(3, 6)):
            deprecated = ' @deprecated(reason: "Use V0.")' if value and draw.random() < 0.1 else ""
            lines.append(_described(draw, japanese, "value", value, "  ") + "  V%d%s" % (value, deprecated))
        parts.append("\n".join(lines) + "\n}\n")

    def default(base: str, typed: str) -> str:
        """Returns the default value of an argument or input field of `typed`, drawn for some."""
        if typed.startswith("[") or draw.random() < 0.6:
            return ""
        if base == "Int":
            return " = %d" % draw.randrange(100)
        if base == "Float":
            return " = %s" % draw.choice(("1.5", "0.25", "10"))
        if base in ("String", "ID"):
            return " = %s" % print_string(draw.choice(("a", "default", "x y")))
        if base == "Boolean":
            return " = %s" % draw.choice(("true", "false"))
        if base.startswith("Kind"):
            return " = V0"
        return ""

    for index in range(inputs):
        lines = [_described(draw, japanese, "input", index) + "input Filter%d {" % index]
        for field in range(draw.randint(3, 5)):
            base = draw.choice(SCALARS) if draw.random() < 0.7 else "Kind%d" % draw.randrange(enums)
            typed = _wrap(draw, base)
            deprecated = ' @deprecated(reason: "Unused.")' if not typed.endswith("!") and draw.random() < 0.1 else ""
            lines.append(_described(draw, japanese, "input field", field, "  ") + "  in%d: %s%s%s" % (field, typed, default(base, typed), deprecated))
        parts.append("\n".join(lines) + "\n}\n")

    shared = {}
    for shape in range(interfaces):
        shared[shape] = [("id", "ID!"), ("label%d" % shape, _wrap(draw, "String"))]
        lines = [_described(draw, japanese, "interface", shape) + "interface Shape%d {" % shape]
        for position, (name, typed) in enumerate(shared[shape]):
            lines.append(_described(draw, japanese, "field", position, "  ") + "  %s: %s" % (name, typed))
        parts.append("\n".join(lines) + "\n}\n")

    def field_type() -> str:
        """Returns the type of a field: a scalar, an enum, an object type or an interface."""
        roll = draw.random()
        if roll < 0.5:
            base = draw.choice(SCALARS)
        elif roll < 0.65:
            base = "Kind%d" % draw.randrange(enums)
        elif roll < 0.9:
            base = "Type%d" % draw.randrange(types)
        else:
            base = "Shape%d" % draw.randrange(interfaces)
        return _wrap(draw, base)

    def arguments() -> str:
        """Returns the arguments of a field, none for most."""
        if draw.random() < 0.6:
            return ""
        drawn = []
        for position in range(draw.randint(1, 3)):
            roll = draw.random()
            if roll < 0.6:
                base = draw.choice(SCALARS)
            elif roll < 0.8:
                base = "Kind%d" % draw.randrange(enums)
            else:
                base = "Filter%d" % draw.randrange(inputs)
            typed = _wrap(draw, base)
            deprecated = ' @deprecated(reason: "Unused.")' if not typed.endswith("!") and draw.random() < 0.05 else ""
            drawn.append("a%d: %s%s%s" % (position, typed, default(base, typed), deprecated))
        return "(" + ", ".join(drawn) + ")"

    for index in range(types):
        implemented = sorted(draw.sample(range(interfaces), min(interfaces, draw.choice((0, 0, 1, 1, 2)))))
        head = "type Type%d" % index
        if implemented:
            head += " implements " + " & ".join("Shape%d" % shape for shape in implemented)
        lines = [_described(draw, japanese, "type", index) + head + " {"]
        names = []
        for shape in implemented:
            for name, typed in shared[shape]:
                if name not in names:
                    names.append(name)
                    lines.append(_described(draw, japanese, "field", len(names), "  ") + "  %s: %s" % (name, typed))
        own = 0
        while len(names) < FIELDS:
            name = "field%d" % own
            own += 1
            names.append(name)
            deprecated = ' @deprecated(reason: "Use field0.")' if draw.random() < 0.05 else ""
            lines.append(_described(draw, japanese, "field", own, "  ") + "  %s%s: %s%s" % (name, arguments(), field_type(), deprecated))
        parts.append("\n".join(lines) + "\n}\n")

    lines = ["type Query {"]
    for field in range(FIELDS):
        lines.append("  item%d%s: Type%d" % (field, arguments(), draw.randrange(types)))
    parts.append("\n".join(lines) + "\n}\n")
    return "\n".join(parts)
