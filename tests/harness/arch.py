# SPDX-License-Identifier: MIT
"""Aosabi harness: the static musl build of the other architecture and the
runner of its emulator.

The same tree on two architectures in one run catches a platform dependence of
the layout.

The host builds a package as a static musl binary of the architecture it is
not, and runs it under a user mode emulator: the parameter `emulator` when it
is set, otherwise `qemu-x86_64-static` on aarch64 and `qemu-aarch64-static` on
x86-64 from the search path, then `FEXInterpreter` under `muvm` when the
emulator fails on 16 KiB pages. When none runs, the step reads the digests of
`estate_recorded_dir` and says so. A run on the other machine writes those
digests per schema with `record_text`, in the form of the file list of
`manifest.json`, and the step reads them with `read_record`.

The player is the other static musl build of the harness: the card of
`mipsel-unknown-linux-musl`, whose standard library builds from the rust-src of
one pinned nightly. The function `player_kit` gathers what that build needs
beside the nightly, the C library musl built from its release archive and the
unwinder of LLVM, and the kit gives cargo the arguments of the target and the
flags that link the player with the host's clang and ld.lld.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shlex
import shutil
import struct
import tarfile
from dataclasses import dataclass
from pathlib import Path

from .fetch import fetch_url
from .procs import CommandFailed, run

# The architectures the determinism gate compares, by the name
# `platform.machine` reports, with the musl target and the emulator of each.
ARCHES = {
    "aarch64": ("aarch64-unknown-linux-musl", "qemu-aarch64-static"),
    "x86_64": ("x86_64-unknown-linux-musl", "qemu-x86_64-static"),
}
ALIASES = {"arm64": "aarch64", "amd64": "x86_64"}

# The ELF machine numbers of the two architectures.
ELF_MACHINES = {62: "x86_64", 183: "aarch64"}

# The program header type of an interpreter, whose absence is what a static
# binary is.
PT_INTERP = 3

# The program headers one binary may carry before the reader stops.
MAX_PROGRAM_HEADERS = 256

# The musl targets link with the self contained start files of the toolchain,
# and a cross build needs a linker for the other architecture, which the host
# compiler driver is not. rust-lld ships with the toolchain and is taken to
# link both.
CROSS_LINKER = "rust-lld"

# The version of a record of the digests of a run on the other architecture.
RECORD_VERSION = 1

BUILD_TIMEOUT_S = 3600.0


class NoEmulator(RuntimeError):
    """No emulator runs a binary of the other architecture on this host."""


@dataclass(frozen=True)
class ElfFacts:
    """The architecture of an ELF binary and whether it links statically."""

    arch: str
    static: bool


def host_arch() -> str:
    """Returns the architecture of the host, `aarch64` or `x86_64`."""
    machine = platform.machine().lower()
    machine = ALIASES.get(machine, machine)
    if machine not in ARCHES:
        raise RuntimeError("the determinism gate runs on aarch64 and x86_64, not %s" % machine)
    return machine


def other_arch() -> str:
    """Returns the architecture the host is not."""
    return "x86_64" if host_arch() == "aarch64" else "aarch64"


def other_target() -> str:
    """Returns the musl target of the other architecture."""
    return ARCHES[other_arch()][0]


def elf_facts(path) -> ElfFacts:
    """Reads the ELF header of `path` and returns its architecture and linkage.

    Only a 64 bit little endian ELF is read, which both architectures are.
    """
    data = Path(path).read_bytes()
    if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise ValueError("%s is not a 64 bit little endian ELF" % Path(path).name)
    machine = struct.unpack_from("<H", data, 18)[0]
    phoff = struct.unpack_from("<Q", data, 32)[0]
    phentsize, phnum = struct.unpack_from("<HH", data, 54)
    if phnum > MAX_PROGRAM_HEADERS:
        raise ValueError("%s carries %d program headers" % (Path(path).name, phnum))
    static = True
    for index in range(phnum):
        kind = struct.unpack_from("<I", data, phoff + index * phentsize)[0]
        if kind == PT_INTERP:
            static = False
    return ElfFacts(ELF_MACHINES.get(machine, "machine %d" % machine), static)


def build_static(
    package: str,
    binary: str,
    cargo=("cargo",),
    root=None,
    profile: str = "release",
    target=None,
    timeout_s: float = BUILD_TIMEOUT_S,
) -> Path:
    """Builds `package` as a static musl binary and returns its path.

    `target` defaults to the musl target of the other architecture. `cargo` is
    the command prefix, such as a lock wrapper, and the build always passes
    `--locked`. A target of the other architecture links through `rust-lld`,
    set in the environment of this build alone.

    Raises `CommandFailed` when the build fails and `ValueError` when the
    binary is not a static ELF of the target's architecture.
    """
    target = target or other_target()
    arch = target.split("-", 1)[0]
    argv = list(cargo) + ["build", "--locked", "-p", package, "--bin", binary, "--target", target]
    if profile == "release":
        argv.append("--release")
    elif profile != "dev":
        argv += ["--profile", profile]
    env = {}
    if arch != host_arch():
        env["CARGO_TARGET_%s_LINKER" % target.upper().replace("-", "_")] = CROSS_LINKER
    run(argv, cwd=root, env=env, timeout_s=timeout_s)
    directory = "debug" if profile == "dev" else profile
    base = Path(root) if root else Path.cwd()
    # Cargo writes under CARGO_TARGET_DIR when it is set, read against the
    # directory of the build.
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR") or "target")
    if not target_dir.is_absolute():
        target_dir = base / target_dir
    path = target_dir / target / directory / binary
    facts = elf_facts(path)
    if facts.arch != arch or not facts.static:
        raise ValueError(
            "%s is %s and %s, expected a static %s binary"
            % (binary, facts.arch, "static" if facts.static else "dynamic", arch)
        )
    return path


def candidates(emulator: str = "") -> list:
    """A set `emulator` parameter is the one candidate. Otherwise the qemu user
    mode emulator of the other architecture, then `FEXInterpreter` under
    `muvm` for an x86-64 binary on an aarch64 host, each only when it is on
    the search path.
    """
    if emulator:
        return [shlex.split(emulator)]
    found = []
    qemu = ARCHES[other_arch()][1]
    if shutil.which(qemu):
        found.append([qemu])
    if other_arch() == "x86_64" and shutil.which("muvm") and shutil.which("FEXInterpreter"):
        found.append(["muvm", "--", "FEXInterpreter"])
    return found


def resolve(
    binary,
    probe_args,
    emulator: str = "",
    expect_stdout: bytes = None,
    timeout_s: float = 120.0,
) -> list:
    """Returns the first candidate that runs `binary` with `probe_args` to exit 0.

    With `expect_stdout` a candidate counts only when the standard output of
    the probe holds those bytes, because a runner that exits 0 without handing
    the output back, as a microVM may, would pass an empty tree to the
    comparison. Raises `NoEmulator` naming every candidate and why it failed,
    which is the line a notebook prints before it falls back to
    `estate_recorded_dir`.
    """
    reasons = []
    for prefix in candidates(emulator):
        try:
            result = run(list(prefix) + [str(binary)] + list(probe_args), timeout_s=timeout_s)
        except CommandFailed as failed:
            reasons.append("%s: %s" % (" ".join(prefix), failed.reason))
            continue
        if expect_stdout is not None and expect_stdout not in result.stdout:
            reasons.append("%s: the probe printed no %r" % (" ".join(prefix), expect_stdout))
            continue
        return list(prefix)
    if not reasons:
        reasons.append("no emulator of %s on the search path" % other_arch())
    raise NoEmulator(". ".join(reasons))


def run_other(
    prefix,
    binary,
    args,
    cwd=None,
    stdin: bytes = None,
    timeout_s: float = 1800.0,
    check: bool = True,
):
    """Runs `binary` of the other architecture under the emulator `prefix`."""
    argv = list(prefix) + [str(binary)] + list(args)
    return run(argv, cwd=cwd, stdin=stdin, timeout_s=timeout_s, check=check)


def version(prefix) -> str:
    """Returns the first line the emulator prints for `--version`, or a note."""
    try:
        result = run(list(prefix) + ["--version"], check=False, timeout_s=60)
    except CommandFailed as failed:
        return "no version: %s" % failed.reason
    lines = (result.out or result.err).strip().splitlines()
    return lines[0] if lines else "no version line"


def record(tree: dict, arch: str) -> dict:
    """Returns the record of the digests of a tree, as a run on `arch` writes it.

    `tree` maps a POSIX relative path to its bytes, the form `docs.read_tree`
    returns. The record is `{"version", "arch", "files"}` with `files` as
    `[path, sha256, bytes]` in the order of the UTF-8 bytes of the paths, the
    form of the files of `manifest.json`, so a record and a manifest read
    alike.
    """
    files = []
    for path in sorted(tree, key=lambda p: p.encode("utf-8")):
        data = tree[path]
        files.append([path, hashlib.sha256(data).hexdigest(), len(data)])
    return {"version": RECORD_VERSION, "arch": arch, "files": files}


def record_text(recorded: dict) -> str:
    """Returns the text of a record, one file per line, ending in a line feed."""
    head = json.dumps(
        {"version": recorded["version"], "arch": recorded["arch"]}, ensure_ascii=True
    )[:-1]
    lines = [json.dumps(entry, ensure_ascii=True) for entry in recorded["files"]]
    return head + ', "files": [\n' + ",\n".join(lines) + "\n]}\n"


def read_record(path) -> dict:
    """Reads a record written by `record_text` and checks its version."""
    recorded = json.loads(Path(path).read_text(encoding="utf-8"))
    if recorded.get("version") != RECORD_VERSION:
        raise ValueError("%s is not a record of version %d" % (Path(path).name, RECORD_VERSION))
    return recorded


def compare_record(tree: dict, recorded: dict) -> list:
    """Returns one line per file where `tree` and a record of the other side differ.

    A file on one side alone, another digest and another length each give a
    line, and the paths alone are printed, so a line names no content of the
    tree.
    """
    here = {entry[0]: entry[1:] for entry in record(tree, host_arch())["files"]}
    there = {entry[0]: entry[1:] for entry in recorded["files"]}
    lines = []
    for path in sorted(set(here) | set(there), key=lambda p: p.encode("utf-8")):
        if path not in there:
            lines.append("%s is in this run and not in the record" % path)
        elif path not in here:
            lines.append("%s is in the record and not in this run" % path)
        elif here[path] != there[path]:
            lines.append(
                "%s differs, %d bytes here and %d bytes on %s"
                % (path, here[path][1], there[path][1], recorded.get("arch"))
            )
    return lines


# The player, the digital audio player the wasabi service runs on, builds for a
# tier 3 target with no prebuilt standard library, so its standard library
# builds from the rust-src of one pinned nightly, statically linked with soft
# float.
PLAYER_TARGET = "mipsel-unknown-linux-musl"
PLAYER_TOOLCHAIN = "nightly-2026-09-20"
PLAYER_BUILD_STD = ("-Z", "build-std=std,panic_abort")

# The C library of the player, musl 1.2.5 from its release archive, under the
# SHA-256 of the copy the audit built.
MUSL_URL = "https://musl.libc.org/releases/musl-1.2.5.tar.gz"
MUSL_SHA256 = "a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4"
MUSL_CACHE = "musl-1.2.5"
MUSL_DIR = "musl-1.2.5"

# The code clang writes for the player: MIPS32 release 2 with soft float, the
# CPU and the float ABI the target specification of rustc gives the triple.
PLAYER_CFLAGS = ("--target=mipsel-linux-musl", "-march=mips32r2", "-msoft-float")

# The standard library links libunwind.a on a musl target with crt-static,
# which the build of Rust compiles from the LLVM sources rust-src carries with
# these flags, the defaults of the cc crate among them. The last C++ flag is
# the form clang 21 names for `-fvisibility-global-new-delete-hidden` of that
# step.
UNWIND_FLAGS = (
    "-O3",
    "-ffunction-sections",
    "-fdata-sections",
    "-fPIC",
    "-fstrict-aliasing",
    "-funwind-tables",
    "-fvisibility=hidden",
    "-D_LIBUNWIND_DISABLE_VISIBILITY_ANNOTATIONS",
    "-D_LIBUNWIND_IS_NATIVE_ONLY=1",
)
UNWIND_CXX_FLAGS = ("-nostdinc++", "-fno-exceptions", "-fno-rtti", "-fvisibility-global-new-delete=force-hidden")
UNWIND_SOURCES = (
    "Unwind-sjlj.c",
    "UnwindLevel1-gcc-ext.c",
    "UnwindLevel1.c",
    "UnwindRegistersRestore.S",
    "UnwindRegistersSave.S",
    "Unwind-EHABI.cpp",
    "Unwind-seh.cpp",
    "libunwind.cpp",
)

# The version of the stamp of a kit, which names what its parts are built from.
KIT_VERSION = 1

# The programs a kit takes from the search path.
KIT_PROGRAMS = ("clang", "clang++", "ld.lld", "llvm-ar", "llvm-ranlib", "make")


class PlayerNotBuilt(RuntimeError):
    """A part a build of the player needs is missing, which a notebook prints as NOT RUN with this reason."""


@dataclass(frozen=True)
class PlayerKit:
    """The parts of a build of the player: the nightly, the sysroot of musl and the unwinder."""

    toolchain: str
    rustc: str
    clang: str
    sysroot: Path
    unwind: Path
    musl: Path
    built: bool

    def build_args(self) -> list:
        """Returns the arguments of a build for the player's target with its standard library from source."""
        return ["--target", PLAYER_TARGET, *PLAYER_BUILD_STD]

    def rustflags(self) -> list:
        """Returns the flags of rustc that link the player statically over the sysroot.

        The driver clang runs ld.lld with the start files of musl around the
        objects of rustc, the libraries of the sysroot and libunwind.a. The
        target links dynamically unless `crt-static` is asked for, and with it
        rustc would pass start files of its own, which a tier 3 target does not
        ship, so `link-self-contained=no` leaves them to the sysroot. The flag
        `-Qunused-arguments` keeps clang quiet about the `-no-pie` rustc
        passes, which the build prints as a warning of the lint
        `linker_messages` otherwise, a lint that `-D warnings` leaves a
        warning.
        """
        lib = self.sysroot / "lib"
        flags = ["-C", "linker=%s" % self.clang]
        for flag in (*PLAYER_CFLAGS, "-fuse-ld=lld", "-nostartfiles", "-Qunused-arguments"):
            flags += ["-C", "link-arg=%s" % flag]
        flags += ["-C", "link-self-contained=no", "-C", "target-feature=+crt-static"]
        flags += ["-L", str(lib), "-L", str(self.unwind)]
        flags += ["-Z", "pre-link-arg=%s" % (lib / "crt1.o"), "-Z", "pre-link-arg=%s" % (lib / "crti.o")]
        flags += ["-C", "link-arg=%s" % (lib / "crtn.o")]
        return flags


