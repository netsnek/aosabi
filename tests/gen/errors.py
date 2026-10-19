# SPDX-License-Identifier: MIT
"""tests/gen: the error classes E01 to E16 and the tables that carry them.

Each class names one way a resolver fails, and each case of the family is one
table, which is the base table with one field replaced by the behaviour of the
class, and one document that selects that field. A class whose shape the schema
has no field for is left out, which the counts of the corpus show.
"""

from __future__ import annotations

from dataclasses import dataclass

from .arguments import required_arguments
from .build import path_selection
from .cases import Case, make_case
from .document import print_document
from .schema import Schema, Step
from .tables import inject

#: The integer above the range of the `Int` scalar, which E13 sends into one.
INT_ABOVE_RANGE = 2147483648


@dataclass(frozen=True)
class Target:
    """The path from a root field to the field an error class is written on."""

    path: tuple[Step, ...]

    @property
    def root_type(self) -> str:
        """Returns the root type the path starts in."""
        return self.path[0].parent

    @property
    def type_name(self) -> str:
        """Returns the type that declares the field the class is written on."""
        return self.path[-1].parent

    @property
    def field_name(self) -> str:
        """Returns the name of the field the class is written on."""
        return self.path[-1].field.name

    @property
    def names(self) -> tuple[str, ...]:
        """Returns the field names of the path, from the root field down."""
        return tuple(step.field.name for step in self.path)

    @property
    def slug(self) -> str:
        """Returns the text that names this target in a table name."""
        return "-".join((self.root_type,) + self.names)


@dataclass(frozen=True)
class Injection:
    """One error class placed on one target, ready for [`inject`]."""

    key: str
    family: str
    behaviour: dict
    target: Target


def error_cases(
    schema: Schema, rng, base_table: dict, depth: int
) -> tuple[list[dict], list[Case]]:
    """Returns the tables and the cases of the error family.

    The tables are copies of `base_table`, one per class and target, and each
    case sends the document that selects the field the class sits on.
    """
    fill = required_arguments(schema)
    tables: list[dict] = []
    cases: list[Case] = []
    for injection in injections(schema, rng, base_table["mode"], depth):
        try:
            table = inject(base_table, injection, injection.target)
        except (KeyError, IndexError):
            continue
        selections = path_selection(schema, list(injection.target.path), fill)
        document = print_document(schema.operation_of(list(injection.target.path)), selections)
        tables.append(table)
        cases.append(make_case(injection.family, table["name"], document))
    return tables, cases


def injections(schema: Schema, rng, mode: str, depth: int) -> list[Injection]:
    """Returns one injection per class and variant the schema has a field for."""
    paths = _candidates(schema, mode, depth)
    out: list[Injection] = []
    for key, family, pick, behaviour in _classes(schema):
        target = pick(schema, rng, paths, key)
        if target is None:
            continue
        out.append(
            Injection(key=key, family=family, behaviour=behaviour(schema, target), target=target)
        )
    return out


def _candidates(schema: Schema, mode: str, depth: int) -> list[list[Step]]:
    """Returns the paths a class may be written on, in this mode.

    In `pylon` mode a path through an inline fragment is left out, because the
    tree a root function returns holds one possible type per abstract position
    and a member of another one has no place in it.
    """
    paths = schema.field_paths(depth)
    if mode != "pylon":
        return paths
    return [path for path in paths if all(step.condition is None for step in path)]


def _pick(purpose_suffix: str, test):
    """Returns a chooser that draws one path the test accepts."""

    def choose(schema: Schema, rng, paths, key: str) -> Target | None:
        matching = [path for path in paths if test(schema, path)]
        if not matching:
            return None
        return Target(path=tuple(rng.choice(f"error.target:{key}{purpose_suffix}", matching)))

    return choose


def _nullable(schema: Schema, path) -> bool:
    """Says whether the field of the last step is nullable."""
    return not path[-1].field.type.is_non_null


def _non_null(schema: Schema, path) -> bool:
    """Says whether the field of the last step is non null."""
    return path[-1].field.type.is_non_null


