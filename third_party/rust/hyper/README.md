# hyper 1.11.1, vendored

This directory holds the crate `hyper` 1.11.1, the HTTP/1.1 and HTTP/2 server of Aosabi, as
crates.io published it and vendored the kernel way. It changes only through the commits of its
series below, each one behaviour, and the tool `tools/provenance` is the one writer of the files it
imports ([CONTRIBUTING.md](../../../CONTRIBUTING.md) section 6).

The owner took hyper on 2026-09-24 as a fork patched from the start. Layer A builds its HTTP/1.1
server path `no_std` with `alloc` in a form upstream can take, layer B reproduces every behaviour of
Node and llhttp the byte contract needs as a quirk of the register behind a feature, and layer C
cuts the client, the C interface, tracing and every outside crate the dependency rule does not admit
(the plan of M5, sections The fork and The patch layers).

## Provenance

| What | Value |
| --- | --- |
| Crate | `hyper` |
| Version | 1.11.1, released on 2026-08-28 (PROVEN, https://crates.io/api/v1/crates/hyper/1.11.1, read on 2026-09-26) |
| Upstream | https://github.com/hyperium/hyper |
| Tag | `v1.11.1` at `6371cd425017155f7fbecef0e57b218edbe6a93a` (PROVEN, `git ls-remote --tags`, read on 2026-09-26) |
| Commit the archive names | `6371cd425017155f7fbecef0e57b218edbe6a93a`, not dirty |
| Archive | https://static.crates.io/crates/hyper/hyper-1.11.1.crate |
| SHA-256 of the archive | `27b501faa50e7a26c3d3560ca625132f4078a17771f4810baf70475ae48cbe43`, equal to the checksum crates.io publishes |
| Licence | `MIT`, Copyright (c) 2014-2026 Sean McArthur |
| Notice | `NOTICE` sections 2 and 4 |
| SPDX line of commit S | `// SPDX-License-Identifier: MIT` above the first line of every Rust file under `src/` |
| Lines and `unsafe` tokens | 22,248 lines of Rust under `src/` and 67 `unsafe` tokens as published, counted over the import set, the SPDX lines of commit S aside (PROVEN, `wc -l` of the Rust files under `src/` and `grep -ow unsafe` over the import set, on 2026-09-26) |
| Reached by path | `http`, `http-body`, `h2`, through commit W |

The archive names the tag commit itself, not dirty.

## The import set

The import commit takes, byte for byte, the files of the archive that the build and the licence
need: `Cargo.toml` as cargo published it, `LICENSE` and `src/**`, 66 files in all (PROVEN, the
import set listed against the archive on 2026-09-26). It leaves out every other file of the archive,
because the archive's own `README.md` would collide with this file, the packaging records and the
tests serve no build, and a `.gitignore` of an archive would drop files from the commit. The SHA-256
of every file it leaves out stands below, so the verify mode checks every file of the archive all
the same (the plan of M5, section The fork, fact 92).

| File left out | SHA-256 |
| --- | --- |
| `.cargo_vcs_info.json` | `8e08544c1e6778d3606bb6a827f7ea6496033bc9b3356289710d1954fb71a5cc` |
| `Cargo.lock` | `ce0c4521c833d3f57ac27a27c99f5bd6791d0d00e34773bb6417f609494a3b13` |
| `Cargo.toml.orig` | `2dc09a6ffbb995c75837fd0e2bcf0bf08e81f1f4623f97ac740698b6a7759013` |
| `README.md` | `9f8d9269b8bfb0cfb17d119c65cebcc33f67310bcec7fbf06b8527def78c1502` |

The manifest names no target outside `src/`, because the published manifest of hyper lists the files
it packages under `include` and names no test, example or bench.

## Verify

```text
python3 tools/provenance/extract.py --crates verify hyper
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
S, this README in commit R, then the layers in the order A, B, C, then the fixes of the rounds. The
command `python3 tools/provenance/extract.py --crates series hyper` lists the commits after I with
their layer, id and quirk ids, and holds each layer to its budget. It counts the patched lines of a
layer by `git diff --numstat` from the commit before the layer to its last commit, so a line that two
commits of the layer touch counts once. The count leaves out the SPDX lines, and it lists the files
the layer removes and the lines of this README apart. Each layer adds its subjects here with its last
commit (the plan of M5, sections Staying close to upstream and The patch series, one commit each).
A fix of the rounds, commit 160 of the plan, is a commit of the type `fix` that corrects the commit
of its layer its id names and names the quirk of that commit. It stands after the layers in the
order the rounds land it, the order A, B, C binds the other commits alone, and its lines count
toward the budget of its layer beside the span of that layer. A later milestone corrects a commit of
a layer the same way, as item X7 of EXEC1 corrects the body write of B19, so a chunk of an answer is
appended as one slice (the plan of EXEC1, lever L11).

A commit of the scope `third_party` touches no path outside `third_party/rust/`, which is also how
no commit of the series changes `Cargo.lock`. The one exception is the fix of a row of the rated
table of quirks. Every fixed row is a commit of its own that builds and passes its test alone (the
owner's rule of 2026-10-27), so the fix of a row in this fork carries its test and its record beside
the fork in the same commit and names its row in the trailer `Row: QD-nnn` directly before
`Signed-off-by`. Such a commit may touch paths outside `third_party/rust/`, `Cargo.lock` aside, and
the mode `series` judges its part of the fork by its paths under `third_party/rust/` alone, as it
judges every commit. A commit without the trailer that mixes the fork with other paths fails the
mode, and so does a commit of the fork whose summary names no layer and id.

| Id | Summary after the scope `third_party` |
| --- | --- |
| I | `hyper vendor i, import hyper 1.11.1 as published` |
| W | `hyper vendor w, reach http, http-body and h2 by path` |
| S | `hyper vendor s, spdx identifiers` |
| R | `hyper vendor r, the readme of provenance` |
| A1 | `hyper nostd a1, the feature std on by default and no_std with alloc for the http/1.1 server` |
| A2 | `hyper nostd a2, core and alloc in place of std on the server path` |
| A3 | `hyper nostd a3, bare io traits with hyper's own error where io::error stands` |
| A4 | `hyper nostd a4, time through a timer measured from its own epoch` |
| A5 | `hyper nostd a5, the date written in the crate without httpdate` |
| A6 | `hyper nostd a6, the upgrade on a oneshot of its own in place of tokio's, and left out without std` |
| A7 | `hyper nostd a7, shared state on one thread without std` |
| A8 | `hyper nostd a8, the body channel on one thread without std` |
| A9 | `hyper nostd a9, buffers below 8 kib for small targets` |
| A10 | `hyper nostd a10, tokio for the client and http/2 alone` |
| A11 | `hyper nostd a11, the tests that need std under std` |
| B0 | `hyper node b0, the features of node parity, the node configuration and the empty node modules` |
| B1 | `hyper node b1, the 35 methods of node` |
| B2 | `hyper node b2, the target class of llhttp and the raw target` |
| B3 | `hyper node b3, the four versions of llhttp` |
| B4 | `hyper node b4, the request line of llhttp and the bytes it skips` |
| B4, a fix of row QD-322 of LANED, divergence D73 of quirk Q21 | `hyper node b4, the request line as hyper reads it, D73` |
| B5 | `hyper node b5, every line ends in crlf` |
| B6 | `hyper node b6, the field grammar of llhttp` |
| B7 | `hyper node b7, the framing fields as llhttp decides them` |
| B8 | `hyper node b8, the head counted as node counts it` |
| B9 | `hyper node b9, the three windows of the fields and the head facts node reads` |
| B9, a fix of row QD-325 of LANED, divergence D74 of quirk Q26 | `hyper node b9, every field of a head read alike up to its bound, D74` |
| B10 | `hyper node b10, the llhttp parser in place of httparse on hyper's own path` |
| B11 | `hyper node b11, the status line http/1.1 for every version` |
| B12 | `hyper node b12, the record's lines as the record spells and writes them` |
| B13 | `hyper node b13, chunked on another version with te chunked` |
| B14 | `hyper node b14, a chunked 204 or 304 closing` |
| B15 | `hyper node b15, chunk sizes in lower case` |
| B16 | `hyper node b16, trailers counted as a head, parsed strictly and dropped` |
| B17 | `hyper node b17, chunk extensions counted per chunk and refused as 413` |
| B18 | `hyper node b18, the chunk size line of llhttp` |
| B19 | `hyper node b19, the node connection, every byte of a read parsed and the answers in order` |
| B20 | `hyper node b20, the fixed answers of node on node's condition` |
| B21 | `hyper node b21, host and expect answered for http/1.1 inside the read` |
| B22 | `hyper node b22, 100 continue at the head and in its queue` |
| B23 | `hyper node b23, connect ends the connection with no answer` |
| B24 | `hyper node b24, an upgrade answered as an ordinary request and the rest of its read dropped` |
| B25 | `hyper node b25, a request after one not kept alive refused` |
| B26 | `hyper node b26, a refused body answered or closed as node does` |
| B27 | `hyper node b27, the timers of node` |
| B28 | `hyper node b28, date and the connection lines after the record` |
| B29 | `hyper node b29, handlers run to their answers after the end` |
| B30 | `hyper node b30, a failed stream ends the connection with no further byte` |
| B31 | `hyper node b31, an unread body cleaned up as the adapter does` |
| B32 | `hyper node b32, a half closed client answered` |
| B33 | `hyper node b33, the head bounds of a device card in the parser` |
| B34 | `hyper node b34, the answer bounds of a device card in the connection` |
| C1 | `hyper trim c1, the client cut` |
| C2 | `hyper trim c2, the c interface cut` |
| C11 | `hyper trim c11, connect over http/2 cut` |
| C3 | `hyper trim c3, the io traits of h2 in place of tokio's` |
| C4 | `hyper trim c4, tracing cut` |
| C5 | `hyper trim c5, the feature sync split from std, and http2 without it` |
| C6 | `hyper trim c6, waker slots without atomic-waker` |
| C7 | `hyper trim c7, httparse and smallvec behind the feature httparse` |
| C8 | `hyper trim c8, integers written without itoa` |
| C9 | `hyper trim c9, core's ready in place of futures-core's` |
| C10 | `hyper trim c10, the server connections without pin-project-lite` |
| B29, a fix of the rounds | `hyper node b29, a body that ended handed whole to its handler after the end` |
| B7, a fix of the rounds | `hyper node b7, a transfer coding without chunked last refused after the head` |
| B19, a fix of the rounds | `hyper node b19, the service polled once at its head as node runs the request listener` |
| B19, a fix of the rounds | `hyper node b19, the services moved after each piece of a body as node runs its ticks` |
| B19, a fix of item X7 of EXEC1 | `hyper node b19, a body chunk copied as one slice` |
| R, the rule of a fixed row | `hyper vendor r, the fix of a row with its test and its record beside the fork` |

| Layer | What it does | Ids | Patched lines in upstream files, at most | Lines in new files, at most |
| --- | --- | --- | ---: | ---: |
| A | The HTTP/1.1 server path `no_std` with `alloc` | A1 to A11 | 1,500 | 900 |
| B | Node parity, the quirks Q18 to Q48 behind the features `node` to `node-half-open` | B0 to B34 | 400 | 3,500 |
| C | The client, the C interface, `CONNECT` over HTTP/2, `tokio`, tracing, `sync` split from `std`, `atomic-waker`, `httparse` behind a feature, `itoa`, `futures-core` and `pin-project-lite` | C1, C2, C11, C3 to C10 | 500 | 100 |

The ceilings are ASSUMED, the series mode counts each layer in the vendored crates check, and a
count above its ceiling fails there.

Layer A builds the HTTP/1.1 server `no_std` with `alloc`, on `core` and `alloc` alone, and adds to the
public interface without changing it. The feature `std` is on by default and turns on the `std` of
`bytes`, `http`, `http-body`, `httparse` and `futures-core`, which the manifest names without their
default features, and the thread safe state of `atomic-waker` and `futures-channel`. The features
`client`, `http2` and `ffi` refuse to build without `std`, and `tokio` is named by `client` and
`http2` alone. The module `rt::bare` holds the traits `Read` and `Write` with hyper's own `IoError`
and the trait `Timer`, which measures time as a `Duration` from an epoch of its own and gives the
wall clock of the `Date` through `unix_seconds`. With `std` every `rt::Read`, `rt::Write` and
`rt::Timer` serves through a blanket implementation or a wrapper, the vectored write included, and
the `std::io::Error` of an IO comes back unchanged. The date is an IMF fixdate written from Howard
Hinnant's `civil_from_days`, so `httpdate` leaves the manifest. The upgrade rides on a oneshot of the
crate under the `Mutex` of `std`, so `tokio` leaves the server path, and without `std` the upgrade
machinery is left out and an upgrade the service answers ends the connection as a connection
polled without `with_upgrades` ends it. Without `std` the timer, the watch and the channels of a
body share their state on one thread through `Rc`, `Cell` and `RefCell`, so the connection is not
`Send`, which serves a target without atomics in safe code and with no crate. The builder's
`max_buf_size` accepts 1,024 bytes, and `init_buf_size` sets the first size of the read buffer and
of the buffer of a head. The unit tests run under `std` (the plan of M5, section Layer A and facts 96
to 98, 105, 118, 157 and 158).

Layer B reproduces every behaviour of Node 24.13.0 and llhttp 9.3.0 that the byte contract needs and
hyper does not have, each a quirk of the register of `docs/byte-equality.md` section 13.1 behind a
feature, and it changes nothing when its features are off. The feature `node` turns all of them on.
The head parser of `src/proto/h1/node/parse.rs` takes llhttp's grammar in place of `httparse`, one
pass per byte with the bytes llhttp skips dropped as it reads them, and reaches hyper's own path
through a hook in `Server::parse`. Patches of the upstream files `role.rs`, `encode.rs`, `decode.rs`
and `ext/mod.rs` write the status line, the record, the framing and the chunk size as Node writes
them and read chunk extensions and trailers as llhttp reads them. The Node connection
`server::conn::node1`, beside hyper's `http1`, parses every byte of a read before the services move
but polls a service once at its head and moves the services after each piece of a body, where Node
runs code inside a read, answers in order, writes Node's own answers where Node writes them and runs
Node's timers, with the bounds of `NodeConfig::ESP32C3` for a device card, which are no quirk (the
plan of M5, section Layer B, Node parity).

