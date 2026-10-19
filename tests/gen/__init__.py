# SPDX-License-Identifier: MIT
"""tests/gen: the seeded operation generator of the Aosabi harness.

The package reads a schema, enumerates the operation families of
`docs/harness.md` section 4 over it, writes the behaviour tables the two sides
run those operations against, and returns both as a [`Corpus`] with the counts
and the seed a run records. It decides nothing about correctness. The verdict of
a run is the byte comparison of the node sidecar and the Rust devtool over the
same corpus.

The configuration and invalid document families of that section belong to later
milestones. Both are reached by name.

One run of the generator prints its counts with

    python -m tests.gen
    python -m tests.gen --family http
    python -m tests.gen --family wire
"""

from __future__ import annotations

from . import login, pubsub, streams, subscriptions, zitadel
from .arguments import argument_cases
from .cases import Case, Corpus, budget, canonical, case_id, counts_of, deduplicate, make_case
from .directives import directive_cases
from .errors import error_cases
from .federation import (
    delegate_cases,
    document_requests,
    merge_requests,
    operations,
    patch_result_requests,
    transform_requests,
)
from .http import (
    HttpCase,
    http_manifest,
    over_http,
    route_cases,
    static_cases,
    transport_cases,
    upload_cases,
)
from .identity import identity_cases, log_case
from .introspection import introspection_cases
from .ordering import ordering_cases, ordering_tables
from .plugins import (
    BootCase,
    ServedCase,
    boot_cases,
    boot_descriptions,
    error_lists,
    order_counts,
    order_vectors,
    plugin_cases,
    plugins_manifest,
)
from .rng import Rng
from .schema import EXHAUSTIVE_FIELD_BOUND, Schema, load
from .schemas import ensure_branded_scalars, named_sdl, schema_names
from .selections import selection_cases
from .sentry import sentry_cases
from .tables import DEFAULT_DEPTH, inject, synthesize
from .variables import variable_cases
from .wire import NODE_METHODS, WIRE_CASE_COUNT, WireCase, Write, wire_cases, wire_manifest

#: The seed a run takes when its parameters name none.
DEFAULT_SEED = 1

#: The fan out per composite level.
DEFAULT_FANOUT = 6

#: The budget per schema and mode.
DEFAULT_MAX_OPS = 5000

#: The modes a behaviour table is written in.
MODES = ("plain", "pylon")

__all__ = [
    "BootCase",
    "Case",
    "Corpus",
    "DEFAULT_DEPTH",
    "EXHAUSTIVE_FIELD_BOUND",
    "DEFAULT_FANOUT",
    "DEFAULT_MAX_OPS",
    "DEFAULT_SEED",
    "HttpCase",
    "MODES",
    "NODE_METHODS",
    "Rng",
    "Schema",
    "ServedCase",
    "WIRE_CASE_COUNT",
    "WireCase",
    "Write",
    "as_json",
    "behaviour_tables",
    "boot_cases",
    "boot_descriptions",
    "budget",
    "canonical",
    "case_id",
    "counts_of",
    "delegate_cases",
    "document_requests",
    "error_lists",
    "generate",
    "generate_named",
    "http_manifest",
    "identity_cases",
    "inject",
    "load",
    "log_case",
    "login",
    "make_case",
    "merge_requests",
    "named_sdl",
    "operations",
    "over_http",
    "patch_result_requests",
    "order_counts",
    "order_vectors",
    "plugin_cases",
    "plugins_manifest",
    "pubsub",
    "route_cases",
    "schema_names",
    "sentry_cases",
    "static_cases",
    "streams",
    "subscriptions",
    "synthesize",
    "transform_requests",
    "transport_cases",
    "upload_cases",
    "wire_cases",
    "wire_manifest",
    "zitadel",
]


