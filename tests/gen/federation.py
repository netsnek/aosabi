# SPDX-License-Identifier: MIT
"""The module writes what the federation notebooks send to both sides:

- `operations`, the client documents over a front schema with aliases,
  fragments on the abstract types, the forced and the allowed argument of a
  policy and variables, 120 over the front of G1 for case gw1-23 and 80 over
  the interface of G2 for case gw2-07,
- `transform_requests`, 300 requests of the operation `transform` over the
  five target schemas of `TARGETS`, each through a subset of the four
  transforms, with needs, arguments of every JSON kind and files, policies
  with constant and context values and the refusals they meet, beside the two
  named cases `dg-force-needs` and `dg-needs-typename`,
- `patch_result_requests`, 200 result trees through the patch transform with
  the behaviour table of the sidecar, strict and not,
- `merge_requests`, 500 seeded error sets through `merge_data_and_errors`,
- `document_requests`, 2000 requests of `prepare`, `finalize` and
  `create_request` over the front and target schemas of a pair,
  `create_request` over the client's document and the other two over the
  document the delegation builds from it, so a difference of the wire
  document bisects to one function, and
- `delegate_cases`, 150 delegations of a front document through the remote
  stub with the canned answers of the stub, `dg-order-01` and a delegation from
  outside a resolver among them.

Every request is a line of the protocol of tests/sidecar/federation.mjs,
`tests/sidecar/federation.md`, without its `id`, which a case carries beside it.
A case id is a hash of the request, and every family draws on until it holds its
count of distinct ids, so an id is a key of the goldens and of the remote stub.
The module decides nothing about correctness. The verdict of a run is the byte
comparison of the sidecar and `aosabi-devtool federation` over the same
requests. Every value is a neutral test string.

The manifest of the families that need no pair prints with the command below,
and two runs print the same bytes. The package imports this module, so the
form `python -m` would run it a second time and warn.

    python -c "import sys; from tests.gen.federation import main; sys.exit(main([]))"
"""

from __future__ import annotations

import argparse
import itertools
import json
import sys

from .cases import case_id
from .document import Fragment, Sel, VarDef, field, inline, print_document, spread
from .rng import Rng
from .schema import Schema, load
from .sdl import EnumValue, TypeRef, print_value

#: The seed a run takes when its parameters name none.
SEED = 20261003

#: The counts of the plan, section The notebooks and section The Federation cases.
COUNTS = {
    "operations_g1": 120,
    "operations_g2": 80,
    "transforms": 300,
    "patch_results": 200,
    "merges": 500,
    "documents": 2000,
    "delegations": 150,
}

#: The four transforms in the array order the federation hands `delegateToSchema`.
TRANSFORMS = ("inject_needs", "inline_args", "force_args", "patch")

#: The patch ids of the behaviour table of the sidecar.
PATCH_IDS = (
    "identity",
    "passthrough",
    "spread_full_name",
    "stamp_kind",
    "nested_org",
    "returns_array",
    "returns_null",
    "returns_string",
    "throws_plain",
    "throws_graphql",
    "project_id",
    "project_handle",
)

#: The scalars a schema of the compatibility layer declares, which a front SDL carries.
BRANDED_SCALARS = (
    "scalar String\nscalar ID\nscalar Int\nscalar Float\nscalar Boolean\nscalar Number\n"
    "scalar Any\nscalar Void\nscalar JSONObject\nscalar File\nscalar Date\nscalar JSON\n"
)

#: The five target schemas the transforms run over: the schema of the
#: reference's tests of the policy and of the uploads, and the remotes of G1, G2
#: and G4 as their builds write them.
TARGETS = {
    "collections": (
        "type Product { id: ID!, title: String! }\n"
        "type ProductConnection { totalCount: Int!, nodes: [Product!]! }\n"
        "type ProductCollection { handle: String! products(query: String, first: Int): ProductConnection! }\n"
        "type Vendor { name: String! products(query: String, first: Int): ProductConnection! }\n"
        "type Query { products(query: String, first: Int): ProductConnection! productCollections(first: Int): [ProductCollection!]! }\n"
    ),
    "uploads": (
        "scalar File\n"
        "input SubmitInput { note: String, attachments: [File!] }\n"
        "type Result { ok: Boolean! }\n"
        "type Mutation { submit(input: SubmitInput!): Result! }\n"
        "type Query { _: Boolean }\n"
    ),
    "users": (
        "type Query { user(id: String!): User }\n"
        "type User { id: String! email: String! firstName: String! lastName: String! orgId: String! "
        "seenAuth: String! orders(status: String, limit: Number): [String!]! }\n"
        "scalar Number\n"
    ),
    "polyusers": (
        "type Query { user(id: String!): User }\n"
        "type User { id: String! email: String! kind: String! specialty: String insuranceId: String }\n"
    ),
    "catalogue": (
        "type Query { product(handle: String!): Product collection(handle: String!): ProductCollection "
        "rejecting(kind: String!): String! partial: Partial! }\n"
        "type Mutation { submit(input: SubmitInput!): SubmitResult! }\n"
        "type Product { id: ID! title: String! status: String! isPublished: Boolean! "
        "price(currency: String!): Money! related(query: String, first: Number): [String!]! }\n"
        "type Money { amount: Number! currency: String! }\n"
        "type ProductCollection { title: String! products(first: Number): [Product!]! }\n"
        "type Partial { ok: String! broken: String }\n"
        "type SubmitResult { ok: Boolean! names: [String!]! sizes: [Number!]! }\n"
        "input SubmitInput { note: String! attachments: [File!]! }\n"
        "scalar Number\nscalar File\n"
    ),
}

