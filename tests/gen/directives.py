# SPDX-License-Identifier: MIT
"""tests/gen: family D, the directives a document carries.

The family sends `@skip` and `@include` on a root field and on a leaf below
it, with a true condition, a false condition and a variable condition in both
values, and it sends every form of every directive the schema declares on an
operation, which is `@inContext` in a schema of the compatibility layer, in the literal and in the
variable spelling. A form is one subset of the arguments the directive
declares, so a directive with two nullable arguments has four forms.
"""

from __future__ import annotations

from dataclasses import replace
from itertools import combinations

from .arguments import base_value, required_arguments
from .build import path_selection
from .cases import Case, make_case
from .document import Sel, VarDef, print_document
from .schema import Schema, Step
from .sdl import DirectiveDef, json_value, print_value

#: The operation name the family writes, so that a run reads the document by
#: name as a client does.
OPERATION_NAME = "Gen"

#: The name of the boolean variable the conditional spelling declares.
FLAG = "flag"

#: The locations that carry an operation directive, by operation kind.
OPERATION_LOCATIONS = {"query": "QUERY", "mutation": "MUTATION", "subscription": "SUBSCRIPTION"}


def directive_cases(schema: Schema, rng, table: str, depth: int) -> list[Case]:
    """Returns family D over the first root field of the query type."""
    path = _base_path(schema)
    if path is None:
        return []
    cases = _conditional_cases(schema, table, path)
    for directive in schema.document.directives.values():
        cases.extend(_operation_cases(schema, table, path, directive))
    return cases


def _base_path(schema: Schema) -> list[Step] | None:
    """Returns the path the family writes its directives on."""
    for path in schema.field_paths(1, operations=("query",)):
        return path
    return None


def _conditional_cases(schema: Schema, table: str, path: list[Step]) -> list[Case]:
    """Returns `@skip` and `@include` on the root field and on a leaf below it."""
    cases: list[Case] = []
    for name in ("skip", "include"):
        for condition in ("true", "false"):
            cases.append(
                _conditional_case(schema, table, path, name, condition, "root", None)
            )
            cases.append(
                _conditional_case(schema, table, path, name, condition, "leaf", None)
            )
        for value in (True, False):
            cases.append(
                _conditional_case(schema, table, path, name, f"${FLAG}", "root", value)
            )
            cases.append(
                _conditional_case(schema, table, path, name, f"${FLAG}", "leaf", value)
            )
    return cases


def _conditional_case(
    schema: Schema,
    table: str,
    path: list[Step],
    name: str,
    condition: str,
    position: str,
    flag: bool | None,
) -> Case:
    """Builds one conditional directive case on the root field or on a leaf."""
    text = f"@{name}(if: {condition})"
    fill = required_arguments(schema)
    selections = path_selection(schema, path, fill)
    if position == "root":
        selections = tuple(
            _with_directives(selection, (text,)) for selection in selections
        )
    else:
        selections = tuple(_leaf_directive(selection, text) for selection in selections)
    definitions = () if flag is None else (VarDef(name=FLAG, type_text="Boolean!"),)
    variables = None if flag is None else {FLAG: flag}
    document = print_document(
        "query", selections, name=OPERATION_NAME, variables=definitions
    )
    return make_case("D", table, document, variables, operation_name=OPERATION_NAME)


def _with_directives(selection: Sel, directives: tuple[str, ...]) -> Sel:
    """Returns the selection with the directives appended to the ones it has."""
    return replace(selection, directives=selection.directives + directives)


def _leaf_directive(selection: Sel, text: str) -> Sel:
    """Returns the selection with the directive placed on its first leaf."""
    if not selection.children:
        return _with_directives(selection, (text,))
    first = _leaf_directive(selection.children[0], text)
    return replace(selection, children=(first,) + selection.children[1:])


def _operation_cases(
    schema: Schema, table: str, path: list[Step], directive: DirectiveDef
) -> list[Case]:
    """Returns every form of one operation directive, in both spellings."""
    operation = "query"
    if OPERATION_LOCATIONS[operation] not in directive.locations:
        return []
    fill = required_arguments(schema)
    selections = path_selection(schema, path, fill)
    cases: list[Case] = []
    for form in _forms(directive):
        for spelling in ("literal", "variable"):
            text, definitions, variables = _spell(schema, directive, form, spelling)
            document = print_document(
                operation,
                selections,
                name=OPERATION_NAME,
                variables=definitions,
                directives=(text,),
            )
            cases.append(
                make_case("D", table, document, variables, operation_name=OPERATION_NAME)
            )
    return cases


def _forms(directive: DirectiveDef) -> list[tuple[int, ...]]:
    """Returns the argument subsets of a directive, the empty one first."""
    count = len(directive.args)
    forms: list[tuple[int, ...]] = []
    for size in range(count + 1):
        forms.extend(combinations(range(count), size))
    return forms


def _spell(schema: Schema, directive: DirectiveDef, form: tuple[int, ...], spelling: str):
    """Returns the directive text, the variable definitions and the values."""
    if not form:
        return f"@{directive.name}", (), None
    parts = []
    definitions = []
    variables: dict[str, object] = {}
    for index in form:
        argument = directive.args[index]
        path = f"@{directive.name}.{argument.name}"
        value = base_value(schema, argument.type, path)
        if spelling == "literal":
            parts.append(f"{argument.name}: {print_value(value)}")
            continue
        name = f"d{index}"
        parts.append(f"{argument.name}: ${name}")
        definitions.append(VarDef(name=name, type_text=argument.type.text()))
        variables[name] = json_value(value)
    text = f"@{directive.name}(" + ", ".join(parts) + ")"
    return text, tuple(definitions), (variables or None)
