# SPDX-License-Identifier: MIT
"""The private copy of the pinned toolchain with rust-src, a dev tool of the size harness.

The standard library built from source needs the component rust-src beside the
compiler that builds it, and the shared pin 1.95.0 of the machine carries none. The
component stays out of the shared pin until every build remaps the source of the
standard library, since rustc writes the panic locations of inlined code of the
standard library with the path of the machine while rust-src is present, and cargo
keeps the artifacts built in that window.

1. Every file of the pin is hard linked under target/size/toolchain/ of the tree,
   on the file system of the rustup home, so the copy takes no second copy of the
   compiler and never writes into the shared pin. A file the copy changes, the
   list of its components, is written anew and never through the link.
2. The tarball of rust-src of 1.95.0, whose address and hash the channel
   manifest of the pin names, is downloaded into target/size/toolchain/downloads/,
   retried while the network fails, and checked against the hash of that manifest
   and against the hash file published beside the tarball. The tool never
   downloads into /tmp.
3. The files of the component are unpacked into the copy with their manifest.
4. One build of the probe hello with the standard library from source under
   RUSTC_BOOTSTRAP=1 warms the cargo cache with the six crates of the standard
   library's own lock, rustc-literal-escaper, getopts, rustc-demangle, addr2line,
   object and memchr, and is retried while a download fails.

The harness finds the copy through the variable AOSABI_SIZE_TOOLCHAIN, or at its
place under target/size/toolchain/ when the variable is unset, and runs it as the
rustup proxies run a toolchain named by its path in RUSTUP_TOOLCHAIN, so every
cargo call stays a call of the cargo of the rustup home. Every call this tool makes
names its toolchain in RUSTUP_TOOLCHAIN, the pin 1.95.0 or the copy, since the
default toolchain of the machine is another stable.

    python3 tools/size_toolchain.py            make the copy, or check the one there
    python3 tools/size_toolchain.py --check    check the copy and make nothing

A download that still fails after its retries ends the tool with the exit status
75, EX_TEMPFAIL of sysexits.h, and no whole copy, at most one without its mark that
the next run makes again, since the plan retries such a failure and never judges
it, and the rows that need the copy print WAITING. Every other failure ends it with
the status 1.

The tool runs without the harness, so it keeps its own copies of the pin, its
commit, the host, the variable, the mark, the place of the copy, the status of a
failed download, and the flags of its warming build with the remap of the source
of the standard library, which the module text of tests/harness/toolchain.py
names, so a bump of the pin or a renamed flag edits both files.
"""

from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import time
import tomllib
import urllib.request
from pathlib import Path

#: The pinned toolchain of rust-toolchain.toml.
PIN = "1.95.0"
#: The commit the pinned rustc reports in `rustc -vV`.
PIN_COMMIT = "59807616e1fa2540724bfbac14d7976d7e4a3860"
#: The host of the development machine, whose pin the copy is made from.
HOST = "aarch64-unknown-linux-gnu"
#: The variable that names the place of the copy for the harness.
VARIABLE = "AOSABI_SIZE_TOOLCHAIN"
#: The file the tool writes into a whole copy, which the harness reads as its mark.
MARK = ".aosabi-size-toolchain.json"
#: The six crates of the standard library's own lock that a build from source fetches.
STD_LOCK_CRATES = ("rustc-literal-escaper", "getopts", "rustc-demangle", "addr2line", "object", "memchr")
#: How often a download and the warming build are tried, and the seconds before each retry.
TRIES = 6
BACKOFF_S = (5, 15, 30, 60, 120)
#: The seconds one read of a download may wait, so six tries of a download that hangs end well within the
#: timeout of a notebook that runs the tool.
READ_TIMEOUT_S = 60
#: The exit status of a run whose download still fails after its retries, EX_TEMPFAIL of sysexits.h.
DOWNLOAD_FAILED = 75
#: The words of a failed build that a download over the network explains.
NETWORK = re.compile(r"download|network|spurious|timed out|failed to get|resolve host|connect|proxy|ssl",
                     re.IGNORECASE)


class DownloadFailed(Exception):
    """A download that still fails after its retries, which the tool ends on with DOWNLOAD_FAILED."""


def find_root(start: Path) -> Path:
    """Return the root of the tree above `start`, the directory that holds rust-toolchain.toml and tests/."""
    for base in [start, *start.parents]:
        if (base / "rust-toolchain.toml").is_file() and (base / "tests").is_dir():
            return base
    raise SystemExit("no tree of Aosabi above %s" % start)


