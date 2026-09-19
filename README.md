# Aosabi

Aosabi is a very small, very fast GraphQL server that runs from microcontrollers to the edge. A
service is written in plain Rust or in TypeScript, and the build derives its GraphQL schema from
that code. One engine, written in Rust, answers it: on a Linux server, on Cloudflare Workers and Bun
through a WebAssembly build or a native library, on small Linux devices and, as far as they run
today, on boards without an operating system.

The engine validates a document in a single pass over its tokens against a table compiled from the
schema. It is secure by default, with TLS 1.3 alone, no unsafe block in its own code and every
optional feature off until a service names it. Its parser, validator and executor are checked byte
for byte against graphql-js 16.12.0 by the test suite.

```rust
use aosabi::prelude::*;

pub struct Query;

impl Query {
    pub fn hello(&self) -> String {
        String::from("Hello, world!")
    }
}

pub fn app() -> impl IntoApp {
    App::new().query(Query)
}
```

That file is a whole service. It holds no macro, no derive and no attribute of Aosabi, and
`cargo aosabi build` reads it as it stands, writes its schema and builds the server that answers it.
The schema begins with the one type the file states:

```graphql
type Query {
  hello: String!
}
```

## Principles

1. The engine validates a document in a single pass over its tokens against a field table built from
   the schema, and the 27 validation rules stay behind the walk to write every error, so an invalid
   document gets the error graphql-js gives.
2. Speed is designed in and then measured: the engine runs a request on one thread in Rust with no
   garbage collector, and every speed figure in this repository names the notebook that measured it.
3. Security comes before features, so a feature stays off until a service names it, and a feature
   that would widen the attack surface waits until it can be made safe.
4. Boilerplate comes before magic, so a service writes what it means in its own language, and its
   index file is the one place that looks like a framework.
5. Embedded comes first, so the core builds without the standard library, and where a cost on a
   server weighs against a cost on a device, the device wins.

[Documentation/principles.md](Documentation/principles.md) says what each one means in the code.

## Where it runs

| Where | How | State in 0.1.0 |
| --- | --- | --- |
| Linux servers and containers | One native binary for aarch64 or x86_64 | Runs |
| Cloudflare Workers | A TypeScript app on the engine built as `core.wasm` | Runs on workerd |
| Bun on Linux | A TypeScript app on the engine as a native library or as `core.wasm` | Runs |
| Small Linux devices | One static binary for `mipsel-unknown-linux-musl` | Builds |
| Boards with no operating system | The core crates for the ESP32-C3 | Engine in progress |

A native binary speaks HTTP/1.1 over TCP and over a Unix socket, and HTTP/2 through TLS 1.3 once a
service turns TLS on. On Workers the platform serves HTTP and hands each request to the engine.
GraphTP, the Graph Transfer Protocol, carries the same GraphQL requests where HTTP does not fit,
with a head of 3 bytes on a request and 2 on an answer, over Homa on Unix sockets, raw IP, serial
lines, USB and Bluetooth. The [embedded guide](Documentation/guides/embedded.md) says what runs on
which device today and what is still a build alone.

## Quick start

Aosabi builds on the stable Rust toolchain that `rust-toolchain.toml` pins. From a checkout of this
repository:

```sh
cargo install --locked --path crates/aosabi-cli
aosabi new hello --aosabi-path .
cd hello && cargo aosabi dev
```

The first command installs `cargo aosabi` and `aosabi`, the second writes a service that names this
checkout by path, and the third builds the service, serves it on port 3000 and builds it again on
every change. The folder [examples/](examples/) holds complete services, each with a README that
says how to build and run it.

## Documentation

- [Documentation/](Documentation/README.md) is the index of every page.
- [Documentation/principles.md](Documentation/principles.md) holds the five principles.
- [Documentation/architecture.md](Documentation/architecture.md) shows the layers and the crates.
- [Documentation/guides/](Documentation/README.md#guides) holds the guides to the Rust layer, the
  TypeScript layer, GraphTP, embedded targets, the security defaults, compatibility with Pylon v3
  and the conformance tests against graphql-js.
- [roadmap/](roadmap/README.md) is the public roadmap.

## Status

Version 0.1.0 is the first public release. Aosabi is before 1.0, so its API may still change between
releases. The test suite of the repository judges the engine, the native server, the command line,
the TypeScript layer and the GraphTP codec. The crates and the npm packages are not published to a
registry yet, so a service names them by path from a checkout. [CHANGELOG.md](CHANGELOG.md) lists
what the release holds.

## Repository layout

| Folder | Holds |
| --- | --- |
| `crates/` | The core: the engine, the server, the protocols, the build and the command line |
| `plugins/` | The optional crates a service names by feature, such as authentication |
| `aosabi-ts/`, `aosabi-ts-build/`, `aosabi-ts-native/` | The npm packages of the TypeScript layer |
| `graphtp-js/` | The npm package `graphtp`, a GraphTP client for JavaScript |
| `tests/` | The test suite, Jupyter notebooks run by papermill |
| `examples/` | Complete services to start from |
| `Documentation/` | The documentation |
| `roadmap/` | The public roadmap |
| `third_party/` | The vendored forks, each with its own license |
| `LICENSES/` | The license texts |
| `website/` | The project website |
| `tools/` | Developer tools |

## Contributing and security

[CONTRIBUTING.md](CONTRIBUTING.md) says how a change is written, tested and signed off. A security
issue goes to the private vulnerability reporting of this repository, as [SECURITY.md](SECURITY.md)
describes, and never to a public issue.

## The name

Aosabi is 青錆, the Japanese word for verdigris, the blue green patina that copper and bronze take on
as they weather. Sabi (錆) alone is rust.

## License

Aosabi is MIT licensed, Copyright (c) 2026 Florian Kleber. See [LICENSE](LICENSE). The forks under
`third_party/` keep their own licenses, and [NOTICE](NOTICE) and
[Documentation/licensing.md](Documentation/licensing.md) name every license that applies.
