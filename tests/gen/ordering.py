# SPDX-License-Identifier: MIT
"""tests/gen: the ordering cases O01 to O12 over a schema of their own.

An ordering case pins the order in which the executor settles fields, which is
the part of the port that a value comparison alone does not reach. Each case
carries a table of its own over [`ORDERING_SDL`] and one or two documents, and
a case that names two document orders sends both, so a difference between the
document order and the settle order is visible in the key order of `data` and
in the order of the `errors` array.

In `pylon` mode a root member is a function, because a root spec outside a
function creates no timer and no promise, which the table grammar states as a
rule of that mode. Everything below the root is the same text in both modes.
"""

from __future__ import annotations

from dataclasses import dataclass

from .cases import Case, make_case
from .schemas import ensure_branded_scalars

ORDERING_SDL = """type Query {
  a: String
  b: String
  c: String
  d: String
  nn: String!
  box: Box
  parent: Parent
  items: [String]
  strictItems: [String!]
  thing: Thing
}

type Box {
  a: String
  b: String
  c: String
  d: String
}

type Parent {
  child: String!
  sibling: String
}

type Widget {
  name: String
}

type Gadget {
  power: String
}

union Thing = Widget | Gadget

type Mutation {
  a: String
  b: String
  c: String
}
"""


@dataclass(frozen=True)
class OrderingCase:
    """One ordering case, its resolvers and the documents it sends."""

    key: str
    resolvers: dict
    documents: tuple[str, ...]


def value(text: str, label: str) -> dict:
    """Returns a value that is there at once."""
    return {"do": "value", "value": text, "label": label}


def throw(message: str, label: str) -> dict:
    """Returns a synchronous throw of a plain error."""
    return {"do": "throw", "label": label, "error": {"kind": "plain", "message": message}}


def rejected(message: str, label: str) -> dict:
    """Returns a promise that is rejected when it is made."""
    return {"do": "rejected", "label": label, "error": {"kind": "plain", "message": message}}


def resolved(text: str, label: str) -> dict:
    """Returns a promise that is fulfilled when it is made."""
    return {"do": "resolved", "label": label, "with": {"do": "value", "value": text}}


def after_value(units: int, text: str, label: str) -> dict:
    """Returns a promise that fulfils when a timer of `units` fires."""
    return {"do": "after", "units": units, "label": label, "with": {"do": "value", "value": text}}


def after_error(units: int, message: str, label: str) -> dict:
    """Returns a promise that rejects when a timer of `units` fires."""
    return {
        "do": "after",
        "units": units,
        "label": label,
        "error": {"kind": "plain", "message": message},
    }


def function(returns: dict, label: str) -> dict:
    """Returns a function that makes `returns` on every call."""
    return {"do": "fn", "label": label, "returns": returns}


def flat(*names: str) -> str:
    """Prints a document that selects the named root fields of a query."""
    lines = ["{"] + [f"  {name}" for name in names] + ["}", ""]
    return "\n".join(lines)


def mutation(*names: str) -> str:
    """Prints a document that selects the named root fields of a mutation."""
    lines = ["mutation {"] + [f"  {name}" for name in names] + ["}", ""]
    return "\n".join(lines)


def nested(parent: str, *names: str) -> str:
    """Prints a document that selects fields under one root field."""
    lines = ["{", f"  {parent} {{"]
    lines.extend(f"    {name}" for name in names)
    lines.extend(["  }", "}", ""])
    return "\n".join(lines)


THING_DOCUMENT = """{
  a
  thing {
    __typename
    ... on Widget {
      name
    }
  }
}
"""