def copy_path(root: Path) -> Path:
    """Return where the copy lives: the path AOSABI_SIZE_TOOLCHAIN names, or its place under target/."""
    named = os.environ.get(VARIABLE)
    if named:
        return Path(named)
    return root / "target" / "size" / "toolchain" / ("%s-%s" % (PIN, HOST))


def rustup_tool(name: str) -> Path:
    """Return the proxy `name` of the rustup home, which runs the toolchain RUSTUP_TOOLCHAIN names."""
    return Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo") / "bin" / name


def toolchain_env(toolchain, **extra) -> dict:
    """Return the environment of a call that runs `toolchain`, a name or the path of a toolchain."""
    env = dict(os.environ)
    env["RUSTUP_TOOLCHAIN"] = str(toolchain)
    env.update({key: str(value) for key, value in extra.items()})
    return env


def run(argv, env, cwd=None) -> subprocess.CompletedProcess:
    """Run `argv` with both streams captured as text."""
    return subprocess.run([str(part) for part in argv], env=env, cwd=cwd, capture_output=True, text=True)


def pin_sysroot() -> Path:
    """Return the sysroot of the shared pin, read from its rustc."""
    done = run([rustup_tool("rustc"), "--print", "sysroot"], toolchain_env(PIN))
    if done.returncode != 0:
        raise SystemExit("the pin %s does not answer: %s" % (PIN, done.stderr.strip()))
    return Path(done.stdout.strip())


def commit_of(toolchain) -> str:
    """Return the commit `rustc -vV` of `toolchain` reports."""
    done = run([rustup_tool("rustc"), "-vV"], toolchain_env(toolchain))
    for line in done.stdout.splitlines():
        if line.startswith("commit-hash:"):
            return line.split(":", 1)[1].strip()
    return ""


def link_tree(source: Path, dest: Path) -> int:
    """Hard link every file of `source` under `dest`, which must not exist yet, and return the count.

    A directory is made, a file is linked and a symbolic link is made again
    with its own target, so no byte of the source is copied and none of its
    files is opened for writing.
    """
    linked = 0
    for directory, names, files in os.walk(source):
        here = Path(directory)
        there = dest / here.relative_to(source)
        there.mkdir(parents=True, exist_ok=True)
        for name in names:
            if (here / name).is_symlink():
                os.symlink(os.readlink(here / name), there / name)
        for name in files:
            path = here / name
            if path.is_symlink():
                os.symlink(os.readlink(path), there / name)
            else:
                os.link(path, there / name)
            linked += 1
    return linked


def rust_src_entry(sysroot: Path) -> dict:
    """Return the address and the hash of the tarball of rust-src that the pin's channel manifest names."""
    manifest = sysroot / "lib" / "rustlib" / "multirust-channel-manifest.toml"
    with manifest.open("rb") as handle:
        document = tomllib.load(handle)
    entry = document["pkg"]["rust-src"]["target"]["*"]
    if not entry.get("available"):
        raise SystemExit("the channel manifest of the pin marks rust-src unavailable")
    return {"url": entry["xz_url"], "sha256": entry["xz_hash"], "version": document["pkg"]["rust-src"]["version"],
            "date": document.get("date", "")}


def fetch(url: str) -> bytes:
    """Return the bytes at `url`, tried again after each failure of the network.

    A failure of the network is an OSError or an HTTPException of
    http.client. A refused or reset connection, a read that times out, a
    failure of TLS and the URLError of urllib are kinds of OSError, and a
    transfer the network cuts short raises IncompleteRead, a kind of
    HTTPException. A download that fails on its last try raises
    DownloadFailed.
    """
    for attempt in range(TRIES):
        try:
            with urllib.request.urlopen(url, timeout=READ_TIMEOUT_S) as answer:
                return answer.read()
        except (OSError, http.client.HTTPException) as failure:
            if attempt + 1 == TRIES:
                raise DownloadFailed("the download of %s failed %d times, last: %s" % (url, TRIES, failure))
            wait = BACKOFF_S[min(attempt, len(BACKOFF_S) - 1)]
            print("the download of %s failed (%s), retried in %d s" % (url, failure, wait), flush=True)
            time.sleep(wait)
    raise AssertionError("unreachable")


