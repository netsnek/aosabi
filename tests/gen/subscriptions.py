# SPDX-License-Identifier: MIT
"""tests/gen: the family SUB, the subscription cases of `docs/harness.md` section 4.1.

The family runs over every root field of `Subscription` of a schema, in `plain`
and in `pylon` mode, through the operation `subscribe` of
`tests/sidecar/execute.mjs` and of `aosabi-devtool execute`.

| Case | What it sends |
| --- | --- |
| SUB1 | the field with its valid base arguments, pulled until done |
| SUB2 | the same, `pull` 1, the first frame alone |
| SUB3 | the same with `return_after` 1, the client disconnect |
| SUB4 | every argument deviation of family A that validation refuses, with `validate` true and false, the error before the first yield |
| SUB5 | a repeater table injection that stops with a throw after two pushes, the error mid stream |
| SUB6 | a repeater table injection that throws before the first push |
| SUB7 | a `not_iterable` injection, the assertion message |
| SUB8 | the field selected under two aliases, and with `__typename` beside it, each with `validate` true and false |
| SUB9 | a repeater table injection with 1025 pushes and no pull between them, the overflow message on both sides |
| SUB10 | a `pubsub_topic` subscription whose first `next` is called, then a `pubsub_publish` mutation as its `interleave` request, then the first `next` awaited |

The base table of a schema is the table `tables.synthesize` writes, with every
member of `Subscription` replaced by a repeater that pushes three values of the
field's type and stops. In `plain` mode a pushed value is the event, whose
member of the field's name the default resolver reads, so it is
`{"<field>": value}`, and in `pylon` mode it is the value itself, the reading of
each mode the sidecar states.

Two cases do not run over every field. SUB9 runs on the first field of a schema
whose type is a leaf, because its table carries 1025 push steps and one table of
that size per schema and mode reaches the overflow. SUB10 needs a root field of
`Mutation` to publish from, so a schema without that root has none.

A case id is the SHA-256 of the canonical request with `pull`, `return_after`
and `interleave` added.
"""

from __future__ import annotations

import copy
from dataclasses import dataclass
from pathlib import Path

from .arguments import MISSING, base_value, deviations, required, required_arguments
from .build import leaf_children, path_selection
from .cases import case_id, counts_of, deduplicate
from .document import field, print_document
from .schema import TYPENAME, Schema, Step, load
from .schemas import RICH_SDL, ensure_branded_scalars, repository_root
from .sdl import print_value
from .tables import Cycles, flat_spec, synthesize, tree_spec

#: The root of `Subscription` the rich schema gains for this family, one field
#: per kind of type the executor completes, two of them with arguments.
SUBSCRIPTION_ROOT = """type Subscription {
  leafs: Leafs!
  branch: Branch
  node: Node
  anything: Anything
  text: String
  count: Int!
  amount: Number
  colour: Colour
  bag: JSONObject
  search(filter: Filter, tag: String, count: Int, colour: Colour, ids: [ID!], when: Date): [Leafs!]
  strict(filter: Filter!, tag: String!, count: Int!): Leafs!
}
"""

FIXTURE_SCHEMAS = {
    "subscription-app": "subscription-app/.aosabi/schema.graphql",
    "pubsub-app": "pubsub-app/.aosabi/schema.graphql",
}

#: The number of pushes of SUB9, one more than the queue of the repeater holds.
OVERFLOW_PUSHES = 1025

#: The topic every SUB10 table publishes on.
TOPIC = "sub10"

#: The values each base repeater pushes before it stops.
PUSHES = 3


@dataclass(frozen=True)
class SubscriptionCase:
    """One `subscribe` request of the family."""

    id: str
    family: str
    table: str
    document: str
    variables: dict | None
    operation_name: str | None
    validate: bool
    pull: int
    return_after: int | None
    interleave: list | None

    def as_json(self) -> dict:
        """The case as a notebook sends it, with `return_after` and `interleave` only when set.

        The operation `subscribe` reads only an absent field as unset and
        refuses a `null` in either, so a case leaves an unset one out.
        """
        case = {
            "id": self.id,
            "family": self.family,
            "table": self.table,
            "document": self.document,
            "variables": self.variables,
            "operationName": self.operation_name,
            "validate": self.validate,
            "pull": self.pull,
        }
        if self.return_after is not None:
            case["return_after"] = self.return_after
        if self.interleave is not None:
            case["interleave"] = self.interleave
        return case


def make_case(family: str, table: str, document: str, validate: bool = True, pull: int = 0,
              return_after: int | None = None, interleave: list | None = None) -> SubscriptionCase:
    """Builds a case and gives it the identity of its request."""
    request = {
        "document": document,
        "interleave": interleave,
        "operationName": None,
        "pull": pull,
        "return_after": return_after,
        "table": table,
        "validate": validate,
        "variables": None,
    }
    return SubscriptionCase(case_id(request), family, table, document, None, None, validate, pull,
                            return_after, interleave)


