#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Clones the oracles of the conformance suites at their pinned commits.

    python3 tests/fetch_oracles.py            # every oracle of tests/oracles.toml
    python3 tests/fetch_oracles.py graphql-js # one oracle by its name
    python3 tests/fetch_oracles.py --list     # the pins and whether each is present

The suites compare Aosabi with a reference implementation byte for byte, and tests/oracles.toml
says what each oracle is the oracle of. Each one is a git repository at the commit tests/oracles.toml
pins, cloned into the folder the notebooks read, which git ignores. A folder that already holds the
pinned commit is left as it is, and a folder that holds anything else stops the script, so a local
change in an oracle is never lost.

The script only fetches. Installing the packages of an oracle, which the TypeScript side of a suite
needs, is described in section 5 of Documentation/guides/conformance.md.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PINS = ROOT / "tests" / "oracles.toml"


def git(*args: str, cwd: Path | None = None, check: bool = True) -> subprocess.CompletedProcess:
    """Runs git with the arguments given and returns the finished process."""
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True, check=check)


def head(path: Path) -> str | None:
    """The commit checked out in `path`, or None when it is no git repository."""
    if not (path / ".git").exists():
        return None
    done = git("rev-parse", "HEAD", cwd=path, check=False)
    return done.stdout.strip() if done.returncode == 0 else None


def fetch(oracle: dict) -> None:
    """Clones one oracle at its pinned commit into its folder."""
    path = ROOT / oracle["path"]
    commit = oracle["commit"]
    present = head(path)
    if present == commit:
        print("%s: present at %s" % (oracle["name"], commit[:12]))
        return
    if path.exists() and any(path.iterdir()):
        sys.exit("%s: %s holds something other than %s, move it away first" % (oracle["name"], oracle["path"], commit[:12]))
    path.mkdir(parents=True, exist_ok=True)
    git("init", "--quiet", cwd=path)
    git("remote", "add", "origin", oracle["url"], cwd=path)
    git("fetch", "--quiet", "--depth", "1", "origin", commit, cwd=path)
    git("checkout", "--quiet", "--detach", "FETCH_HEAD", cwd=path)
    if head(path) != commit:
        sys.exit("%s: the fetched commit is not %s" % (oracle["name"], commit))
    print("%s: fetched %s into %s" % (oracle["name"], commit[:12], oracle["path"]))


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Clone the oracles of the conformance suites at their pinned commits.")
    parser.add_argument("names", nargs="*", help="The oracles to fetch, all of them when none is named")
    parser.add_argument("--list", action="store_true", help="Print the pins and whether each oracle is present")
    args = parser.parse_args(argv)

    oracles = tomllib.loads(PINS.read_text(encoding="utf-8")).get("oracle", [])
    known = {oracle["name"]: oracle for oracle in oracles}
    unknown = [name for name in args.names if name not in known]
    if unknown:
        parser.error("unknown oracle %s, the pins name %s" % (", ".join(unknown), ", ".join(known)))
    chosen = [known[name] for name in args.names] or oracles
    for oracle in chosen:
        if args.list:
            state = "present" if head(ROOT / oracle["path"]) == oracle["commit"] else "absent"
            print("%-12s %s %s %s" % (oracle["name"], oracle["commit"][:12], oracle["path"], state))
        else:
            fetch(oracle)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
