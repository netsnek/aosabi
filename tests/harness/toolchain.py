# SPDX-License-Identifier: MIT
"""Aosabi harness: the one flag set of a shipped build and the toolchain it runs on.

Item Z21 of that plan keeps the flags of such a build in this module alone, so a flag that a bump of the pin renames
is one edit here: the variable `RUSTC_BOOTSTRAP=1`, the `-Z` flags of each card, the features of the standard
library of each card, and the remap of the source of the standard library.

The remap. With the component `rust-src` beside a compiler, rustc writes the panic locations of the inlined code of
the standard library with the path of the machine, and cargo keeps the artifacts built in that window. The flag
`source_remap` names, `--remap-path-prefix=<sysroot>/lib/rustlib/src/rust=` `/rustc/<commit>`, gives those locations
the path the prebuilt standard library carries, so a build with the component writes no path of the machine. While
the component is absent the flag matches no path, and a build with it writes the bytes of a build without it, while
cargo keeps the artifacts of both side by side. So `harness_env` gives the flag to every build of the harness
through one call in fixtures.py. The last of several matching prefixes wins, and the private copy lies inside the
tree, so a build of the harness on it names the remap of the source after the prefix of the tree. The release build
of the command line names the prefix of the tree after the remap the harness gives it, and it writes the bytes of
the build on the pin all the same.

The private copy. The shared pin carries no `rust-src` until step 3 of Z21, so tools/size_toolchain.py makes a hard
linked copy of the pin under target/size/toolchain/ that alone carries it, and `private_copy` finds it through the
variable AOSABI_SIZE_TOOLCHAIN or at that place. A cargo call runs a toolchain through the proxies of the rustup
home with RUSTUP_TOOLCHAIN naming the pin or the path of the copy, never the default of the machine, which is
another stable.

The options. A build takes `RUSTC_BOOTSTRAP=1` only where a notebook measures it, in the environment of that one
call, and a dev build takes no `-Z` flag and no bootstrap. A notebook takes the flags of a set from here, the cargo
arguments of `Options.cargo_args`, the `--config` arguments of `Options.config`, the rustc flags of
`Options.rustflags` and the flags the wrapper of Z24 adds, `NO_DEBUG_TEXT` and `NO_LOCATION_DETAIL`, and writes none
of them again.

The switch. Item Z23 puts every option of the standard library from source behind the switch `AOSABI_STD`,
`prebuilt` by default or `source`, which the command line reads and the harness passes through. The command line
gives a release build of a card at `source` the set its table names, `RUSTC_BOOTSTRAP=1`, the crates of
`-Zbuild-std`, the wrapper of Z24 and a target directory of the value, and puts the binary where a build at
`prebuilt` puts it. It refuses a toolchain with no rust-src, and the shared pin carries none until step 3 of Z21, so
`switch_env` gives a release build through the command line at `source` the private copy as RUSTUP_TOOLCHAIN, the
stand in of that step, and nothing at `prebuilt` and nothing to a dev build, and `cli_toolchain` gives every other
call of the command line of a notebook the same copy at `source` through `run` of procs.py. Where the copy is
missing a build at `source` runs on the pin and the command line refuses it with `RUST_SRC_REFUSAL`, which names the
command that adds the component, and the harness never adds it.

The copies a bump edits as well. The standalone tool tools/size_toolchain.py
runs without the harness, so it keeps its own copies of `PIN`, `PIN_COMMIT`,
`HOST`, `VARIABLE`, `MARK` and `DOWNLOAD_FAILED`, of the place this module
names `COPY_PLACE`, which its function `copy_path` builds, and of the flags of
its warming build, `-Zbuild-std=std,panic_abort` and `-Zbuild-std-features=`
under `RUSTC_BOOTSTRAP=1` with the remap of `source_remap`. A bump of the pin or
a flag it renames edits those copies in the same change as this module.
"""

from __future__ import annotations

