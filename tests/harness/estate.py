# SPDX-License-Identifier: MIT
"""Aosabi harness: the reader of the schemas and the goldens of the estate
outside the repository.

The ten schemas of the estate are read from estate_schemas_dir, their goldens
from estate_goldens_dir and the digests of a run on the other architecture from
estate_recorded_dir, and a notebook prints repository names, counts, digests
and timings and nothing finer. A path outside the repository prints by its last
component only.

Nothing here returns a type name, a field name or a line of a schema. A reader
that needs the bytes, such as the generator it hands them to, takes the path
of `Schema` and passes it on without printing it.
"""

from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass
from pathlib import Path

from .paths import find_root

SCHEMA_FILE = "schema.graphql"

# The pins of docs/harness.md section 8, which sit in the parameters cell of
# each of the three notebooks and are repeated here as the defaults a helper
# checks against.
EXPECTED_SCHEMAS = 10
EXPECTED_EGO_VIEWS = 361
EXPECTED_LARGEST_BOXES = 127
EXPECTED_LARGEST_TYPES = 176

# The directories one estate directory may hold. Ten are expected, and the cap
# keeps a wrong parameter, such as a home directory, from a long walk.
MAX_REPOSITORIES = 64

# The files one golden directory of the estate may hold: 361 ego drawings in
# two themes and the trees of ten schemas, about 145 diagrams per theme for the
# largest, fit well below it.
MAX_GOLDEN_FILES = 200_000

# A definition keyword at the start of a line of SDL, the count a notebook
# prints beside a repository name. It counts, it never returns the name after
# the keyword. An extension is counted apart, because it declares no type.
DEFINITION = re.compile(
    rb"^(extend[ \t]+)?(type|input|interface|union|enum|scalar)[ \t]+[_A-Za-z]", re.MULTILINE
)

# A block string, a string and a comment of SDL, whose text is description and
# never a definition. A description line may start with a keyword. One
# alternation, so the token that opens first wins, as a lexer reads it, and
# possessive repetitions, so an unclosed string costs one pass and no
# backtracking.
NOT_DEFINITION = re.compile(
    rb'"""(?:\\"""|[^"]|"(?!""))*+"""' rb'|"(?:\\.|[^"\\\n])*+"' rb"|#[^\n]*"
)

# The key under which the count of extensions is kept, apart from the kinds.
EXTENSIONS = "extend"


class EstateMissing(RuntimeError):
    """A parameter of the estate is empty or names no directory."""


class PinMismatch(AssertionError):
    """A count of the estate differs from its pin."""


@dataclass(frozen=True)
class Schema:
    """One schema of the estate: its repository name, its path and its counts."""

    repository: str
    path: Path
    bytes: int
    sha256: str
    definitions: dict

    @property
    def types(self) -> int:
        """The number of type definitions of every kind, extensions left out."""
        return sum(v for k, v in self.definitions.items() if k != EXTENSIONS)

    def line(self) -> str:
        """The one line a notebook prints: name, bytes, types and the digest."""
        kinds = " ".join("%s %d" % (k, self.definitions[k]) for k in sorted(self.definitions))
        return "%s  %d bytes  %d types (%s)  sha256 %s" % (
            self.repository,
            self.bytes,
            self.types,
            kinds,
            self.sha256,
        )


def _root() -> Path:
    """Returns the repository this file lies in, whatever the working
    directory of the kernel is."""
    return find_root(Path(__file__).resolve().parent)


def shown(path) -> str:
    """Returns `path` as a notebook may print it.

    A path inside the repository prints repository relative, and any other
    path by its last component only.
    """
    path = Path(path).resolve()
    try:
        return str(path.relative_to(_root()))
    except ValueError:
        return path.name


def _outside_directory(value, parameter: str) -> Path:
    if not value:
        raise EstateMissing("%s is empty, so the estate half does not run" % parameter)
    directory = Path(value).expanduser().resolve()
    if not directory.is_dir():
        raise EstateMissing("%s names no directory: %s" % (parameter, shown(directory)))
    root = _root()
    if directory == root or root in directory.parents:
        raise EstateMissing(
            "%s lies inside the repository, which holds no estate schema" % parameter
        )
    return directory


def available(value) -> bool:
    """Reports whether an estate parameter names a directory, for a not run line."""
    if not value:
        return False
    return Path(value).expanduser().is_dir()


