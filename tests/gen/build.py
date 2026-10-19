# SPDX-License-Identifier: MIT
"""tests/gen: the selection builders every family shares.

A family names a path through the schema and this module turns it into the
selection tree the printer takes. It knows which selection set ends a path,
how a field of a possible type is wrapped in an inline fragment, and how the
arguments of a step are filled, which the caller hands in as a function so
that the argument family can deviate from the valid base at one step.
"""

from __future__ import annotations

from .document import Sel, field, inline
from .schema import TYPENAME, Schema, Step


def no_arguments(step: Step) -> tuple[tuple[str, str], ...]:
    """Returns no arguments, which is what a field without required ones takes."""
    return ()


def terminal_children(schema: Schema, type_name: str) -> tuple[Sel, ...]:
    """Returns the smallest selection set a composite type accepts.

    A union takes `__typename`, and an object or interface takes its first leaf
    field that needs no argument, or `__typename` when it has none.
    """
    if schema.kind(type_name) == "union":
        return (field(TYPENAME),)
    for candidate in schema.fields(type_name).values():
        if not schema.is_leaf(candidate.type.named_name()):
            continue
        if any(argument.type.is_non_null for argument in candidate.args):
            continue
        return (field(candidate.name),)
    return (field(TYPENAME),)


def leaf_children(schema: Schema, type_name: str, args_for=no_arguments) -> tuple[Sel, ...]:
    """Returns every leaf field of a composite type, or `__typename` for a union.

    A field that needs an argument takes the arguments `args_for` gives it, so
    the selection set stays one a run can send.
    """
    if schema.kind(type_name) == "union":
        return (field(TYPENAME),)
    children = []
    for candidate in schema.fields(type_name).values():
        if not schema.is_leaf(candidate.type.named_name()):
            continue
        children.append(
            field(candidate.name, args=args_for(Step(parent=type_name, field=candidate)))
        )
    if not children:
        return (field(TYPENAME),)
    return tuple(children)


def path_selection(
    schema: Schema,
    path: list[Step],
    args_for=no_arguments,
    tail: tuple[Sel, ...] | None = None,
) -> tuple[Sel, ...]:
    """Builds the selection tree of one path, from its last step outwards.

    `tail` replaces the selection set of the last step, which the caller uses
    to ask for every leaf of the type the path ends in. A step whose field
    belongs to a possible type of an abstract position is wrapped in an inline
    fragment on that type.
    """
    selections: tuple[Sel, ...] | None = None
    for index in reversed(range(len(path))):
        step = path[index]
        target = step.field.type.named_name()
        if selections is None:
            children = tail if tail is not None else _default_children(schema, target)
        else:
            children = selections
        selection = field(step.field.name, children, args=args_for(step))
        if step.condition is not None:
            selection = inline(step.condition, (selection,))
        selections = (selection,)
    return selections or ()


def _default_children(schema: Schema, type_name: str) -> tuple[Sel, ...]:
    """Returns the selection set a path ends with when the caller names none."""
    if schema.is_composite(type_name):
        return terminal_children(schema, type_name)
    return ()