def tarball(entry: dict, downloads: Path) -> Path:
    """Return the checked tarball of rust-src under `downloads`, downloaded once.

    A tarball already there is kept while its bytes hold the hash of the
    channel manifest, and a new one is held to that hash and to the hash file
    the server publishes beside it before it is kept.
    """
    downloads.mkdir(parents=True, exist_ok=True)
    path = downloads / entry["url"].rsplit("/", 1)[1]
    if path.is_file() and hashlib.sha256(path.read_bytes()).hexdigest() == entry["sha256"]:
        return path
    data = fetch(entry["url"])
    digest = hashlib.sha256(data).hexdigest()
    if digest != entry["sha256"]:
        raise SystemExit("the tarball %s has the hash %s, the channel manifest names %s"
                         % (path.name, digest, entry["sha256"]))
    published = fetch(entry["url"] + ".sha256").decode("ascii", "replace").split()[0]
    if published != digest:
        raise SystemExit("the hash file beside %s names %s, the tarball has %s" % (path.name, published, digest))
    partial = path.with_suffix(path.suffix + ".part")
    partial.write_bytes(data)
    partial.replace(path)
    print("downloaded %s, %d bytes, sha256 %s, equal to the channel manifest and to the published hash file"
          % (path.name, len(data), digest), flush=True)
    return path


def install_rust_src(archive: Path, copy: Path) -> int:
    """Unpack the files of rust-src from `archive` into `copy` and record the component, and return the count.

    The tarball holds the installer's tree `rust-src-<version>/rust-src/` with
    the files under `lib/rustlib/src/` and their list `manifest.in`. The files
    land at their place in the copy, the list as `lib/rustlib/manifest-rust-src`,
    and the component joins `lib/rustlib/components`, which is written anew, so
    the file the copy shares with the pin through its link keeps its bytes.
    """
    rustlib = copy / "lib" / "rustlib"
    installed = []
    with tarfile.open(archive, "r:xz") as tar:
        members = []
        for member in tar.getmembers():
            parts = member.name.split("/")
            if len(parts) < 3 or parts[1] != "rust-src":
                continue
            relative = "/".join(parts[2:])
            if not relative.startswith("lib/rustlib/src/"):
                continue
            member.name = relative
            members.append(member)
            if member.isfile():
                installed.append(relative)
        tar.extractall(copy, members=members, filter="data")
    (rustlib / "manifest-rust-src").write_text("".join("file:%s\n" % name for name in sorted(installed)),
                                              encoding="utf-8")
    components = rustlib / "components"
    listed = components.read_text(encoding="utf-8").splitlines()
    if "rust-src" not in listed:
        components.unlink()
        components.write_text("\n".join(listed + ["rust-src"]) + "\n", encoding="utf-8")
    return len(installed)


def warm(root: Path, copy: Path) -> float:
    """Build the probe hello with the standard library from source on `copy` and return the seconds.

    The build reads the six crates of the standard library's own lock from the
    cargo cache, and a first try offline that finds one missing is followed by
    tries that may download it. A try online whose error names the network
    is tried again, and on the last try it raises DownloadFailed. A try online
    that fails for another reason ends the tool at once.
    """
    sysroot = copy
    env = toolchain_env(
        copy,
        RUSTC_BOOTSTRAP="1",
        CARGO_TARGET_DIR=root / "target" / "size" / "toolchain" / "warm",
        RUSTFLAGS="--remap-path-prefix=%s/lib/rustlib/src/rust=/rustc/%s" % (sysroot, PIN_COMMIT),
        NO_COLOR="1",
    )
    command = [rustup_tool("cargo"), "build", "--locked", "--manifest-path", root / "tests" / "size" / "Cargo.toml",
               "-p", "size-hello", "--profile", "size", "--target", HOST, "-Zbuild-std=std,panic_abort",
               "-Zbuild-std-features="]
    started = time.monotonic()
    for attempt in range(TRIES):
        done = run(command + (["--offline"] if attempt == 0 else []), env, cwd=root)
        if done.returncode == 0:
            return time.monotonic() - started
        missing = [name for name, versions in cached_crates().items() if not versions]
        if attempt > 0 and not NETWORK.search(done.stderr):
            raise SystemExit("the warming build failed for another reason than a download:\n%s"
                             % done.stderr[-3000:])
        if attempt + 1 == TRIES:
            raise DownloadFailed("the warming build failed offline once and online %d times over the network,"
                                 " the crates %s missing from the cache, last: %s"
                                 % (TRIES - 1, ", ".join(missing) or "none", done.stderr.strip()[-600:]))
        wait = 0 if attempt == 0 else BACKOFF_S[min(attempt - 1, len(BACKOFF_S) - 1)]
        print("the warming build failed (%s), tried again%s" % (
            done.stderr.strip().splitlines()[-1] if done.stderr.strip() else "no output",
            " in %d s" % wait if wait else " online"), flush=True)
        time.sleep(wait)
    raise AssertionError("unreachable")


