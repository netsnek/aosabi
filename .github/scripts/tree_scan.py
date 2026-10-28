#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Scan the tree for private keys and access tokens.

    python3 -I .github/scripts/tree_scan.py
    python3 -I .github/scripts/tree_scan.py --root path/to/tree

A secret that reaches a public repository stays public, whatever commit removes it later, so
the scan fails on every file that holds one. It reads every text file of the tree and looks for
two things.

1. A private key, which is a line that opens the PEM block of a private key or of a PGP private
   key, followed by the base64 of its body. A line that only names the header, as a check of a
   key does, holds no key.
2. An access token of a service whose tokens carry a fixed prefix, which are GitHub, GitLab,
   npm, PyPI, Docker Hub, Slack, Stripe, Google, AWS, Sentry, DigitalOcean, Hugging Face,
   Anthropic and OpenAI.

A test that needs a key generates it when it runs, so no key is ever committed, and no file
is exempt. A finding prints its path, its line and its kind, never the secret itself.

The files are the ones git tracks when the root is a git work tree, and otherwise every file
below the root outside the directories of build output. The exit status is 1 when the scan
finds anything.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath

#: The root of the tree, two levels above this file.
ROOT = Path(__file__).resolve().parents[2]

#: The bytes read to tell a binary file, which holds a NUL among them.
HEAD_BYTES = 8192

