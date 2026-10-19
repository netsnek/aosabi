# SPDX-License-Identifier: MIT
"""tests/gen: the typed view of a schema the families enumerate over.

`load` reads a schema text with the reader of `sdl.py` and wraps it in a
[`Schema`], which answers the questions the families ask: what the root types
are, which fields a type carries, which of them are leaves, which types an
abstract position can hold, and which fields are reachable from the roots at
which depth. Nothing here decides whether a document is valid. The verdict of a
run is the byte comparison of the two executors.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass

from .sdl import Field, SchemaDocument, TypeDef, parse_schema

#: The scalars the specification builds into every schema, which an SDL text
#: may declare again and which it need not declare at all.
BUILTIN_SCALARS = ("String", "Int", "Float", "Boolean", "ID")

#: The introspection fields a selection set carries beside the declared fields.
TYPENAME = "__typename"

#: The number of reachable fields up to which the tree enumeration is
#: exhaustive. A schema above the bound is enumerated as a covering array
#: instead, so a large schema stays finite without losing a field or a pair of
#: siblings.
EXHAUSTIVE_FIELD_BOUND = 64


@dataclass(frozen=True)
class Step:
    """One step of a path through the schema, from a parent type into a field.

    `condition` names the type an inline fragment has to carry before the field
    is selectable, which is the case when the parent position is an interface
    or a union and the field belongs to one of its possible types.
    """

    parent: str
    field: Field
    condition: str | None = None


class Schema:
    """One schema, read from its text, with the tables the families need."""

    def __init__(self, text: str) -> None:
        self.text = text
        self.document: SchemaDocument = parse_schema(text)
        self.digest = hashlib.sha256(text.encode()).hexdigest()

    def root(self, operation: str) -> str | None:
        """Returns the type name of a root operation, or `None` when there is none."""
        return self.document.roots.get(operation)

    def type(self, name: str) -> TypeDef | None:
        """Returns the definition of a named type, or `None` for a built in scalar."""
        return self.document.types.get(name)

    def kind(self, name: str) -> str:
        """Returns the kind of a named type, which is `scalar` for a built in one."""
        definition = self.type(name)
        if definition is not None:
            return definition.kind
        if name in BUILTIN_SCALARS:
            return "scalar"
        raise KeyError(f"the schema declares no type {name}")

    def is_leaf(self, name: str) -> bool:
        """Says whether a named type ends a selection path."""
        return self.kind(name) in ("scalar", "enum")

    def is_abstract(self, name: str) -> bool:
        """Says whether a named type needs a runtime type to complete."""
        return self.kind(name) in ("interface", "union")

    def is_composite(self, name: str) -> bool:
        """Says whether a named type carries a selection set."""
        return self.kind(name) in ("object", "interface", "union")

    def fields(self, name: str) -> dict[str, Field]:
        """Returns the declared fields of an object or interface type."""
        definition = self.type(name)
        if definition is None:
            return {}
        return definition.fields

    def possible_types(self, name: str) -> tuple[str, ...]:
        """Returns the object types an abstract position can hold.

        An interface answers with the object types that implement it in
        declaration order, and a union with the members it names.
        """
        definition = self.type(name)
        if definition is None:
            return ()
        if definition.kind == "union":
            return definition.members
        if definition.kind == "interface":
            return tuple(
                other.name
                for other in self.document.types.values()
                if other.kind == "object" and name in other.interfaces
            )
        return (name,)

    def selectable(self, name: str) -> list[Step]:
        """Returns every field a selection set on `name` can carry.

        A field of the type itself carries no condition. On an interface or a
        union the fields of a possible type follow, each with that type as its
        condition, so a path into them is spelled as an inline fragment.
        """
        steps = [Step(parent=name, field=field) for field in self.fields(name).values()]
        if not self.is_abstract(name):
            return steps
        seen = {step.field.name for step in steps}
        for possible in self.possible_types(name):
            for field in self.fields(possible).values():
                if field.name in seen:
                    continue
                steps.append(Step(parent=possible, field=field, condition=possible))
        return steps

    def leaf_steps(self, name: str) -> list[Step]:
        """Returns the selectable fields of `name` whose type is a leaf."""
        return [step for step in self.selectable(name) if self.is_leaf(step.field.type.named_name())]

    def reachable_types(self) -> list[str]:
        """Returns the composite types reachable from the roots, in walk order."""
        found: dict[str, None] = {}
        queue = [name for name in self.root_types()]
        while queue:
            name = queue.pop(0)
            if name in found:
                continue
            found[name] = None
            for possible in self.possible_types(name) if self.is_abstract(name) else ():
                queue.append(possible)
            for step in self.selectable(name):
                target = step.field.type.named_name()
                if self.is_composite(target) and target not in found:
                    queue.append(target)
        return list(found)

    def root_types(self) -> list[str]:
        """Returns the root operation types the schema declares, query first."""
        names = []
        for operation in ("query", "mutation", "subscription"):
            name = self.root(operation)
            if name is not None:
                names.append(name)
        return names

    def reachable_fields(self) -> list[Step]:
        """Returns every field reachable from the roots, in walk order.

        A field of an object type that also implements an interface is
        selectable in both positions and counts once, because the bound of
        `docs/harness.md` section 4.2 is a bound on the fields of the schema
        and not on the positions a document can spell them in.
        """
        seen: set[tuple[str, str]] = set()
        steps: list[Step] = []
        for name in self.reachable_types():
            for step in self.selectable(name):
                key = (step.parent, step.field.name)
                if key in seen:
                    continue
                seen.add(key)
                steps.append(step)
        return steps

    def is_exhaustive(self, bound: int = EXHAUSTIVE_FIELD_BOUND) -> bool:
        """Says whether the tree enumeration over this schema is exhaustive.

        At or below `bound` reachable fields every composite level carries
        every field it declares. Above it the enumeration draws a fan out per
        level with the seed, which is the covering array of `docs/harness.md`
        section 4.2.
        """
        return len(self.reachable_fields()) <= bound

    def field_paths(self, depth: int, operations=("query", "mutation")) -> list[list[Step]]:
        """Returns one path per field and depth it is reachable at.

        A path starts at a root field and is at most `depth` steps long. The
        first path a breadth first walk finds for a field at a depth is the one
        that is kept, so every field appears at every depth it is reachable at
        and the number of paths stays linear in the size of the schema.
        """
        paths: list[list[Step]] = []
        layer: list[list[Step]] = []
        for operation in operations:
            root = self.root(operation)
            if root is None:
                continue
            for step in self.selectable(root):
                layer.append([step])
        paths.extend(layer)
        seen = {self._path_key(path) for path in layer}
        for _ in range(depth - 1):
            following: list[list[Step]] = []
            for path in layer:
                target = path[-1].field.type.named_name()
                if not self.is_composite(target):
                    continue
                for step in self.selectable(target):
                    candidate = path + [step]
                    key = self._path_key(candidate)
                    if key in seen:
                        continue
                    seen.add(key)
                    following.append(candidate)
            paths.extend(following)
            layer = following
        return paths

    @staticmethod
    def _path_key(path: list[Step]) -> tuple:
        """Returns the identity of a path, which is its depth and its last step."""
        last = path[-1]
        return (len(path), last.parent, last.field.name, last.condition)

    def operation_of(self, path: list[Step]) -> str:
        """Returns the operation kind a path starts in."""
        root = path[0].parent
        for operation in ("query", "mutation", "subscription"):
            if self.root(operation) == root:
                return operation
        return "query"


def load(text: str) -> Schema:
    """Reads a schema text and returns the typed view of it."""
    return Schema(text)
