# SPDX-License-Identifier: MIT
"""tests/gen: the behaviour tables the generated cases run against.

A behaviour table describes a schema and what every resolver does, and both
sides of a comparison interpret the same table, the node sidecar with real
promises and the Rust devtool through the promise type of `aosabi-execute`.
`synthesize` writes the table of one schema, filled to one level below the
document depth bound, and `inject` writes a copy of a table with one field
replaced by the behaviour of an error class.

A root field whose name is one of [`NAMED_BEHAVIOURS`] carries that behaviour
instead of a value, which is how the rich schema of `schemas.py` reaches every
kind of the table grammar.
"""

from __future__ import annotations

import copy

from .schema import Schema

#: The depth a document is generated to, and one level less than the depth a
#: synthesized table is filled to.
DEFAULT_DEPTH = 3

#: The numbers a `Number` field cycles through. The text entries are the ones
#: JSON cannot spell, which the table grammar writes as a `number` spec.
NUMBER_CYCLE = (0, -42, 1.5, 1e21, 1e-7, "-0", 9007199254740992)

#: The dates a `Date` field cycles through, as ISO 8601 texts.
DATE_CYCLE = (
    "2020-01-02T03:04:05.000Z",
    "1999-12-31T23:59:59.999Z",
    "2026-06-15T12:00:00.000Z",
)

INT_CYCLE = (0, 1, -1, 2147483647)
FLOAT_CYCLE = (0.5, -1.5, 2.0)
BOOLEAN_CYCLE = (True, False)

#: The behaviours the rich schema reaches by the name of a root field. Every
#: kind of the table grammar appears here or in a synthesized value.
NAMED_BEHAVIOURS: dict[str, dict] = {
    "undefinedLeaf": {"do": "undefined", "label": "undefinedLeaf"},
    "nanLeaf": {"do": "number", "text": "NaN", "label": "nanLeaf"},
    "infinityLeaf": {"do": "number", "text": "Infinity", "label": "infinityLeaf"},
    "fnLeaf": {
        "do": "fn",
        "label": "fnLeaf",
        "returns": {"do": "value", "value": "fn", "label": "fnLeaf.returns"},
    },
    "argLeaf": {"do": "arg", "name": "text", "index": 0, "label": "argLeaf"},
    "resolvedLeaf": {
        "do": "resolved",
        "label": "resolvedLeaf",
        "with": {"do": "value", "value": "resolved"},
    },
    "rejectedLeaf": {
        "do": "rejected",
        "label": "rejectedLeaf",
        "error": {"kind": "plain", "message": "rejectedLeaf"},
    },
    "afterLeaf": {
        "do": "after",
        "units": 1,
        "label": "afterLeaf",
        "with": {"do": "value", "value": "after"},
    },
    "afterErrorLeaf": {
        "do": "after",
        "units": 2,
        "label": "afterErrorLeaf",
        "error": {"kind": "plain", "message": "afterErrorLeaf"},
    },
    "throwLeaf": {
        "do": "throw",
        "label": "throwLeaf",
        "error": {"kind": "plain", "message": "throwLeaf"},
    },
    "serviceLeaf": {
        "do": "throw",
        "label": "serviceLeaf",
        "error": {
            "kind": "service",
            "message": "serviceLeaf",
            "code": "NOT_FOUND",
            "statusCode": 404,
            "details": {"id": 7},
        },
    },
    "graphqlLeaf": {
        "do": "throw",
        "label": "graphqlLeaf",
        "error": {
            "kind": "graphql",
            "message": "graphqlLeaf",
            "extensions": {"code": "BAD_USER_INPUT"},
        },
    },
    "aggregateLeaf": {
        "do": "throw",
        "label": "aggregateLeaf",
        "error": {
            "kind": "aggregate",
            "message": "aggregateLeaf",
            "errors": [
                {"kind": "plain", "message": "aggregateLeaf first"},
                {"kind": "plain", "message": "aggregateLeaf second"},
            ],
        },
    },
    "errorValueLeaf": {
        "do": "error_value",
        "label": "errorValueLeaf",
        "error": {"kind": "plain", "message": "errorValueLeaf"},
    },
}


class Cycles:
    """Keeps one position per cycle, so a value type walks its texts in turn."""

    def __init__(self) -> None:
        self.positions: dict[str, int] = {}

    def next(self, key: str, values):
        """Returns the next entry of the cycle `key` names."""
        items = tuple(values)
        position = self.positions.get(key, 0)
        self.positions[key] = position + 1
        return items[position % len(items)]