#: The policies the transforms meet over each target, keyed `Type.field`.
TARGET_POLICIES = {
    "collections": ("ProductCollection.products", "Vendor.products"),
    "uploads": (),
    "users": ("User.orders",),
    "polyusers": (),
    "catalogue": ("Product.related", "Product.price", "ProductCollection.products"),
}


#: The draws a family may spend per case of its count before it raises.
DRAWS_PER_CASE = 20


def _case(family: str, request: dict, **extra) -> dict:
    """One case: its id over the request, its family, the request and what else it carries."""
    return {"id": "%s-%s" % (family, case_id(request)[:16]), "family": family, "request": request, **extra}


def _distinct(count: int, draw, first=()) -> list:
    """The cases of `count` distinct ids, the named `first` ones and then one draw per index.

    A case id is the hash of its request, so two equal draws carry one id, and
    the later is dropped as `cases.deduplicate` drops it. The drawing goes on
    with the next index until the count is reached. A draw reads only the
    streams of its own index, so the cases of a smaller count are a prefix of
    the cases of a larger one. A family whose space is too small for its count
    raises rather than return fewer cases than the plan counts.
    """
    out: list = []
    seen: set = set()
    for case in first:
        if case["id"] not in seen:
            seen.add(case["id"])
            out.append(case)
    index = 0
    while len(out) < count:
        if index >= count * DRAWS_PER_CASE:
            raise ValueError("Only %d distinct cases in %d draws, short of the count %d" % (len(out), index, count))
        case = draw(index)
        index += 1
        if case["id"] in seen:
            continue
        seen.add(case["id"])
        out.append(case)
    return out


# ── values ───────────────────────────────────────────────────────────────────


def scalar_value(rng: Rng, purpose: str, name: str):
    """One value of a scalar type, as the model of `sdl.print_value` holds it."""
    stream = rng.stream(purpose)
    if name in ("Int",):
        return stream.choice((0, 1, 2, 5, 10, -3))
    if name in ("Float",):
        return stream.choice((0.5, 1.25, -2.75, 3.0e-3))
    if name == "Number":
        return stream.choice((1, 2, 7, 1.5, 0.25))
    if name == "Boolean":
        return stream.choice((True, False))
    if name == "ID":
        return stream.choice(("id-1", "id-2", "p1"))
    return stream.choice(("value one", "status:ACTIVE", "status:DRAFT", "u1", "h1", "c1", "EUR", "text é"))


def input_value(schema: Schema, rng: Rng, purpose: str, ref: TypeRef, depth: int = 0):
    """One value of an input type reference, `None` for a nullable position at times."""
    stream = rng.stream(purpose)
    if not ref.is_non_null and stream.random() < 0.1:
        return None
    inner = ref.nullable()
    if inner.kind == "list":
        return [input_value(schema, rng, purpose, inner.of_type, depth + 1) for _ in range(stream.randint(0, 2))]
    name = inner.name
    try:
        kind = schema.kind(name)
    except KeyError:
        kind = "scalar"
    if kind == "enum":
        return EnumValue(stream.choice(schema.type(name).enum_values))
    if kind == "input" and depth < 3:
        out = {}
        for member in schema.type(name).input_fields.values():
            if member.type.is_non_null or stream.random() < 0.6:
                out[member.name] = input_value(schema, rng, purpose, member.type, depth + 1)
        return out
    return scalar_value(rng, purpose, name)