def _blank(match) -> bytes:
    return re.sub(rb"[^\n]", b" ", match.group(0))


def count_definitions(sdl: bytes) -> dict:
    """Returns the number of definitions of each kind keyword in `sdl`.

    Block strings, strings and comments are blanked first, keeping their line
    breaks, so a description line that starts with a keyword counts nothing.
    Extensions are counted under `EXTENSIONS` and not under their kind.
    """
    text = NOT_DEFINITION.sub(_blank, sdl)
    counts = {}
    for match in DEFINITION.finditer(text):
        kind = EXTENSIONS if match.group(1) else match.group(2).decode("ascii")
        counts[kind] = counts.get(kind, 0) + 1
    return counts


def schemas(estate_schemas_dir) -> list:
    """Returns the schemas of `estate_schemas_dir` in repository name order.

    Every directory of it that holds `schema.graphql` is one repository, and a
    directory without one is skipped. Raises `EstateMissing` when the
    parameter is empty or names no directory outside the repository.
    """
    base = _outside_directory(estate_schemas_dir, "estate_schemas_dir")
    found = []
    entries = sorted(base.iterdir(), key=lambda p: p.name.encode("utf-8"))
    if len(entries) > MAX_REPOSITORIES:
        raise EstateMissing(
            "estate_schemas_dir holds %d entries, more than %d" % (len(entries), MAX_REPOSITORIES)
        )
    for entry in entries:
        sdl_path = entry / SCHEMA_FILE
        if not entry.is_dir() or not sdl_path.is_file():
            continue
        data = sdl_path.read_bytes()
        found.append(
            Schema(
                repository=entry.name,
                path=sdl_path,
                bytes=len(data),
                sha256=hashlib.sha256(data).hexdigest(),
                definitions=count_definitions(data),
            )
        )
    return found


def check_pin(name: str, found: int, expected: int) -> str:
    """Returns the line of a pin that holds, and raises `PinMismatch` otherwise."""
    if found != expected:
        raise PinMismatch("%s is %d, the pin is %d" % (name, found, expected))
    return "%s %d, equal to the pin" % (name, found)


def goldens(estate_goldens_dir, repository: str = None) -> Path:
    """Returns the golden directory of the estate, or of one repository in it.

    The directory of a repository may not exist yet, which is what a run under
    `update_goldens` fills.
    """
    base = _outside_directory(estate_goldens_dir, "estate_goldens_dir")
    if repository is None:
        return base
    if not repository or "/" in repository or repository in (".", ".."):
        raise ValueError("a repository is one plain name, got %r" % repository)
    return base / repository


def recorded(estate_recorded_dir, repository: str) -> Path:
    """Returns the record of the digests of one repository's run on the other
    architecture, `<repository>.json` in `estate_recorded_dir`.

    The record is read by `arch.read_record`. Raises `EstateMissing` when the
    parameter is empty or names no directory outside the repository.
    """
    base = _outside_directory(estate_recorded_dir, "estate_recorded_dir")
    if not repository or "/" in repository or repository in (".", ".."):
        raise ValueError("a repository is one plain name, got %r" % repository)
    return base / ("%s.json" % repository)


def golden_files(directory) -> dict:
    """Returns every file under `directory` as its relative path and its bytes.

    The paths are POSIX relative paths, sorted by their UTF-8 bytes, which is
    the order a tree comparison walks.
    """
    directory = Path(directory)
    files = {}
    for path in sorted(directory.rglob("*"), key=lambda p: str(p).encode("utf-8")):
        if not path.is_file():
            continue
        if len(files) >= MAX_GOLDEN_FILES:
            raise EstateMissing(
                "%s holds more than %d files" % (shown(directory), MAX_GOLDEN_FILES)
            )
        files[path.relative_to(directory).as_posix()] = path.read_bytes()
    return files


def summary(estate_goldens_dir) -> list:
    """Returns one line per repository of the golden directory: its name and counts."""
    base = goldens(estate_goldens_dir)
    lines = []
    for entry in sorted(base.iterdir(), key=lambda p: p.name.encode("utf-8")):
        if not entry.is_dir():
            continue
        files = golden_files(entry)
        lines.append(
            "%s  %d files  %d bytes" % (entry.name, len(files), sum(len(v) for v in files.values()))
        )
    return lines