#: The header line of the PEM block of a private key, of any algorithm, and of a PGP private key.
KEY_HEADER = re.compile(r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----")
#: The characters after a header that the scan reads to find the body of the key.
KEY_WINDOW = 512
#: A line of the body of a key, the base64 of at least 40 characters.
KEY_BODY = re.compile(r"[A-Za-z0-9+/]{40,}={0,2}")
#: A header line inside the block, such as Proc-Type, DEK-Info, Comment or Version.
KEY_FIELD = re.compile(r"[A-Za-z][A-Za-z-]*:.*")
#: What surrounds a line of a key held in a string of source code.
KEY_QUOTES = " \t\"'`,+;()[]{}"

#: The access tokens, each with the fixed prefixes it starts with and its whole form. The scan
#: runs a pattern only on a text that holds one of its prefixes.
TOKENS = (
    ("a GitHub token", ("ghp_", "gho_", "ghu_", "ghs_", "ghr_"), r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36}\b"),
    ("a GitHub fine grained token", ("github_pat_",), r"\bgithub_pat_[A-Za-z0-9_]{70,}"),
    ("a GitLab token", ("glpat-",), r"\bglpat-[A-Za-z0-9_-]{20,}"),
    ("an npm token", ("npm_",), r"\bnpm_[A-Za-z0-9]{36}\b"),
    ("a PyPI token", ("pypi-AgEI",), r"\bpypi-AgEIcHlwaS5vcmc[A-Za-z0-9_-]{50,}"),
    ("a Docker Hub token", ("dckr_pat_",), r"\bdckr_pat_[A-Za-z0-9_-]{27,}"),
    ("a Slack token", ("xox",), r"\bxox[abposr]-[0-9A-Za-z-]{10,}"),
    ("a Slack webhook", ("hooks.slack.com",), r"hooks\.slack\.com/services/T[A-Z0-9]+/B[A-Z0-9]+/[A-Za-z0-9]{20,}"),
    ("a Stripe live key", ("_live_",), r"\b(?:sk|rk)_live_[0-9A-Za-z]{20,}"),
    ("a Google API key", ("AIza",), r"\bAIza[0-9A-Za-z_-]{35}\b"),
    ("an AWS access key", ("AKIA", "ASIA"), r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
    ("a Sentry token", ("sntry",), r"\bsntry[us]_[A-Za-z0-9+/=_]{40,}"),
    ("a DigitalOcean token", ("_v1_",), r"\bdo[opr]_v1_[a-f0-9]{64}\b"),
    ("a Hugging Face token", ("hf_",), r"\bhf_[A-Za-z]{34}\b"),
    ("an Anthropic key", ("sk-ant-",), r"\bsk-ant-(?:api|admin)[0-9]{2}-[A-Za-z0-9_-]{80,}"),
    ("an OpenAI key", ("sk-proj-", "sk-svcacct-", "sk-admin-"), r"\bsk-(?:proj|svcacct|admin)-[A-Za-z0-9_-]{40,}"),
)
TOKEN_PATTERNS = [(kind, prefixes, re.compile(pattern)) for kind, prefixes, pattern in TOKENS]

#: Build output and dependency trees, skipped when the root is no git work tree.
SKIP_DIRS = {".git", "target", "node_modules", ".venv", "__pycache__"}
SKIP_PATHS = {"tests/notebooks/out", "tests/.cache"}


class Failure(Exception):
    """A failure of the scan itself, printed without a traceback."""


def tracked_files(root: Path) -> list[PurePosixPath]:
    """The files of the tree as paths relative to the root."""
    if (root / ".git").exists():
        result = subprocess.run(["git", "-C", str(root), "ls-files", "-z"], capture_output=True, check=False)
        if result.returncode != 0:
            raise Failure("git ls-files failed in %s: %s" % (root, result.stderr.decode(errors="replace").strip()))
        names = [name for name in result.stdout.decode("utf-8", errors="surrogateescape").split("\0") if name]
        return sorted(PurePosixPath(name) for name in names if not os.path.islink(root / name))
    files = []
    for directory, subdirectories, names in os.walk(root):
        relative = PurePosixPath(Path(directory).relative_to(root).as_posix())
        subdirectories[:] = sorted(
            name for name in subdirectories
            if name not in SKIP_DIRS and str(relative / name) not in SKIP_PATHS
        )
        for name in names:
            if not os.path.islink(os.path.join(directory, name)):
                files.append(relative / name if str(relative) != "." else PurePosixPath(name))
    return sorted(files)


def key_follows(text: str, end: int) -> bool:
    """Whether the base64 of a key body follows a header that ends at `end`."""
    window = text[end : end + KEY_WINDOW].replace("\\r", "\n").replace("\\n", "\n")
    for line in window.split("\n"):
        line = line.strip(KEY_QUOTES).rstrip("\\").strip(KEY_QUOTES)
        if not line or KEY_FIELD.fullmatch(line):
            continue
        return bool(KEY_BODY.fullmatch(line))
    return False


def line_of(text: str, offset: int) -> int:
    """The line number of a position in `text`, counted from 1."""
    return text.count("\n", 0, offset) + 1


def scan(text: str) -> list[tuple[int, str]]:
    """The findings in one text, each a line number and a kind."""
    findings = []
    if "PRIVATE KEY" in text:
        for match in KEY_HEADER.finditer(text):
            if key_follows(text, match.end()):
                findings.append((line_of(text, match.start()), "a private key, %s" % match.group(0).strip("-")))
    for kind, prefixes, pattern in TOKEN_PATTERNS:
        if not any(prefix in text for prefix in prefixes):
            continue
        for match in pattern.finditer(text):
            findings.append((line_of(text, match.start()), kind))
    return sorted(findings)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Scan the tree for private keys and access tokens.")
    parser.add_argument("--root", type=Path, default=ROOT, help="The root of the tree, by default the repository.")
    args = parser.parse_args(argv)
    root = args.root.resolve()

    try:
        files = tracked_files(root)
    except Failure as broken:
        print("tree_scan: %s" % broken, file=sys.stderr)
        return 1

    read = 0
    findings = []
    for path in files:
        try:
            data = (root / path).read_bytes()
        except OSError as broken:
            findings.append("%s: the file cannot be read: %s" % (path, broken))
            continue
        if b"\0" in data[:HEAD_BYTES]:
            continue
        read += 1
        for line, kind in scan(data.decode("utf-8", errors="replace")):
            findings.append("%s:%d: %s" % (path, line, kind))

    for finding in findings:
        print("tree_scan: %s" % finding)
    print("tree_scan: files %d, text files read %d, findings %d" % (len(files), read, len(findings)))
    return 1 if findings else 0


if __name__ == "__main__":
    raise SystemExit(main())