def synthesize(
    schema: Schema,
    seed: int,
    mode: str,
    depth: int = DEFAULT_DEPTH,
    name: str | None = None,
) -> dict:
    """Returns the behaviour table of one schema in one mode.

    In `plain` mode every object type reachable from the roots carries a
    resolver per field, so the depth of a document does not matter. In `pylon`
    mode only the root types carry resolvers, each root field is a function,
    and the value it returns is a tree one level deeper than `depth`, which is
    the deepest a generated document reaches.

    `seed` is recorded in the table name, because a value cycle is the same for
    one schema and one mode and the seed says which run wrote the table.
    """
    cycles = Cycles()
    if mode == "pylon":
        resolvers = _compat_resolvers(schema, cycles, depth)
    else:
        resolvers = _plain_resolvers(schema, cycles)
    return {
        "version": 1,
        "name": name or f"{mode}-base",
        "mode": mode,
        "sdl": schema.text,
        "resolvers": resolvers,
    }


def _plain_resolvers(schema: Schema, cycles: Cycles) -> dict:
    """Returns one resolver per field of every object type reachable from the roots."""
    roots = set(schema.root_types())
    resolvers: dict[str, dict] = {}
    for type_name in schema.reachable_types():
        if schema.kind(type_name) != "object":
            continue
        fields: dict[str, dict] = {}
        for field in schema.fields(type_name).values():
            behaviour = NAMED_BEHAVIOURS.get(field.name) if type_name in roots else None
            if behaviour is not None:
                fields[field.name] = copy.deepcopy(behaviour)
                continue
            fields[field.name] = flat_spec(
                schema, cycles, field.type, f"{type_name}.{field.name}"
            )
        if fields:
            resolvers[type_name] = fields
    return resolvers


def _compat_resolvers(schema: Schema, cycles: Cycles, depth: int) -> dict:
    """Returns one function per root field, each returning the tree of its type.

    Only the query and the mutation root carry resolvers.
    """
    resolvers: dict[str, dict] = {}
    for operation in ("query", "mutation"):
        root = schema.root(operation)
        if root is None:
            continue
        fields: dict[str, dict] = {}
        for field in schema.fields(root).values():
            behaviour = NAMED_BEHAVIOURS.get(field.name)
            if behaviour is not None:
                returns = copy.deepcopy(behaviour)
            else:
                returns = tree_spec(
                    schema, cycles, field.type, f"{root}.{field.name}", depth
                )
            fields[field.name] = {"do": "fn", "returns": returns}
        resolvers[root] = fields
    return resolvers


def flat_spec(schema: Schema, cycles: Cycles, type_ref, path: str) -> dict:
    """Returns the value of a field in `plain` mode, one level deep.

    A composite field answers with an empty object, because the fields of that
    object have resolvers of their own, and an abstract field answers with the
    discriminator of one of its possible types.
    """
    inner = type_ref.nullable()
    if inner.is_list:
        items = [
            flat_spec(schema, cycles, inner.of_type, path),
            flat_spec(schema, cycles, inner.of_type, path),
        ]
        return {"do": "list", "items": items}
    name = inner.named_name()
    if schema.is_leaf(name):
        return leaf_spec(schema, cycles, name, path)
    if schema.is_abstract(name):
        target = cycles.next(f"abstract:{name}", schema.possible_types(name) or (name,))
        return {"do": "object", "typename": target, "members": {}}
    return {"do": "value", "value": {}}


def tree_spec(schema: Schema, cycles: Cycles, type_ref, path: str, level: int) -> dict:
    """Returns the value of a field in `pylon` mode, down to `level` more levels."""
    inner = type_ref.nullable()
    if inner.is_list:
        if level <= 0:
            return {"do": "list", "items": []}
        items = [
            tree_spec(schema, cycles, inner.of_type, path, level - 1),
            tree_spec(schema, cycles, inner.of_type, path, level - 1),
        ]
        return {"do": "list", "items": items}
    name = inner.named_name()
    if schema.is_leaf(name):
        return leaf_spec(schema, cycles, name, path)
    if level <= 0 and not type_ref.is_non_null:
        return {"do": "value", "value": None}
    return _object_spec(schema, cycles, name, path, level)


def _object_spec(schema: Schema, cycles: Cycles, name: str, path: str, level: int) -> dict:
    """Returns the object a composite position holds, with the members it needs.

    At the depth bound only the leaf members are written, so a self reference
    ends there rather than growing without an end.
    """
    abstract = schema.is_abstract(name)
    target = name
    if abstract:
        target = cycles.next(f"abstract:{name}", schema.possible_types(name) or (name,))
    members: dict[str, dict] = {}
    for field in schema.fields(target).values():
        child = field.type.named_name()
        if level <= 0 and not schema.is_leaf(child):
            continue
        members[field.name] = tree_spec(
            schema, cycles, field.type, f"{path}.{field.name}", level - 1
        )
    if abstract:
        return {"do": "object", "typename": target, "members": members}
    return {"do": "object", "members": members}


