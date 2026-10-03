# h2 0.4.19, vendored

This directory holds the crate `h2` 0.4.19, the HTTP/2 implementation hyper's HTTP/2 server speaks
through, as crates.io published it and vendored the kernel way. It changes only through the commits
of its series below, each one behaviour, and the tool `tools/provenance` is the one writer of the
files it imports ([CONTRIBUTING.md](../../../CONTRIBUTING.md) section 6).

The crate brings `tokio`, `tokio-util`, `tracing`, `indexmap`, `slab`, `fnv`, `futures-sink`,
`futures-core` and `atomic-waker`, which the dependency rule does not admit where own code is
possible, so layer C writes or cuts each of them. HTTP/2 lies outside the byte contract,
so the crate has no layer A and no layer B (the plan of M5, sections The fork and Layer C, trims,
facts 128 and 155).

## Provenance

| What | Value |
| --- | --- |
| Crate | `h2` |
| Version | 0.4.19, released on 2026-08-24 (PROVEN, https://crates.io/api/v1/crates/h2/0.4.19, read on 2026-09-26) |
| Upstream | https://github.com/hyperium/h2 |
| Tag | `v0.4.19` at `d57d1b852fec9dda6d42d3454502006d52104da8` (PROVEN, `git ls-remote --tags`, read on 2026-09-26) |
| Commit the archive names | `d57d1b852fec9dda6d42d3454502006d52104da8`, not dirty |
| Archive | https://static.crates.io/crates/h2/h2-0.4.19.crate |
| SHA-256 of the archive | `ef8e5e5a340588f4452631496976cf8636d4a7ecf600239fdc27615d2530bc16`, equal to the checksum crates.io publishes |
| Licence | `MIT`, Copyright (c) 2017 h2 authors |
| Notice | `NOTICE` sections 2 and 4 |
| SPDX line of commit S | `// SPDX-License-Identifier: MIT` above the first line of every Rust file under `src/` |
| Lines and `unsafe` tokens | 22,328 lines of Rust under `src/` and 2 `unsafe` tokens as published, counted over the import set, the SPDX lines of commit S aside (PROVEN, `wc -l` of the Rust files under `src/` and `grep -ow unsafe` over the import set, on 2026-09-26) |
| Reached by path | `http`, through commit W |

The archive names the tag commit itself, not dirty.

## The import set

The import commit takes, byte for byte, the files of the archive that the build and the licence
need: `Cargo.toml` as cargo published it, `LICENSE` and `src/**`, 54 files in all (PROVEN, the
import set listed against the archive on 2026-09-26). It leaves out every other file of the archive,
because the archive's own `README.md` would collide with this file, the packaging records and the
tests serve no build, and a `.gitignore` of an archive would drop files from the commit. The SHA-256
of every file it leaves out stands below, so the verify mode checks every file of the archive all
the same (the plan of M5, section The fork, fact 92).

| File left out | SHA-256 |
| --- | --- |
| `.cargo_vcs_info.json` | `30727e36628a75a96b9a00affc394649318d9b8d31a80b9df54756d2a73cab3b` |
| `.github/FUNDING.yml` | `f1b6ab0526242e2b5e8833ce7887b4926ddd0f4d29fca04e827a0187aa2ae8b8` |
| `.github/workflows/CI.yml` | `fc066edc735d931dfd5e182bd02138d014e3decd6c1ec514044e6b99a5601aff` |
| `.gitignore` | `916f7c1138a5bbefc3bbcb67c578f81392bd4715d9700e5f6b08d6b1163618cb` |
| `CHANGELOG.md` | `a493e2d0034d46877091290aec7071da55a28080c3b3250e7d3d2598b2327a45` |
| `CONTRIBUTING.md` | `a91827cdd114cec4d15b5c6aefce40b418e37ad43c8dc80d063391364fefec42` |
| `Cargo.lock` | `f6ddcfc61abe89553ac41e8a5075e54fc7334289abc41a5caadcc477061cfef5` |
| `Cargo.toml.orig` | `6664ea9dee6a56004dc181d66b498653d9a00587680bf091327f74e6bb83e17e` |
| `README.md` | `5981f9df0b61ed7e098fb802b154f892401ceea00fd83cf43fc55b11a4dee303` |
| `benches/main.rs` | `747448f7ca43226ff36f497a3d2aa391dca451b33ac87642793ffdb21243e17b` |
| `examples/akamai.rs` | `3e79749881a9e823477cf98ee50d8d6f2687eda457d57606bcf4f96389d7533f` |
| `examples/client.rs` | `5ad136b838e9d55ae3d1fd8801cec4af88139b58864d6438f75d0e173eb3aeb3` |
| `examples/server.rs` | `8cc7927c7fe98a2474c41ab0094ba2dc7a2ae6a7f58ce342a4c05bb8056a63a5` |

The manifest stays byte for byte, so it still names targets whose files the import set leaves out:
`akamai` of `example`, `client` of `example`, `server` of `example`, `main` of `bench`. Cargo
resolves such a target only when a command builds it, so the path dependency builds and `cargo check
--lib` passes, and the fork's own tests in the copy of the vendored crates check run `cargo test
--lib --tests` (the plan of M5, section The fork, fact 150). The archive of `h2` ships no `tests/`,
so the copy restores no test file from it. The unit tests of the module `hpack` read the stories
under `fixtures/hpack/` of the repository, 418 files the archive does not ship either, so the copy
of section Tests takes that directory from the tag `v0.4.19` the tool keeps under
`target/vendored/git/h2/`, whose commit the verify mode checks against the row (PROVEN, `git
archive` of the tag's `fixtures/` on 2026-09-26).

## Verify

```text
python3 tools/provenance/extract.py --crates verify h2
```

The mode reads the row `[[crate]]` of `tools/provenance/provenance.toml`, checks the SHA-256 of the
archive against the row and against the checksum crates.io publishes, compares every file of the
import set at the import commit with the archive, compares every file of the archive with the file
at the tag, and checks that this file records the digest of every file the import set leaves out.
The expected differences against the tag are the packaging records `.cargo_vcs_info.json` and
`Cargo.lock`, which the tag does not hold, and `Cargo.toml`, which cargo normalizes and whose
original `Cargo.toml.orig` equals the manifest at the tag. Any other difference fails the mode. The
vendored crates check of the project runs it in its section Provenance.

## The series

The history of this directory is one line: the import commit I, the wiring commit W, the SPDX commit
S, this README in commit R, then layer C, the one layer of this crate. The command `python3
tools/provenance/extract.py --crates series h2` lists the commits after I with their layer, id and
quirk ids, and holds each layer to its budget. It counts the patched lines of a layer by
`git diff --numstat` from the commit before the layer to its last commit, so a line that two commits
of the layer touch counts once. The count leaves out the SPDX lines, and it lists the files the
layer removes and the lines of this README apart. Each layer adds its subjects here with its last
commit (the plan of M5, sections Staying close to upstream and The patch series, one commit each).

| Id | Summary after the scope `third_party` |
| --- | --- |
| I | `h2 vendor i, import h2 0.4.19 as published` |
| W | `h2 vendor w, reach http by path` |
| S | `h2 vendor s, spdx identifiers` |
| R | `h2 vendor r, the readme of provenance` |
| C1 | `h2 trim c1, log macros, spans and instrument that record nothing, without tracing` |
| C2 | `h2 trim c2, io traits of its own in place of tokio's` |
| C3 | `h2 trim c3, the frame reader and the buffered write without tokio-util` |
| C4 | `h2 trim c4, the map of stream ids without indexmap` |
| C5 | `h2 trim c5, the slab of streams without the slab crate` |
| C6 | `h2 trim c6, the hasher of the hpack table without fnv` |
| C7 | `h2 trim c7, the sink implementation of the codec cut` |
| C8 | `h2 trim c8, the ping wakers without atomic-waker` |
| C9 | `h2 trim c9, a stream trait of its own in place of futures-core's` |

| Layer | What it does | Ids | Patched lines in upstream files, at most | Lines in new files, at most |
| --- | --- | --- | ---: | ---: |
| C | Tracing, the IO traits of `tokio`, `tokio-util`, `indexmap`, `slab`, `fnv`, the sink of the codec, `atomic-waker` and `futures-core` written in the crate or cut | C1 to C9 | 600 | 800 |

The ceilings are ASSUMED, the series mode counts the layer in the vendored crates check, and a count
above its ceiling fails there.

Layer C lands in the order C1, C3 to C9, and C2, which gives the crate IO traits of its own in place
of those of `tokio`, lands last, directly before commit C3 of hyper, which implements them. Hyper
with `http2` builds after every commit but C2 of `h2`, because the `Compat` of hyper implements the
traits of `tokio` until C3, and the pair keeps that gap to one commit (the plan of M5, section
Staying close to upstream, which pairs the two). Each commit writes one crate in the crate or cuts
its use, in safe code, and none changes a byte on the wire.
The log macros of `src/tracing.rs` expand to `()` with no argument evaluated, so the bindings and
items that only a log call reads carry a leading underscore or `#[allow(dead_code)]`. The reader of
the frames in `src/codec/framed_read.rs` keeps the states of the length delimited reader of
`tokio-util` it replaces, reads into zeroed memory, and answers a frame larger than the maximum
frame size with the `GOAWAY` of `FRAME_SIZE_ERROR`. The map of stream ids keeps the insertion order
and removes by swap, the slab keeps its keys and reuses the last removed, the hash of the HPACK table
is FNV-1a with the offset basis of the crate `fnv`, and the wakers of a user ping sit under a `Mutex`.
The `Sink` of the codec has no caller and is cut, and the crate's own trait `Stream` replaces the one
of `futures-core`, so the feature `stream`, whose public implementations of that trait are cut,
refuses to build. The module `h2::io` of C2 holds the traits `AsyncRead` and `AsyncWrite` with the
methods the crate calls of the traits of `tokio` of the same names, and a `ReadBuf` over a slice that
is initialized already, so a read needs no `unsafe` on either side, and the `write_all` of the
preface of the client is a function of the module. The codec, the client, the server and the files
of `src/proto/` name them in place of `tokio::io`, and the fork of hyper implements them in its
trim C3. The examples of the documentation still name `tokio`, and no build runs them. After C2 the
crate names `bytes` and `http` alone (the plan of M5, section Layer C and facts 128, 149 and 155).

## The features Aosabi builds

No crate of the workspace names this fork before commit 98 of M5, the wiring of the forks, because
the features the builds below name exist only once the layers have landed (the plan of M5, section
The serial step, The wiring of the forks).

| Build | Features |
| --- | --- |
| A Linux card with the opt in `tls` or `h2c`, through hyper's `http2` | Its defaults, after layer C |
| Every other build | None, the fork is not built |

## Staying close to upstream

The fork follows the newest release of the upstream line 0.4. A release that fixes an advisory of
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
python3 tools/provenance/extract.py --crates check-release h2
```