def rich_sdl() -> str:
    """The rich schema of `schemas.py` with the root of `Subscription` of this module."""
    head = "schema {\n  query: Query\n  mutation: Mutation\n}\n"
    if not RICH_SDL.startswith(head):
        raise ValueError("the rich schema no longer starts with its schema block")
    block = "schema {\n  query: Query\n  mutation: Mutation\n  subscription: Subscription\n}\n"
    return block + RICH_SDL[len(head):] + "\n" + SUBSCRIPTION_ROOT


def schema_names() -> list[str]:
    """The schemas the family runs over."""
    return ["rich", *FIXTURE_SCHEMAS]


def named_sdl(name: str, fixtures_dir: Path | None = None) -> str:
    """The schema text of one name, a fixture's read from its build."""
    if name == "rich":
        return rich_sdl()
    relative = FIXTURE_SCHEMAS.get(name)
    if relative is None:
        raise KeyError(f"no subscription schema named {name}")
    directory = fixtures_dir if fixtures_dir is not None else repository_root() / "tests" / "fixtures" / "ts"
    return (Path(directory) / relative).read_text()


def _root_fields(schema: Schema) -> list:
    root = schema.root("subscription")
    return [] if root is None else list(schema.fields(root).values())


def _value(schema: Schema, cycles: Cycles, mode: str, root_field, depth: int) -> dict:
    """One pushed value of a field: the event of `plain` mode or the value of `pylon` mode.

    A document of the family selects the leaves of the field's type and no
    deeper, so the tree of `pylon` mode is one level deep whatever `depth` is.
    """
    root = schema.root("subscription")
    path = f"{root}.{root_field.name}"
    if mode == "pylon":
        return tree_spec(schema, cycles, root_field.type, path, min(depth, 1))
    return {"do": "object", "members": {root_field.name: flat_spec(schema, cycles, root_field.type, path)}}


def _repeater(steps: list) -> dict:
    return {"do": "repeater", "steps": steps}


def base_table(schema: Schema, seed: int, mode: str, depth: int, name: str) -> dict:
    """The synthesized table with a repeater of three pushes and a stop per member of `Subscription`.

    The roots `Query` and `Mutation` keep their first member alone, which is all
    a subscription reaches: `pylon` mode refuses a table with no member of
    `Query`, SUB10 publishes from the first member of `Mutation`, and a field
    with no member takes the default resolver. The object types keep every
    member, because the events of `plain` mode are completed through them.
    """
    table = synthesize(schema, seed, mode, depth, name=name)
    for operation in ("query", "mutation"):
        root = schema.root(operation)
        members = table["resolvers"].get(root)
        if members:
            first = next(iter(members))
            table["resolvers"][root] = {first: members[first]}
    cycles = Cycles()
    members = {}
    for root_field in _root_fields(schema):
        steps = [{"push": _value(schema, cycles, mode, root_field, depth)} for _ in range(PUSHES)]
        members[root_field.name] = _repeater(steps + [{"stop": None}])
    if members:
        table["resolvers"][schema.root("subscription")] = members
    return table


def _injected(base: dict, schema: Schema, suffix: str, member: str, spec: dict) -> dict:
    """A copy of the base table whose `Subscription` holds the one member a case selects."""
    table = copy.deepcopy(base)
    table["name"] = f"{base['name']}-{suffix}-{member}"
    table["resolvers"][schema.root("subscription")] = {member: spec}
    return table


def _document(schema: Schema, root_field, alias_pair: bool = False, typename: bool = False,
              args=None) -> str:
    """The subscription document of one root field with its leaves selected."""
    root = schema.root("subscription")
    step = Step(parent=root, field=root_field)
    fill = required_arguments(schema)
    target = root_field.type.named_name()
    tail = leaf_children(schema, target, fill) if schema.is_composite(target) else None
    selections = path_selection(schema, [step], args if args is not None else fill, tail)
    if alias_pair:
        only = selections[0]
        selections = (
            field(only.name, only.children, alias="first", args=only.args),
            field(only.name, only.children, alias="second", args=only.args),
        )
    if typename:
        selections = (field(TYPENAME),) + selections
    return print_document("subscription", selections)


def _deviation_documents(schema: Schema, root_field) -> list[tuple[str, bool]]:
    """The documents of SUB4, one per argument and deviation, with the refusal marker."""
    root = schema.root("subscription")
    path = f"{root}.{root_field.name}"
    base = {argument.name: base_value(schema, argument.type, f"{path}.{argument.name}")
            for argument in root_field.args}
    documents = []
    for argument in root_field.args:
        for _, value, refused in deviations(schema, argument, base[argument.name]):
            def args_for(step, argument=argument, value=value):
                if step.field is not root_field:
                    return required_arguments(schema)(step)
                printed = []
                for one in root_field.args:
                    if one.name == argument.name:
                        if value is not MISSING:
                            printed.append((one.name, print_value(value)))
                    elif required(one):
                        printed.append((one.name, print_value(base[one.name])))
                return tuple(printed)
            documents.append((_document(schema, root_field, args=args_for), refused))
    return documents


