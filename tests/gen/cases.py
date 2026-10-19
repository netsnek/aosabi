# SPDX-License-Identifier: MIT
"""tests/gen: the case, its identity, the corpus and the budget.

A case is one request a run sends to both sides, and its identity is the
SHA-256 of the canonical form of that request, so the same request carries the
same name in every run and a golden directory is found by it. The budget cuts a
corpus that is over `max_ops` down with a seeded sample, and it keeps every
case of the introspection, error and ordering families, because those are the
cases a golden is stored for.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field

#: The families a sample never drops. Their cases carry a golden of their own,
#: so dropping one would drop a recorded comparison.
KEPT_PREFIXES = ("E", "I", "O")


@dataclass(frozen=True)
class Case:
    """One generated request, with the family and the table it belongs to."""

    id: str
    family: str
    table: str
    document: str
    variables: dict | None
    operation_name: str | None
    validate: bool


def request_of(
    table: str,
    document: str,
    variables: dict | None,
    operation_name: str | None,
    validate: bool,
) -> dict:
    """Returns the request object a case stands for, with the keys of the protocol."""
    return {
        "document": document,
        "operationName": operation_name,
        "table": table,
        "validate": validate,
        "variables": variables,
    }


def canonical(request: dict) -> bytes:
    """Returns the bytes a case identity is taken over.

    The keys are sorted, the separators carry no space and non ASCII text stays
    as it is, so the same request gives the same bytes on every machine.
    """
    return json.dumps(
        request, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode()


def case_id(request: dict) -> str:
    """Returns the SHA-256 hex digest of the canonical request."""
    return hashlib.sha256(canonical(request)).hexdigest()


def make_case(
    family: str,
    table: str,
    document: str,
    variables: dict | None = None,
    operation_name: str | None = None,
    validate: bool = True,
) -> Case:
    """Builds a case and gives it the identity of its request."""
    request = request_of(table, document, variables, operation_name, validate)
    return Case(
        id=case_id(request),
        family=family,
        table=table,
        document=document,
        variables=variables,
        operation_name=operation_name,
        validate=validate,
    )


@dataclass(frozen=True)
class Corpus:
    """The tables a run registers, the cases it sends and the counts it records."""

    tables: list[dict]
    cases: list[Case]
    counts: dict[str, int]
    seed: int
    sampled: bool
    schema_digest: str = ""
    parameters: dict = field(default_factory=dict)
    reachable_fields: int = 0
    exhaustive: bool = True

    @property
    def total(self) -> int:
        """Returns the number of cases the corpus carries."""
        return len(self.cases)

    def manifest(self) -> dict:
        """Returns the record a run stores beside its goldens.

        It carries the seed, the parameters the corpus was built with, the
        digest of the schema text, the number of reachable fields with the
        enumeration that number chose, the table names and the count per
        family, so a later run proves that its corpus did not shrink.
        """
        return {
            "version": 1,
            "seed": self.seed,
            "sampled": self.sampled,
            "parameters": dict(self.parameters),
            "schema_sha256": self.schema_digest,
            "reachable_fields": self.reachable_fields,
            "exhaustive": self.exhaustive,
            "tables": [table["name"] for table in self.tables],
            "counts": dict(self.counts),
            "total": self.total,
        }


def counts_of(cases: list[Case]) -> dict[str, int]:
    """Returns the number of cases per family, in the order the families appear."""
    counts: dict[str, int] = {}
    for case in cases:
        counts[case.family] = counts.get(case.family, 0) + 1
    return counts


def deduplicate(cases: list[Case]) -> list[Case]:
    """Drops a case whose identity another case already carries, keeping the first."""
    seen: set[str] = set()
    kept: list[Case] = []
    for case in cases:
        if case.id in seen:
            continue
        seen.add(case.id)
        kept.append(case)
    return kept


def is_kept(case: Case) -> bool:
    """Says whether a sample has to keep this case whatever the budget is."""
    return case.family.startswith(KEPT_PREFIXES)


def budget(cases: list[Case], max_ops: int, rng) -> tuple[list[Case], bool]:
    """Cuts a corpus down to `max_ops` cases with a seeded sample.

    The cases of the kept families stay whole and the rest is sampled in its
    original order, so the corpus reads in family order after the cut. The
    second value says whether anything was dropped, which a run records as
    `sampled`.
    """
    if len(cases) <= max_ops:
        return list(cases), False
    kept = [case for case in cases if is_kept(case)]
    rest = [index for index, case in enumerate(cases) if not is_kept(case)]
    room = max_ops - len(kept)
    if room <= 0:
        return kept, True
    chosen = set(rng.pick("corpus.sample", rest, room))
    sampled = [case for index, case in enumerate(cases) if is_kept(case) or index in chosen]
    return sampled, True