import os
import shlex
import tomllib
from dataclasses import dataclass, replace
from pathlib import Path

from .procs import run

#: The pinned toolchain of rust-toolchain.toml.
PIN = "1.95.0"
#: The commit `rustc -vV` of the pin reports, the path the prebuilt standard library names its source by.
PIN_COMMIT = "59807616e1fa2540724bfbac14d7976d7e4a3860"
#: The host of the development machine.
HOST = "aarch64-unknown-linux-gnu"
#: The variable that names the place of the private copy.
VARIABLE = "AOSABI_SIZE_TOOLCHAIN"
#: The file tools/size_toolchain.py writes into a whole copy.
MARK = ".aosabi-size-toolchain.json"
#: The place of the copy under the root of the tree when the variable is unset.
COPY_PLACE = Path("target") / "size" / "toolchain" / ("%s-%s" % (PIN, HOST))
#: The variable that lets the pinned stable compiler take the `-Z` flags.
BOOTSTRAP = {"RUSTC_BOOTSTRAP": "1"}
#: Where the component rust-src puts the source of the standard library in a sysroot.
SOURCE = Path("lib") / "rustlib" / "src" / "rust"
#: The path the prebuilt standard library carries for its source.
REMAPPED = "/rustc/" + PIN_COMMIT
#: The separator of CARGO_ENCODED_RUSTFLAGS.
FLAG_SEPARATOR = "\x1f"
#: The exit status of tools/size_toolchain.py when a download still fails after its retries, EX_TEMPFAIL of
#: sysexits.h, on which a notebook prints the rows that need the private copy as WAITING, since such a failure is
#: retried and never judged.
DOWNLOAD_FAILED = 75

#: The crates `-Zbuild-std` builds: the whole standard library for a card with an operating system and the Worker,
#: and `core` and `alloc` alone for the ESP32-C3.
BUILD_STD = {"std": "std,panic_abort", "core": "core,alloc"}
#: The features of the standard library the prebuilt one carries, which L1 keeps and L2 drops.
PREBUILT_FEATURES = "backtrace,panic-unwind"
#: The feature of L3, the small algorithms of core, alloc and std.
SMALL = "optimize_for_size"

#: An empty value counts as unset, and unset is `prebuilt`.
SWITCH = "AOSABI_STD"
#: The value of the switch that builds the stock form on the prebuilt standard library of the pin.
PREBUILT = "prebuilt"
#: The value of the switch that builds the standard library of a release build from source with the card's set.
FROM_SOURCE = "source"
#: The target directory of a release build at the switch source, relative to its project, one target directory per
#: value of the switch, so the two forms never build each other's crates again. The command line names it to cargo
#: as `std-source` inside the target directory of the project and keeps its generated workspace in `target/aosabi/`
#: at both values. It lies inside `target/`, which every copy of a twin the harness and the notebooks write leaves
#: out, so no copy takes the build of the other value with it.
SOURCE_TARGET_DIR = "target/std-source"
SMALL_SWITCH = "AOSABI_STD_SMALL"
#: The command that adds the component rust-src to the pin, which a build at the switch source needs and no build
#: runs.
RUST_SRC_COMMAND = "rustup component add rust-src --toolchain %s" % PIN
#: The refusal of a build at the switch source on a toolchain that holds no rust-src, as the command line writes it.
RUST_SRC_REFUSAL = ("the switch AOSABI_STD at source builds the standard library of the card from source, and the "
                    "toolchain %s holds no rust-src: run `%s` once, which no build runs for you"
                    % (PIN, RUST_SRC_COMMAND))
#: The variable cargo reads the features of `-Zbuild-std-features` from, which the command line leaves as the
#: environment names it.
BUILD_STD_FEATURES = "CARGO_UNSTABLE_BUILD_STD_FEATURES"
#: The program of the command line whose calls `cli_toolchain` gives the private copy at the switch source.
CLI_PROGRAM = "cargo-aosabi"


