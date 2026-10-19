# SPDX-License-Identifier: MIT
"""tests/gen: the selection tree the families build and the printer for it.

A family builds a tree of [`Sel`] nodes and hands it to [`print_document`],
which writes the document text a case carries. One printer keeps the spelling
of every family the same, so a difference between two runs is a difference of
what was enumerated and never of how it was written down.
"""

from __future__ import annotations

from dataclasses import dataclass

INDENT = "  "


@dataclass(frozen=True)
class Sel:
    """One selection, a field, an inline fragment or a fragment spread.

    `args` carries the arguments as printed literals, because a family decides
    the spelling of a value and the printer only places it. `directives` are
    printed texts of the same kind, such as `@skip(if: true)`.
    """

    kind: str
    name: str
    alias: str | None = None
    args: tuple[tuple[str, str], ...] = ()
    directives: tuple[str, ...] = ()
    children: tuple["Sel", ...] = ()


@dataclass(frozen=True)
class Fragment:
    """One named fragment definition printed under the operation."""

    name: str
    type_condition: str
    selections: tuple[Sel, ...]


@dataclass(frozen=True)
class VarDef:
    """One variable definition of an operation, with its printed default."""

    name: str
    type_text: str
    default_text: str | None = None


def field(
    name: str,
    children: tuple[Sel, ...] = (),
    alias: str | None = None,
    args: tuple[tuple[str, str], ...] = (),
    directives: tuple[str, ...] = (),
) -> Sel:
    """Returns a field selection."""
    return Sel(
        kind="field",
        name=name,
        alias=alias,
        args=tuple(args),
        directives=tuple(directives),
        children=tuple(children),
    )


def inline(type_condition: str, children: tuple[Sel, ...], directives: tuple[str, ...] = ()) -> Sel:
    """Returns an inline fragment on `type_condition`."""
    return Sel(
        kind="inline",
        name=type_condition,
        directives=tuple(directives),
        children=tuple(children),
    )


def spread(fragment_name: str, directives: tuple[str, ...] = ()) -> Sel:
    """Returns a fragment spread."""
    return Sel(kind="spread", name=fragment_name, directives=tuple(directives))


def print_document(
    operation: str,
    selections: tuple[Sel, ...],
    name: str | None = None,
    variables: tuple[VarDef, ...] = (),
    directives: tuple[str, ...] = (),
    fragments: tuple[Fragment, ...] = (),
) -> str:
    """Prints one operation and the fragments it uses.

    An anonymous query with no variables and no directives prints as a bare
    selection set, which is the short form a client sends, and every other
    operation prints with its keyword.
    """
    head = _operation_head(operation, name, variables, directives)
    lines = [f"{head}{{"] if head else ["{"]
    for selection in selections:
        lines.extend(_print_selection(selection, 1))
    lines.append("}")
    for fragment in fragments:
        lines.append("")
        lines.append(f"fragment {fragment.name} on {fragment.type_condition} {{")
        for selection in fragment.selections:
            lines.extend(_print_selection(selection, 1))
        lines.append("}")
    return "\n".join(lines) + "\n"


def _operation_head(
    operation: str,
    name: str | None,
    variables: tuple[VarDef, ...],
    directives: tuple[str, ...],
) -> str:
    """Returns the text before the selection set of an operation."""
    if operation == "query" and name is None and not variables and not directives:
        return ""
    head = operation
    if name is not None:
        head += f" {name}"
    if variables:
        head += "(" + ", ".join(_print_variable(one) for one in variables) + ")"
    if directives:
        head += " " + " ".join(directives)
    return head + " "


def _print_variable(definition: VarDef) -> str:
    """Prints one variable definition with its default when it has one."""
    text = f"${definition.name}: {definition.type_text}"
    if definition.default_text is not None:
        text += f" = {definition.default_text}"
    return text


def _print_selection(selection: Sel, level: int) -> list[str]:
    """Prints one selection and its children, indented by `level`."""
    pad = INDENT * level
    head = _selection_head(selection)
    if not selection.children:
        return [f"{pad}{head}"]
    lines = [f"{pad}{head} {{"]
    for child in selection.children:
        lines.extend(_print_selection(child, level + 1))
    lines.append(f"{pad}}}")
    return lines


def _selection_head(selection: Sel) -> str:
    """Returns the text of one selection without its selection set."""
    if selection.kind == "inline":
        head = f"... on {selection.name}"
    elif selection.kind == "spread":
        head = f"...{selection.name}"
    else:
        head = selection.name if selection.alias is None else f"{selection.alias}: {selection.name}"
        if selection.args:
            head += "(" + ", ".join(f"{key}: {text}" for key, text in selection.args) + ")"
    if selection.directives:
        head += " " + " ".join(selection.directives)
    return head