def cases() -> list[OrderingCase]:
    """Returns the twelve ordering cases in their order."""
    return [
        OrderingCase(
            key="O01",
            resolvers={
                "Query": {
                    "a": throw("O01 synchronous", "O01.a"),
                    "b": rejected("O01 asynchronous", "O01.b"),
                }
            },
            documents=(flat("a", "b"), flat("b", "a")),
        ),
        OrderingCase(
            key="O02",
            resolvers={
                "Query": {
                    "a": throw("O02 synchronous", "O02.a"),
                    "b": rejected("O02 one tick", "O02.b"),
                    "c": after_error(0, "O02 timer zero", "O02.c"),
                    "d": after_error(5, "O02 timer five", "O02.d"),
                }
            },
            documents=(flat("a", "b", "c", "d"), flat("d", "c", "b", "a")),
        ),
        OrderingCase(
            key="O03",
            resolvers={
                "Query": {
                    "a": value("O03 synchronous", "O03.a"),
                    "b": resolved("O03 one tick", "O03.b"),
                    "c": after_value(0, "O03 timer zero", "O03.c"),
                    "d": after_value(5, "O03 timer five", "O03.d"),
                }
            },
            documents=(flat("a", "b", "c", "d"), flat("d", "c", "b", "a")),
        ),
        OrderingCase(
            key="O04",
            resolvers={
                "Query": {
                    "box": {
                        "do": "object",
                        "label": "O04.box",
                        "members": {
                            "a": {"do": "value", "value": "O04 a"},
                            "b": after_value(1, "O04 b", "O04.box.b"),
                            "c": {"do": "value", "value": "O04 c"},
                            "d": after_value(0, "O04 d", "O04.box.d"),
                        },
                    }
                }
            },
            documents=(nested("box", "a", "b", "c", "d"), nested("box", "d", "c", "b", "a")),
        ),
        OrderingCase(
            key="O05",
            resolvers={
                "Query": {
                    "a": rejected("O05 root", "O05.a"),
                    "parent": {
                        "do": "object",
                        "label": "O05.parent",
                        "members": {
                            "child": function(
                                throw("O05 nested", "O05.parent.child.throw"),
                                "O05.parent.child",
                            ),
                            "sibling": {"do": "value", "value": "O05 sibling"},
                        },
                    },
                }
            },
            documents=("{\n  a\n  parent {\n    child\n    sibling\n  }\n}\n",),
        ),
        OrderingCase(
            key="O06",
            resolvers={
                "Query": {
                    "strictItems": {
                        "do": "list",
                        "label": "O06.strictItems",
                        "items": [
                            after_error(2, "O06 item two units", "O06.item0"),
                            {"do": "value", "value": None},
                            after_error(1, "O06 item one unit", "O06.item2"),
                        ],
                    }
                }
            },
            documents=(flat("strictItems"),),
        ),
        OrderingCase(
            key="O07",
            resolvers={
                "Query": {
                    "parent": {
                        "do": "object",
                        "label": "O07.parent",
                        "members": {
                            "child": after_error(1, "O07 child one unit", "O07.parent.child"),
                            "sibling": after_error(2, "O07 sibling two units", "O07.parent.sibling"),
                        },
                    }
                }
            },
            documents=(nested("parent", "child", "sibling"),),
        ),
        OrderingCase(
            key="O08",
            resolvers={
                "Query": {
                    "nn": throw("O08 non null", "O08.nn"),
                    "a": after_value(1, "O08 a", "O08.a"),
                    "b": resolved("O08 b", "O08.b"),
                }
            },
            documents=(flat("nn", "a", "b"), flat("a", "b", "nn")),
        ),
        OrderingCase(
            key="O09",
            resolvers={
                "Mutation": {
                    "a": after_value(5, "O09 a", "O09.a"),
                    "b": after_value(1, "O09 b", "O09.b"),
                    "c": value("O09 c", "O09.c"),
                }
            },
            documents=(mutation("a", "b", "c"),),
        ),
        OrderingCase(
            key="O10",
            resolvers={
                "Query": {
                    "a": {
                        "do": "resolved",
                        "label": "O10.a",
                        "with": resolved("O10 inner", "O10.a.inner"),
                    },
                    "b": value("O10 b", "O10.b"),
                    "c": resolved("O10 c", "O10.c"),
                }
            },
            documents=(flat("a", "b", "c"),),
        ),
        OrderingCase(
            key="O11",
            resolvers={
                "Query": {
                    "a": value("O11 a", "O11.a"),
                    "thing": {
                        "do": "after",
                        "units": 1,
                        "label": "O11.thing",
                        "with": {
                            "do": "object",
                            "typename": "Widget",
                            "members": {"name": {"do": "value", "value": "O11 widget"}},
                        },
                    },
                }
            },
            documents=(THING_DOCUMENT,),
        ),
        OrderingCase(
            key="O12",
            resolvers={
                "Query": {
                    "a": after_value(0, "O12 a", "O12.a"),
                    "b": after_value(1, "O12 b", "O12.b"),
                }
            },
            documents=(flat("a", "b"), flat("b", "a")),
        ),
    ]


def ordering_tables(mode: str, prefix: str = "ordering") -> list[dict]:
    """Returns one table per ordering case, in the mode a run drives."""
    sdl = ensure_branded_scalars(ORDERING_SDL) if mode == "pylon" else ORDERING_SDL
    tables = []
    for case in cases():
        tables.append(
            {
                "version": 1,
                "name": f"{prefix}-{mode}-{case.key}",
                "mode": mode,
                "sdl": sdl,
                "resolvers": _resolvers(case.resolvers, mode),
            }
        )
    return tables


def ordering_cases(mode: str, prefix: str = "ordering") -> list[Case]:
    """Returns the cases of the ordering family, in the mode a run drives."""
    out: list[Case] = []
    for case in cases():
        table = f"{prefix}-{mode}-{case.key}"
        for document in case.documents:
            out.append(make_case(case.key, table, document))
    return out


def _resolvers(resolvers: dict, mode: str) -> dict:
    """Returns the resolvers of a case, with the root wrapping the mode asks for.

    An app of the compatibility layer carries at least one query resolver, and the reference refuses
    a root object without one, so a case that names no `Query` root gets the
    one member [`QUERY_PLACEHOLDER`] in `pylon` mode, which no document of the
    case selects.
    """
    if mode != "pylon":
        return resolvers
    wrapped: dict[str, dict] = {}
    for type_name, fields in resolvers.items():
        wrapped[type_name] = {
            name: function(spec, f"{spec.get('label', name)}.root") for name, spec in fields.items()
        }
    if "Query" not in wrapped:
        wrapped = {"Query": dict(QUERY_PLACEHOLDER), **wrapped}
    return wrapped


#: The query root of a pylon mode case that has none, one function under the
#: first field of [`ORDERING_SDL`].
QUERY_PLACEHOLDER = {"a": function(value("", "placeholder.a"), "placeholder.a.root")}