def json_kind_value(rng: Rng, purpose: str, depth: int = 0):
    """One JSON value of any kind, the arguments of a need and of a policy among them.

    The tagged forms of the codec stand for what JSON cannot spell, so an
    argument meets `undefined`, a number JSON has no text for and a date.
    """
    stream = rng.stream(purpose)
    kinds = ["string", "int", "float", "true", "false", "null", "list", "object", "undefined", "special", "date"]
    if depth >= 2:
        kinds = kinds[:6]
    kind = stream.choice(kinds)
    if kind == "string":
        return stream.choice(("status:ACTIVE", "a \"quoted\" text", "line\nbreak", "日本"))
    if kind == "int":
        return stream.choice((0, 7, -12, 2**31))
    if kind == "float":
        return stream.choice((0.5, -1.25, 1e21, 1.5e-7))
    if kind in ("true", "false"):
        return kind == "true"
    if kind == "null":
        return None
    if kind == "list":
        return [json_kind_value(rng, purpose, depth + 1) for _ in range(stream.randint(0, 3))]
    if kind == "object":
        return {"k%d" % index: json_kind_value(rng, purpose, depth + 1) for index in range(stream.randint(0, 3))}
    if kind == "undefined":
        return {"$undefined": True}
    if kind == "special":
        return {"$number": stream.choice(("NaN", "Infinity", "-Infinity", "-0"))}
    return {"$date": stream.choice(("2026-01-02T03:04:05.000Z", None))}


def _json_of(value):
    """A value of the model of `sdl` as a request carries it in JSON."""
    if isinstance(value, EnumValue):
        return value.name
    if isinstance(value, list):
        return [_json_of(item) for item in value]
    if isinstance(value, dict):
        return {key: _json_of(item) for key, item in value.items()}
    return value


# ── selections ───────────────────────────────────────────────────────────────


class _Builder:
    """Builds one document over a schema and keeps the variables and fragments it used."""

    def __init__(self, schema: Schema, rng: Rng, purpose: str, policies=(), forced=(), choices=None) -> None:
        self.schema = schema
        self.rng = rng
        self.purpose = purpose
        self.policies = set(policies)
        self.forced = set(forced)
        self.choices = choices or {}
        self.variables: list[VarDef] = []
        self.values: dict = {}
        # The JSON value of every argument of a root field by its name, which
        # a root resolver hands the delegation as its arguments.
        self.root_values: dict = {}
        self.fragments: list[Fragment] = []
        self.aliases = itertools.count(1)

    def chance(self, p: float) -> bool:
        return self.rng.stream(self.purpose).random() < p

    def arguments(self, parent: str, one, root: bool) -> tuple:
        """The arguments of one field, the policy's forced and allowed ones weighed apart."""
        key = "%s.%s" % (parent, one.name)
        out = []
        for argument in one.args:
            slot = "%s.%s" % (key, argument.name)
            if key in self.policies:
                weight = 0.25 if slot in self.forced else 0.5
            else:
                weight = 1.0 if argument.type.is_non_null else 0.4
            if not argument.type.is_non_null and not self.chance(weight):
                continue
            if slot in self.choices:
                value = self.rng.choice(self.purpose, self.choices[slot])
            else:
                value = input_value(self.schema, self.rng, self.purpose, argument.type)
            if root:
                self.root_values[argument.name] = _json_of(value)
            if root and self.chance(0.4):
                name = "v%d" % (len(self.variables) + 1)
                self.variables.append(VarDef(name, argument.type.text()))
                self.values[name] = _json_of(value)
                out.append((argument.name, "$" + name))
            else:
                out.append((argument.name, print_value(value)))
        return tuple(out)

    def selections(self, type_name: str, depth: int) -> tuple:
        """A selection set on `type_name`, fragments on an abstract type among it."""
        stream = self.rng.stream(self.purpose)
        out: list[Sel] = []
        if self.schema.is_abstract(type_name):
            if self.chance(0.5):
                out.append(field("__typename"))
            common = [step for step in self.schema.selectable(type_name) if step.condition is None]
            for step in self.rng.pick(self.purpose, common, stream.randint(1, max(1, len(common)))):
                out.append(self.field(step.parent, step.field, depth))
            for possible in self.schema.possible_types(type_name):
                if self.chance(0.7):
                    inner = self.selections(possible, depth)
                    if self.chance(0.3):
                        name = "F%d" % (len(self.fragments) + 1)
                        self.fragments.append(Fragment(name, possible, inner))
                        out.append(spread(name))
                    else:
                        out.append(inline(possible, inner))
            return tuple(out) or (field("__typename"),)
        steps = self.schema.selectable(type_name)
        if not steps:
            return (field("__typename"),)
        chosen = self.rng.pick(self.purpose, steps, stream.randint(1, min(4, len(steps))))
        for step in chosen:
            out.append(self.field(step.parent, step.field, depth))
        if self.chance(0.15):
            out.insert(stream.randint(0, len(out)), field("__typename"))
        if self.chance(0.1):
            out.append(inline(type_name, (self.field(chosen[0].parent, chosen[0].field, depth),)))
        return tuple(out)

    def field(self, parent: str, one, depth: int, root: bool = False) -> Sel:
        """One field with its arguments, an alias at times, and its selection set."""
        target = one.type.named_name()
        alias = "a%d" % next(self.aliases) if self.chance(0.3) else None
        args = self.arguments(parent, one, root)
        if self.schema.is_composite(target):
            children = self.selections(target, depth - 1) if depth > 1 else (field("__typename"),)
            return field(one.name, children, alias=alias, args=args)
        return field(one.name, alias=alias, args=args)


