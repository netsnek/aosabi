#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Check that every file of the tree states its license and that every license has its text.

    python3 -I .github/scripts/license_lint.py
    python3 -I .github/scripts/license_lint.py --root path/to/tree

The lint follows the conventions of the Linux kernel. A file names its license in its tag,
an SPDX-License-Identifier line on the first line of the file or on the second under a #!
line, and LICENSES/ holds the text of every license a file names. A tag anywhere else, such
as an example in a document, is no tag of the file. The rules are these.

1. LICENSE at the root holds the MIT License, which GitHub reads as the license of the
   repository.
2. Every tag holds a valid SPDX expression, license identifiers joined by AND, OR and WITH, with
   the operators in capitals.
3. Outside third_party/, every license an expression names has its text under LICENSES/. A
   text declares its identifier in a Valid-License-Identifier line, or by its file name when
   it holds no such line.
4. Every license field of a Cargo.toml or a package.json outside third_party/ follows rules 2
   and 3, and every license-file field names a file that exists.
5. Every package under third_party/ keeps a license file of its own.
6. Every source file of the shipped code carries a tag. The shipped code is the Rust of
   every member of the cargo workspace, the Python of tools/ and .github/, and the JavaScript
   and TypeScript of every npm package outside tests/ and third_party/, declaration files
   apart. A file that cannot carry the tag, such as a text kept byte for byte, takes it from
   a file beside it whose name adds .license, as REUSE does.
7. NOTICE exists at the root when a file outside third_party/ names Apache-2.0.

The files are the ones git tracks when the root is a git work tree, and otherwise every file
below the root outside the directories of build output. Every failure is printed with its
path, and the exit status is 1 when any rule fails.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path, PurePosixPath

#: The root of the tree, two levels above this file.
ROOT = Path(__file__).resolve().parents[2]

#: The lines read of a file, enough for its tag and for the tag of a REUSE file beside it.
HEAD_LINES = 15
#: The bytes read to find the tag and to tell a binary file, which holds a NUL among them.
HEAD_BYTES = 8192

#: A tag counts when it opens a line, after the comment marker of the language at most, so a
#: sentence that mentions the tag in prose is no tag.
TAG_LINE = re.compile(
    r"^\s*(?://[/!]?|/\*+|\*|#+|--|<!--|;+|%+|\{#|\(\*|\.\.)?\s*SPDX-License-Identifier:"
    r"(?P<expression>.*?)\s*(?:\*/|-->|#\}|\*\))?\s*$"
)
#: The tokens of an expression.
TOKEN = re.compile(r"\(|\)|[^\s()]+")
#: A license identifier, an exception identifier, or a reference to a license of the project.
IDENTIFIER = re.compile(r"(?:DocumentRef-[A-Za-z0-9.-]+:)?(?:LicenseRef-)?[A-Za-z0-9.-]+\+?")
OPERATORS = ("AND", "OR", "WITH")

#: How a text under LICENSES/ declares what it is.
VALID_LICENSE = re.compile(r"^Valid-License-Identifier:\s*(\S+)\s*$", re.M)
VALID_EXCEPTION = re.compile(r"^SPDX-Exception-Identifier:\s*(\S+)\s*$", re.M)

#: The sentences that make LICENSE the MIT License.
MIT_SENTENCES = (
    "MIT License",
    "Permission is hereby granted, free of charge",
    'THE SOFTWARE IS PROVIDED "AS IS"',
)

#: The names of a license file of a package under third_party/.
LICENSE_FILE = re.compile(r"^(?:LICEN[CS]E|COPYING|UNLICENSE)(?:[-._].*)?$", re.I)

#: The suffixes of the Rust, Python, JavaScript and TypeScript sources rule 6 covers.
RUST = (".rs",)
PYTHON = (".py",)
SCRIPT = (".js", ".mjs", ".cjs", ".ts", ".mts", ".cts")
DECLARATION = (".d.ts", ".d.mts", ".d.cts")

#: Build output and dependency trees, skipped when the root is no git work tree.
SKIP_DIRS = {".git", "target", "node_modules", ".venv", "__pycache__"}
SKIP_PATHS = {"tests/notebooks/out", "tests/.cache"}


class Failure(Exception):
    """A failure of the lint itself, printed without a traceback."""


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


def head(root: Path, path: PurePosixPath) -> list[str] | None:
    """The first lines of a text file, or None for a binary file."""
    try:
        with open(root / path, "rb") as stream:
            data = stream.read(HEAD_BYTES)
    except OSError:
        return None
    if b"\0" in data:
        return None
    return data.decode("utf-8", errors="replace").splitlines()[:HEAD_LINES]