def leaf_spec(schema: Schema, cycles: Cycles, name: str, path: str) -> dict:
    """Returns the value of a leaf position, by the type the schema gives it."""
    if schema.kind(name) == "enum":
        values = schema.type(name).enum_values or ("UNKNOWN",)
        return {"do": "value", "value": cycles.next(f"enum:{name}", values)}
    return scalar_spec(cycles, name, path)


def scalar_spec(cycles: Cycles, name: str, path: str) -> dict:
    """Returns the value of a scalar position, by the name of the scalar."""
    if name == "String":
        return {"do": "value", "value": f"s:{path}"}
    if name == "ID":
        return {"do": "value", "value": f"id:{path}"}
    if name == "Int":
        return {"do": "value", "value": cycles.next("Int", INT_CYCLE)}
    if name == "Float":
        return {"do": "value", "value": cycles.next("Float", FLOAT_CYCLE)}
    if name == "Boolean":
        return {"do": "value", "value": cycles.next("Boolean", BOOLEAN_CYCLE)}
    if name == "Number":
        return _number_spec(cycles.next("Number", NUMBER_CYCLE))
    if name == "Date":
        return {"do": "date", "iso": cycles.next("Date", DATE_CYCLE)}
    if name in ("JSON", "JSONObject"):
        return {"do": "value", "value": {"key": f"j:{path}", "count": 1}}
    if name == "Void":
        return {"do": "value", "value": None}
    if name == "Any":
        return {"do": "value", "value": f"any:{path}"}
    if name == "File":
        return {"do": "value", "value": f"file:{path}"}
    return {"do": "value", "value": f"x:{path}"}


def _number_spec(value) -> dict:
    """Returns a number value, as a `number` spec for the texts JSON cannot spell."""
    if isinstance(value, str):
        return {"do": "number", "text": value}
    return {"do": "value", "value": value}


def inject(table: dict, error_class, target) -> dict:
    """Returns a copy of `table` with one field replaced by an error behaviour.

    `error_class` carries the key of the class and the behaviour it writes, and
    `target` names the path from a root field to the field that carries it. In
    `plain` mode the behaviour becomes the resolver of that field, and in
    `pylon` mode it becomes the member of the object the root function returns,
    wrapped in a function when it throws, so the throw happens where the field
    is read and not while the object is built.
    """
    copied = copy.deepcopy(table)
    key = error_class.key.replace(".", "-")
    copied["name"] = f"{table['name']}-{key}-{target.slug}"
    behaviour = copy.deepcopy(error_class.behaviour)
    if table["mode"] == "pylon":
        _place_member(copied["resolvers"], target, behaviour)
        return copied
    fields = copied["resolvers"].setdefault(target.type_name, {})
    fields[target.field_name] = behaviour
    return copied


def _place_member(resolvers: dict, target, behaviour: dict) -> None:
    """Writes a behaviour into the tree one root function returns.

    A path that crosses a list reaches every item of it, so the field carries
    the behaviour wherever the document reads it.
    """
    names = target.names
    root = resolvers[target.root_type][names[0]]
    if len(names) == 1:
        root["returns"] = behaviour
        return
    nodes = _objects(root["returns"])
    for name in names[1:-1]:
        following: list[dict] = []
        for node in nodes:
            following.extend(_objects(node["members"][name]))
        nodes = following
    if not nodes:
        raise KeyError(names[-1])
    for node in nodes:
        node["members"][names[-1]] = as_member(copy.deepcopy(behaviour))


def _objects(node: dict) -> list[dict]:
    """Returns the object specs a node holds, through any list nesting.

    A node that is no object and no list carries no member, which the caller
    reads as a target the tree does not reach.
    """
    if node.get("do") == "list":
        found: list[dict] = []
        for item in node["items"]:
            found.extend(_objects(item))
        return found
    if "members" not in node:
        return []
    return [node]


def as_member(behaviour: dict) -> dict:
    """Wraps a behaviour that throws or reads an argument in a function.

    A member that throws while the object around it is built would move the
    error to the parent position, and a function member throws where the field
    is read, which is the position the class names.
    """
    if behaviour.get("do") in ("throw", "arg"):
        return {"do": "fn", "returns": behaviour}
    return behaviour
