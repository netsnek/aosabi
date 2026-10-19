# SPDX-License-Identifier: MIT
"""tests/gen: family A, the valid argument base and one deviation at a time.

The base is the value the generator writes for an argument of a given type, and
a deviation changes exactly one argument of one field, so a difference a run
reports names one rule. The deviations are the list of `docs/harness.md`
section 4.1: missing, null, a wrong type, a wrong enum value, an integer out of
range, an empty list, an oversized list, an input object without a required
field and an input object with an unknown field. A deviation the specified
rules refuse travels twice, once with validation and once without it, so the
run time coercion of the executor is reached as well.
"""

from __future__ import annotations

from .build import path_selection
from .cases import Case, make_case
from .document import print_document
from .schema import Schema, Step
from .sdl import EnumValue, InputValue, TypeRef, print_value

#: The scalars of a schema of the compatibility layer that take any literal, so a wrong type is no
#: deviation for them.
PERMISSIVE_SCALARS = ("JSON", "JSONObject", "Any", "File", "Void")

#: The date texts the generated values cycle through.
DATE_TEXTS = (
    "2020-01-02T03:04:05.000Z",
    "1999-12-31T23:59:59.999Z",
    "2026-06-15T12:00:00.000Z",
)

#: The first integer above the range the `Int` scalar takes.
INT_ABOVE_RANGE = 2147483648


def base_value(schema: Schema, type_ref: TypeRef, path: str) -> object:
    """Returns a valid literal for a type reference.

    A list takes one item, an input object takes the fields it requires, and a
    string carries the path it was written for, so a value in a response names
    the position it came from.
    """
    inner = type_ref.nullable()
    if inner.is_list:
        return [base_value(schema, inner.of_type, path)]
    return base_named_value(schema, inner.named_name(), path)


def base_named_value(schema: Schema, name: str, path: str) -> object:
    """Returns a valid literal for a named type."""
    kind = schema.kind(name)
    if kind == "enum":
        values = schema.type(name).enum_values
        return EnumValue(values[0]) if values else EnumValue("UNKNOWN")
    if kind == "input":
        return base_input_object(schema, name, path)
    return base_scalar_value(name, path)


def base_scalar_value(name: str, path: str) -> object:
    """Returns a valid literal for a scalar, by the name the schema gives it."""
    if name == "String":
        return f"s:{path}"
    if name == "ID":
        return f"id:{path}"
    if name == "Int":
        return 1
    if name == "Float":
        return 1.5
    if name == "Boolean":
        return True
    if name == "Number":
        return 1.5
    if name == "Date":
        return DATE_TEXTS[0]
    if name in ("JSON", "JSONObject"):
        return {"key": f"j:{path}"}
    if name == "Void":
        return None
    return f"x:{path}"


def base_input_object(schema: Schema, name: str, path: str) -> dict:
    """Returns an input object literal with the fields the type requires."""
    fields: dict[str, object] = {}
    for input_field in schema.type(name).input_fields.values():
        if not required(input_field):
            continue
        fields[input_field.name] = base_value(
            schema, input_field.type, f"{path}.{input_field.name}"
        )
    return fields


def required(input_field: InputValue) -> bool:
    """Says whether an input value has to be given, which a default lifts."""
    return input_field.type.is_non_null and not input_field.has_default


def base_arguments(schema: Schema, step: Step, only_required: bool = True):
    """Returns the printed arguments of one step of a path.

    With `only_required` the result carries the arguments the field demands,
    which keeps a selection short, and without it every argument of the field,
    which is the base an argument deviation is taken from.
    """
    path = f"{step.parent}.{step.field.name}"
    printed = []
    for argument in step.field.args:
        if only_required and not required(argument):
            continue
        value = base_value(schema, argument.type, f"{path}.{argument.name}")
        printed.append((argument.name, print_value(value)))
    return tuple(printed)


def required_arguments(schema: Schema):
    """Returns a function that fills the required arguments of a step."""

    def fill(step: Step):
        return base_arguments(schema, step, only_required=True)

    return fill


def deviations(schema: Schema, argument: InputValue, base: object) -> list[tuple[str, object, bool]]:
    """Returns the deviations of one argument, each with a refusal marker.

    A tuple is the name of the deviation, the value it writes and whether the
    specified validation rules refuse the document it makes. A value of the
    sentinel [`MISSING`] means that the argument is left out.
    """
    inner = argument.type.nullable()
    name = inner.named_name()
    kind = schema.kind(name)
    out: list[tuple[str, object, bool]] = []
    out.append(("missing", MISSING, required(argument)))
    out.append(("null", None, argument.type.is_non_null))
    if name not in PERMISSIVE_SCALARS:
        out.append(("wrong-type", wrong_literal(base), True))
    if kind == "enum":
        out.append(("wrong-enum", EnumValue("NOT_A_MEMBER"), True))
    if name == "Int" and not inner.is_list:
        out.append(("int-out-of-range", INT_ABOVE_RANGE, True))
    if inner.is_list:
        out.append(("empty-list", [], False))
        item = base[0] if isinstance(base, list) and base else None
        out.append(("oversized-list", [item] * 64, False))
    if kind == "input":
        without, unknown = input_deviations(schema, name, base)
        if without is not None:
            out.append(("input-missing-field", without, True))
        out.append(("input-unknown-field", unknown, True))
    return out


def input_deviations(schema: Schema, name: str, base: object):
    """Returns the input object without a required field and with an unknown one."""
    fields = dict(base) if isinstance(base, dict) else {}
    without = None
    for input_field in schema.type(name).input_fields.values():
        if required(input_field) and input_field.name in fields:
            without = {key: value for key, value in fields.items() if key != input_field.name}
            break
    unknown = dict(fields)
    unknown["unknownField"] = "u"
    return without, unknown


def wrong_literal(base: object) -> object:
    """Returns a literal of a kind the argument does not take."""
    if isinstance(base, str):
        return True
    return "wrong"


class _Missing:
    """The sentinel that stands for an argument the deviation leaves out."""

    def __repr__(self) -> str:
        return "MISSING"


#: The sentinel a deviation writes when the argument is left out entirely.
MISSING = _Missing()


def argument_cases(schema: Schema, rng, table: str, depth: int) -> list[Case]:
    """Returns family A over every reachable field that takes an argument."""
    cases: list[Case] = []
    fill = required_arguments(schema)
    for path in _argument_paths(schema, depth):
        target = path[-1]
        base = {
            argument.name: base_value(
                schema,
                argument.type,
                f"{target.parent}.{target.field.name}.{argument.name}",
            )
            for argument in target.field.args
        }
        for argument in target.field.args:
            for name, value, refused in deviations(schema, argument, base[argument.name]):
                document = _deviation_document(schema, path, fill, base, argument.name, value)
                cases.append(make_case("A", table, document, operation_name=None))
                if refused:
                    cases.append(
                        make_case("A", table, document, operation_name=None, validate=False)
                    )
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


def _deviation_document(schema, path, fill, base, argument_name, value) -> str:
    """Prints the document of one deviation against the valid base."""
    target = path[-1]

    def args_for(step: Step):
        if step is not target:
            return fill(step)
        printed = []
        for argument in step.field.args:
            if argument.name == argument_name:
                if value is MISSING:
                    continue
                printed.append((argument.name, print_value(value)))
                continue
            printed.append((argument.name, print_value(base[argument.name])))
        return tuple(printed)

    selections = path_selection(schema, path, args_for)
    return print_document(schema.operation_of(path), selections)