def operations(sdl: str, count: int, seed: int = SEED, depth: int = 3, operation: str = "query",
               roots=None, policies=(), forced=(), choices=None, family: str = "fo") -> list:
    """Client documents over a front schema, with their variables.

    `roots` names the root fields a document may select, all of the root type
    when absent. `policies` names the `Type.field` keys a policy covers and
    `forced` the `Type.field.argument` slots it forces, so a document supplies a
    forced argument at times, which is a refusal, and an allowed one more often.
    `choices` maps a `Type.field.argument` slot to the values it takes, such as
    the rows a remote holds.
    """
    schema = load(sdl)
    root = schema.root(operation)
    rng = Rng(seed)

    def draw(index: int) -> dict:
        purpose = "%s.%d" % (family, index)
        builder = _Builder(schema, rng, purpose, policies, forced, choices)
        candidates = [step for step in schema.selectable(root) if roots is None or step.field.name in roots]
        picked = rng.pick(purpose, candidates, rng.stream(purpose).randint(1, min(2, len(candidates))))
        selections = tuple(builder.field(step.parent, step.field, depth, root=True) for step in picked)
        name = "Op%d" % index if builder.variables or rng.stream(purpose).random() < 0.2 else None
        if operation != "query" and name is None:
            name = "Op%d" % index
        document = print_document(operation, selections, name=name, variables=tuple(builder.variables),
                                  fragments=tuple(builder.fragments))
        request = {"document": document}
        if builder.values:
            request["variables"] = builder.values
        return _case(family, request)

    return _distinct(count, draw)


# ── transforms ───────────────────────────────────────────────────────────────


def _needs(schema: Schema, rng: Rng, purpose: str, type_name: str, depth: int) -> dict:
    """A needs map over the fields of `type_name`, with `__args` on a field that takes arguments."""
    stream = rng.stream(purpose)
    needs: dict = {}
    for step in rng.pick(purpose, schema.selectable(type_name), stream.randint(1, 4)):
        one = step.field
        target = one.type.named_name()
        if schema.is_composite(target) and depth > 1:
            inner = _needs(schema, rng, purpose, target, depth - 1)
            if one.args and stream.random() < 0.6:
                inner["__args"] = {arg.name: json_kind_value(rng, purpose) for arg in one.args[:2]}
            needs[one.name] = inner
        elif one.args and stream.random() < 0.5:
            needs[one.name] = {"__args": {arg.name: json_kind_value(rng, purpose) for arg in one.args[:2]}}
        else:
            needs[one.name] = stream.choice((True, True, True, False, 0, ""))
    return needs


def _policies(rng: Rng, purpose: str, schema: Schema, keys) -> dict:
    """Policies over `keys`, each forcing one argument of the field and at times allowing the others."""
    stream = rng.stream(purpose)
    out = {}
    for key in keys:
        if stream.random() < 0.3:
            continue
        type_name, field_name = key.split(".")
        one = schema.fields(type_name).get(field_name)
        if one is None or not one.args:
            continue
        forced = one.args[0].name
        value = stream.choice((
            "status:ACTIVE",
            {"$fn": "tenant:t1"},
            {"$ctx": "tenantId"},
            {"$fn": {"$undefined": True}},
            7,
        ))
        policy = {"force": {forced: value}}
        if stream.random() < 0.6:
            policy["args"] = [arg.name for arg in one.args[1:]]
        out[key] = policy
    return out


