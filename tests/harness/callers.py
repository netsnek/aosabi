# SPDX-License-Identifier: MIT
"""Aosabi harness: the callers a served identity fixture is driven as.

`30_http_tier1` runs its generated families over the schema of
`use-identity-app` once per caller, so the answers under an anonymous request,
under an admin, under an editor and under a principal of another tenant are
all compared. A caller is the list of the trusted headers the provider of the
fixture reads, `x-user-id`, `x-org`, `x-role` and `x-perm`, in the order they
are sent, and the anonymous caller sends none. Every value is a neutral test
string, `u1`, `orgA` and the like, and names no customer.
"""

from __future__ import annotations

#: The four callers of the plan, each the header fields it sends in order.
CALLERS = {
    "anonymous": (),
    "u1": (
        ("x-user-id", "u1"),
        ("x-org", "orgA"),
        ("x-role", "admin,crm,billing"),
        ("x-perm", "crm:export,admin:secure"),
    ),
    "u2": (
        ("x-user-id", "u2"),
        ("x-role", "editor"),
    ),
    "u3": (
        ("x-user-id", "u3"),
        ("x-org", "orgB"),
        ("x-role", "billing"),
    ),
}

#: The callers each fixture is driven as, the anonymous caller alone for every other one.
FIXTURE_CALLERS = {
    "use-identity-app": ("anonymous", "u1", "u2", "u3"),
}


def callers_of(fixture: str) -> dict:
    """The callers `fixture` is driven as, each name mapped to its header fields."""
    return {name: CALLERS[name] for name in FIXTURE_CALLERS.get(fixture, ("anonymous",))}


def with_caller(headers, caller: str) -> tuple:
    """`headers` followed by the fields of `caller`, the order a request carries them in."""
    return tuple(headers) + CALLERS[caller]
