# SPDX-License-Identifier: MIT
"""tests/gen: families S1 to S8, the selection shapes over a schema.

S1 sends a root field alone with every leaf of its type. S2 sends one path per
field and depth, so every field appears at every depth it is reachable at. S3
sends the full tree at each depth up to the bound, capped by the fan out. S4
sends every pair of sibling fields once. S5 sends aliases and one field twice
under two aliases. S6 sends `__typename` at every level and once under an
alias. S7 replaces a selection set with a named and with an inline fragment. S8
sends fragments on every interface and union, with and without `__typename`.
"""

from __future__ import annotations

from .arguments import required_arguments
from .build import leaf_children, path_selection, terminal_children
from .cases import Case, make_case
from .document import Fragment, Sel, field, inline, print_document, spread
from .schema import TYPENAME, Schema, Step

#: The name every generated fragment of S7 and S8 carries.
FRAGMENT_NAME = "F"


def selection_cases(
    schema: Schema, rng, table: str, depth: int, fanout: int, exhaustive: bool = True
) -> list[Case]:
    """Returns families S1 to S8 over the query and mutation roots.

    With `exhaustive` every composite level of S3 carries every field it
    declares. Without it the fan out of a level is `fanout` fields drawn with
    the seed, which is the covering array of `docs/harness.md` section 4.2.
    S2 and S4 run in both cases, because that section asks the covering array
    for every field at every depth it is reachable at and for every pair of
    siblings together at least once. The number of pairs of a type grows with
    the square of its fields and not with the depth, so both stay finite, and
    a corpus over the budget is cut by a seeded sample that the manifest
    records rather than by a silent cap.
    """
    cases: list[Case] = []
    cases.extend(root_field_cases(schema, table))
    cases.extend(path_cases(schema, table, depth))
    cases.extend(tree_cases(schema, rng, table, depth, None if exhaustive else fanout))
    cases.extend(pair_cases(schema, rng, table, depth))
    cases.extend(alias_cases(schema, table))
    cases.extend(typename_cases(schema, table, depth))
    cases.extend(fragment_cases(schema, table))
    cases.extend(abstract_cases(schema, table, depth))
    return cases


def root_paths(schema: Schema) -> list[list[Step]]:
    """Returns the one step paths of every root field of query and mutation."""
    return schema.field_paths(1)


def root_field_cases(schema: Schema, table: str) -> list[Case]:
    """S1, a root field alone with every leaf its type carries."""
    fill = required_arguments(schema)
    cases: list[Case] = []
    for path in root_paths(schema):
        target = path[-1].field.type.named_name()
        tail = leaf_children(schema, target, fill) if schema.is_composite(target) else None
        selections = path_selection(schema, path, fill, tail)
        cases.append(make_case("S1", table, print_document(schema.operation_of(path), selections)))
    return cases


def path_cases(schema: Schema, table: str, depth: int) -> list[Case]:
    """S2, one path per field and per depth it is reachable at."""
    fill = required_arguments(schema)
    cases: list[Case] = []
    for path in schema.field_paths(depth):
        selections = path_selection(schema, path, fill)
        cases.append(make_case("S2", table, print_document(schema.operation_of(path), selections)))
    return cases


def tree_cases(schema: Schema, rng, table: str, depth: int, fanout: int | None) -> list[Case]:
    """S3, the full tree under a root field at each depth up to the bound."""
    fill = required_arguments(schema)
    cases: list[Case] = []
    for path in root_paths(schema):
        for level in range(1, depth + 1):
            tail = _tree(schema, rng, path[-1].field.type.named_name(), level - 1, fanout, fill)
            selections = path_selection(schema, path, fill, tail)
            cases.append(
                make_case("S3", table, print_document(schema.operation_of(path), selections))
            )
    return cases