def _mutation_member(schema: Schema) -> tuple[str, object] | None:
    """The first root field of `Mutation`, which SUB10 publishes from."""
    root = schema.root("mutation")
    if root is None:
        return None
    fields = list(schema.fields(root).values())
    return (root, fields[0]) if fields else None


def _publishing(base: dict, schema: Schema, mode: str, cycles: Cycles, depth: int, root_field,
                mutation) -> dict:
    """The SUB10 table: the member on the topic and the first mutation publishing to it."""
    table = _injected(base, schema, "SUB10", root_field.name, {"do": "pubsub_topic", "name": TOPIC})
    root, mutation_field = mutation
    entry = table["resolvers"][root][mutation_field.name]
    publish = {
        "do": "pubsub_publish",
        "name": TOPIC,
        "payload": _value(schema, cycles, mode, root_field, depth),
    }
    if mode == "pylon":
        publish["then"] = entry["returns"]
        entry["returns"] = publish
    else:
        publish["then"] = entry
        table["resolvers"][root][mutation_field.name] = publish
    return table


def subscription_cases(schema: Schema, seed: int, mode: str, depth: int, name: str) -> tuple[list[dict], list]:
    """The tables and the cases of SUB1 to SUB10 over one schema in one mode."""
    base = base_table(schema, seed, mode, depth, f"{mode}-{name}-sub")
    tables = [base]
    cases: list[SubscriptionCase] = []
    cycles = Cycles()
    mutation = _mutation_member(schema)
    overflowed = False
    for root_field in _root_fields(schema):
        document = _document(schema, root_field)
        cases.append(make_case("SUB1", base["name"], document))
        cases.append(make_case("SUB2", base["name"], document, pull=1))
        cases.append(make_case("SUB3", base["name"], document, return_after=1))
        for deviation, refused in _deviation_documents(schema, root_field):
            if not refused:
                continue
            for validate in (True, False):
                cases.append(make_case("SUB4", base["name"], deviation, validate=validate))
        value = _value(schema, cycles, mode, root_field, depth)
        injected = [
            ("SUB5", _repeater([{"push": value}, {"push": copy.deepcopy(value)},
                                {"throw": {"kind": "plain", "message": f"SUB5 {root_field.name}"}}])),
            ("SUB6", _repeater([{"throw": {"kind": "plain", "message": f"SUB6 {root_field.name}"}}])),
            ("SUB7", {"do": "not_iterable", "value": {"do": "value", "value": {"iterable": False}}}),
        ]
        if not overflowed and schema.is_leaf(root_field.type.named_name()):
            overflowed = True
            pushes = [{"push": copy.deepcopy(value), "await": False} for _ in range(OVERFLOW_PUSHES)]
            injected.append(("SUB9", _repeater(pushes + [{"stop": None}])))
        for family, spec in injected:
            table = _injected(base, schema, family, root_field.name, spec)
            tables.append(table)
            cases.append(make_case(family, table["name"], document))
        for validate in (True, False):
            cases.append(make_case("SUB8", base["name"], _document(schema, root_field, alias_pair=True),
                                   validate=validate))
            cases.append(make_case("SUB8", base["name"], _document(schema, root_field, typename=True),
                                   validate=validate))
        if mutation is not None:
            table = _publishing(base, schema, mode, cycles, depth, root_field, mutation)
            tables.append(table)
            root, mutation_field = mutation
            mutation_document = path_selection(
                schema, [Step(parent=root, field=mutation_field)], required_arguments(schema),
                leaf_children(schema, mutation_field.type.named_name(), required_arguments(schema))
                if schema.is_composite(mutation_field.type.named_name()) else None,
            )
            interleave = [{"after_pull": 1, "document": print_document("mutation", mutation_document)}]
            cases.append(make_case("SUB10", table["name"], document, pull=1, interleave=interleave))
    return tables, deduplicate(cases)


def generate(name: str, seed: int = 1, mode: str = "plain", depth: int = 3,
             fixtures_dir: Path | None = None) -> dict:
    """The corpus of one schema in one mode, as the committed file holds it."""
    text = named_sdl(name, fixtures_dir)
    if mode == "pylon":
        text = ensure_branded_scalars(text)
    schema = load(text)
    tables, cases = subscription_cases(schema, seed, mode, depth, name)
    return {
        "manifest": {
            "version": 1,
            "seed": seed,
            "parameters": {"name": name, "mode": mode, "depth": depth},
            "schema_sha256": schema.digest,
            "fields": [root_field.name for root_field in _root_fields(schema)],
            "tables": [table["name"] for table in tables],
            "counts": counts_of(cases),
            "total": len(cases),
        },
        "tables": tables,
        "cases": [case.as_json() for case in cases],
    }
