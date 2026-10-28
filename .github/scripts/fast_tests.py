#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Run the fast subset of the notebook matrix, the one continuous integration runs.

    python3 -I .github/scripts/fast_tests.py --python tests/.venv/bin/python
    python3 -I .github/scripts/fast_tests.py --python tests/.venv/bin/python --jobs 2
    python3 -I .github/scripts/fast_tests.py --list

The subset is a list of entries of tests/matrix.yaml. An entry belongs to it when it needs
no reference implementation, no network, no privileges and no device, runs no timer that a
loaded machine can trip, and finishes in seconds. Every entry of the subset builds what it
checks with the pinned toolchain, and some read the sizes of their builds with llvm-size and
llvm-nm, which the component llvm-tools of the toolchain provides.

The conformance notebooks compare Aosabi with graphql-js and need it at run time, so they run
in the full matrix and not here.

Every id of the subset must name exactly one entry of the matrix, and every entry must pass.
The script asks the runner for the entries the ids select before it runs them and reads the
report afterwards, so an entry that was renamed, removed or marked to be skipped fails the run
instead of shrinking the subset without a word.

The interpreter given with --python is the one of the harness environment, with the pinned
packages of tests/pyproject.toml and the kernel aosabi installed. This script itself needs
the standard library alone.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

#: The root of the tree, two levels above this file.
ROOT = Path(__file__).resolve().parents[2]

#: The runner of the notebook matrix and the matrix itself, relative to the root.
RUNNER = Path("tests") / "run.py"
MATRIX = Path("tests") / "matrix.yaml"

#: Where the runner writes the executed notebooks and its report, relative to the root.
OUT = Path("tests") / "notebooks" / "out"
REPORT = OUT / "report.json"

#: The ids of the subset, in the order of the matrix.
FAST = (
    # The packet codec of the Homa transport, its fuzzing included.
    "03_homa_packets",
    # The frame format the carriers share.
    "04_aoframe",
    # The full wire form of Homa over a synthetic link.
    "05_homa_wire_full-synthetic",
    # The frame of the LoRa carrier.
    "06_lora_frame",
    # Loss recovery of the Homa engine over seeded lossy links, without sockets.
    "22_homa_lossy-engine",
    # The size budgets of the client, of GraphTP and of the LoRa build for the ESP32-C3.
    "23_homa_budget-client",
    "23_homa_budget-graphtp",
    "23_homa_budget-lora",
    # The model of the LoRa carrier, its duty cycle and its restarts.
    "29_carrier_lora-model",
)


class SubsetError(Exception):
    """The subset and the matrix disagree, printed without a traceback."""


def only_arguments(ids) -> list[str]:
    """The --only arguments of the runner that select the entries of `ids`."""
    arguments = []
    for entry in ids:
        arguments += ["--only", entry]
    return arguments


def listed_entries(python: str) -> list[str]:
    """The ids of the entries the runner selects for the subset."""
    command = [python, str(RUNNER), "--matrix", str(MATRIX), "--list", *only_arguments(FAST)]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise SubsetError("the runner could not list the matrix:\n%s%s" % (result.stdout, result.stderr))
    return [line.split()[0] for line in result.stdout.splitlines() if line.strip()]


def check_selection(listed: list[str]) -> None:
    """Fail unless the runner selects each entry of the subset once and nothing else."""
    problems = []
    for entry in FAST:
        count = listed.count(entry)
        if count == 0:
            problems.append("%s names no entry of %s" % (entry, MATRIX))
        elif count > 1:
            problems.append("%s names %d entries of %s" % (entry, count, MATRIX))
    for entry in sorted(set(listed) - set(FAST)):
        problems.append("the ids of the subset also select %s, so one id is a part of another" % entry)
    if problems:
        raise SubsetError("the subset does not match the matrix:\n  " + "\n  ".join(problems))


def check_report() -> None:
    """Fail unless the report of the run holds every entry of the subset as passed."""
    try:
        report = json.loads((ROOT / REPORT).read_text(encoding="utf-8"))
    except (OSError, ValueError) as broken:
        raise SubsetError("the runner left no readable report at %s: %s" % (REPORT, broken)) from None
    status = {row.get("id"): row.get("status") for row in report.get("notebooks") or []}
    problems = ["%s is %s" % (entry, status.get(entry, "missing")) for entry in FAST if status.get(entry) != "passed"]
    if problems:
        raise SubsetError("the run did not pass the whole subset:\n  " + "\n  ".join(problems))


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Run the fast subset of the notebook matrix.")
    parser.add_argument("--python", help="The interpreter of the harness environment.")
    parser.add_argument("--jobs", type=int, default=1, help="The entries the runner runs at once, 1 by default.")
    parser.add_argument("--list", action="store_true", help="Print the ids of the subset and exit.")
    args = parser.parse_args(argv)

    if args.list:
        for entry in FAST:
            print(entry)
        return 0
    if not args.python:
        parser.error("--python must name the interpreter of the harness environment")

    try:
        check_selection(listed_entries(args.python))
        (ROOT / REPORT).unlink(missing_ok=True)
        print("fast_tests: %d entries of %s" % (len(FAST), MATRIX), flush=True)
        command = [args.python, str(RUNNER), "--matrix", str(MATRIX), "--out", str(OUT), "--report", str(REPORT),
                   "--jobs", str(args.jobs), *only_arguments(FAST)]
        status = subprocess.run(command, cwd=ROOT, check=False).returncode
        check_report()
    except SubsetError as failure:
        print("fast_tests: %s" % failure, file=sys.stderr)
        return 1
    return status


if __name__ == "__main__":
    raise SystemExit(main())
