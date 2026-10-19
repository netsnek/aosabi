# SPDX-License-Identifier: MIT
"""Aosabi harness: the lock of a scratch workspace held to the locks of the tree.

A notebook that builds a probe outside the committed locks writes the probe's
lock with `cargo generate-lockfile --offline`, which takes the newest version
of every package that the local cargo cache holds. The cache is shared by
every checkout of the machine and grows on its own, so a package version that
no lock of the tree names reaches a probe the day another checkout fetches it,
as `libc` 0.2.190 did while every lock of the tree holds 0.2.189. A probe lock
written on a fresh worktree then differs from the lock of the tree it
measures, and a notebook that holds the two to each other fails.

`drift` names every package of the registry in a probe lock whose version no
reference lock holds while a reference lock holds a version of the same name
that cargo takes as compatible, and the version it moves to, the highest such
one. `pin` hands each of them to a callable of the notebook, which runs
`cargo update --offline --precise <version> -p <name>@<held>` over the probe,
and reads the lock again, until no package drifts or a round moves none. The
notebook runs the cargo call itself, so `00_environment` reads every cargo
call of the committed notebooks where it stands. A package that no reference
lock names, such as a dependency of a test of a vendored crate, stays at the
version the cache gives it, since the tree has no version to hold it to.
"""

from __future__ import annotations

import tomllib
from pathlib import Path

#: The rounds `pin` runs at most. One round moves every drifting package, and
#: a second one is needed only where a moved package changes what another
#: package resolves to.
ROUNDS = 4


def registry_versions(lock) -> dict:
    """Return the versions of every package of the registry `lock` holds, by name."""
    with Path(lock).open("rb") as handle:
        packages = tomllib.load(handle).get("package", [])
    held = {}
    for package in packages:
        if package.get("source", "").startswith("registry+"):
            held.setdefault(package["name"], set()).add(package["version"])
    return held


def _release(version: str) -> tuple:
    """Return the numbers of a version without its pre release and build parts."""
    core = version.split("+", 1)[0].split("-", 1)[0]
    return tuple(int(part) for part in core.split("."))


def compatible(left: str, right: str) -> bool:
    """Tell whether cargo takes the two versions as compatible: the same major
    above 0, the same minor under major 0 above minor 0, the same patch under 0.0."""
    a, b = _release(left), _release(right)
    if a[0] != 0 or b[0] != 0:
        return a[0] == b[0]
    if a[1] != 0 or b[1] != 0:
        return a[1] == b[1]
    return a[2] == b[2]


def drift(lock, references) -> list:
    """Return (name, held, wanted) for every package of the registry in `lock`
    whose version no lock of `references` holds while one of them holds a
    compatible version of the same name, `wanted` the highest such one."""
    wanted_by_name = {}
    for reference in references:
        for name, versions in registry_versions(reference).items():
            wanted_by_name.setdefault(name, set()).update(versions)
    out = []
    for name, versions in sorted(registry_versions(lock).items()):
        known = wanted_by_name.get(name)
        if not known:
            continue
        for held in sorted(versions, key=_release):
            if held in known:
                continue
            candidates = sorted((v for v in known if compatible(v, held)), key=_release)
            if candidates:
                out.append((name, held, candidates[-1]))
    return out


def pin(lock, references, update) -> list:
    """Move every drifting package of `lock` to the version of `references`.

    `update(name, held, wanted)` runs the cargo call that moves one package
    and returns its result, whose `returncode` is 0 where cargo moved it.
    Returns one line per call, which the notebook prints, and the line of a
    package that stays because cargo refused the move.
    """
    lines = []
    for _ in range(ROUNDS):
        moving = drift(lock, references)
        if not moving:
            break
        moved = 0
        for name, held, wanted in moving:
            result = update(name, held, wanted)
            if getattr(result, "returncode", 1) == 0:
                moved += 1
                lines.append("%s %s pinned to %s, the version of the tree" % (name, held, wanted))
            else:
                text = (getattr(result, "err", "") or "").strip().splitlines()
                lines.append("%s %s stays, cargo refused %s: %s" % (name, held, wanted, text[-1] if text else "no reason"))
        if not moved:
            break
    return lines