def rustup_tool(name: str) -> Path:
    """Returns the proxy `name` of the rustup home, which runs the toolchain RUSTUP_TOOLCHAIN names."""
    return Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo") / "bin" / name


_SYSROOTS = {}


def sysroot(toolchain=PIN) -> Path:
    """Returns the sysroot of `toolchain`, the name of the pin or the path of a toolchain, read once per kernel."""
    key = str(toolchain)
    if key not in _SYSROOTS:
        done = run([rustup_tool("rustc"), "--print", "sysroot"], env={"RUSTUP_TOOLCHAIN": key}, timeout_s=60)
        _SYSROOTS[key] = Path(done.out.strip())
    return _SYSROOTS[key]


def has_source(toolchain=PIN) -> bool:
    """Returns whether the sysroot of `toolchain` carries the component rust-src."""
    return (sysroot(toolchain) / SOURCE).is_dir()


def commit(toolchain=PIN) -> str:
    """Returns the commit the rustc of `toolchain` reports in `rustc -vV`."""
    done = run([rustup_tool("rustc"), "-vV"], env={"RUSTUP_TOOLCHAIN": str(toolchain)}, timeout_s=60)
    for line in done.out.splitlines():
        if line.startswith("commit-hash:"):
            return line.split(":", 1)[1].strip()
    return ""


def copy_path(root) -> Path:
    """Returns the place of the private copy: the path AOSABI_SIZE_TOOLCHAIN names, or its place under `root`."""
    named = os.environ.get(VARIABLE)
    return Path(named) if named else Path(root) / COPY_PLACE


def private_copy(root) -> Path | None:
    """Returns the private copy of the pin with rust-src when tools/size_toolchain.py made it whole, or None."""
    copy = copy_path(root)
    if (copy / MARK).is_file() and (copy / SOURCE / "library" / "Cargo.lock").is_file():
        return copy
    return None


def source_remap(sysroot_path) -> str:
    """Returns the flag that remaps the source of the standard library under `sysroot_path` to its prebuilt path."""
    return "--remap-path-prefix=%s=%s" % (Path(sysroot_path) / SOURCE, REMAPPED)


def with_flags(env: dict, flags) -> dict:
    """Returns `env` with `flags` after the flags of its CARGO_ENCODED_RUSTFLAGS, or else of its RUSTFLAGS.

    Cargo reads CARGO_ENCODED_RUSTFLAGS before RUSTFLAGS, so the flags join
    whichever of the two the environment sets, and RUSTFLAGS when it sets
    neither. A value of None in `env` stands for a variable the child does
    not see, as `run` of procs.py takes it.
    """
    full = dict(env)
    encoded = full.get("CARGO_ENCODED_RUSTFLAGS")
    if encoded is not None:
        held = [flag for flag in encoded.split(FLAG_SEPARATOR) if flag]
        full["CARGO_ENCODED_RUSTFLAGS"] = FLAG_SEPARATOR.join(held + list(flags))
    else:
        held = (full.get("RUSTFLAGS") or "").split()
        full["RUSTFLAGS"] = " ".join(held + list(flags))
    return full


def harness_env(env=None, toolchain=None) -> dict:
    """Returns `env` with the remap of the source of the standard library of the toolchain a build runs.

    The toolchain is `toolchain`, else the one RUSTUP_TOOLCHAIN names in `env`
    or in the process, else the pin, which rust-toolchain.toml names for every
    call inside the tree. A flags variable the process holds and `env` does not
    name is carried over, since the value `env` gives replaces it in the child.
    """
    full = dict(env or {})
    for key in ("CARGO_ENCODED_RUSTFLAGS", "RUSTFLAGS"):
        if key not in full and os.environ.get(key) is not None:
            full[key] = os.environ[key]
    chosen = toolchain or full.get("RUSTUP_TOOLCHAIN") or os.environ.get("RUSTUP_TOOLCHAIN") or PIN
    return with_flags(full, [source_remap(sysroot(chosen))])