def player_kit(work, cargo_bin, root=None, offline: bool = False, jobs: int = 3,
               timeout_s: float = BUILD_TIMEOUT_S) -> PlayerKit:
    """Returns the kit of a build of the player, building the parts `work` does not hold yet.

    The nightly `PLAYER_TOOLCHAIN` with its rust-src comes from the rustup
    home that `cargo_bin` belongs to, the archive of musl from
    `tests/.cache/musl-1.2.5/` through `fetch_url`, checked by its SHA-256, and
    clang, ld.lld, llvm-ar and make from the search path. The call configures
    musl for `mipsel-linux-musl` without shared objects and builds it with
    clang into `work/sysroot`, and the unwinder of LLVM from the rust-src of
    the nightly into `work/unwind/libunwind.a`, with `jobs` jobs of make. A
    stamp names what the parts are built from, so a later call with the same
    nightly, archive, clang and flags builds nothing.

    Raises `PlayerNotBuilt` naming the missing part: the nightly or its
    rust-src, a program of the search path, or the archive of musl when its
    fetch fails or answers another digest.
    """
    work = Path(work)
    rustc = Path(cargo_bin) / "rustc"
    try:
        version = run([rustc, "+" + PLAYER_TOOLCHAIN, "-vV"], timeout_s=120).out.strip()
        rust_sysroot = Path(run([rustc, "+" + PLAYER_TOOLCHAIN, "--print", "sysroot"], timeout_s=120).out.strip())
    except CommandFailed as failed:
        said = failed.result.err.strip().splitlines()
        raise PlayerNotBuilt("the toolchain %s does not run: %s%s" % (
            PLAYER_TOOLCHAIN, failed.reason, ", " + said[-1] if said else "")) from None
    except OSError as failed:
        raise PlayerNotBuilt("the toolchain %s does not run: %s" % (PLAYER_TOOLCHAIN, failed)) from None
    source = rust_sysroot / "lib" / "rustlib" / "src" / "rust"
    libunwind = source / "src" / "llvm-project" / "libunwind"
    if not (source / "library" / "std" / "Cargo.toml").is_file() or not (libunwind / "src" / "libunwind.cpp").is_file():
        raise PlayerNotBuilt("the toolchain %s holds no rust-src with the sources of std and libunwind" % PLAYER_TOOLCHAIN)
    tools = {name: shutil.which(name) for name in KIT_PROGRAMS}
    missing = [name for name in KIT_PROGRAMS if not tools[name]]
    if missing:
        raise PlayerNotBuilt("the search path holds no %s" % ", ".join(missing))
    clang_version = run([tools["clang"], "--version"], timeout_s=60).out.splitlines()[0].strip()
    try:
        archive = fetch_url(MUSL_URL, MUSL_SHA256, MUSL_CACHE, offline=offline, root=root)
    # Every way the fetch fails, a refused digest, the network or the server,
    # is a reason the row prints as NOT RUN.
    except Exception as refused:
        raise PlayerNotBuilt("the archive of musl is not at hand: %s" % refused) from None
    stamp = {
        "version": KIT_VERSION,
        "toolchain": version,
        "clang": clang_version,
        "musl": MUSL_SHA256,
        "cflags": list(PLAYER_CFLAGS),
        "unwind": list(UNWIND_FLAGS + UNWIND_CXX_FLAGS),
    }
    sysroot, unwind, stamp_path = work / "sysroot", work / "unwind", work / "kit.json"
    held = json.loads(stamp_path.read_text(encoding="utf-8")) if stamp_path.is_file() else None
    built = not (held == stamp and (sysroot / "lib" / "libc.a").is_file() and (unwind / "libunwind.a").is_file())
    if built:
        shutil.rmtree(work, ignore_errors=True)
        sources = work / "src"
        sources.mkdir(parents=True)
        with tarfile.open(archive) as tar:
            tar.extractall(sources, filter="data")
        musl = sources / MUSL_DIR
        env = {"CC": " ".join([tools["clang"], *PLAYER_CFLAGS]), "AR": tools["llvm-ar"], "RANLIB": tools["llvm-ranlib"],
               "CFLAGS": None, "LDFLAGS": None, "CROSS_COMPILE": None}
        run(["./configure", "--target=mipsel-linux-musl", "--prefix=%s" % sysroot, "--disable-shared"], cwd=musl,
            env=env, timeout_s=timeout_s)
        run([tools["make"], "-j%d" % jobs], cwd=musl, env=env, timeout_s=timeout_s)
        run([tools["make"], "install"], cwd=musl, env=env, timeout_s=timeout_s)
        unwind.mkdir(parents=True)
        common = [*PLAYER_CFLAGS, "--sysroot=%s" % sysroot, *UNWIND_FLAGS, "-I%s" % (libunwind / "include")]
        objects = []
        for name in UNWIND_SOURCES:
            cxx = name.endswith(".cpp")
            obj = unwind / (name.rsplit(".", 1)[0] + ".o")
            compiler = [tools["clang++"], *common, *UNWIND_CXX_FLAGS] if cxx else [tools["clang"], *common]
            run([*compiler, "-c", libunwind / "src" / name, "-o", obj], timeout_s=timeout_s)
            objects.append(obj)
        run([tools["llvm-ar"], "rcs", unwind / "libunwind.a", *objects], timeout_s=timeout_s)
        stamp_path.write_text(json.dumps(stamp, indent=1) + "\n", encoding="utf-8")
    return PlayerKit(PLAYER_TOOLCHAIN, version.splitlines()[0], tools["clang"], sysroot, unwind, archive, built)