def _leaf(schema: Schema, path) -> bool:
    """Says whether the field of the last step ends in a leaf, with no list around it."""
    field = path[-1].field
    return schema.is_leaf(field.type.named_name()) and not field.type.nullable().is_list


def _list_field(schema: Schema, path) -> bool:
    """Says whether the field of the last step is a list."""
    return path[-1].field.type.nullable().is_list


def _strict_list(schema: Schema, path) -> bool:
    """Says whether the field of the last step is a list of a non null type."""
    field = path[-1].field
    inner = field.type.nullable()
    return inner.is_list and inner.of_type.is_non_null


def _abstract(schema: Schema, path) -> bool:
    """Says whether the field of the last step holds one abstract value."""
    field = path[-1].field
    if field.type.nullable().is_list:
        return False
    return schema.is_abstract(field.type.named_name())


def _scalar(name: str):
    """Returns a test for a field of one named scalar, with no list around it."""

    def test(schema: Schema, path) -> bool:
        field = path[-1].field
        if field.type.nullable().is_list:
            return False
        return field.type.named_name() == name

    return test


def _at_depth(depth: int):
    """Returns a test for a non null field at one depth."""

    def test(schema: Schema, path) -> bool:
        return len(path) == depth and path[-1].field.type.is_non_null

    return test


def _plain(message: str) -> dict:
    """Returns the throw of a plain error."""
    return {"kind": "plain", "message": message}