def generate(
    sdl: str,
    seed: int = DEFAULT_SEED,
    depth: int = DEFAULT_DEPTH,
    fanout: int = DEFAULT_FANOUT,
    max_ops: int = DEFAULT_MAX_OPS,
    mode: str = "plain",
    name: str = "base",
    bound: int = EXHAUSTIVE_FIELD_BOUND,
) -> Corpus:
    """Returns the corpus of one schema in one mode.

    `sdl` is the schema text, `seed` decides every draw, `depth` bounds the
    documents, `fanout` caps the fields per composite level, `max_ops` is the
    budget over which a seeded sample is taken, and `mode` is `plain` or
    `pylon`. In `pylon` mode the schema text gains the scalars and the
    directive a schema of the compatibility layer declares, so that the seven bound scalars find
    their types.

    `bound` is the number of reachable fields up to which the tree enumeration
    is exhaustive. A schema above it is enumerated as the covering array of
    `docs/harness.md` section 4.2, where `fanout` decides the fields per level.

    The tables of the result are the base table, one table per error class and
    target, and one table per ordering case. The cases name the table they run
    against, and the counts say how many cases each family contributed after
    the sample.
    """
    if mode not in MODES:
        raise ValueError(f"no mode named {mode}")
    text = ensure_branded_scalars(sdl) if mode == "pylon" else sdl
    schema = load(text)
    exhaustive = schema.is_exhaustive(bound)
    rng = Rng(seed)
    base = synthesize(schema, seed, mode, depth, name=f"{mode}-{name}")
    tables = [base]
    cases: list[Case] = []
    cases.extend(selection_cases(schema, rng, base["name"], depth, fanout, exhaustive))
    cases.extend(argument_cases(schema, rng, base["name"], depth))
    cases.extend(variable_cases(schema, rng, base["name"], depth))
    cases.extend(directive_cases(schema, rng, base["name"], depth))
    cases.extend(introspection_cases(schema, rng, base["name"]))
    injected_tables, injected_cases = error_cases(schema, rng, base, depth)
    tables.extend(injected_tables)
    cases.extend(injected_cases)
    tables.extend(ordering_tables(mode))
    cases.extend(ordering_cases(mode))
    kept, sampled = budget(deduplicate(cases), max_ops, rng)
    return Corpus(
        tables=tables,
        cases=kept,
        counts=counts_of(kept),
        seed=seed,
        sampled=sampled,
        schema_digest=schema.digest,
        parameters={
            "name": name,
            "mode": mode,
            "depth": depth,
            "fanout": fanout,
            "max_ops": max_ops,
            "bound": bound,
        },
        reachable_fields=len(schema.reachable_fields()),
        exhaustive=exhaustive,
    )


def generate_named(
    name: str = "rich",
    seed: int = DEFAULT_SEED,
    depth: int = DEFAULT_DEPTH,
    fanout: int = DEFAULT_FANOUT,
    max_ops: int = DEFAULT_MAX_OPS,
    mode: str = "plain",
    bound: int = EXHAUSTIVE_FIELD_BOUND,
) -> Corpus:
    """Returns the corpus of one schema the generator knows by name.
    """
    return generate(
        named_sdl(name),
        seed=seed,
        depth=depth,
        fanout=fanout,
        max_ops=max_ops,
        mode=mode,
        name=name,
        bound=bound,
    )


def behaviour_tables(
    seed: int = DEFAULT_SEED, mode: str = "plain", depth: int = DEFAULT_DEPTH
) -> list[dict]:
    """Returns the base table of every schema the generator knows.

    A run registers these tables before it sends the cases of a corpus, and a
    notebook that only needs a schema to run a document against takes the table
    of that schema from here.
    """
    tables = []
    for name in schema_names():
        text = named_sdl(name)
        if mode == "pylon":
            text = ensure_branded_scalars(text)
        tables.append(synthesize(load(text), seed, mode, depth, name=f"{mode}-{name}"))
    return tables


def as_json(corpus: Corpus) -> dict:
    """Returns the corpus as the object a notebook stores and sends.

    The manifest comes first, the tables follow in the order a run registers
    them, and the cases follow in the order a run sends them.
    """
    return {
        "manifest": corpus.manifest(),
        "tables": corpus.tables,
        "cases": [
            {
                "id": case.id,
                "family": case.family,
                "table": case.table,
                "document": case.document,
                "variables": case.variables,
                "operationName": case.operation_name,
                "validate": case.validate,
            }
            for case in corpus.cases
        ],
    }
