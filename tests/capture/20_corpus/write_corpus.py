# SPDX-License-Identifier: MIT
"""tests/capture/20_corpus: writes the committed corpus of the executor notebook.

One file per schema and per mode holds the behaviour tables and the operations
`tests/notebooks/20_execute.ipynb` sends to both sides. The generator is seeded
and deterministic, so the committed file is a record of what one call produced
and the notebook regenerates it and compares the bytes before it judges
anything, which keeps a stale corpus and a defect of the executor apart.

The call that produced the committed files, from the repository root:

    tests/.venv/bin/python tests/capture/20_corpus/write_corpus.py

The flags exist for a reader who wants another corpus in the output directory
of a notebook run. A committed file is written by the call above and by no
other.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

#: The file name of the ledger that carries every manifest of one run.
MANIFEST_NAME = "manifest.json"

#: The version of the ledger format, which the notebook reads back.
LEDGER_VERSION = 1


def repository_root(start: Path) -> Path:
    """Returns the directory that carries the cargo workspace manifest.

    The walk starts at `start` and climbs, so the script runs from the
    repository root and from its own directory alike.
    """
    for base in [start, *start.parents]:
        if (base / "Cargo.toml").is_file() and (base / "tests").is_dir():
            return base
    raise SystemExit("no cargo workspace above %s" % start)


def corpus_name(name: str, mode: str) -> str:
    """Returns the file name of one corpus."""
    return "%s-%s.json" % (name, mode)


def write_one(out: Path, name: str, mode: str, seed: int, depth: int, fanout: int, max_ops: int, bound: int) -> dict:
    """Writes one corpus and returns its manifest.

    The text is compact and ASCII, so a file is the same bytes on every
    machine and a reader diffs it line by line.
    """
    from tests.gen import as_json, generate_named

    corpus = generate_named(
        name, seed=seed, depth=depth, fanout=fanout, max_ops=max_ops, mode=mode, bound=bound
    )
    payload = as_json(corpus)
    text = json.dumps(payload, ensure_ascii=True, separators=(",", ":"), sort_keys=False)
    (out / corpus_name(name, mode)).write_text(text + "\n", encoding="utf-8")
    return payload["manifest"]


def main(argv: list[str] | None = None) -> int:
    """Writes every corpus of the run and the ledger beside them."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, default=None, help="the directory the files land in")
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--depth", type=int, default=3)
    parser.add_argument("--fanout", type=int, default=6)
    parser.add_argument("--max-ops", type=int, default=5000)
    parser.add_argument("--bound", type=int, default=64)
    args = parser.parse_args(argv)

    here = Path(__file__).resolve().parent
    root = repository_root(here)
    sys.path.insert(0, str(root))
    from tests.gen import MODES, schema_names

    out = args.out or here
    out.mkdir(parents=True, exist_ok=True)

    ledger = {
        "version": LEDGER_VERSION,
        "written_by": "tests/capture/20_corpus/write_corpus.py",
        "call": (
            "tests/.venv/bin/python tests/capture/20_corpus/write_corpus.py"
            " --seed %d --depth %d --fanout %d --max-ops %d --bound %d"
            % (args.seed, args.depth, args.fanout, args.max_ops, args.bound)
        ),
        "corpora": [],
    }
    for name in schema_names():
        for mode in MODES:
            manifest = write_one(
                out, name, mode, args.seed, args.depth, args.fanout, args.max_ops, args.bound
            )
            ledger["corpora"].append({"schema": name, "mode": mode, "manifest": manifest})
            print(
                "%-16s %-5s %5d cases  %3d tables  sampled %s"
                % (name, mode, manifest["total"], len(manifest["tables"]), manifest["sampled"])
            )

    text = json.dumps(ledger, ensure_ascii=True, indent=2, sort_keys=False)
    (out / MANIFEST_NAME).write_text(text + "\n", encoding="utf-8")
    total = sum(entry["manifest"]["total"] for entry in ledger["corpora"])
    print("%d corpora, %d cases" % (len(ledger["corpora"]), total))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