def transform_requests(count: int = COUNTS["transforms"], seed: int = SEED) -> list:
    """Requests of the operation `transform` over the five target schemas.

    Each draws a target, a root field, a subset of the four transforms in their
    array order, a document over the root field, wrapper arguments of the root
    field with files where an input takes `File`, a needs map, policies and a
    context. The two named cases come first: `dg-force-needs`, a need with
    `__args` on the field a policy forces, which reaches the remote because the
    policy runs before the need (quirk Q10-5), and `dg-needs-typename`, a nested
    need under a patched type, whose selection set carries no `__typename`
    because the patch transform ran before the need was added.
    """
    rng = Rng(seed)
    first = [
        _case("dg-force-needs", {
            "transforms": list(TRANSFORMS),
            "target_sdl": TARGETS["catalogue"],
            "field_name": "product",
            "document": 'query { product(handle: "h1") { title } }',
            "needs": {"related": {"__args": {"query": "status:DRAFT"}}},
            "args": {"handle": "h1"},
            "policies": {"Product.related": {"args": ["first"], "force": {"query": "status:ACTIVE"}}},
            "ctx": {},
            "patches": {"Product": "project_id"},
        }),
        _case("dg-needs-typename", {
            "transforms": list(TRANSFORMS),
            "target_sdl": TARGETS["catalogue"],
            "field_name": "collection",
            "document": 'query { collection(handle: "c1") { title } }',
            "needs": {"products": {"__args": {"first": 1}, "id": True, "price": {"__args": {"currency": "EUR"}, "amount": True}}},
            "args": {"handle": "c1"},
            "patches": {"Product": "project_id"},
        }),
    ]
    subsets = [list(one) for size in range(1, 5) for one in itertools.combinations(TRANSFORMS, size)]

    def draw(index: int) -> dict:
        purpose = "transform.%d" % index
        stream = rng.stream(purpose)
        target = stream.choice(sorted(TARGETS))
        schema = load(TARGETS[target])
        operation = "mutation" if target == "uploads" else stream.choice(("query",) * 3 + (("mutation",) if schema.root("mutation") else ()))
        root = schema.root(operation)
        root_field = stream.choice(list(schema.fields(root).values()))
        builder = _Builder(schema, rng, purpose, TARGET_POLICIES[target], (), None)
        selection = builder.field(root, root_field, 3)
        document = print_document(operation, (selection,), variables=tuple(builder.variables),
                                  fragments=tuple(builder.fragments))
        request = {
            "transforms": subsets[stream.randrange(len(subsets))] if index % 15 else list(TRANSFORMS),
            "target_sdl": TARGETS[target],
            "field_name": root_field.name,
            "document": document,
        }
        if builder.values or stream.random() < 0.5:
            request["variables"] = dict(builder.values)
            if stream.random() < 0.3:
                request["variables"]["unused"] = json_kind_value(rng, purpose)
        if stream.random() < 0.8:
            args = {}
            for argument in root_field.args:
                value = _json_of(input_value(schema, rng, purpose, argument.type))
                if target == "uploads":
                    value = {"note": stream.choice(("n", {"$undefined": True})),
                             "attachments": [{"$file": {"name": "f%d.txt" % slot, "size": slot + 1}}
                                             for slot in range(stream.randint(0, 3))]}
                args[argument.name] = value
            if stream.random() < 0.3:
                args["extra"] = json_kind_value(rng, purpose)
            request["args"] = args
        target_type = root_field.type.named_name()
        if schema.is_composite(target_type) and stream.random() < 0.7:
            request["needs"] = _needs(schema, rng, purpose, target_type, 2)
        policies = _policies(rng, purpose, schema, TARGET_POLICIES[target])
        if policies or stream.random() < 0.2:
            request["policies"] = policies
        if stream.random() < 0.7:
            request["ctx"] = {"tenantId": stream.choice(("t1", "t2"))}
        if "patch" in request["transforms"] and stream.random() < 0.5:
            request["patches"] = {"Product": "project_id", "Money": "passthrough"}
            request["strict"] = stream.random() < 0.5
        return _case("dg-transform", request)

    return _distinct(count, draw, first)


# ── results, merges ──────────────────────────────────────────────────────────


def _tree(rng: Rng, purpose: str, depth: int, errors: list):
    """A result tree of objects with and without a typename, lists, scalars and errors."""
    stream = rng.stream(purpose)
    roll = stream.random()
    if depth <= 0 or roll < 0.25:
        return stream.choice(("text", 3, 1.5, True, None, {"$undefined": True}))
    if roll < 0.35:
        errors.append(None)
        kind = stream.choice(("GraphQLError", "Error"))
        error = {"name": kind, "message": "upstream error %d" % len(errors)}
        if kind == "GraphQLError" and stream.random() < 0.5:
            error["extensions"] = {"code": "UPSTREAM"}
        return {"$error": error, "ref": len(errors)}
    if roll < 0.5:
        return [_tree(rng, purpose, depth - 1, errors) for _ in range(stream.randint(0, 3))]
    out = {}
    if stream.random() < 0.8:
        out["__typename"] = stream.choice(("Product", "Money", "User", "ProductCollection", "Org", "Query"))
    for key in rng.pick(purpose, ("id", "title", "firstName", "lastName", "kind", "handle", "price", "related", "message"),
                        stream.randint(1, 4)):
        out[key] = _tree(rng, purpose, depth - 1, errors)
    if out.get("__typename") == "User":
        out.setdefault("firstName", "Ada")
        out.setdefault("lastName", "One")
        out.setdefault("kind", stream.choice(("doctor", "patient")))
    return out


def patch_result_requests(count: int = COUNTS["patch_results"], seed: int = SEED) -> list:
    """Requests of the operation `patch_result`, strict and not, over the behaviour table."""
    rng = Rng(seed)

    def draw(index: int) -> dict:
        purpose = "patch.%d" % index
        stream = rng.stream(purpose)
        patches = {}
        for type_name in rng.pick(purpose, ("Product", "Money", "User", "ProductCollection", "Org", "Query"),
                                  stream.randint(0, 4)):
            patch_id = stream.choice(PATCH_IDS)
            patches[type_name] = patch_id if stream.random() < 0.8 else {
                "id": patch_id, "policy": {"related": {"args": ["first"], "force": {"query": "status:ACTIVE"}}}}
        request = {"result": _tree(rng, purpose, 4, [])}
        if patches or stream.random() < 0.5:
            request["patches"] = patches
        if stream.random() < 0.5:
            request["strict"] = True
        return _case("dg-patch", request)

    return _distinct(count, draw)