def cached_crates() -> dict:
    """Return the versions of the six crates of the standard library's lock in the cargo cache."""
    registry = Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo") / "registry" / "cache"
    held = {}
    for name in STD_LOCK_CRATES:
        versions = sorted(path.name[len(name) + 1:-len(".crate")] for path in registry.glob("*/%s-*.crate" % name)
                          if path.name[len(name) + 1:len(name) + 2].isdigit())
        held[name] = versions
    return held


def check(root: Path, copy: Path, pin: Path) -> list:
    """Return the faults of the copy at `copy`, none for a whole copy of the pin with rust-src."""
    faults = []
    if not (copy / MARK).is_file():
        faults.append("the copy carries no mark %s" % MARK)
    if not (copy / "lib" / "rustlib" / "src" / "rust" / "library" / "Cargo.lock").is_file():
        faults.append("the copy carries no lib/rustlib/src/rust/library/Cargo.lock")
    if (pin / "lib" / "rustlib" / "src").exists():
        faults.append("the shared pin carries lib/rustlib/src")
    commit = commit_of(copy)
    if commit != PIN_COMMIT:
        faults.append("the rustc of the copy reports the commit %r, the pin is %s" % (commit, PIN_COMMIT))
    return faults


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="make the private copy of the pin with rust-src")
    parser.add_argument("--root", default=None, help="the root of the tree, found from this file by default")
    parser.add_argument("--check", action="store_true", help="check the copy and make nothing")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve() if args.root else find_root(Path(__file__).resolve().parent)
    copy = copy_path(root)
    pin = pin_sysroot()
    print("the shared pin: %s, %s" % (PIN, "no lib/rustlib/src" if not (pin / "lib" / "rustlib" / "src").exists()
                                      else "WITH lib/rustlib/src"))
    print("the copy: %s" % (copy.relative_to(root) if copy.is_relative_to(root) else copy))
    if args.check:
        faults = check(root, copy, pin)
        for fault in faults:
            print("fault:", fault)
        print("the copy is whole" if not faults else "the copy is not whole")
        return 1 if faults else 0
    if (pin / "lib" / "rustlib" / "src").exists():
        raise SystemExit("the shared pin carries lib/rustlib/src, which this tool never adds, so it stops")
    if (copy / MARK).is_file() and not check(root, copy, pin):
        print("the copy is whole, nothing to make")
        return 0
    if copy.exists():
        # A copy without its mark stopped half way, so it is made again from
        # the pin. Removing a link never touches the file of the pin.
        shutil.rmtree(copy)
    copy.parent.mkdir(parents=True, exist_ok=True)
    if os.stat(pin).st_dev != os.stat(copy.parent).st_dev:
        raise SystemExit("the copy and the pin lie on two file systems, so no hard link joins them")
    entry = rust_src_entry(pin)
    try:
        archive = tarball(entry, root / "target" / "size" / "toolchain" / "downloads")
    except DownloadFailed as failure:
        print("WAITING: %s, so no copy is made and the next run tries again" % failure, flush=True)
        return DOWNLOAD_FAILED
    started = time.monotonic()
    linked = link_tree(pin, copy)
    installed = install_rust_src(archive, copy)
    commit = commit_of(copy)
    if commit != PIN_COMMIT:
        raise SystemExit("the rustc of the copy reports the commit %r, the pin is %s" % (commit, PIN_COMMIT))
    print("linked %d files of the pin and unpacked %d files of rust-src %s in %.1f s"
          % (linked, installed, entry["version"], time.monotonic() - started))
    try:
        seconds = warm(root, copy)
    except DownloadFailed as failure:
        # The copy carries no mark yet, so the next run makes it again from
        # the pin and the tarball kept under downloads/.
        print("WAITING: %s, so the copy stays without its mark and the next run tries again" % failure, flush=True)
        return DOWNLOAD_FAILED
    crates = cached_crates()
    print("warmed the cargo cache by one build of hello in %.1f s, the six crates of the standard library's lock: %s"
          % (seconds, ", ".join("%s %s" % (name, " and ".join(versions) or "absent") for name, versions in crates.items())))
    mark = {"pin": PIN, "commit": PIN_COMMIT, "host": HOST, "rust-src": {"url": entry["url"], "sha256": entry["sha256"],
            "version": entry["version"], "files": installed}, "linked files": linked}
    (copy / MARK).write_text(json.dumps(mark, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    faults = check(root, copy, pin)
    for fault in faults:
        print("fault:", fault)
    print("the copy is whole" if not faults else "the copy is not whole")
    return 1 if faults else 0


if __name__ == "__main__":
    sys.exit(main())
