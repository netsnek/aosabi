# SPDX-License-Identifier: MIT
"""Aosabi harness: where the repository is, seen from a notebook kernel.

A notebook is executed with the repository root as its working directory, and
papermill is invoked from there. A reader who opens the same notebook by hand
starts somewhere else, so the root is resolved by walking up rather than
assumed, and every path a notebook prints stays repository relative, because no
machine path enters a committed file.
"""

from __future__ import annotations

import hashlib
import os
from dataclasses import dataclass
from pathlib import Path

CARGO_TOML = "Cargo.toml"
WORKSPACE_MARKER = "[workspace]"


class RootNotFound(RuntimeError):
    """No ancestor of the starting directory carries a Cargo.toml."""


class SocketDirUnfit(RuntimeError):
    """No folder whose sockets fit `sun_path` can be made for a path."""


@dataclass(frozen=True)
class Paths:
    """The directories a notebook needs, all absolute, all derived from root."""

    root: Path
    tests: Path
    notebooks: Path
    out: Path
    goldens: Path
    target: Path

    def rel(self, path) -> str:
        """`path` as a repository relative string, which is what a cell prints."""
        try:
            return str(Path(path).resolve().relative_to(self.root))
        except ValueError:
            return str(Path(path))


def find_root(start=None) -> Path:
    """Walk up from `start` to the cargo workspace root.

    The first ancestor whose Cargo.toml declares a workspace wins, because a
    Rust twin under tests/fixtures/rs is a cargo package of its own and its
    manifest would otherwise stop the walk.
    """
    probe = Path(start or os.getcwd()).resolve()
    candidates = []
    for directory in [probe, *probe.parents]:
        manifest = directory / CARGO_TOML
        if manifest.is_file():
            candidates.append(directory)
            try:
                text = manifest.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            if WORKSPACE_MARKER in text:
                return directory
    if candidates:
        return candidates[0]
    raise RootNotFound(
        "no %s above %s, run the notebook from inside the repository" % (CARGO_TOML, probe)
    )


def paths(start=None, goldens_dir=None) -> Paths:
    """Resolve the repository root and the directories the harness uses.

    `goldens_dir` is the papermill parameter of docs/harness.md section 8 and
    defaults to tests/goldens. A relative value is read against the root, so a
    notebook passes the parameter through verbatim.
    """
    root = find_root(start)
    tests = root / "tests"
    if goldens_dir:
        goldens = Path(goldens_dir)
        if not goldens.is_absolute():
            goldens = root / goldens
    else:
        goldens = tests / "goldens"
    return Paths(
        root=root,
        tests=tests,
        notebooks=tests / "notebooks",
        out=tests / "notebooks" / "out",
        goldens=goldens.resolve() if goldens.exists() else goldens,
        target=root / "target",
    )


# The bytes of `sun_path` in `struct sockaddr_un` on Linux, the closing NUL
# included. Rust's std and Node refuse a longer path.
SUN_PATH = 108
# The bytes a socket directory keeps free below SUN_PATH, for the separator,
# the closing NUL and a margin.
SUN_MARGIN = 8


def socket_dir(path, longest_name: int) -> Path:
    """A directory whose sockets of up to `longest_name` bytes fit `sun_path`.

    The directory is `path` when `path`, a separator and the longest name fit,
    and otherwise a directory named by a hash of `path` inside
    `$XDG_RUNTIME_DIR/aosabi-sock-<uid>/`, or `/tmp/aosabi-sock-<uid>/` where
    that variable is unset, a folder kept at mode 0700, so each entry and each
    worktree keeps one of its own. A worktree with a longer path then binds
    the same sockets as one with a shorter path, where it failed with "path
    must be shorter than SUN_LEN" before. The directory is created and not
    emptied. A `$XDG_RUNTIME_DIR` that names no directory, and a hashed
    directory whose sockets do not fit either, raise `SocketDirUnfit` here
    rather than at the first bind.
    """
    path = Path(path)
    if _fits(path, longest_name):
        path.mkdir(parents=True, exist_ok=True)
        return path
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    if runtime and not Path(runtime).is_dir():
        raise SocketDirUnfit("XDG_RUNTIME_DIR names %s, which is no directory" % runtime)
    base = Path(runtime or "/tmp") / ("aosabi-sock-%d" % os.getuid())
    short = base / hashlib.sha256(os.fsencode(str(path))).hexdigest()[:16]
    if not _fits(short, longest_name):
        raise SocketDirUnfit("a socket name of %d bytes fits neither in %s nor in %s"
                             % (longest_name, path, short))
    base.mkdir(mode=0o700, exist_ok=True)
    os.chmod(base, 0o700)
    short.mkdir(exist_ok=True)
    return short


def _fits(folder: Path, longest_name: int) -> bool:
    """Whether `folder`, a separator and a name of `longest_name` bytes fit `sun_path`."""
    return len(os.fsencode(str(folder))) + 1 + longest_name <= SUN_PATH - SUN_MARGIN