def merge_requests(count: int = COUNTS["merges"], seed: int = SEED) -> list:
    """Requests of the operation `merge_data_and_errors` over seeded data and error sets.

    The data is a root field's value or null, the errors carry paths that start
    at the root field's key and reach into the data at times, none at times and
    elsewhere at times, so the merge relocates, aggregates, distributes and
    leaves unpathed in every combination.
    """
    rng = Rng(seed)

    def draw(index: int) -> dict:
        purpose = "merge.%d" % index
        stream = rng.stream(purpose)
        key = stream.choice(("user", "product", "a"))
        data = None if stream.random() < 0.3 else {
            "id": "u1",
            "orders": stream.choice((None, ["o1"], [None, "o2"])),
            "org": stream.choice((None, {"name": None, "id": "org1"})),
            "list": [{"x": None}, {"x": 1}],
        }
        errors = []
        for slot in range(stream.randint(0, 4)):
            error = {"message": "remote error %d" % slot}
            shape = stream.random()
            if shape < 0.2:
                pass
            elif shape < 0.4:
                error["path"] = [key]
            elif shape < 0.6:
                error["path"] = [key, stream.choice(("orders", "org", "missing"))]
            elif shape < 0.8:
                error["path"] = [key, "list", stream.randint(0, 2), "x"]
            else:
                error["path"] = [key, "org", "name"]
            if stream.random() < 0.5:
                error["extensions"] = {"code": stream.choice(("UNAUTHENTICATED", "UNAVAILABLE"))}
            if stream.random() < 0.2:
                error["locations"] = [{"line": 1, "column": stream.randint(1, 20)}]
            errors.append(error)
        request = {"data": data, "errors": errors}
        if stream.random() < 0.8:
            request["path"] = stream.choice((["fullUser"], ["a", 0, "b"], ["product"]))
        return _case("dg-merge", request)

    return _distinct(count, draw)


# ── documents of the delegation ──────────────────────────────────────────────


def delegated_document(pair: dict, operation: str, front_field: str, selection: Sel, builder: _Builder,
                       name: str | None) -> tuple:
    """The document and the variables the delegation builds for one root field of the front.

    `delegateToSchema` hands `createRequest` the field node of the front, the
    name of the remote field and the arguments the root resolver passes. The
    function writes the remote field without the alias of the front, keeps an
    argument the client passed as a variable of the same value, writes every
    other argument as a new variable named after it, typed by the remote field
    and valued in the variables, and keeps the variables and the fragments of
    the client. The root resolver of a pair passes the arguments its `roots`
    entry names, `{"$arg": i}` the i-th argument of the front field. `prepare`
    and `finalize` meet the document in this form inside a delegation, so their
    cases name a root field the target has.
    """
    target = load(pair["target_sdl"])
    front = load(pair["front_sdl"])
    root_name = front.root(operation)
    target_field = pair["fields"][front_field]
    spec = pair["roots"][root_name][front_field]["delegate"]
    declared = front.fields(root_name)[front_field].args
    remote_args = {argument.name: argument for argument in target.fields(target.root(operation))[target_field].args}
    passed = dict(selection.args)
    variables = list(builder.variables)
    values = dict(builder.values)
    args = []
    for arg_name, raw in spec.get("args", {}).items():
        front_arg = declared[raw["$arg"]].name
        bound = front_arg in builder.root_values
        value = builder.root_values.get(front_arg)
        written = passed.get(arg_name, "")
        if bound and written.startswith("$") and values.get(written[1:]) == value:
            args.append((arg_name, written))
            continue
        var_name = arg_name
        taken = {one.name for one in variables} | {key for key, item in values.items() if item is not None}
        if var_name in taken:
            var_name = "_%s_%s" % (target_field, arg_name)
            index = 0
            while var_name in taken:
                var_name = "_%d_%s_%s" % (index, target_field, arg_name)
                index += 1
        variables.append(VarDef(var_name, remote_args[arg_name].type.text()))
        if bound:
            values[var_name] = value
        args.append((arg_name, "$" + var_name))
    root = field(target_field, selection.children, args=tuple(args), directives=selection.directives)
    document = print_document(operation, (root,), name=name, variables=tuple(variables),
                              fragments=tuple(builder.fragments))
    return document, values