def std_packages(copy) -> list[str]:
    """Returns the names of the packages of the standard library's own lock in the sysroot `copy`."""
    with (Path(copy) / SOURCE / "library" / "Cargo.lock").open("rb") as handle:
        packages = tomllib.load(handle).get("package", [])
    return sorted({package["name"] for package in packages})


def framework_crates(root) -> list[str]:
    """Returns the crate names of the framework: the members of the workspace at `root` and the forks under
    third_party/rust/, which the options per crate of Z24 reach."""
    root = Path(root)
    with (root / "Cargo.toml").open("rb") as handle:
        members = tomllib.load(handle)["workspace"]["members"]
    manifests = [root / member / "Cargo.toml" for member in members]
    manifests += sorted((root / "third_party" / "rust").glob("*/Cargo.toml"))
    names = set()
    for manifest in manifests:
        if not manifest.is_file():
            continue
        with manifest.open("rb") as handle:
            package = tomllib.load(handle).get("package", {})
        if "name" in package:
            names.add(package["name"].replace("-", "_"))
    return sorted(names)


@dataclass(frozen=True)
class Options:
    """One set of the levers of the plan, as a build the harness measures takes it.

    `build_std` is None for the prebuilt standard library, `std` or `core` for
    the crates of BUILD_STD, `features` the value of `-Zbuild-std-features`
    beside `small`, the feature of L3. `fmt_debug` is L4 on every crate, `none`
    or `shallow`, `location_detail` L5 on every crate, `per_crate` the form of
    Z24 that gives the framework's crates both and the standard library its
    locations alone, `immediate_abort` L6, `lld` the pin's LLD of L7 and `icf`
    its identical code folding of L8, `vfe` L9, `trim_paths` the `-Z
    trim-paths` of cargo in place of the hand written prefixes, and
    `std_level` the opt-level the crates of the standard library take through
    the overrides per package of Z23, None for the level of the card.
    """

    name: str = "stock"
    build_std: str | None = None
    features: str = ""
    small: bool = False
    fmt_debug: str | None = None
    location_detail: bool = False
    per_crate: bool = False
    immediate_abort: bool = False
    lld: bool = False
    icf: bool = False
    vfe: bool = False
    trim_paths: bool = False
    std_level: str | None = None

    @property
    def bootstrap(self) -> bool:
        """Returns whether the build needs `RUSTC_BOOTSTRAP=1`, since a flag of the set is unstable."""
        return bool(self.build_std or self.fmt_debug or self.location_detail or self.per_crate
                    or self.immediate_abort or self.vfe or self.trim_paths)

    def cargo_args(self) -> list[str]:
        """Returns the `-Z` flags of cargo the set takes, before the other arguments of the call."""
        args = []
        if self.build_std:
            args.append("-Zbuild-std=%s" % BUILD_STD[self.build_std])
            features = [name for name in self.features.split(",") if name] + ([SMALL] if self.small else [])
            args.append("-Zbuild-std-features=%s" % ",".join(features))
        if self.trim_paths:
            args.append("-Ztrim-paths")
        return args

    def rustflags(self) -> list[str]:
        """Returns the flags of rustc the set gives every unit compiled for the card's target."""
        flags = []
        if self.fmt_debug:
            flags.append("-Zfmt-debug=%s" % self.fmt_debug)
        if self.location_detail:
            flags.append("-Zlocation-detail=none")
        if self.immediate_abort:
            flags += ["-Zunstable-options", "-Cpanic=immediate-abort"]
        if self.vfe:
            flags += ["-Clto=fat", "-Zvirtual-function-elimination"]
        return flags

    def link_flags(self, toolchain_root) -> list[str]:
        """Returns the flags that link an ELF card with the pin's LLD, with identical code folding for `icf`."""
        flags = []
        if self.lld or self.icf:
            flags += ["-Clink-arg=-B%s" % (Path(toolchain_root) / "lib" / "rustlib" / HOST / "bin" / "gcc-ld"),
                      "-Clink-arg=-fuse-ld=lld"]
        if self.icf:
            flags.append("-Clink-arg=-Wl,--icf=all")
        return flags

    def config(self, profile: str, std_names) -> list[str]:
        """Returns the `--config` arguments of the set: the level of the standard library and trim-paths."""
        args = []
        if self.std_level is not None:
            # A level of digits is an integer of TOML and a letter level a string.
            level = self.std_level if self.std_level.isdigit() else '"%s"' % self.std_level
            for name in std_names:
                args += ["--config", 'profile.%s.package."%s".opt-level=%s' % (profile, name, level)]
        if self.trim_paths:
            args += ["--config", 'profile.%s.trim-paths="all"' % profile]
        return args

    def but(self, **changes) -> Options:
        """Returns a copy of the set with `changes`."""
        return replace(self, **changes)


