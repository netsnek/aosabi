# SPDX-License-Identifier: MIT
"""Aosabi harness: a download by URL and SHA-256 into tests/.cache/.

Every download of the harness is checked against the SHA-256 its lock records,
and a mismatch is refused.

A lock is a JSON file beside the committed copies it pins, in the form the
writer of the fixtures gave the locks of the presets and of the CommonMark
examples:

    {"version": 1, <the source, its licence and more>,
     "cache": "tests/.cache/<directory>",
     "files": [{"path": "<name>", "url": "<url>", "sha256": "<hex>",
                "bytes": <n>}, ...]}

where `path` is the file name of the committed copy in the directory of the
lock, and the download is cached under the last component of its URL in the
directory `cache` names. A file entry may carry more keys, such as the name
of a preset, which a pin keeps and `lock_text` writes back after the four. A
notebook reads a lock and never writes one, and the writer of a fixture or of
a capture writes its lock through `lock_text`.

A program that is not Python, such as the capture of Voyager in node, runs the
same download through the command line of this module, from the tests
directory of the repository:

    python -m harness.fetch lock <lock path> [--offline]
    python -m harness.fetch url <url> <sha256> <cache directory> [--offline]
    python -m harness.fetch verify <lock path>

Each prints one line per file, its path, its length and its digest, and exits
0, or prints the refusal on standard error and exits 1, or 64 on a wrong
invocation. The package imports this module before runpy runs it, so Python
writes one RuntimeWarning about that order to standard error, which changes
nothing of the run and nothing of standard output.
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

from .paths import paths

LOCK_VERSION = 1

# The largest download the harness takes. The Voyager archive is the largest
# the plan names and is a few MB, so 64 MiB leaves room without letting a wrong
# URL fill the quota of the machine.
MAX_BYTES = 64 * 1024 * 1024

# One read of the response body, and the cap on the reads of one download.
CHUNK_BYTES = 1024 * 1024
MAX_CHUNKS = MAX_BYTES // CHUNK_BYTES + 1

TIMEOUT_S = 60.0

# The keys of a file entry of a lock that a pin reads, in the order the lock
# writes them.
ENTRY_KEYS = ("path", "url", "sha256", "bytes")

# The exit codes of the command line: a refusal, and a wrong invocation, the
# usage code of `sysexits.h`.
EXIT_REFUSED = 1
EXIT_USAGE = 64

# A fixed agent, so that a server sees the harness and nothing of the machine.
USER_AGENT = "aosabi-harness-fetch/1"


class FetchRefused(RuntimeError):
    """A download or a cached copy does not carry the pinned SHA-256."""


class LockMismatch(AssertionError):
    """A committed copy does not match the entry of its lock."""


@dataclass(frozen=True)
class Pin:
    """One entry of a lock: the committed name, the URL, the digest and the length.

    `extra` holds the other keys of the entry in the order the lock lists
    them, such as the name of a preset.
    """

    path: str
    url: str
    sha256: str
    bytes: int = None
    extra: tuple = field(default=(), compare=False)

    @property
    def cached_name(self) -> str:
        """The name the download takes in the cache, the last part of its URL."""
        return _plain_name(self.url.rstrip("/").rsplit("/", 1)[-1].split("?", 1)[0])


def sha256_hex(data: bytes) -> str:
    """Returns the SHA-256 of `data` in lower case base 16."""
    return hashlib.sha256(data).hexdigest()


def cache_dir(subdir: str = None, root=None) -> Path:
    """Returns `tests/.cache/`, or the directory `subdir` inside it.

    `subdir` is one plain name, such as `voyager-656b8dd`, so that a download
    never lands outside the cache.
    """
    base = paths(root).tests / ".cache"
    if subdir is None:
        return base
    if not subdir or "/" in subdir or subdir in (".", ".."):
        raise ValueError("the cache subdirectory is one plain name, got %r" % subdir)
    return base / subdir


def _plain_name(name: str) -> str:
    if not name or "/" in name or name in (".", "..") or name.startswith("."):
        raise ValueError("a cached file is one plain name, got %r" % name)
    return name


def _download(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    parts = []
    total = 0
    with urllib.request.urlopen(request, timeout=TIMEOUT_S) as response:
        for _ in range(MAX_CHUNKS):
            chunk = response.read(CHUNK_BYTES)
            if not chunk:
                return b"".join(parts)
            total += len(chunk)
            if total > MAX_BYTES:
                break
            parts.append(chunk)
    raise FetchRefused("%s is larger than %d bytes" % (url, MAX_BYTES))


def fetch(pin: Pin, directory, offline: bool = False) -> Path:
    """Returns the path of the download of `pin` inside `directory`.

    A copy already in `directory` with the pinned digest is returned without a
    request. Otherwise the URL is downloaded, the digest compared and the file
    written through a temporary name in the same directory, so an interrupted
    run never leaves a half file under the final name. `offline` refuses the
    download and reads the cache alone.

    Raises `FetchRefused` when the downloaded bytes or the cached copy carry
    another digest, or another length when the pin records one. A cached copy
    with a wrong digest is left in place for the reader to inspect and is
    never overwritten.
    """
    directory = Path(directory)
    target = directory / pin.cached_name
    expected = pin.sha256.lower()
    if target.is_file():
        data = target.read_bytes()
        found = sha256_hex(data)
        if found != expected:
            raise FetchRefused(
                "the cached %s carries sha256 %s, the pin is %s" % (pin.cached_name, found, expected)
            )
        return target
    if offline:
        raise FetchRefused("%s is not cached and the fetch is offline" % pin.cached_name)
    data = _download(pin.url)
    found = sha256_hex(data)
    if found != expected:
        raise FetchRefused(
            "%s answered %d bytes with sha256 %s, the pin is %s"
            % (pin.url, len(data), found, expected)
        )
    if pin.bytes is not None and len(data) != pin.bytes:
        raise FetchRefused(
            "%s answered %d bytes, the pin is %d bytes" % (pin.url, len(data), pin.bytes)
        )
    directory.mkdir(parents=True, exist_ok=True)
    partial = directory / (".partial-%d-%s" % (os.getpid(), pin.cached_name))
    partial.write_bytes(data)
    os.replace(partial, target)
    return target


def _lock(lock_path) -> dict:
    document = json.loads(Path(lock_path).read_text(encoding="utf-8"))
    if document.get("version") != LOCK_VERSION:
        raise ValueError("%s is not a lock of version %d" % (Path(lock_path).name, LOCK_VERSION))
    return document


def read_lock(lock_path) -> list:
    """Returns the pins of the lock at `lock_path` in the order it lists them."""
    pins = []
    for entry in _lock(lock_path)["files"]:
        extra = tuple((k, v) for k, v in entry.items() if k not in ENTRY_KEYS)
        pins.append(
            Pin(
                path=entry["path"],
                url=entry["url"],
                sha256=entry["sha256"],
                bytes=entry.get("bytes"),
                extra=extra,
            )
        )
    return pins


def lock_text(pins, **fields) -> str:
    """Returns the text of a lock over `pins`, the one form every lock takes.

    `fields` are the keys written between `version` and `files`, in the order
    given, such as `source`, `commit`, `licence` and `cache`. A file entry is
    `path`, then the keys of `extra`, then `url`, `sha256` and `bytes`, the
    order the committed locks use. The text has an indent of two spaces and
    ends in one line feed.
    """
    files = []
    seen = set()
    for pin in pins:
        _plain_name(pin.path)
        if pin.path in seen:
            raise ValueError("the lock names %s twice" % pin.path)
        seen.add(pin.path)
        entry = {"path": pin.path}
        for key, value in pin.extra:
            if key in ENTRY_KEYS:
                raise ValueError("the extra key %s of %s is one of the pin" % (key, pin.path))
            entry[key] = value
        entry.update({"url": pin.url, "sha256": pin.sha256.lower(), "bytes": pin.bytes})
        files.append(entry)
    document = {"version": LOCK_VERSION}
    document.update(fields)
    document["files"] = files
    return json.dumps(document, indent=2, ensure_ascii=False) + "\n"


def verify_lock(lock_path) -> list:
    """Checks every committed copy beside the lock at `lock_path` against it.

    Returns one line per file, its name, its length and its digest, which is
    what a setup cell prints. Raises `LockMismatch` naming the first file that
    is missing or whose digest or length differs.
    """
    lock_path = Path(lock_path)
    lines = []
    for pin in read_lock(lock_path):
        copy = lock_path.parent / _plain_name(pin.path)
        if not copy.is_file():
            raise LockMismatch("%s names %s, which is not beside it" % (lock_path.name, pin.path))
        data = copy.read_bytes()
        found = sha256_hex(data)
        if found != pin.sha256.lower():
            raise LockMismatch(
                "%s carries sha256 %s, %s pins %s" % (pin.path, found, lock_path.name, pin.sha256)
            )
        if pin.bytes is not None and len(data) != pin.bytes:
            raise LockMismatch(
                "%s has %d bytes, %s pins %d" % (pin.path, len(data), lock_path.name, pin.bytes)
            )
        lines.append("%s  %d bytes  %s" % (pin.path, len(data), found))
    return lines


def fetch_lock(lock_path, offline: bool = False, root=None) -> list:
    """Fetches every pin of the lock at `lock_path` into the cache it names.

    The key `cache` of the lock names one directory under `tests/.cache/`.
    Returns the cached paths in the order of the lock. This is how a capture
    script or the writer of a fixture refreshes its copies, and the committed
    copies are then checked by `verify_lock`.
    """
    cache = _lock(lock_path)["cache"]
    prefix = "tests/.cache/"
    if not cache.startswith(prefix):
        raise ValueError("the cache of a lock lies under %s, got %r" % (prefix, cache))
    directory = cache_dir(cache[len(prefix) :].rstrip("/"), root)
    return [fetch(pin, directory, offline=offline) for pin in read_lock(lock_path)]


def fetch_url(url: str, sha256: str, subdir: str, offline: bool = False, root=None) -> Path:
    """Downloads `url` into `tests/.cache/<subdir>/` against the digest `sha256`.

    This is `fetch` for a download that no committed copy mirrors, such as the
    source archive of a capture, and the file takes the last component of the
    URL as its name.
    """
    pin = Pin(path=_plain_name(url.rstrip("/").rsplit("/", 1)[-1].split("?", 1)[0]), url=url, sha256=sha256)
    return fetch(pin, cache_dir(subdir, root), offline=offline)


def _line(path: Path) -> str:
    data = path.read_bytes()
    try:
        shown = path.resolve().relative_to(paths().root).as_posix()
    except ValueError:
        shown = path.name
    return "%s  %d bytes  %s" % (shown, len(data), sha256_hex(data))


def main(argv) -> int:
    """Runs the command line of the module docstring and returns its exit code."""
    args = [a for a in argv if a != "--offline"]
    offline = len(args) != len(argv)
    try:
        if len(args) == 2 and args[0] == "lock":
            for path in fetch_lock(args[1], offline=offline):
                print(_line(path))
        elif len(args) == 4 and args[0] == "url":
            print(_line(fetch_url(args[1], args[2], args[3], offline=offline)))
        elif len(args) == 2 and args[0] == "verify" and not offline:
            for line in verify_lock(args[1]):
                print(line)
        else:
            print(
                "usage: python -m harness.fetch lock <lock> [--offline] | "
                "url <url> <sha256> <cache directory> [--offline] | verify <lock>",
                file=sys.stderr,
            )
            return EXIT_USAGE
    except (FetchRefused, LockMismatch, ValueError, OSError) as refused:
        print("fetch: %s" % refused, file=sys.stderr)
        return EXIT_REFUSED
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