def document_requests(pairs: list, count: int = COUNTS["documents"], seed: int = SEED) -> list:
    """Requests of `prepare`, `finalize` and `create_request` over the schemas of the pairs.

    A pair is `{"name", "front_sdl", "target_sdl", "fields", "roots"}`, where
    `fields` maps a root field of the front to the root field of the target it
    delegates to, as G1, G2 and G4 do, and `roots` names the arguments each
    root resolver passes. Each request draws a pair, an operation, and a
    document of `operations` over the front schema, so a fragment on an
    abstract type of the front meets the target's view of it.
    `create_request` takes the client's document, and `prepare` and
    `finalize` take the document `delegated_document` builds from it.
    """
    rng = Rng(seed)

    def draw(index: int) -> dict:
        purpose = "document.%d" % index
        stream = rng.stream(purpose)
        pair = pairs[stream.randrange(len(pairs))]
        front_field = stream.choice(sorted(pair["fields"]))
        target_field = pair["fields"][front_field]
        front = load(pair["front_sdl"])
        target = load(pair["target_sdl"])
        operation = "mutation" if front.root("query") is None or front_field not in front.fields(front.root("query")) else "query"
        builder = _Builder(front, Rng(seed + index), purpose, pair.get("policies", ()), pair.get("forced", ()),
                           pair.get("choices"))
        root = front.root(operation)
        step = next(one for one in front.selectable(root) if one.field.name == front_field)
        selection = builder.field(step.parent, step.field, 3, root=True)
        name = "Op%d" % index if builder.variables or operation != "query" else None
        case = {"document": print_document(operation, (selection,), name=name, variables=tuple(builder.variables),
                                           fragments=tuple(builder.fragments))}
        if builder.values:
            case["variables"] = builder.values
        op = stream.choice(("prepare", "finalize", "create_request"))
        if op == "create_request":
            root_type = target.root(operation)
            args = {}
            for argument in target.fields(root_type)[target_field].args:
                args[argument.name] = _json_of(input_value(target, rng, purpose, argument.type))
            request = {"target_sdl": pair["target_sdl"], "document": case["document"],
                       "field_name": selection.alias or selection.name, "target_field_name": target_field, "args": args}
            if "variables" in case:
                request["variables"] = case["variables"]
        elif op == "prepare":
            root_type = front.root(operation)
            document, _ = delegated_document(pair, operation, front_field, selection, builder, name)
            request = {"target_sdl": pair["target_sdl"], "info_sdl": pair["front_sdl"],
                       "return_type": front.fields(root_type)[front_field].type.text(), "document": document}
        else:
            document, values = delegated_document(pair, operation, front_field, selection, builder, name)
            request = {"target_sdl": pair["target_sdl"], "document": document}
            if values:
                request["variables"] = values
        return _case("dg-" + op.replace("_", "-"), {"op": op, **request})

    return _distinct(count, draw)


# ── delegations through the remote stub ──────────────────────────────────────


def _answer(schema: Schema, rng: Rng, purpose: str, type_name: str, selections, fragments: dict, depth: int):
    """A remote's data for a selection set, keyed by the response keys the delegation keeps."""
    stream = rng.stream(purpose)
    if schema.is_abstract(type_name):
        type_name = stream.choice(schema.possible_types(type_name))
    out = {"__typename": type_name}
    for selection in selections:
        if selection.kind == "inline":
            if selection.name == type_name:
                out.update(_answer(schema, rng, purpose, type_name, selection.children, fragments, depth))
            continue
        if selection.kind == "spread":
            fragment = fragments.get(selection.name)
            if fragment is not None and fragment.type_condition == type_name:
                out.update(_answer(schema, rng, purpose, type_name, fragment.selections, fragments, depth))
            continue
        if selection.name == "__typename":
            continue
        one = schema.fields(type_name).get(selection.name)
        if one is None:
            continue
        key = selection.alias or selection.name
        out[key] = _value(schema, rng, purpose, one.type, selection, fragments, depth)
    return out


def _value(schema: Schema, rng: Rng, purpose: str, ref: TypeRef, selection: Sel, fragments: dict, depth: int):
    stream = rng.stream(purpose)
    if not ref.is_non_null and stream.random() < 0.1:
        return None
    inner = ref.nullable()
    if inner.kind == "list":
        return [_value(schema, rng, purpose, inner.of_type, selection, fragments, depth) for _ in range(stream.randint(0, 2))]
    if schema.is_composite(inner.name):
        return _answer(schema, rng, purpose, inner.name, selection.children, fragments, depth - 1)
    if schema.kind(inner.name) == "enum":
        return stream.choice(schema.type(inner.name).enum_values)
    return _json_of(scalar_value(rng, purpose, inner.name))