| Feature | Quirks | Where |
| --- | --- | --- |
| `node-llhttp` | Q18, Q19, Q20, Q21, Q22, Q23, Q24, Q25, Q26 | `src/proto/h1/node/parse.rs` and `methods.rs`, the hook in `role.rs` |
| `node-status-line` | Q27 | `src/proto/h1/role.rs` |
| `node-record` | Q28 | `src/ext/mod.rs`, `src/proto/h1/role.rs` |
| `node-framing` | Q29, Q30 | `src/proto/h1/role.rs` |
| `node-chunk-hex` | Q31 | `src/proto/h1/encode.rs` |
| `node-chunked` | Q32, Q33, Q34 | `src/proto/h1/decode.rs` and `parse.rs` |
| `node-conn` | Q35, Q36, Q37, Q39, Q40, Q41, Q42, Q45, Q46, Q47 | `src/proto/h1/node/conn.rs`, `answers.rs`, `head.rs` and `src/server/conn/node1.rs` |
| `node-continue` | Q38 | `src/proto/h1/node/conn.rs` |
| `node-timers` | Q43 | `src/proto/h1/node/timers.rs` |
| `node-lines` | Q44 | `src/proto/h1/node/head.rs` |
| `node-half-open` | Q48, kept because case H100 shows Node answering | `src/proto/h1/node/conn.rs` |

