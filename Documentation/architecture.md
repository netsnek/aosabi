# Architecture

Aosabi is one cargo workspace on the stable Rust toolchain that `rust-toolchain.toml` pins, in
edition 2024. Its crates form layers from a kernel that builds without the standard library up to
the facade a service depends on, and no layer reaches above itself. This page shows the path of a
request, the layers with their crates, how one engine serves two languages, and how a build target
decides what a binary links.

## 1. The path of a request

A request reaches the engine either as HTTP or as GraphTP, and both take the same path from the
routes of the app on.

```text
HTTP     host or native server ------------+
                                           +--> routes and middleware --> GraphQL over HTTP
GraphTP  carrier --> Homa --> GraphTP -----+                                      |
                                                                                  v
         JSON <-- execute <-- single pass, then the rules where needed <-- parse <-- hooks
```

1. On a Linux server the native server of [crates/aosabi-serve](../crates/aosabi-serve/) accepts the
   connection, speaks HTTP/1.1, and HTTP/2 where TLS or prior knowledge turns it on. On Cloudflare
   Workers and on Bun the host serves the connection and hands each request to the engine through
   the wrapper of the TypeScript layer.
2. The routes and the middleware of the application run first, and the GraphQL over HTTP layer reads
   the parameters of a GraphQL request from the method, the query string and the body.
3. The hooks of the plugins run around each step of an operation: the parse, the validation, the
   building of the context, the execution and a subscription.
4. The parser reads the document, the single pass validates it, and the 27 validation rules run
   where the single pass refuses or defers it. Section 1 of [principles.md](principles.md) says how
   the two fit.
5. The executor resolves the fields in the job order of ECMA-262 promises, and one writer turns the
   result into JSON bytes.

A GraphTP server reads one Homa message, checks the head of GraphTP and hands the decoded content to
the handler of the app as an HTTP POST with fixed fields, so a document gets the same answer over
GraphTP as over HTTP, and the server writes the status, the substatus and the body back as one Homa
message. [guides/graphtp.md](guides/graphtp.md) describes the protocol.

## 2. The layers

The layers stand bottom up, and a crate of a lower layer never names a crate of a higher one.

1. The kernel, [crates/aosabi-kernel](../crates/aosabi-kernel/), holds the framing of packets on a
   byte stream, the publish primitive of server push and the entry traits. It builds without the
   standard library, with allocation optional.
2. The byte layer holds `aosabi-json`, the one writer of JSON bytes, `aosabi-lang`, the lexer,
   parser and printer of GraphQL, and `aosabi-http`, the zlib wrapper and the neutral messages of
   HTTP.
3. The GraphQL layer holds `aosabi-type`, the type system and introspection, `aosabi-validate`, the
   rules and the single pass, `aosabi-execute`, the executor, `aosabi-scalars`, the scalars the
   engine binds, and `aosabi-cache`, the request cache.
4. The HTTP and hook layer holds three crates: the GraphQL over HTTP layer, the hook layer that
   every carrier runs, and the web layer of requests, responses, routes and middleware.
5. The application layer holds `aosabi-core`, with the app, its context, its plugins, its logger and
   its pub sub, `aosabi-ir`, the model of the schema and its printer, `aosabi-infer`, the inference
   of the schema, and `aosabi-build` with `aosabi-codegen`, the builder and the glue it generates.
   The derives of `aosabi-macros` stay for code that a command writes, such as the registry of a
   remote schema.
6. The serving layer holds `aosabi-serve`, the native server with TLS and HTTP/2, `aosabi-fetch`,
   the outbound HTTP client, and one crate per carrier, `aosabi-carrier-unix`,
   `aosabi-carrier-serial`, `aosabi-carrier-usb`, `aosabi-carrier-bluetooth`,
   `aosabi-carrier-network` and `aosabi-carrier-lora`.
7. The facade, [crates/aosabi](../crates/aosabi/), is the one crate a Rust service names, with every
   optional part behind a cargo feature.

Beside the layers stand the crates of the protocol and of the TypeScript layer.

- [crates/graphtp](../crates/graphtp/) holds the types and the wire codec of GraphTP/1 with its
  DEFLATE encoder and decoder, with no standard library, no dependency and no feature.
  [crates/graphtp-homa](../crates/graphtp-homa/) binds it to Homa as a server and a client that do
  no input or output of their own, and [crates/homa](../crates/homa/) is the Homa transport, written
  in the project, with its compact form for device carriers and its full form for raw IP.