def _tree(
    schema: Schema, rng, type_name: str, level: int, fanout: int | None, fill
) -> tuple[Sel, ...] | None:
    """Builds the selection set of a type down to `level` further levels.

    A `fanout` of `None` takes every selectable field of the level.
    """
    if not schema.is_composite(type_name):
        return None
    if level <= 0:
        return terminal_children(schema, type_name)
    selectable = schema.selectable(type_name)
    if fanout is None:
        steps = selectable
    else:
        steps = rng.pick(f"S3.fanout:{type_name}", selectable, fanout)
    children: list[Sel] = []
    for step in steps:
        target = step.field.type.named_name()
        selection = field(
            step.field.name,
            _tree(schema, rng, target, level - 1, fanout, fill) or (),
            args=fill(step),
        )
        if step.condition is not None:
            selection = inline(step.condition, (selection,))
        children.append(selection)
    if not children:
        return terminal_children(schema, type_name)
    return tuple(children)


def pair_cases(schema: Schema, rng, table: str, depth: int) -> list[Case]:
    """S4, every pair of sibling fields of a type reachable from the roots."""
    fill = required_arguments(schema)
    cases: list[Case] = []
    for type_name, path in _type_paths(schema, depth).items():
        steps = schema.selectable(type_name)
        pairs = [(left, right) for index, left in enumerate(steps) for right in steps[index + 1 :]]
        for left, right in pairs:
            tail = (_selection(schema, left, fill), _selection(schema, right, fill))
            selections = path_selection(schema, path, fill, tail) if path else tail
            operation = schema.operation_of(path) if path else _root_operation(schema, type_name)
            cases.append(make_case("S4", table, print_document(operation, selections)))
    return cases


def _selection(schema: Schema, step: Step, fill) -> Sel:
    """Returns one field selection of a step with the set its type needs."""
    target = step.field.type.named_name()
    children = terminal_children(schema, target) if schema.is_composite(target) else ()
    selection = field(step.field.name, children, args=fill(step))
    if step.condition is not None:
        return inline(step.condition, (selection,))
    return selection


def _type_paths(schema: Schema, depth: int) -> dict[str, list[Step]]:
    """Returns the shortest path to every composite type reachable from the roots."""
    paths: dict[str, list[Step]] = {}
    for name in schema.root_types():
        paths[name] = []
    for path in schema.field_paths(depth):
        target = path[-1].field.type.named_name()
        if schema.is_composite(target) and target not in paths:
            paths[target] = path
    return paths


def _root_operation(schema: Schema, type_name: str) -> str:
    """Returns the operation kind a root type belongs to."""
    for operation in ("query", "mutation", "subscription"):
        if schema.root(operation) == type_name:
            return operation
    return "query"


def alias_cases(schema: Schema, table: str) -> list[Case]:
    """S5, a root field under an alias and the same field under two aliases."""
    fill = required_arguments(schema)
    cases: list[Case] = []
    for path in root_paths(schema):
        step = path[-1]
        single = _selection(schema, step, fill)
        aliased = _alias(single, "one")
        twice = (_alias(single, "one"), _alias(single, "two"))
        operation = schema.operation_of(path)
        cases.append(make_case("S5", table, print_document(operation, (aliased,))))
        cases.append(make_case("S5", table, print_document(operation, twice)))
    return cases


def _alias(selection: Sel, alias: str) -> Sel:
    """Returns the selection under an alias, through an inline fragment as well."""
    if selection.kind == "inline":
        inner = _alias(selection.children[0], alias)
        return inline(selection.name, (inner,))
    return field(
        selection.name,
        selection.children,
        alias=alias,
        args=selection.args,
        directives=selection.directives,
    )


def typename_cases(schema: Schema, table: str, depth: int) -> list[Case]:
    """S6, `__typename` at every level of a chain and once under an alias."""
    fill = required_arguments(schema)
    cases: list[Case] = []
    for path in _chain_paths(schema, depth):
        selections = _typename_chain(schema, path, fill, alias=None)
        operation = schema.operation_of(path)
        cases.append(make_case("S6", table, print_document(operation, selections)))
        aliased = _typename_chain(schema, path, fill, alias="kind")
        cases.append(make_case("S6", table, print_document(operation, aliased)))
    return cases