Layer C is the fork's own and is never offered upstream. It cuts the client, the C interface and
`CONNECT` over HTTP/2 with their files, and each feature it cuts, `client`, `ffi` and `tracing`,
stays declared and refuses to build with a `compile_error!` that names the plan, so `full`, which
names `client`, refuses as well and the own tests run with `http1`, `http2`, `server` and
`httparse` over the defaults. A `CONNECT` over HTTP/2 reaches the service as any request with its
stream as its body. The `Compat` through which the HTTP/2 server hands its transport to `h2`
implements the IO traits of the fork of `h2` in place of those of `tokio`. The feature `sync`, on
by default beside `std`, takes the thread safe state with `futures-channel` and `futures-core`, and
without it the timer, the watch and the channels of a body share their state on one thread, which
every build of Aosabi takes, `http2` among them. The waker slot of a watch sits under a `Mutex` of
`std` with `sync`, `httparse` and `smallvec` sit behind the feature `httparse`, the length of an
answer is written through `core::fmt::Write`, the macro `ready` comes from `core::task`, and the
server connections hold no projection, so `want`, `futures-util`, `http-body-util`, `tokio`,
`tracing`, `atomic-waker`, `itoa` and `pin-project-lite` leave the manifest. The feature `std`
turns on the `std` of `bytes`, `http` and `http-body` alone and `sync` the one of `futures-core`,
because cargo locks the dependency a weak feature such as `httparse?/std` names even when no build
enables it. The server refuses to build without one of its two parsers, `httparse` or the parser of
llhttp with the trailers of `node-chunked`. With `httparse` the fork keeps every public item of the
release outside the cut modules, and the one change that asks more of a caller is the pair of
bounds `S::Future: Unpin` and `Bd: Unpin` of `server::conn::http2::Builder::serve_connection`,
whose future and body the stream of HTTP/2 polls as plain fields. C11 takes the supertrait
`Http2UpgradedExec` out of the sealed public trait `rt::bounds::Http2ServerConnExec`, so an executor
of the stream meets it with one bound fewer. A build without `httparse`, as every build of Aosabi
is, also lacks the two methods of `server::conn::http1::Builder` that configure `httparse`,
`allow_multiple_spaces_in_request_line_delimiters` and `ignore_invalid_headers` (the plan of M5,
section Layer C, trims, and facts 96, 122, 155 and 157). Where a trim takes a macro, a branch or an
argument away, the lines it keeps stay as upstream indented and wrapped them, the items that
`pin_project!` wrapped among them, so a later release rebases over as few changed lines as the
budget below counts. Those lines, in `src/proto/h2/mod.rs` and `server.rs` and in
`src/server/conn/http1.rs` and `http2.rs`, are therefore not rustfmt clean, a deliberate saving of
the budget, and every other line of layer C is.