#: The stock build, the prebuilt standard library of the pin and no lever.
STOCK = Options()
#: The rungs of the ladder of the plan's lever table, each on the one it names.
L1 = Options("L1, std inside the link time optimization", build_std="std", features=PREBUILT_FEATURES)
L2 = Options("L2, the symbolizer out", build_std="std")
L3 = L2.but(name="L3, optimize_for_size", small=True)
L4 = L3.but(name="L4, no Debug text on every crate", fmt_debug="none")
L5 = L4.but(name="L5, no location detail on every crate", location_detail=True)
L6 = L2.but(name="L6, the immediate abort over L2", immediate_abort=True)
L6_SMALL = L3.but(name="L6, the immediate abort over L3", immediate_abort=True)
EVERY = L6_SMALL.but(name="every option of the standard library", fmt_debug="none", location_detail=True)

#: The Linux cards and the player take L1 and L2 with the Debug text and the location detail per crate of Z24. The
#: Worker and the modules of a page take L1 and L2 with the standard library held at opt-level 3, the default of Z23
#: on a card at `z`, and the immediate abort of Z25, since no panic text of theirs reaches anyone. L3 joins a set
#: only on the speed verdict of Z26, L9 only on the rule of Z37, and the linker stands behind its own switch
#: AOSABI_LINKER.
SHIPPED = {
    "server": Options("the set of the Linux cards", build_std="std", per_crate=True),
    "server-legacy": Options("the set of the Linux cards", build_std="std", per_crate=True),
    "docker": Options("the set of the Linux cards", build_std="std", per_crate=True),
    "web": Options("the set of the Linux cards", build_std="std", per_crate=True),
    "player": Options("the set of the player", build_std="std", per_crate=True),
    "worker": Options("the set of the Worker", build_std="std", immediate_abort=True, std_level="3"),
    "graphtp.wasm": Options("the set of the Worker", build_std="std", immediate_abort=True, std_level="3"),
    "subset": Options("the set of the release image of the ESP32-C3 on wasm32", build_std="std",
                      immediate_abort=True, std_level="3"),
    "esp32c3": Options("the set of the release image of the ESP32-C3", build_std="core", immediate_abort=True,
                       std_level="3"),
    "esp32c3-debug": Options("the set of the debug image of the ESP32-C3", build_std="core", std_level="3"),
}
#: The cards whose release build goes through the command line, which `switch_env` serves: their sets need no
#: flag of rustc and no `--config` beside the crates of the standard library and the wrapper of Z24.
COMMAND_LINE_CARDS = ("server", "server-legacy", "docker", "web", "player")


#: The flag of L4 that the wrapper of Z24 adds to a unit of the framework's crates compiled for the target, as
#: `Options.rustflags` writes it.
NO_DEBUG_TEXT = Options(fmt_debug="none").rustflags()[0]
#: The flag of L5 that the wrapper of Z24 adds to a unit of the framework's crates and to `core`, `alloc` and `std`
#: compiled for the target, as `Options.rustflags` writes it.
NO_LOCATION_DETAIL = Options(location_detail=True).rustflags()[0]