def find_tag(lines: list[str], placed: bool = True) -> str | None:
    """The expression of the tag among the first `lines` of a file, or None.

    With `placed`, the tag counts on the first line alone, or on the second under a #! line,
    which is the placement of the kernel. Without it, as for a REUSE file beside another file,
    any of the lines may hold it.
    """
    if placed:
        lines = lines[1:2] if lines and lines[0].startswith("#!") else lines[:1]
    for line in lines:
        match = TAG_LINE.match(line)
        if match:
            return match.group("expression").strip()
    return None


def parse_expression(text: str) -> tuple[list[str], list[str]]:
    """The licenses and exceptions of an SPDX expression, or ValueError when it is not one."""
    tokens = TOKEN.findall(text)
    if not tokens:
        raise ValueError("the expression is empty")
    for token in tokens:
        if token.lower() in ("and", "or", "with") and token not in OPERATORS:
            raise ValueError("the operator %r must be written in capitals" % token)
    licenses, exceptions = [], []
    position = 0

    def peek():
        return tokens[position] if position < len(tokens) else None

    def take():
        nonlocal position
        if position >= len(tokens):
            raise ValueError("the expression ends early")
        position += 1
        return tokens[position - 1]

    def atom():
        token = take()
        if token == "(":
            disjunction()
            if take() != ")":
                raise ValueError("a parenthesis is not closed")
            return
        if token in OPERATORS or token == ")" or not IDENTIFIER.fullmatch(token):
            raise ValueError("%r is no license identifier" % token)
        licenses.append(token[:-1] if token.endswith("+") else token)

    def exception():
        atom()
        if peek() == "WITH":
            take()
            token = take()
            if token in OPERATORS or token in ("(", ")") or not IDENTIFIER.fullmatch(token):
                raise ValueError("%r is no exception identifier" % token)
            exceptions.append(token)

    def conjunction():
        exception()
        while peek() == "AND":
            take()
            exception()

    def disjunction():
        conjunction()
        while peek() == "OR":
            take()
            conjunction()

    disjunction()
    if position != len(tokens):
        raise ValueError("%r follows a complete expression" % tokens[position])
    return licenses, exceptions


def license_texts(root: Path, files: list[PurePosixPath]) -> tuple[dict, dict]:
    """The licenses and exceptions whose texts lie under LICENSES/, each with its file."""
    licenses, exceptions = {}, {}
    for path in files:
        if path.parts[0] != "LICENSES" or len(path.parts) < 2:
            continue
        try:
            text = (root / path).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        declared = VALID_LICENSE.findall(text)
        excepted = VALID_EXCEPTION.findall(text)
        if not declared and not excepted:
            name = path.name[:-4] if path.name.endswith(".txt") else path.name
            if "exceptions" in path.parts[1:-1]:
                excepted = [name]
            else:
                declared = [name]
        for identifier in declared:
            licenses.setdefault(identifier, path)
        for identifier in excepted:
            exceptions.setdefault(identifier, path)
    return licenses, exceptions


def workspace_members(root: Path) -> list[PurePosixPath]:
    """The directories of the members of the cargo workspace at the root."""
    manifest = root / "Cargo.toml"
    try:
        workspace = tomllib.loads(manifest.read_text(encoding="utf-8")).get("workspace") or {}
    except (OSError, ValueError) as broken:
        raise Failure("the workspace manifest %s cannot be read: %s" % (manifest, broken)) from None
    excluded = {PurePosixPath(entry) for entry in workspace.get("exclude") or []}
    members = set()
    for pattern in workspace.get("members") or []:
        matches = sorted(glob.glob(str(root / pattern)))
        if not matches:
            raise Failure("the workspace member %r of Cargo.toml matches no directory" % pattern)
        for match in matches:
            relative = PurePosixPath(Path(match).relative_to(root).as_posix())
            if (Path(match) / "Cargo.toml").is_file() and relative not in excluded:
                members.add(relative)
    if not members:
        raise Failure("Cargo.toml at the root names no workspace member")
    return sorted(members)


def under(path: PurePosixPath, directory: PurePosixPath) -> bool:
    """Whether `path` lies below `directory`."""
    return path.parts[: len(directory.parts)] == directory.parts


