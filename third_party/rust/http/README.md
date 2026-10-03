# http 1.5.0, vendored

This directory holds the crate `http` 1.5.0, the types of hyper's public interface, as crates.io
published it and vendored the kernel way. It changes only through the commits of its series below,
each one behaviour, and the tool `tools/provenance` is the one writer of the files it imports
([CONTRIBUTING.md](../../../CONTRIBUTING.md) section 6).

The crate does not build without the standard library, and hyper's server path names it in its
public items, so the fork patches it for `no_std` with `alloc` in layer A and writes its integers
without `itoa` in layer C (the plan of M5, section The fork, facts 99 and 104). Layer U replaces the derived `Debug` of `header::IntoIter` with one that formats the entries not
yet yielded.

## Provenance

| What | Value |
| --- | --- |
| Crate | `http` |
| Version | 1.5.0, released on 2026-07-29 (PROVEN, https://crates.io/api/v1/crates/http/1.5.0, read on 2026-09-26) |
| Upstream | https://github.com/hyperium/http |
| Tag | `v1.5.0` at `16fc9a7b840c2181e7f8b37397c107b0ffcd050d` (PROVEN, `git ls-remote --tags`, read on 2026-09-26) |
| Commit the archive names | `e559023f67e3fad6ecc3ee91307be178e0f13626`, dirty |
| Archive | https://static.crates.io/crates/http/http-1.5.0.crate |
| SHA-256 of the archive | `918d3568bebf352712bc2ef3d46a8bcf1a75b373be6539de198e9105cbbf9ce0`, equal to the checksum crates.io publishes |
| Licence | `MIT OR Apache-2.0`, Copyright (c) 2017 http-rs authors |
| Notice | `NOTICE` sections 2 and 4 |
| SPDX line of commit S | `// SPDX-License-Identifier: MIT OR Apache-2.0` above the first line of every Rust file under `src/` |
| Lines and `unsafe` tokens | 14,603 lines of Rust under `src/` and 65 `unsafe` tokens as published, counted over the import set, the SPDX lines of commit S aside (PROVEN, `wc -l` of the Rust files under `src/` and `grep -ow unsafe` over the import set, on 2026-09-26) |

The archive names the commit `e559023f67e3fad6ecc3ee91307be178e0f13626` with `"dirty": true`, the
parent of the tag commit. The tag commit changes `CHANGELOG.md` and `Cargo.toml` alone, the version
bump, and the archive holds both files as the tag does, so the dirty tree was that bump.

## The import set

The import commit takes, byte for byte, the files of the archive that the build and the licence
need: `Cargo.toml` as cargo published it, `LICENSE-APACHE`, `LICENSE-MIT` and `src/**`, 24 files in
all (PROVEN, the import set listed against the archive on 2026-09-26). It leaves out every other
file of the archive, because the archive's own `README.md` would collide with this file, the
packaging records and the tests serve no build, and a `.gitignore` of an archive would drop files
from the commit. The SHA-256 of every file it leaves out stands below, so the verify mode checks
every file of the archive all the same (the plan of M5, section The fork, fact 92).

| File left out | SHA-256 |
| --- | --- |
| `.cargo_vcs_info.json` | `3c059d947727f372a8acde6605c6a3e07ebb38b5c868eca16d1202f4b018d78d` |
| `.github/FUNDING.yml` | `f1b6ab0526242e2b5e8833ce7887b4926ddd0f4d29fca04e827a0187aa2ae8b8` |
| `.github/workflows/ci.yml` | `8aa9b141f338f8a192b18f9a3e75cd8a71fff13ea1396c38cacd1b1a3d4e9055` |
| `.gitignore` | `f9b1ca6ae27d1c18215265024629a8960c31379f206d9ed20f64e0b2dcf79805` |
| `CHANGELOG.md` | `fe0f80a62d6dbbeccd1cec25a18f682aed48a272f2531b4a5e89859df7df66ee` |
| `Cargo.lock` | `109b4934906d271c629d8511df2289683f7e4a15ef2eb6a5cb597aa8d14c1799` |
| `Cargo.toml.orig` | `d746fe0ed51dfd2a4e92c9a8c2348278adea73671187bbb711f231cd3c19669b` |
| `README.md` | `b5bf3d27deab5741b4563ea0c42e34c86a35ae1a67cf1f0c771c80d3b0725118` |
| `tests/header_map.rs` | `5db45b086b15b51bb601c7d2aeb6d47a6bb53644a631fa4e9c55896972f1233e` |
| `tests/header_map_fuzz.rs` | `a1d3b3c6998f69113fe9288a71e1179077aca91e56bf3b215891695942936676` |
| `tests/status_code.rs` | `165cfa22544de42c9ce708be0921a445cd2b75f042192b7d677501fff79cb641` |

The manifest stays byte for byte, so it still names targets whose files the import set leaves out:
`header_map` of `test`, `header_map_fuzz` of `test`, `status_code` of `test`. Cargo resolves such a
target only when a command builds it, so the path dependency builds and `cargo check --lib` passes,
and the fork's own tests in the copy of the vendored crates check run `cargo test --lib --tests`
with the shipped `tests/` restored from the archive (the plan of M5, section The fork, fact 150).

## Verify

```text
python3 tools/provenance/extract.py --crates verify http
```

The mode reads the row `[[crate]]` of `tools/provenance/provenance.toml`, checks the SHA-256 of the
archive against the row and against the checksum crates.io publishes, compares every file of the
import set at the import commit with the archive, compares every file of the archive with the file
at the tag, and checks that this file records the digest of every file the import set leaves out.
The expected differences against the tag are the packaging records `.cargo_vcs_info.json` and
`Cargo.lock`, which the tag does not hold, and `Cargo.toml`, which cargo normalizes and whose
original `Cargo.toml.orig` equals the manifest at the tag, and the version bump of the tag commit.
Any other difference fails the mode. The vendored crates check of the project runs it in its section
Provenance.

## The series

The history of this directory is one line: the import commit I, the SPDX commit S, this README in
commit R, then layer A, layer C and layer U, the three layers of this crate. The command `python3
tools/provenance/extract.py --crates series http` lists the commits after I with their layer, id and
quirk ids, and holds each layer to its budget. It counts the patched lines of a layer by
`git diff --numstat` from the commit before the layer to its last commit, so a line that two commits
of the layer touch counts once. The count leaves out the SPDX lines, and it lists the files the
layer removes and the lines of this README apart. Each layer adds its subjects here with its last
commit (the plan of M5, sections Staying close to upstream and The patch series, one commit each).

| Id | Summary after the scope `third_party` |
| --- | --- |
| I | `http vendor i, import http 1.5.0 as published` |
| S | `http vendor s, spdx identifiers` |
| R | `http vendor r, the readme of provenance` |
| A1 | `http nostd a1, no_std with alloc, std behind the default feature` |
| A2 | `http nostd a2, core and alloc in place of std where both hold the item` |
| A3 | `http nostd a3, the error trait of core without std` |
| A4 | `http nostd a4, extensions over a vector without std` |
| A5 | `http nostd a5, the red state of the header map keyed by the crate's own hasher without std` |
| A6 | `http nostd a6, the conversion from a hash map of std with std alone` |
| C1 | `http trim c1, integers written without itoa` |
| U1 | `http debug u1, the debug of IntoIter formats the entries not yet yielded` |

| Layer | What it does | Ids | Patched lines in upstream files, at most | Lines in new files, at most |
| --- | --- | --- | ---: | ---: |
| A | Builds `no_std` with `alloc`, `std` behind the default feature | A1 to A6 | 700 | 150 |
| C | Integers written without `itoa` | C1 | 60 | 0 |
| U | The fork's own `Debug` of `header::IntoIter` | U1 | 80 | 0 |

The ceilings are ASSUMED, the series mode counts each layer in the vendored crates check, and a
count above its ceiling fails there.

Layer A builds the crate as `no_std` with `alloc` when the default feature `std` is off, and
changes nothing a build with `std` can see. It uncomments upstream's own line
`#![cfg_attr(not(feature = "std"), no_std)]`, and `std` turns on `bytes/std`, since `bytes` is named
without its default features. A build with `std` keeps upstream's minimum Rust version, 1.57, and a
build without it needs Rust 1.81, the version of `core::error::Error`. Without `std` the map of
`Extensions` is a vector searched in order, because an extension map holds a handful of entries and
no crate is added for a hash map, and the red state of `HeaderMap` keys the crate's own FNV-1a hash
with a second fixed offset basis in place of the random seed of `std`. A fixed key gives no
protection against an adversary who knows it. A map of at most 64 fields, the bound of the
benchmark variant of `esp32c3-devkitm1`, never reaches the red state, and a build with `std` keeps
`RandomState` (the plan of M5, section Layer A and facts 99, 103 and 104, owner question 14).

Layer C is the fork's own and is never offered upstream. Its one commit writes the integer of a
`HeaderValue` made from a number through `core::fmt::Write` into the `BytesMut` of the value, so
the crate `itoa`, which fails the word well known, leaves the manifest and the crate names `bytes`
alone (the plan of M5, section Layer C, trims, and fact 94).

Layer U is the fork's own. Its one commit U1 replaces the derived `Debug` of `header::IntoIter`
with one that formats `next` and `entries`, the entries not yet yielded, and ends with
`finish_non_exhaustive`, in 13 patched lines, 12 added and 1 removed, with no `unsafe` token added
or removed. The ceilings of the layer are 80 patched lines and none in new files.

## The features Aosabi builds

Commit 98 of M5, the wiring of the forks, names this fork in `aosabi-serve` with the feature `std`,
and no crate of the workspace names it before that commit, because the builds below need the layers
of the forks that reach it (the plan of M5, section The serial step, The wiring of the forks).

| Build | Features |
| --- | --- |
| `server`, `docker`, the player and the served twins, with or without `tls` and `h2c` | `std` |
| The benchmark variant of `esp32c3-devkitm1` | None, no `std` |
| The default build of `esp32c3-devkitm1`, and `worker` | None, the fork is not built |

## Staying close to upstream

The fork follows the newest release of the upstream line 1.5. A release that fixes an advisory of
RustSec or of the GitHub security advisories of `hyperium` affecting the pinned version is imported
within one day, another patch release within 30 days once the upstream check passes, a new minor
release at the next milestone boundary, and a new major release is the owner's decision. An import
is a new commit I' written by the mode `import`, followed by S written by the mode `spdx`, R and the
layers rebased onto it, each patch keeping its subject and its quirk id. A path a commit of layer C
removed resolves as removed. The command below imports every newer release in a clone under
`target/vendored-check/`, rebases the series onto it, runs the fork's own tests in a copy of the
clone's forks and resolves the clone's lock without `--locked` against the workspace's. Its report
names every step that waits, the builds and the upstream corpora until their files exist, and the
lock difference until commit 98 of M5 names the forks by path. The section Upstream releases of the
vendored crates check runs it when its parameter `upstream` is `newest`. The matrix entry
`vendored_upstream` of the plan, which is to run it weekly outside the closing sweep, is not in
`tests/matrix.yaml` yet.

```text
python3 tools/provenance/extract.py --crates check-release http
```