## The features Aosabi builds

Commit 98 of M5, the wiring of the forks, names this fork in `aosabi-serve` and `aosabi-devtool`
with the features of the first build below, and no crate of the workspace names it before that
commit, because those features exist only once the layers have landed (the plan of M5, section The
serial step, The wiring of the forks).

| Build | Features |
| --- | --- |
| `server` and `docker`, the player and the served twins | `std`, `http1`, `server`, `node` |
| The same with the Linux opt in `tls` or `h2c`, each of which enables `http2` | The same and `http2`, without `sync` |
| Hyper's own path, after byte equality ends | `std`, `http1`, `server`, `httparse` |
| The benchmark variant of `esp32c3-devkitm1` | `http1`, `server`, `node`, and `bytes` with `extra-platforms` |
| The default build of `esp32c3-devkitm1`, and `worker` | None, the fork is not built |

## The lines the plan cites

The plan of M5 reads hyper's lines in two releases. Its facts 95 to 122 cite the release 1.11.0 the
local cargo registry holds, and a citation written `<hyper>/...`, the facts that name `<hyper>` and
the commits of layer C that repeat them cite 1.11.1, the release imported here (the plan of M5,
section What was found). Between the two, 1,095 lines of `src/` change, 597 added and 498 removed
(PROVEN, `git diff --no-index --numstat` of the `src/` of the two archives on 2026-09-26), nearly
all `ref mut` patterns rewritten for a lint, and the importer records every cited line here with its
place in 1.11.1 and in this tree, where the SPDX line of commit S moves every line of `src/` down by
one (decision 82 of the plan). Of the 92 cited ranges, 25 read 1.11.0 at a number 1.11.1 moved,
marked Yes in the column Moved (PROVEN, the rows of the table below, each range placed through that
diff of the two archives). The column In this tree names each line at commit R, and the layers after
R move lines of the files they change.

