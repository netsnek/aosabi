# http-body 1.1.0, vendored

This directory holds the crate `http-body` 1.1.0, the body trait of hyper's public interface, as
crates.io published it and vendored the kernel way. It changes only through the commits of its
series below, each one behaviour, and the tool `tools/provenance` is the one writer of the files it
imports ([CONTRIBUTING.md](../../../CONTRIBUTING.md) section 6).

The crate imports from `std` what `core` holds and takes `http` and `bytes` with their default
features, so the fork gives it `no_std` with `alloc` in one commit of layer A (the plan of M5,
section The fork, fact 100).

## Provenance

| What | Value |
| --- | --- |
| Crate | `http-body` |
| Version | 1.1.0, released on 2026-07-13 (PROVEN, https://crates.io/api/v1/crates/http-body/1.1.0, read on 2026-09-26) |
| Upstream | https://github.com/hyperium/http-body, the directory `http-body` |
| Tag | `v1.1.0` at `3396328602f7b147ae7b13f022c2b94dff9434e3` (PROVEN, `git ls-remote --tags`, read on 2026-09-26) |
| Commit the archive names | `2fb78de9c875c364b7eb1a1a117acc3b83ffb13a`, dirty |
| Archive | https://static.crates.io/crates/http-body/http-body-1.1.0.crate |
| SHA-256 of the archive | `ca2a8f2913ee65f60facd6a5905613afaa448497a0230cc41ce022d93290bc2c`, equal to the checksum crates.io publishes |
| Licence | `MIT`, Copyright (c) 2019-2026 Sean McArthur & Hyper Contributors |
| Notice | `NOTICE` sections 2 and 4 |
| SPDX line of commit S | `// SPDX-License-Identifier: MIT` above the first line of every Rust file under `src/` |
| Lines and `unsafe` tokens | 559 lines of Rust under `src/` and 2 `unsafe` tokens as published, counted over the import set, the SPDX lines of commit S aside (PROVEN, `wc -l` of the Rust files under `src/` and `grep -ow unsafe` over the import set, on 2026-09-26) |
| Reached by path | `http`, through commit W |

The archive names the commit `2fb78de9c875c364b7eb1a1a117acc3b83ffb13a` with `"dirty": true`, the
parent of the tag commit, and the package lives in the directory `http-body` of its repository. The
tag commit changes `http-body/CHANGELOG.md` and `http-body/Cargo.toml` alone, the version bump, and
the archive holds both files as the tag does. Its `LICENSE` is a symbolic link to `../LICENSE` at
the tag and its `README.md` comes from `../README.md` through the key `readme` of the manifest, and
cargo packages both as the files they name.

## The import set

The import commit takes, byte for byte, the files of the archive that the build and the licence
need: `Cargo.toml` as cargo published it, `LICENSE` and `src/**`, 5 files in all (PROVEN, the import
set listed against the archive on 2026-09-26). It leaves out every other file of the archive,
because the archive's own `README.md` would collide with this file, the packaging records and the
tests serve no build, and a `.gitignore` of an archive would drop files from the commit. The SHA-256
of every file it leaves out stands below, so the verify mode checks every file of the archive all
the same (the plan of M5, section The fork, fact 92).

| File left out | SHA-256 |
| --- | --- |
| `.cargo_vcs_info.json` | `0798144532ae8b6b36f66a1e807e28559a62352e6c9ab65f6446b3288025fe39` |
| `CHANGELOG.md` | `aecfef73d7ab6406d1f4044511fb6acbdfb8d51b7ffcc9b57cd1f5b0ac2dae34` |
| `Cargo.lock` | `ac6720405dab208b263f6b84aad18db9403fb338eb66c4a2cc543246d61072dd` |
| `Cargo.toml.orig` | `6526c42603e11a2f07f30487a682fc0b33a1186fe8e6499fba7af3895d90919d` |
| `README.md` | `0f90f61ee419eefd4104005ef6900445fafce9a710dd1989463f3cebaf0fafe8` |
| `tests/is_end_stream.rs` | `8f2ebcb3fc0ce4923f984469c4679966c047b5f9e8b0b59c32d37c91bbb33369` |

The manifest stays byte for byte, so it still names targets whose files the import set leaves out:
`is_end_stream` of `test`. Cargo resolves such a target only when a command builds it, so the path
dependency builds and `cargo check --lib` passes, and the fork's own tests in the copy of the
vendored crates check run `cargo test --lib --tests` with the shipped `tests/` restored from the
archive (the plan of M5, section The fork, fact 150).

## Verify

```text
python3 tools/provenance/extract.py --crates verify http-body
```

The mode reads the row `[[crate]]` of `tools/provenance/provenance.toml`, checks the SHA-256 of the
archive against the row and against the checksum crates.io publishes, compares every file of the
import set at the import commit with the archive, compares every file of the archive with the file
at the tag, and checks that this file records the digest of every file the import set leaves out.
The expected differences against the tag are the packaging records `.cargo_vcs_info.json` and
`Cargo.lock`, which the tag does not hold, and `Cargo.toml`, which cargo normalizes and whose
original `Cargo.toml.orig` equals the manifest at the tag, the symbolic link `LICENSE` and the
`README.md` packaged from the repository root, and the version bump of the tag commit. Any other
difference fails the mode. The vendored crates check of the project runs it in its section
Provenance.

## The series

The history of this directory is one line: the import commit I, the wiring commit W, the SPDX commit
S, this README in commit R, then layer A, the one layer of this crate. The command `python3
tools/provenance/extract.py --crates series http-body` lists the commits after I with their layer,
id and quirk ids, and holds each layer to its budget. It counts the patched lines of a layer by
`git diff --numstat` from the commit before the layer to its last commit, so a line that two commits
of the layer touch counts once. The count leaves out the SPDX lines, and it lists the files the
layer removes and the lines of this README apart. Each layer adds its subjects here with its last
commit (the plan of M5, sections Staying close to upstream and The patch series, one commit each).

| Id | Summary after the scope `third_party` |
| --- | --- |
| I | `http-body vendor i, import http-body 1.1.0 as published` |
| W | `http-body vendor w, reach http by path` |
| S | `http-body vendor s, spdx identifiers` |
| R | `http-body vendor r, the readme of provenance` |
| A1 | `http-body nostd a1, no_std with alloc, std behind the default feature` |

| Layer | What it does | Ids | Patched lines in upstream files, at most | Lines in new files, at most |
| --- | --- | --- | ---: | ---: |
| A | Builds `no_std` with `alloc`, `std` behind the default feature | A1 | 40 | 0 |

The ceiling is ASSUMED, the series mode counts the layer in the vendored crates check, and a count
above it fails there.

Layer A makes the crate `#![no_std]` with `extern crate alloc`, because the crate names nothing of
the standard library that `core` and `alloc` do not hold. The default feature `std` turns on
`http/std` and `bytes/std`, and the manifest names `http` and `bytes` without their default
features, because `bytes` named with its defaults would turn its own `std` on whatever `http` is
given (the plan of M5, section Layer A and facts 100 and 158).

## The features Aosabi builds

No crate of the workspace names this fork before commit 98 of M5, the wiring of the forks, because
the features the builds below name exist only once the layers have landed (the plan of M5, section
The serial step, The wiring of the forks).

| Build | Features |
| --- | --- |
| `server`, `docker`, the player and the served twins, with or without `tls` and `h2c` | `std` |
| The benchmark variant of `esp32c3-devkitm1` | None, no `std` |
| The default build of `esp32c3-devkitm1`, and `worker` | None, the fork is not built |

## Staying close to upstream

The fork follows the newest release of the upstream line 1.1. A release that fixes an advisory of
RustSec or of the GitHub security advisories of `hyperium` affecting the pinned version is imported
within one day, another patch release within 30 days once the upstream check passes, a new minor
release at the next milestone boundary, and a new major release is the owner's decision. An import
is a new commit I' written by the mode `import`, followed by W, S written by the mode `spdx`, R and
the layers rebased onto it, each patch keeping its subject and its quirk id. A path a commit of
layer C removed resolves as removed. The command below imports every newer release in a clone under
`target/vendored-check/`, rebases the series onto it, runs the fork's own tests in a copy of the
clone's forks and resolves the clone's lock without `--locked` against the workspace's. Its report
names every step that waits, the builds and the upstream corpora until their files exist, and the
lock difference until commit 98 of M5 names the forks by path. The section Upstream releases of the
vendored crates check runs it when its parameter `upstream` is `newest`. The matrix entry
`vendored_upstream` of the plan, which is to run it weekly outside the closing sweep, is not in
`tests/matrix.yaml` yet.

```text
python3 tools/provenance/extract.py --crates check-release http-body
```