def shipped_sources(root: Path, files: list[PurePosixPath]) -> set[PurePosixPath]:
    """The files rule 6 covers."""
    covered = set()
    for member in workspace_members(root):
        covered.update(path for path in files if path.suffix in RUST and under(path, member))
    for directory in (PurePosixPath("tools"), PurePosixPath(".github")):
        covered.update(path for path in files if path.suffix in PYTHON and under(path, directory))
    packages = [
        path.parent for path in files
        if path.name == "package.json" and path.parts[0] not in ("tests", "third_party") and "node_modules" not in path.parts
    ]
    for package in packages:
        covered.update(
            path for path in files
            if under(path, package) and path.name.endswith(SCRIPT) and not path.name.endswith(DECLARATION)
        )
    return covered


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Check the licenses of the tree.")
    parser.add_argument("--root", type=Path, default=ROOT, help="The root of the tree, by default the repository.")
    args = parser.parse_args(argv)
    root = args.root.resolve()
    failures = []

    def fail(path, message):
        failures.append("%s: %s" % (path, message))

    try:
        files = tracked_files(root)
        shipped = shipped_sources(root, files)
    except Failure as broken:
        print("license_lint: %s" % broken, file=sys.stderr)
        return 1
    known, known_exceptions = license_texts(root, files)
    present = set(files)

    # Rule 1.
    try:
        text = (root / "LICENSE").read_text(encoding="utf-8", errors="replace")
        for sentence in MIT_SENTENCES:
            if sentence not in text:
                fail("LICENSE", "rule 1: the MIT License lacks the sentence %r" % sentence)
    except OSError:
        fail("LICENSE", "rule 1: the root holds no LICENSE")

    # Rules 2, 3 and 6, and what rule 7 needs.
    tagged = 0
    apache = []

    def check_expression(path, expression, origin):
        try:
            licenses, exceptions = parse_expression(expression)
        except ValueError as broken:
            fail(path, "rule 2: %s %r is no SPDX expression, %s" % (origin, expression, broken))
            return
        if path.parts[0] == "third_party":
            return
        for identifier in licenses:
            if identifier not in known:
                fail(path, "rule 3: %s names %s, whose text is not under LICENSES/" % (origin, identifier))
            if identifier.startswith("Apache-2.0"):
                apache.append(path)
        for identifier in exceptions:
            if identifier not in known_exceptions:
                fail(path, "rule 3: %s names the exception %s, whose text is not under LICENSES/" % (origin, identifier))

    for path in files:
        # A text under LICENSES/ shows the tag of its license as an example of its use.
        if path.parts[0] == "LICENSES":
            continue
        lines = head(root, path)
        expression = find_tag(lines) if lines is not None else None
        if expression is not None:
            tagged += 1
            check_expression(path, expression, "the tag")
        elif path in shipped:
            sidecar = PurePosixPath(str(path) + ".license")
            sidecar_lines = head(root, sidecar) if sidecar in present else None
            if sidecar_lines is not None and find_tag(sidecar_lines, placed=False) is not None:
                continue
            fail(path, "rule 6: the file carries no tag on its first line, or on its second under #!")

    # Rule 4.
    for path in files:
        if path.parts[0] == "third_party" or "node_modules" in path.parts:
            continue
        if path.name == "Cargo.toml":
            try:
                manifest = tomllib.loads((root / path).read_text(encoding="utf-8"))
            except (OSError, ValueError) as broken:
                fail(path, "rule 4: the manifest does not parse: %s" % broken)
                continue
            for table, origin in ((manifest.get("package") or {}, "the field license"),
                                  ((manifest.get("workspace") or {}).get("package") or {}, "the workspace license")):
                value = table.get("license")
                if isinstance(value, str):
                    check_expression(path, value, origin)
                elif value is not None and value != {"workspace": True}:
                    fail(path, "rule 4: %s is neither a string nor inherited from the workspace" % origin)
                license_file = table.get("license-file")
                if isinstance(license_file, str) and not (root / path.parent / license_file).is_file():
                    fail(path, "rule 4: the field license-file names %s, which does not exist" % license_file)
        elif path.name == "package.json":
            try:
                package = json.loads((root / path).read_text(encoding="utf-8"))
            except (OSError, ValueError) as broken:
                fail(path, "rule 4: the package does not parse: %s" % broken)
                continue
            value = package.get("license") if isinstance(package, dict) else None
            if value is None:
                continue
            if not isinstance(value, str):
                fail(path, "rule 4: the field license is not a string")
            elif value.startswith("SEE LICENSE IN "):
                if not (root / path.parent / value[len("SEE LICENSE IN "):]).is_file():
                    fail(path, "rule 4: the field license points to a file that does not exist")
            else:
                check_expression(path, value, "the field license")

    # Rule 5.
    for path in files:
        if path.parts[0] != "third_party" or path.name not in ("Cargo.toml", "package.json"):
            continue
        if "node_modules" in path.parts:
            continue
        if not any(other.parent == path.parent and LICENSE_FILE.match(other.name) for other in files):
            fail(path.parent, "rule 5: the package under third_party/ keeps no license file of its own")

    # Rule 7.
    if apache and not (root / "NOTICE").is_file():
        fail("NOTICE", "rule 7: %s names Apache-2.0 and the root holds no NOTICE" % apache[0])

    for line in failures:
        print("license_lint: %s" % line)
    print("license_lint: files %d, with a tag %d, shipped sources %d, license texts %d, failures %d"
          % (len(files), tagged, len(shipped), len(known), len(failures)))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