| File | Plan cites | Release it reads | Where | In 1.11.1 | In this tree | Moved |
| --- | --- | --- | --- | --- | --- | --- |
| `src/body/incoming.rs` | `:9` | 1.11.0 | Fact 97 | `:9` | `:10` |  |
| `src/body/incoming.rs` | `:9` | 1.11.1 | Fact 155 | `:9` | `:10` |  |
| `src/body/incoming.rs` | `:16` | 1.11.1 | Fact 155 | `:16` | `:17` |  |
| `src/body/incoming.rs` | `:32-34` | 1.11.0 | Fact 97 | `:32-34` | `:33-35` |  |
| `src/body/incoming.rs` | `:114-122` | 1.11.0 | Fact 97 | `:114-122` | `:115-123` |  |
| `src/body/incoming.rs` | `:240` | 1.11.1 | Fact 155 | `:240` | `:241` |  |
| `src/body/incoming.rs` | `:265` | 1.11.1 | Fact 155 | `:265` | `:266` |  |
| `src/client/dispatch.rs` | `:11` | 1.11.0 | Fact 96 | `:11` | `:12` |  |
| `src/common/date.rs` | `:1-4` | 1.11.0 | Fact 97 | `:1-4` | `:2-5` |  |
| `src/common/date.rs` | `:44` | 1.11.0 | Fact 97 | `:44` | `:45` |  |
| `src/common/io/compat.rs` | `:22` | 1.11.1 | Fact 96 | `:22` | `:23` |  |
| `src/common/io/compat.rs` | `:67` | 1.11.1 | Fact 96 | `:67` | `:68` |  |
| `src/common/io/mod.rs` | `:1-2` | 1.11.0 | Fact 96 | `:1-2` | `:2-3` |  |
| `src/common/time.rs` | `:6-16` | 1.11.0 | Fact 97 | `:6-16` | `:7-17` |  |
| `src/common/time.rs` | `:53-58` | 1.11.0 | Fact 97 | `:53-58` | `:54-59` |  |
| `src/common/watch.rs` | `:7-47` | 1.11.0 | Fact 97 | `:7-47` | `:8-48` |  |
| `src/ext/h1_reason_phrase.rs` | `:34` | 1.11.0 | Fact 117 | `:34` | `:35` |  |
| `src/ext/mod.rs` | `:161` | 1.11.1 | Fact 117 | `:161` | `:162` |  |
| `src/proto/h1/conn.rs` | `:254-270` | 1.11.0 | Fact 112 | `:254-270` | `:255-271` |  |
| `src/proto/h1/conn.rs` | `:409-420` | 1.11.0 | Fact 109 | `:405-416` | `:406-417` | Yes |
| `src/proto/h1/conn.rs` | `:491-508` | 1.11.0 | Fact 143 | `:487-504` | `:488-505` | Yes |
| `src/proto/h1/conn.rs` | `:592` | 1.11.1 | Fact 90 | `:592` | `:593` |  |
| `src/proto/h1/conn.rs` | `:655-702` | 1.11.0 | Fact 111 | `:665-712` | `:666-713` | Yes |
| `src/proto/h1/conn.rs` | `:682-702` | 1.11.0 | Fact 111 | `:692-712` | `:693-713` | Yes |
| `src/proto/h1/conn.rs` | `:804-826` | 1.11.0 | Fact 112 | `:814-836` | `:815-837` | Yes |
| `src/proto/h1/conn.rs` | `:849-865` | 1.11.0 | Plan line 1758 | `:859-875` | `:860-876` | Yes |
| `src/proto/h1/decode.rs` | `:20-25` | 1.11.1 | Fact 113 | `:20-25` | `:21-26` |  |
| `src/proto/h1/decode.rs` | `:44` | 1.11.1 | Fact 113 | `:44` | `:45` |  |
| `src/proto/h1/decode.rs` | `:103` | 1.11.1 | Fact 113 | `:103` | `:104` |  |
| `src/proto/h1/decode.rs` | `:255-460` | 1.11.0 | Fact 97 | `:255-460` | `:256-461` |  |
| `src/proto/h1/decode.rs` | `:426-446` | 1.11.1 | Fact 113 | `:426-446` | `:427-447` |  |
| `src/proto/h1/decode.rs` | `:642` | 1.11.0 | Plan line 2237 | `:642` | `:643` |  |
| `src/proto/h1/decode.rs` | `:642` | 1.11.1 | Fact 113 | `:642` | `:643` |  |
| `src/proto/h1/dispatch.rs` | `:47-50` | 1.11.0 | Fact 108 | `:47-50` | `:48-51` |  |
| `src/proto/h1/dispatch.rs` | `:279-281` | 1.11.0 | Fact 113 | `:288-290` | `:289-291` | Yes |
| `src/proto/h1/dispatch.rs` | `:292-300` | 1.11.0 | Fact 108 | `:301-309` | `:302-310` | Yes |
| `src/proto/h1/dispatch.rs` | `:300-340` | 1.11.1 | Plan line 390 | `:300-340` | `:301-341` |  |
| `src/proto/h1/dispatch.rs` | `:321-330` | 1.11.1 | Fact 96 | `:321-330` | `:322-331` |  |
| `src/proto/h1/dispatch.rs` | `:329` | 1.11.1 | Plan line 2091 | `:329` | `:330` |  |
| `src/proto/h1/dispatch.rs` | `:355-370` | 1.11.0 | Fact 119 | `:364-379` | `:365-380` | Yes |
| `src/proto/h1/dispatch.rs` | `:576` | 1.11.0 | Fact 122 | `:584` | `:585` | Yes |
| `src/proto/h1/dispatch.rs` | `:626-636` | 1.11.0 | Fact 108 | `:634-644` | `:635-645` | Yes |
| `src/proto/h1/encode.rs` | `:355` | 1.11.0 | Fact 114 | `:355` | `:356` |  |
| `src/proto/h1/io.rs` | `:15-30` | 1.11.0 | Fact 118 | `:15-30` | `:16-31` |  |
| `src/proto/h1/io.rs` | `:88` | 1.11.0 | Fact 118 | `:88` | `:89` |  |
| `src/proto/h1/io.rs` | `:169-217` | 1.11.0 | Fact 110 | `:174-222` | `:175-223` | Yes |
| `src/proto/h1/role.rs` | `:143-377` | 1.11.0 | Fact 110 | `:146-380` | `:147-381` | Yes |
| `src/proto/h1/role.rs` | `:393-454` | 1.11.0 | Fact 111 | `:396-457` | `:397-458` | Yes |
| `src/proto/h1/role.rs` | `:400-405` | 1.11.0 | Fact 111 | `:403-408` | `:404-409` | Yes |
| `src/proto/h1/role.rs` | `:447-450` | 1.11.0 | Fact 120 | `:450-453` | `:451-454` | Yes |
| `src/proto/h1/role.rs` | `:456-476` | 1.11.0 | Fact 111 | `:459-479` | `:460-480` | Yes |
| `src/proto/h1/role.rs` | `:481-499` | 1.11.0 | Fact 112 | `:484-502` | `:485-503` | Yes |
| `src/proto/h1/role.rs` | `:580-647` | 1.11.0 | Fact 111 | `:583-650` | `:584-651` | Yes |
| `src/proto/h1/role.rs` | `:650-980` | 1.11.0 | Fact 111 | `:653-983` | `:654-984` | Yes |
| `src/proto/h1/role.rs` | `:686-687` | 1.11.0 | Fact 111 | `:689-690` | `:690-691` | Yes |
| `src/proto/h1/role.rs` | `:743-780` | 1.11.0 | Fact 119 | `:746-783` | `:747-784` | Yes |
| `src/proto/h1/role.rs` | `:803-873` | 1.11.0 | Fact 111 | `:806-876` | `:807-877` | Yes |
| `src/proto/h1/role.rs` | `:916-969` | 1.11.0 | Fact 119 | `:919-972` | `:920-973` | Yes |
| `src/proto/h1/role.rs` | `:954` | 1.11.0 | Plan line 2327 | `:957` | `:958` | Yes |
| `src/proto/h2/mod.rs` | `:94-110` | 1.11.1 | Fact 155 | `:94-110` | `:95-111` |  |
| `src/proto/h2/ping.rs` | `:25` | 1.11.1 | Fact 155 | `:25` | `:26` |  |
| `src/proto/h2/server.rs` | `:36-41` | 1.11.0 | Fact 130 | `:36-41` | `:37-42` |  |
| `src/proto/h2/server.rs` | `:82-94` | 1.11.1 | Fact 155 | `:82-94` | `:83-95` |  |
| `src/proto/h2/server.rs` | `:263` | 1.11.1 | Fact 155 | `:263` | `:264` |  |
| `src/proto/h2/server.rs` | `:276-300` | 1.11.1 | Fact 155 | `:276-300` | `:277-301` |  |
| `src/proto/h2/server.rs` | `:362-394` | 1.11.1 | Fact 155 | `:362-394` | `:363-395` |  |
| `src/proto/h2/server.rs` | `:486-512` | 1.11.1 | Fact 155 | `:486-512` | `:487-513` |  |
| `src/proto/mod.rs` | `:60` | 1.11.1 | Fact 157 | `:60` | `:61` |  |
| `src/rt/bounds.rs` | `:109-121` | 1.11.1 | Fact 130 | `:109-121` | `:110-122` |  |
| `src/rt/io.rs` | `:74-139` | 1.11.0 | Fact 97 | `:74-139` | `:75-140` |  |
| `src/rt/io.rs` | `:341` | 1.11.1 | Plan line 3349 | `:341` | `:342` |  |
| `src/rt/io.rs` | `:362` | 1.11.1 | Plan line 3349 | `:362` | `:363` |  |
| `src/server/conn/http1.rs` | `:32-45` | 1.11.0 | Fact 122 | `:32-45` | `:33-46` |  |
| `src/server/conn/http1.rs` | `:90-240` | 1.11.1 | Plan line 390 | `:90-240` | `:91-241` |  |
| `src/server/conn/http1.rs` | `:151-157` | 1.11.1 | Fact 157 | `:151-157` | `:152-158` |  |
| `src/server/conn/http1.rs` | `:181` | 1.11.1 | Fact 157 | `:181` | `:182` |  |
| `src/server/conn/http1.rs` | `:199` | 1.11.1 | Fact 157 | `:199` | `:200` |  |
| `src/server/conn/http1.rs` | `:217-230` | 1.11.1 | Fact 157 | `:217-230` | `:218-231` |  |
| `src/server/conn/http1.rs` | `:250` | 1.11.0 | Fact 112 | `:250` | `:251` |  |
| `src/server/conn/http1.rs` | `:265-420` | 1.11.0 | Fact 118 | `:265-420` | `:266-421` |  |
| `src/server/conn/http1.rs` | `:280-345` | 1.11.0 | Plan line 404 | `:280-345` | `:281-346` |  |
| `src/server/conn/http1.rs` | `:290-345` | 1.11.0 | Fact 169 | `:290-345` | `:291-346` |  |
| `src/server/conn/http1.rs` | `:312-319` | 1.11.0 | Fact 117 | `:312-319` | `:313-320` |  |
| `src/server/conn/http1.rs` | `:411-415` | 1.11.0 | Plan line 3352 | `:411-415` | `:412-416` |  |
| `src/server/conn/http2.rs` | `:21-35` | 1.11.1 | Fact 155 | `:21-35` | `:22-36` |  |
| `src/server/conn/http2.rs` | `:129-297` | 1.11.0 | Fact 130 | `:129-297` | `:130-298` |  |
| `src/server/conn/http2.rs` | `:306-313` | 1.11.0 | Fact 130 | `:306-313` | `:307-314` |  |
| `src/upgrade.rs` | `:40-140` | 1.11.0 | Fact 96 | `:40-140` | `:41-141` |  |
| `src/upgrade.rs` | `:53` | 1.11.0 | Fact 96 | `:53` | `:54` |  |
| `src/upgrade.rs` | `:66-68` | 1.11.1 | Fact 157 | `:66-68` | `:67-69` |  |
| `src/upgrade.rs` | `:139-145` | 1.11.1 | Fact 157 | `:139-145` | `:140-146` |  |
| `src/upgrade.rs` | `:269-273` | 1.11.1 | Fact 157 | `:269-273` | `:270-274` |  |

## Staying close to upstream

The fork follows the newest release of the upstream line 1.11. A release that fixes an advisory of
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
python3 tools/provenance/extract.py --crates check-release hyper
```