def _chain_paths(schema: Schema, depth: int) -> list[list[Step]]:
    """Returns one chain per root field, as deep as the bound and the schema allow.

    A chain takes the first composite field of every level, so it reaches the
    depth bound wherever the schema has a path that long.
    """
    chains: list[list[Step]] = []
    for path in root_paths(schema):
        chain = list(path)
        while len(chain) < depth:
            target = chain[-1].field.type.named_name()
            if not schema.is_composite(target):
                break
            following = _first_composite_step(schema, target)
            if following is None:
                break
            chain.append(following)
        chains.append(chain)
    return chains


def _first_composite_step(schema: Schema, type_name: str) -> Step | None:
    """Returns the first selectable field of a type whose own type is composite."""
    for step in schema.selectable(type_name):
        if schema.is_composite(step.field.type.named_name()):
            return step
    return None


def _typename_chain(schema: Schema, path: list[Step], fill, alias: str | None) -> tuple[Sel, ...]:
    """Builds the path with `__typename` beside the field at every level."""
    target = path[-1].field.type.named_name()
    tail = (field(TYPENAME),) + terminal_children(schema, target) if schema.is_composite(target) else None
    selections = path_selection(schema, path, fill, tail)
    return (field(TYPENAME, alias=alias),) + _with_typename(selections)


def _with_typename(selections: tuple[Sel, ...]) -> tuple[Sel, ...]:
    """Returns the selections with `__typename` added under every selection set."""
    out: list[Sel] = []
    for selection in selections:
        if not selection.children:
            out.append(selection)
            continue
        children = _with_typename(selection.children)
        if not any(child.name == TYPENAME for child in children):
            children = (field(TYPENAME),) + children
        out.append(
            field(
                selection.name,
                children,
                alias=selection.alias,
                args=selection.args,
                directives=selection.directives,
            )
            if selection.kind == "field"
            else inline(selection.name, children)
        )
    return tuple(out)


def fragment_cases(schema: Schema, table: str) -> list[Case]:
    """S7, a named fragment and an inline fragment in the place of a selection set."""
    fill = required_arguments(schema)
    cases: list[Case] = []
    for path in root_paths(schema):
        step = path[-1]
        target = step.field.type.named_name()
        if not schema.is_composite(target):
            continue
        leaves = leaf_children(schema, target, fill)
        named = field(step.field.name, (spread(FRAGMENT_NAME),), args=fill(step))
        operation = schema.operation_of(path)
        cases.append(
            make_case(
                "S7",
                table,
                print_document(
                    operation,
                    (named,),
                    fragments=(Fragment(FRAGMENT_NAME, target, leaves),),
                ),
            )
        )
        inlined = field(step.field.name, (inline(target, leaves),), args=fill(step))
        cases.append(make_case("S7", table, print_document(operation, (inlined,))))
    return cases


def abstract_cases(schema: Schema, table: str, depth: int) -> list[Case]:
    """S8, fragments on every possible type of an abstract position."""
    fill = required_arguments(schema)
    cases: list[Case] = []
    for path in _abstract_paths(schema, depth):
        target = path[-1].field.type.named_name()
        fragments = tuple(
            inline(possible, leaf_children(schema, possible, fill))
            for possible in schema.possible_types(target)
        )
        operation = schema.operation_of(path)
        for tail in (fragments, (field(TYPENAME),) + fragments):
            selections = path_selection(schema, path, fill, tail)
            cases.append(make_case("S8", table, print_document(operation, selections)))
    return cases


def _abstract_paths(schema: Schema, depth: int) -> list[list[Step]]:
    """Returns one path per abstract type reachable in a field position."""
    seen: set[str] = set()
    paths: list[list[Step]] = []
    for path in schema.field_paths(depth):
        target = path[-1].field.type.named_name()
        if not schema.is_abstract(target) or target in seen:
            continue
        seen.add(target)
        paths.append(path)
    return paths
