# SPDX-License-Identifier: MIT
"""tests/gen: family V, every literal lifted into a variable and six classes.

The base of the family is the argument document of family A with every literal
of the target field moved into a variable, so that coercion runs over the
variable path of the executor rather than over the literal path. The six
classes of `docs/harness.md` section 4.1 follow: a missing variable, null for a
non null variable, a wrong type, an unused variable, an undeclared variable and
a default on the definition. The two classes the specified rules refuse travel
twice, once with validation and once without it.
"""

from __future__ import annotations

from .arguments import base_value, required_arguments
from .build import path_selection
from .cases import Case, make_case
from .document import VarDef, print_document
from .schema import Schema, Step
from .sdl import json_value, print_value

#: The operation name every case of this family carries, so that the run also
#: reaches the lookup by name of the entry point.
OPERATION_NAME = "Gen"

UNUSED_VARIABLE = "unusedVar"
UNDECLARED_VARIABLE = "undeclaredVar"


def variable_cases(schema: Schema, rng, table: str, depth: int) -> list[Case]:
    """Returns family V over every reachable field that takes an argument."""
    cases: list[Case] = []
    for path in _argument_paths(schema, depth):
        cases.extend(_field_cases(schema, table, path))
    return cases


def _argument_paths(schema: Schema, depth: int) -> list[list[Step]]:
    """Returns one path per reachable field that declares an argument."""
    seen: set[tuple[str, str]] = set()
    paths: list[list[Step]] = []
    for path in schema.field_paths(depth):
        target = path[-1]
        if not target.field.args:
            continue
        key = (target.parent, target.field.name)
        if key in seen:
            continue
        seen.add(key)
        paths.append(path)
    return paths


def _field_cases(schema: Schema, table: str, path: list[Step]) -> list[Case]:
    """Returns the lifted base and the six classes for one target field."""
    target = path[-1]
    lifted = _lift(schema, target)
    cases = [_case(schema, table, path, lifted, "V0")]
    cases.extend(_missing_case(schema, table, path, lifted))
    cases.extend(_null_case(schema, table, path, lifted))
    cases.extend(_wrong_type_case(schema, table, path, lifted))
    cases.extend(_unused_case(schema, table, path, lifted))
    cases.extend(_undeclared_case(schema, table, path, lifted))
    cases.extend(_default_case(schema, table, path, lifted))
    return cases


class _Lifted:
    """The variables of one target field, their types and their values."""

    def __init__(self) -> None:
        self.names: list[str] = []
        self.types: list[str] = []
        self.non_null: list[bool] = []
        self.values: dict[str, object] = {}
        self.arguments: list[str] = []


def _lift(schema: Schema, target: Step) -> _Lifted:
    """Moves every argument of the target field into a variable."""
    lifted = _Lifted()
    for index, argument in enumerate(target.field.args):
        name = f"v{index}"
        path = f"{target.parent}.{target.field.name}.{argument.name}"
        lifted.names.append(name)
        lifted.types.append(argument.type.text())
        lifted.non_null.append(argument.type.is_non_null)
        lifted.values[name] = json_value(base_value(schema, argument.type, path))
        lifted.arguments.append(argument.name)
    return lifted


def _document(
    schema: Schema,
    path: list[Step],
    definitions: tuple[VarDef, ...],
    bindings: dict[str, str],
) -> str:
    """Prints the document of the family with the bindings the class asks for."""
    target = path[-1]
    fill = required_arguments(schema)

    def args_for(step: Step):
        if step is not target:
            return fill(step)
        return tuple((name, bindings[name]) for name in bindings)

    selections = path_selection(schema, path, args_for)
    return print_document(
        schema.operation_of(path),
        selections,
        name=OPERATION_NAME,
        variables=definitions,
    )


def _definitions(lifted: _Lifted, extra: tuple[VarDef, ...] = ()) -> tuple[VarDef, ...]:
    """Returns the variable definitions of the lifted base plus the extra ones."""
    base = tuple(
        VarDef(name=name, type_text=type_text)
        for name, type_text in zip(lifted.names, lifted.types)
    )
    return base + extra


def _bindings(lifted: _Lifted, overrides: dict[str, str] | None = None) -> dict[str, str]:
    """Returns the argument to variable bindings, with the overrides applied."""
    bindings = {
        argument: f"${name}" for argument, name in zip(lifted.arguments, lifted.names)
    }
    if overrides:
        bindings.update(overrides)
    return bindings


