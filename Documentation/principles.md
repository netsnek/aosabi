# Principles

Aosabi follows five principles, and this page says what each one means in the code. A change that
breaks one of them needs a reason that outweighs it. Where this page gives a measured number, the
sentence names the suite of the project that measured it and the day it ran.

Two of them are the project's mantra in its own words: boilerplate over magic, and security over
features.

## 1. Single pass

A GraphQL server checks every document against its schema before it runs it. The usual way is to
parse the document into a syntax tree and then let each validation rule visit that tree, so a
document is walked once per rule. Aosabi validates a document in one walk over the tokens the parser
already read, against a field table that the engine builds once from the schema and keeps as long as
the schema lives.

The walk answers one of three verdicts. It accepts the document, it refuses it with the first rule
it finds broken, or it defers it with a reason. Only an acceptance skips the rules. A refusal and a
defer hand the document to the 27 validation rules, which write every error with its message, its
locations and its order, so an invalid document gets the error graphql-js gives. The walk decides
how fast a valid document reaches the executor and never which error an invalid one gets.

The walk defers what it does not check itself, such as a subscription, a literal of an input type
marked `@oneOf`, a list or object literal at a custom scalar, and nesting beyond the bound of the
build. It runs only where the engine uses the standard rules alone, on the document the parser
returned and on a schema that passed its own validation, so an application that adds a rule of its
own keeps the rules on every document.

| Build setting | Values | Default |
| --- | --- | --- |
| `AOSABI_VALIDATION` | `single-pass` or `conventional` | `single-pass` |
| `AOSABI_SINGLE_PASS_DEPTH` | A whole number of levels from 1 to 1,024 | 64 |

Both are read when the crate compiles, so no setting reaches a running process, and a wrong value
fails the build with the name of the setting. A build of the conventional validation compiles none
of the walk into its binary.

The core of the walk names `core` and `alloc` alone, holds no global, no atomic and no shared
pointer, and denies indexing, `unwrap`, `expect`, `panic!` and `unreachable!` in the whole module,
so every read goes through a checked access. It lives in the module `pass` of
[crates/aosabi-validate](../crates/aosabi-validate/).

The single pass suite of the project holds the walk to graphql-js 16.12.0, which it runs through a
sidecar on Node.js. On 2026-10-20 it found 0 false acceptances and 0 false refusals over 604,800
generated documents of 28 schemas, and its mutation run, which weakens one check of the walk at a
time, caught 55 of the 55 weakened checks it was meant to catch.

The single pass budget suite counted on the same day, for the first request of the small schema it
keeps for a device:

| What | Where | Count |
| --- | --- | ---: |
| The parse and the walk | One ARM64 core, `opt-level` 3, callgrind | 13,246 instructions |
| The walk over a table compiled into the binary | RV32, `opt-level` z, QEMU | 10,817 instructions |
| Allocations of the walk | Both | At most 2 |
| The walk, its table and its lexer | The RV32 image, `opt-level` z | 44,043 bytes |

## 2. Speed

Speed is a property of the design first and of tuning second.

- The engine is Rust with no garbage collector, and a native build runs every request on one thread
  over `epoll`. No crate that serves a request starts a thread, which a lint of `clippy.toml`
  refuses.
- The release profile links with fat link time optimization, one code generation unit and
  `panic = "abort"`.
- The executor models the job queue of ECMA-262 promises, so the order of results and errors matches
  graphql-js without a second pass that sorts them.
- The request cache keeps parsed documents and the verdicts of their validations in one bounded
  table per process: 1,024 documents in 64 MiB on a server, 16 MiB on a Worker and 512 KiB on a
  small device, as the module [cache_budget](../crates/aosabi-core/src/cache_budget.rs) of
  `aosabi-core` sets them.
- One crate, [crates/aosabi-json](../crates/aosabi-json/), turns every value into JSON bytes, with
  its own shortest round trip writer of doubles and its own insertion ordered map, and reaches no
  outside crate for either.

The parser was measured against graphql-js on 2026-10-01 by the language suite of the project, both
sides pinned to one core of an Apple M1 Max under Linux, Aosabi in the release profile of the
workspace and graphql-js 16.12.0 on Node.js 24.13.0 in the same run. Each figure is the median time
of one parse:

| Query | Size | Time of Aosabi | Time of graphql-js 16.12.0 |
| --- | ---: | ---: | ---: |
| The feed query of a blog | 274 bytes | 2.47 µs | 5.85 µs |
| The introspection query | 1,905 bytes | 7.08 µs | 16.81 µs |
| The feed query with Japanese text | 554 bytes | 4.16 µs | 8.34 µs |

A figure without its suite and its date is no claim of this repository. The suites behind the
figures of this page use parts of the test harness that this repository does not carry, so they run
in the project's own pipeline, while [tests/notebooks/](../tests/notebooks/) holds the public suites
of Homa, of its framing and of the carriers.

## 3. Security over features

A feature that is not built cannot be attacked, so the default build is the smallest one.

- Every optional feature of the facade [crates/aosabi](../crates/aosabi/) is off until a service or
  its build target names it: the server engine, TLS, HTTP/2, the outbound client, Sentry, the OpenID
  Connect login through Zitadel, federation and every carrier.
- TLS is version 1.3 alone, through `rustls` with the provider `ring`, on the server and on the
  outbound client of a native build, and a client held at TLS 1.2 is refused in the handshake. A
  TypeScript app takes the TLS and the crypto of its host and links no second stack.
- The workspace forbids `unsafe_code`, and the project's own crates hold no unsafe block, function,
  impl or trait. The three crates that build a WebAssembly module allow the export attribute of each
  exported function, two of them also one block of imports of safe functions, and each such line
  stands under its own `#[expect(unsafe_code)]`.
- A deployed build links few outside crates. The default facade links one, `once_cell`, and the
  server with TLS links fifteen, every one of them listed in [the security
  guide](guides/security.md).
- Aosabi sends no telemetry of any kind.

[The security guide](guides/security.md) gives every default with the code that sets it.

## 4. Boilerplate over magic

A reader of a service sees everything the service does in its own source. Nothing is generated into
the author's files, and nothing changes behaviour because of a name.

- A Rust service is plain Rust, with no macro, no derive and no attribute of Aosabi, and the builder
  reads its types, its `impl` blocks and its doc comments as they stand. Its index file, the crate
  root, is the one place that registers roots, routes, plugins and options on one chain.
- What the source does not state, the builder does not guess. It refuses at the file and the line
  with what to write in its place.
- A security check that depends on the deployment, such as the origins a browser may call from, is
  written by the service once for every route, and the engine adds no default of its own.
- A build setting is an environment variable the build reads by name, and a wrong value stops the
  build with that name.

[The Rust guide](guides/rust.md) shows the forms a service writes.

## 5. Embedded first

Aosabi is meant to run on a board with no operating system, so the layers that every build links are
written for the smallest target first.

- The kernel crate, [crates/aosabi-kernel](../crates/aosabi-kernel/), builds without the standard
  library and with allocation optional. The JSON writer, the GraphTP codec, the Homa transport and
  the request cache build for `riscv32imc-unknown-none-elf`, the core of the ESP32-C3, with no
  atomic operation.
- GraphTP keeps the frame small for slow links: a head of 3 bytes on a request and 2 on an answer,
  one coding, deflate in the zlib format above 128 bytes, and a window of at most 4 KiB, which a
  device can hold.
- A small Linux device takes one static binary, built for size with symbols stripped.
- Where a step weighs a cost on a server or on a Worker against a cost on a device, the device comes
  first.

[The embedded guide](guides/embedded.md) says what runs on a board today and what is in progress.