def _classes(schema: Schema):
    """Returns the classes, each with its key, family, chooser and behaviour."""
    return [
        (
            "E01",
            "E01",
            _pick("", _nullable),
            lambda schema, target: {"do": "throw", "label": "E01", "error": _plain("E01 nullable")},
        ),
        (
            "E02",
            "E02",
            _pick("", _non_null),
            lambda schema, target: {"do": "throw", "label": "E02", "error": _plain("E02 non null")},
        ),
        (
            "E03",
            "E03",
            _pick("", _nullable),
            lambda schema, target: {
                "do": "throw",
                "label": "E03",
                "error": {
                    "kind": "service",
                    "message": "E03 service",
                    "code": "NOT_FOUND",
                    "statusCode": 404,
                    "details": {"id": 7},
                },
            },
        ),
        (
            "E04.string",
            "E04",
            _pick("", _nullable),
            lambda schema, target: {
                "do": "throw",
                "label": "E04.string",
                "error": {"kind": "value", "value": {"do": "value", "value": "E04 string"}},
            },
        ),
        (
            "E04.object",
            "E04",
            _pick("", _nullable),
            lambda schema, target: {
                "do": "throw",
                "label": "E04.object",
                "error": {
                    "kind": "value",
                    "value": {"do": "value", "value": {"message": "E04 object"}},
                },
            },
        ),
        (
            "E05",
            "E05",
            _pick("", _leaf),
            lambda schema, target: {
                "do": "error_value",
                "label": "E05",
                "error": _plain("E05 error value"),
            },
        ),
        (
            "E06",
            "E06",
            _pick("", _nullable),
            lambda schema, target: {
                "do": "rejected",
                "label": "E06",
                "error": _plain("E06 rejected"),
            },
        ),
        (
            "E07",
            "E07",
            _pick("", _nullable),
            lambda schema, target: {
                "do": "after",
                "units": 2,
                "label": "E07",
                "error": _plain("E07 after two units"),
            },
        ),
        (
            "E08.d1",
            "E08",
            _pick("", _at_depth(1)),
            lambda schema, target: {"do": "value", "value": None, "label": "E08.d1"},
        ),
        (
            "E08.d2",
            "E08",
            _pick("", _at_depth(2)),
            lambda schema, target: {"do": "value", "value": None, "label": "E08.d2"},
        ),
        (
            "E08.d3",
            "E08",
            _pick("", _at_depth(3)),
            lambda schema, target: {"do": "value", "value": None, "label": "E08.d3"},
        ),
        (
            "E09",
            "E09",
            _pick("", _non_null),
            lambda schema, target: {"do": "undefined", "label": "E09"},
        ),
        (
            "E10",
            "E10",
            _pick("", _list_field),
            lambda schema, target: {"do": "value", "value": "E10 not a list", "label": "E10"},
        ),
        (
            "E11",
            "E11",
            _pick("", _strict_list),
            lambda schema, target: {
                "do": "list",
                "label": "E11",
                "items": [_item_value(schema, target), {"do": "value", "value": None}],
            },
        ),
        (
            "E12",
            "E12",
            _pick("", _strict_list),
            lambda schema, target: {
                "do": "list",
                "label": "E12",
                "items": [
                    {
                        "do": "after",
                        "units": 1,
                        "label": "E12.item",
                        "error": _plain("E12 item after one unit"),
                    },
                    {"do": "value", "value": None},
                ],
            },
        ),
        (
            "E13.number",
            "E13",
            _pick("", _scalar("Number")),
            lambda schema, target: {
                "do": "value",
                "value": "E13 not a number",
                "label": "E13.number",
            },
        ),
        (
            "E13.date",
            "E13",
            _pick("", _scalar("Date")),
            lambda schema, target: {"do": "date", "iso": None, "label": "E13.date"},
        ),
        (
            "E13.int",
            "E13",
            _pick("", _scalar("Int")),
            lambda schema, target: {
                "do": "value",
                "value": INT_ABOVE_RANGE,
                "label": "E13.int",
            },
        ),
        (
            "E14.missing",
            "E14",
            _pick("", _abstract),
            lambda schema, target: {"do": "value", "value": {}, "label": "E14.missing"},
        ),
        (
            "E14.unknown",
            "E14",
            _pick("", _abstract),
            lambda schema, target: {
                "do": "object",
                "typename": "NoSuchType",
                "members": {},
                "label": "E14.unknown",
            },
        ),
        (
            "E14.nonobject",
            "E14",
            _pick("", _abstract),
            lambda schema, target: {
                "do": "value",
                "value": "E14 not an object",
                "label": "E14.nonobject",
            },
        ),
        (
            "E14.wrong",
            "E14",
            _pick("", _abstract),
            _wrong_type_value,
        ),
        (
            "E15",
            "E15",
            _pick("", _nullable),
            lambda schema, target: {
                "do": "throw",
                "label": "E15",
                "error": {
                    "kind": "aggregate",
                    "message": "E15 aggregate",
                    "errors": [_plain("E15 first"), _plain("E15 second")],
                },
            },
        ),
        (
            "E16.extensions",
            "E16",
            _pick("", _nullable),
            lambda schema, target: {
                "do": "throw",
                "label": "E16.extensions",
                "error": {
                    "kind": "graphql",
                    "message": "E16 graphql",
                    "extensions": {"code": "BAD_USER_INPUT"},
                },
            },
        ),
        (
            "E16.critical",
            "E16",
            _pick("", _nullable),
            lambda schema, target: {
                "do": "throw",
                "label": "E16.critical",
                "error": {
                    "kind": "graphql",
                    "message": "E16 critical",
                    "extensions": {"CRITICAL_ERROR": True},
                },
            },
        ),
    ]


def _item_value(schema: Schema, target: Target) -> dict:
    """Returns one value the item type of the target list takes."""
    from .tables import Cycles, tree_spec

    item = target.path[-1].field.type.nullable().of_type
    return tree_spec(schema, Cycles(), item, f"{target.slug}.item", 0)


def _wrong_type_value(schema: Schema, target: Target) -> dict:
    """Returns an object whose discriminator names a type the position cannot hold."""
    field = target.path[-1].field
    possible = set(schema.possible_types(field.type.named_name()))
    for name, definition in schema.document.types.items():
        if definition.kind == "object" and name not in possible:
            return {"do": "object", "typename": name, "members": {}, "label": "E14.wrong"}
    return {"do": "object", "typename": "NoSuchType", "members": {}, "label": "E14.wrong"}