def per_crate_wrapper(path, crates, remap=None, chain=None) -> Path:
    """Writes the wrapper of rustc of Z24 at `path` and returns it.

    The wrapper adds NO_DEBUG_TEXT and NO_LOCATION_DETAIL to a unit whose command line holds `--target` and the name
    of one of `crates`, adds NO_LOCATION_DETAIL alone to `core`, `alloc` and `std` built for the target, and passes
    every other unit as it is, so a host unit, a build script and a proc macro take no flag of the set and the app
    keeps its Debug text and its locations. Cargo runs it as `<wrapper> <rustc> <arguments>`. The argument `remap`,
    the flag of `source_remap`, ends the command line of every unit for the target, since the last of several
    matching prefixes wins and the command line names the prefix of the tree after the flags of the environment,
    which holds the private copy. The argument `chain` is a wrapper the user set already, which the script runs with
    rustc and the arguments in place of rustc.
    """
    path = Path(path)
    names = " ".join(sorted(crates))
    runner = "%s \"$rustc\"" % shlex.quote(str(chain)) if chain else "\"$rustc\""
    tail = " %s" % shlex.quote(remap) if remap else ""
    lines = [
        "#!/bin/sh",
        "# The wrapper of rustc of item Z24, written by tests/harness/toolchain.py.",
        "rustc=\"$1\"",
        "shift",
        "target=no",
        "name=",
        "previous=",
        "for argument in \"$@\"; do",
        "    case \"$argument\" in --target|--target=*) target=yes ;; esac",
        "    if [ \"$previous\" = \"--crate-name\" ]; then name=\"$argument\"; fi",
        "    previous=\"$argument\"",
        "done",
    ]
    units = []
    if names:
        units.append("    case \" %s \" in *\" $name \"*) exec %s \"$@\" %s %s%s ;; esac" % (
            names, runner, NO_DEBUG_TEXT, NO_LOCATION_DETAIL, tail))
        units.append("    case \"$name\" in core|alloc|std) exec %s \"$@\" %s%s ;; esac" % (
            runner, NO_LOCATION_DETAIL, tail))
    if tail:
        units.append("    exec %s \"$@\"%s" % (runner, tail))
    # The shell refuses a block with no command, so a wrapper with no crate
    # and no remap passes every unit as it is with no block for the target.
    if units:
        lines += ["if [ \"$target\" = yes ]; then", *units, "fi"]
    lines += ["exec %s \"$@\"" % runner, ""]
    text = "\n".join(lines)
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.is_file() or path.read_text(encoding="utf-8") != text:
        # The new text takes the name in one step, so a build that runs the
        # wrapper while another entry writes it never reads half a script.
        staged = path.with_name("%s.%d" % (path.name, os.getpid()))
        staged.write_text(text, encoding="utf-8")
        staged.chmod(0o755)
        os.replace(staged, path)
    path.chmod(0o755)
    return path


def _value(env, name):
    """Returns the value of the variable `name` in `env` where `env` names it, else in the process, else empty."""
    if env is not None and name in env:
        return env[name] or ""
    return os.environ.get(name) or ""


def switch_value(env=None) -> str:
    """Returns the value of the switch AOSABI_STD that `env` names, else the process, else `prebuilt`.

    A value of None in `env` stands for a variable the build does not see, as
    `run` of procs.py takes it, so it reads as unset.
    """
    value = _value(env, SWITCH) or PREBUILT
    if value not in (PREBUILT, FROM_SOURCE):
        raise ValueError("%s is %r, and its values are %s and %s" % (SWITCH, value, PREBUILT, FROM_SOURCE))
    return value