- [crates/aosabi-js](../crates/aosabi-js/) builds the engine for the TypeScript layer, as the module
  `core.wasm` and, for Bun on Linux, as a native library.
  [crates/aosabi-infer-wasm](../crates/aosabi-infer-wasm/) builds `infer.wasm`, which the dev
  package `aosabi-build` runs at build time.
- [crates/aosabi-cli](../crates/aosabi-cli/) is the command line, `cargo aosabi` and `aosabi`, which
  runs on the build host alone. [crates/aosabi-devtool](../crates/aosabi-devtool/) exposes one byte
  level primitive per subcommand to the notebooks of the test suite. The crates `aoviewer-core`,
  `aoviewer-docs` and `aoviewer-wasm` lay out and write the reference documentation of a schema.

The optional crates under [plugins/](../plugins/) are the plugins a service names by feature:
authentication with its contract, the OpenID Connect login through Zitadel, Sentry and federation,
beside two pages, the route `/viewer` and the landing page of an unhandled route.

## 3. One engine, two languages

The engine is written once in Rust, and each language reaches it through the same three seams: the
trait `TypeSource` of `aosabi-infer`, which the inference of the schema reads, the model of the
schema in `aosabi-ir`, which every artifact of a build is written from, and the values of
`aosabi-core`, which the executor resolves.

- A Rust service is plain Rust, and the builder of `aosabi-build` reads its source on the build
  host, follows its modules from the crate root, finds the entry and the types it registers, and
  answers the inference through `TypeSource`. `cargo aosabi build` then writes `schema.graphql` and
  the other artifacts under `target/aosabi/`, and builds the server over a generated copy of the
  crate that holds the glue between the types and the executor. The author's files stay as they are,
  and a diagnostic names the author's file and line. [guides/rust.md](guides/rust.md) shows the
  forms.
- A TypeScript app runs on the same engine. The package `aosabi` carries the engine as `core.wasm`
  with a wrapper of plain ES modules, and its platform packages carry the native library for Bun on
  Linux. The dev package `aosabi-build` infers the schema with the TypeScript checker itself through
  `infer.wasm` and never ships with an app. [guides/typescript.md](guides/typescript.md) says how an
  app is built and served.

## 4. Build targets and cards

A build names one card, and the card decides the target, the features of the facade and the profile.
Five cards are built into the command line.

| Card | Target | What a build compiles |
| --- | --- | --- |
| `server` | The host, aarch64 or x86_64 Linux | The server with the Unix socket and deflate |
| `server-legacy` | As `server` | As `server`, for a loader older than glibc 2.36 |
| `docker` | As `server` | As `server`, and an image that runs as non root on port 3000 |
| `worker` | `wasm32-unknown-unknown` | The library of the service, with no server and no encoder |
| `esp32c3-devkitm1` | `riscv32imc-unknown-none-elf` | The kernel crates for the ESP32-C3 |

`cargo aosabi build --device <card>` takes a card by its name. A release build of a server card also
writes a copy stripped of its symbols. The cards of other devices, which pack a device's board
support into one file, are on the [roadmap](../roadmap/embedded.md).

## 5. Server push

A subscription is a stream of results that the server pushes when its state changes, whatever
changed it: a mutation, another client or code that is no resolver at all. The publish primitive of
`aosabi-kernel` counts a revision per hub, fans each event out to every subscriber in the order they
listened, and coalesces the events of a subscriber whose link falls behind, so a slow link keeps the
latest value, with no thread, no clock and no atomic operation. A service keeps a pub sub of
`aosabi-core` in a `static` over that hub.

Over HTTP a stream answers as server sent events. Over GraphTP the client parks one PULL request at
the server, and each answer of a PULL carries the next batch of events of every stream of that
client, so the server pushes without opening a connection of its own.

## 6. Lints that hold the architecture

- `unsafe_code` is forbidden for the whole workspace, but for the three named exceptions of [the
  security guide](guides/security.md), and `missing_docs` warns.
- `clippy.toml` refuses `HashMap` and `BTreeMap`, because a JSON object keeps the key order of
  JavaScript, and the insertion ordered map of `aosabi-json` is the one map of the engine.
- `clippy.toml` refuses every way the standard library offers to start a thread, so no crate that
  serves a request starts one, and the WebAssembly and device targets need no rewrite.
- Every crate follows the coding style of [coding-style.md](coding-style.md).