def _case(
    schema: Schema,
    table: str,
    path: list[Step],
    lifted: _Lifted,
    family: str,
    definitions: tuple[VarDef, ...] | None = None,
    bindings: dict[str, str] | None = None,
    values: dict[str, object] | None = None,
    validate: bool = True,
) -> Case:
    """Builds one case of the family over the lifted base."""
    document = _document(
        schema,
        path,
        _definitions(lifted) if definitions is None else definitions,
        _bindings(lifted) if bindings is None else bindings,
    )
    variables = dict(lifted.values) if values is None else values
    return make_case(
        family, table, document, variables, operation_name=OPERATION_NAME, validate=validate
    )


def _missing_case(schema, table, path, lifted) -> list[Case]:
    """V1, a variable the request declares and does not carry."""
    if not lifted.names:
        return []
    values = {
        name: value for name, value in lifted.values.items() if name != lifted.names[0]
    }
    return [_case(schema, table, path, lifted, "V1", values=values)]


def _null_case(schema, table, path, lifted) -> list[Case]:
    """V2, null for a variable the schema declares non null."""
    for name, is_non_null in zip(lifted.names, lifted.non_null):
        if not is_non_null:
            continue
        values = dict(lifted.values)
        values[name] = None
        return [_case(schema, table, path, lifted, "V2", values=values)]
    return []


def _wrong_type_case(schema, table, path, lifted) -> list[Case]:
    """V3, a variable value of a type the argument does not take."""
    if not lifted.names:
        return []
    name = lifted.names[0]
    values = dict(lifted.values)
    values[name] = _wrong_json(lifted.values[name])
    return [_case(schema, table, path, lifted, "V3", values=values)]


def _unused_case(schema, table, path, lifted) -> list[Case]:
    """V4, a variable the document declares and no field reads."""
    definitions = _definitions(lifted, (VarDef(name=UNUSED_VARIABLE, type_text="Int"),))
    values = dict(lifted.values)
    values[UNUSED_VARIABLE] = 1
    refused = _case(
        schema, table, path, lifted, "V4", definitions=definitions, values=values
    )
    without = _case(
        schema,
        table,
        path,
        lifted,
        "V4",
        definitions=definitions,
        values=values,
        validate=False,
    )
    return [refused, without]


def _undeclared_case(schema, table, path, lifted) -> list[Case]:
    """V5, an argument that reads a variable the document does not declare."""
    if not lifted.names:
        return []
    bindings = _bindings(lifted, {lifted.arguments[0]: f"${UNDECLARED_VARIABLE}"})
    definitions = _definitions(lifted)[1:]
    values = {
        name: value for name, value in lifted.values.items() if name != lifted.names[0]
    }
    refused = _case(
        schema,
        table,
        path,
        lifted,
        "V5",
        definitions=definitions,
        bindings=bindings,
        values=values,
    )
    without = _case(
        schema,
        table,
        path,
        lifted,
        "V5",
        definitions=definitions,
        bindings=bindings,
        values=values,
        validate=False,
    )
    return [refused, without]


def _default_case(schema, table, path, lifted) -> list[Case]:
    """V6, a default on the definition and no value in the request."""
    if not lifted.names:
        return []
    first = lifted.names[0]
    definitions = list(_definitions(lifted))
    definitions[0] = VarDef(
        name=first,
        type_text=lifted.types[0],
        default_text=print_value(_literal(lifted.values[first])),
    )
    values = {name: value for name, value in lifted.values.items() if name != first}
    return [
        _case(
            schema,
            table,
            path,
            lifted,
            "V6",
            definitions=tuple(definitions),
            values=values,
        )
    ]


def _literal(value: object) -> object:
    """Returns the value again, which the printer spells as a literal.

    A variable value is JSON, and the printer of `sdl.py` writes a string, a
    number, a boolean, a list and an object of it. An enum default is written
    as a string here, which the specified coercion of an enum default refuses,
    so an enum argument is the one case this class does not reach.
    """
    return value


def _wrong_json(value: object) -> object:
    """Returns a JSON value of a kind the argument does not take."""
    if isinstance(value, str):
        return 42
    return "wrong"