def small_value(env=None):
    """Returns True or False where AOSABI_STD_SMALL of `env` or of the process asks for the feature of L3 or
    against it, and None where it is unset and the set of the card stands."""
    value = _value(env, SMALL_SWITCH)
    if not value:
        return None
    if value not in ("on", "off"):
        raise ValueError("%s is %r, and its values are on and off" % (SMALL_SWITCH, value))
    return value == "on"


def card_set(card, env=None) -> Options:
    """Returns the set `card` takes at the switch source, with the feature of L3 as AOSABI_STD_SMALL asks."""
    options = SHIPPED[card]
    small = small_value(env)
    return options if small is None else options.but(small=small)


def command_line_env(card, root, env=None) -> dict:
    """Returns the variables a release build of `card` through the command line takes at `source` in the harness.

    The command line reads the switch and gives the build the set of its card itself, and refuses a toolchain with
    no rust-src, so the harness gives it the private copy as RUSTUP_TOOLCHAIN, the stand in of step 3 of Z21, and
    the features of the standard library where AOSABI_STD_SMALL asks for L3 or against it. Where the copy is missing
    it gives no toolchain, so the build runs on the pin and the command line refuses it with `RUST_SRC_REFUSAL`.
    """
    if card not in COMMAND_LINE_CARDS:
        raise ValueError("the card %s builds through no command line, so its set travels through size.variant" % card)
    full = {}
    copy = private_copy(root)
    if copy is not None:
        full["RUSTUP_TOOLCHAIN"] = str(copy)
    small = small_value(env)
    if small is not None:
        options = card_set(card, env)
        features = [name for name in options.features.split(",") if name] + ([SMALL] if options.small else [])
        full[BUILD_STD_FEATURES] = ",".join(features)
    return full


def switch_env(env, root, card="server") -> dict:
    """Returns `env` with the set of `card` where the switch reads `source`, and `env` as it is at `prebuilt`.

    The caller passes it the environment of a release build through the command line alone, since a dev build takes
    no `-Z` flag and no `RUSTC_BOOTSTRAP` at any value of the switch.
    """
    full = dict(env or {})
    if switch_value(full) != FROM_SOURCE:
        return full
    full.update(command_line_env(card, root, full))
    return full


_READS_SWITCH = {}


def reads_switch(program) -> bool:
    """Returns whether the program of the command line at `program` reads the switch AOSABI_STD, which a command line
    built from a tree before the close of round 1, the baseline of a notebook or `main` as merged among them, does
    not, read once per file and its time of change."""
    path = Path(program)
    try:
        stamp = path.stat()
    except OSError:
        return False
    key = (str(path), stamp.st_size, stamp.st_mtime_ns)
    if key not in _READS_SWITCH:
        _READS_SWITCH[key] = SWITCH.encode("ascii") in path.read_bytes()
    return _READS_SWITCH[key]


def cli_toolchain(argv, env) -> dict:
    """Returns the variables a call `argv` of the command line takes at the switch source, the private copy as
    RUSTUP_TOOLCHAIN, where `env` is the whole environment of the child.

    A notebook may run the program cargo-aosabi itself, for a build of the
    card docker among others, and the command line at `source` refuses the
    shared pin, which carries no rust-src before step 3 of Z21, so `run` and
    `serve` of procs.py give such a call the copy where the switch of its
    environment reads `source`, the environment names no toolchain of its own
    and the program reads the switch. A command line that reads no switch
    builds stock at every value, so it keeps the toolchain it finds, as does
    any other call and every call at `prebuilt`.
    """
    if not argv or Path(str(argv[0])).name != CLI_PROGRAM or env.get("RUSTUP_TOOLCHAIN"):
        return {}
    if (env.get(SWITCH) or PREBUILT) != FROM_SOURCE or not reads_switch(argv[0]):
        return {}
    for parent in Path(__file__).resolve().parents:
        if (parent / "rust-toolchain.toml").is_file():
            copy = private_copy(parent)
            return {} if copy is None else {"RUSTUP_TOOLCHAIN": str(copy)}
    return {}
