# SPDX-License-Identifier: MIT
"""tests/gen: the command line that prints and writes a corpus.

The command prints the manifest of one corpus as JSON, which carries the seed,
the parameters, the digest of the schema text, the table names and the count
per family. With `--family http` it prints the counts of the HTTP families T, K
and U with the digest of their ids, and with `--family wire` the manifest of
family H, every case without its bytes. Two runs with the same arguments print
the same bytes, which is the property every recorded count rests on.

    python -m tests.gen
    python -m tests.gen --schema blog-app --mode pylon --out corpus.json
    python -m tests.gen --family wire
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from . import (
    DEFAULT_DEPTH,
    DEFAULT_FANOUT,
    DEFAULT_MAX_OPS,
    DEFAULT_SEED,
    EXHAUSTIVE_FIELD_BOUND,
    MODES,
    as_json,
    generate_named,
    http_manifest,
    schema_names,
    wire_manifest,
)


def parse_arguments(argv: list[str]) -> argparse.Namespace:
    """Reads the command line of the generator."""
    parser = argparse.ArgumentParser(prog="python -m tests.gen", description=__doc__)
    parser.add_argument("--family", default="operations", choices=["operations", "http", "wire"])
    parser.add_argument("--schema", default="rich", choices=schema_names())
    parser.add_argument("--mode", default="plain", choices=list(MODES))
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    parser.add_argument("--depth", type=int, default=DEFAULT_DEPTH)
    parser.add_argument("--fanout", type=int, default=DEFAULT_FANOUT)
    parser.add_argument("--max-ops", type=int, default=DEFAULT_MAX_OPS)
    parser.add_argument("--bound", type=int, default=EXHAUSTIVE_FIELD_BOUND)
    parser.add_argument("--out", type=Path, default=None)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    """Builds one corpus, writes it when asked and prints its manifest."""
    arguments = parse_arguments(argv)
    if arguments.family == "wire":
        print(json.dumps(wire_manifest(), indent=2, ensure_ascii=False))
        return 0
    if arguments.family == "http":
        print(json.dumps(http_manifest(), indent=2, ensure_ascii=False))
        return 0
    corpus = generate_named(
        arguments.schema,
        seed=arguments.seed,
        depth=arguments.depth,
        fanout=arguments.fanout,
        max_ops=arguments.max_ops,
        mode=arguments.mode,
        bound=arguments.bound,
    )
    if arguments.out is not None:
        arguments.out.write_text(
            json.dumps(as_json(corpus), indent=2, ensure_ascii=False) + "\n"
        )
    print(json.dumps(corpus.manifest(), indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