def _stub_answer(target: Schema, rng: Rng, purpose: str, target_field: str, selection: Sel, fragments: dict) -> dict:
    """The stub's answer to one delegated root field: data, a root error, a nested null, or a refusal."""
    stream = rng.stream(purpose)
    target_type = target.fields(target.root("query"))[target_field].type
    shape = stream.random()
    if shape < 0.6:
        return {"data": {target_field: _value(target, rng, purpose, target_type, selection, fragments, 3)}}
    if shape < 0.75:
        return {"data": {target_field: None}, "errors": [
            {"message": "Session not found", "path": [target_field], "extensions": {"code": "UNAUTHENTICATED"}}]}
    if shape < 0.9:
        value = _value(target, rng, purpose, target_type, selection, fragments, 3)
        nested = next((key for key in value if key != "__typename"), None) if isinstance(value, dict) else None
        if nested is not None:
            value[nested] = None
        path = [target_field] + ([nested] if nested is not None else [])
        return {"data": {target_field: value}, "errors": [{"message": "Rows are not available", "path": path}]}
    return {"errors": [{"message": "The remote refused the request", "extensions": {"code": "FORBIDDEN"}}]}


def delegate_cases(pairs: list, count: int = COUNTS["delegations"], seed: int = SEED) -> list:
    """Delegations of a front document through the remote stub, with the stub's answers.

    A pair here also carries `roots`, the root resolvers of the front in the
    form of the operation `delegate`, `federation`, the federation without its
    URL, which the notebook fills per side from the stub, and `target_sdl`,
    whose introspection the stub answers. Each case draws a pair and a front
    document and carries `answers`, the stub's answers to the data requests,
    built over the target schema for the response keys the delegated document
    keeps: data, a remote error at the root, an error beside a nested null, or
    a remote error with extensions.

    Two cases come first, both over the first pair. `dg-outside` calls the
    federation from outside a resolver, and `dg-order-01` adds a root field
    `sibling` whose promise rejects at tick one beside a delegation that the
    policy refuses, with the document the pair names in `order_document`.
    """
    rng = Rng(seed)
    head = pairs[0]
    first = [_case("dg-outside", {
        "op": "delegate", "federation": head["federation"], "outside_resolver": True,
        "call": {"key": "Query.%s" % next(iter(head["fields"].values())), "args": {}},
    }, target_sdl=head["target_sdl"], answers=[])]
    if "order_document" in head:
        roots = {"Query": {**head["roots"]["Query"], "sibling": {"reject": {"message": "sibling rejected", "ticks": 1}}}}
        first.append(_case("dg-order-01", {
            "op": "delegate",
            "front_sdl": head["front_sdl"].replace("type Query {\n", "type Query {\n  sibling: String\n", 1),
            "roots": roots,
            "federation": head["federation"],
            "document": head["order_document"],
        }, target_sdl=head["target_sdl"], answers=[]))

    def draw(index: int) -> dict:
        purpose = "delegate.%d" % index
        stream = rng.stream(purpose)
        pair = pairs[stream.randrange(len(pairs))]
        front = load(pair["front_sdl"])
        target = load(pair["target_sdl"])
        front_field = stream.choice(sorted(pair["fields"]))
        builder = _Builder(front, Rng(seed + index), purpose, pair.get("policies", ()), pair.get("forced", ()),
                           pair.get("choices"))
        step = next(one for one in front.selectable(front.root("query")) if one.field.name == front_field)
        selection = builder.field(step.parent, step.field, 3, root=True)
        document = print_document("query", (selection,), name="Op%d" % index if builder.variables else None,
                                  variables=tuple(builder.variables), fragments=tuple(builder.fragments))
        fragments = {fragment.name: fragment for fragment in builder.fragments}
        body = _stub_answer(target, rng, purpose, pair["fields"][front_field], selection, fragments)
        request = {"op": "delegate", "front_sdl": pair["front_sdl"], "roots": pair["roots"],
                   "federation": pair["federation"], "document": document}
        if builder.values:
            request["variables"] = builder.values
        if stream.random() < 0.3:
            request["request_headers"] = {"authorization": "Bearer probe-token"}
        return _case("dg-delegate", request, target_sdl=pair["target_sdl"], answers=[body])

    return _distinct(count, draw, first)


# ── the command line ─────────────────────────────────────────────────────────


def manifest(seed: int = SEED) -> dict:
    """The counts, the distinct ids and the digest of the ids of the families that need no pair.

    Two runs print the same bytes, and a count equals its distinct ids because
    every family drops a repeated draw.
    """
    families = {
        "transform": transform_requests(seed=seed),
        "patch_result": patch_result_requests(seed=seed),
        "merge_data_and_errors": merge_requests(seed=seed),
    }
    return {
        "seed": seed,
        "counts": {name: len(cases) for name, cases in families.items()},
        "distinct": {name: len({case["id"] for case in cases}) for name, cases in families.items()},
        "ids": {name: case_id({"ids": [case["id"] for case in cases]}) for name, cases in families.items()},
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(prog="tests.gen.federation", description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--seed", type=int, default=SEED)
    arguments = parser.parse_args(argv)
    print(json.dumps(manifest(arguments.seed), indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
