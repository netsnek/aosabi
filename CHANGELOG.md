# Changelog

All notable changes to Aosabi are written down in this file. The format is based on [Keep a
Changelog](https://keepachangelog.com/en/1.1.0/), and Aosabi follows [Semantic
Versioning](https://semver.org/spec/v2.0.0.html), under which a version before 1.0 may change its
API between minor versions.

## [0.1.0] - 2026-10-30

The first public release.

### Added

- The engine: a GraphQL parser, validator and executor in Rust, held byte for byte to graphql-js
  16.12.0 by the test suite, and one writer of JSON bytes that keeps the key order of JavaScript.
- The single pass, which validates a document in one walk over its tokens against a field table
  built from the schema, with the 27 validation rules behind it to write every error. It is the
  default of every build, and the build settings `AOSABI_VALIDATION` and `AOSABI_SINGLE_PASS_DEPTH`
  choose the validation and the bound on nesting.
- The Rust layer: a service written in plain Rust with no macro, derive or attribute of Aosabi,
  whose schema the builder reads from the source, its index file the crate root.
- The TypeScript layer: the npm package `aosabi` with the engine as `core.wasm` and as a native
  library for Bun on Linux, and the dev package `aosabi-build` with `aosabi migrate` and
  `aosabi build`, serving apps on Bun and on Cloudflare Workers.
- The compatibility layer `aosabi/pylon`, which runs Pylon v3 apps on the Aosabi engine.
- The native server: HTTP/1.1, HTTP/2 through TLS 1.3 with `rustls` and `ring`, HTTP/2 by prior
  knowledge, HTTP over a Unix socket with a check of the peer, and deflate as content coding.
- GraphTP/1, the Graph Transfer Protocol, in the crates `graphtp` and `graphtp-homa`, the Homa
  transport in the crate `homa`, the carriers of GraphTP on Linux, Serial Line, USB, Bluetooth and
  raw IP, and the npm package `graphtp` with `graphtpFetch` for JavaScript clients.
- Server push: a publish primitive with no thread, clock or atomic operation, a pub sub that any
  code of a service can publish to, and subscriptions over server sent events and over GraphTP.
- Plugins: identity and authorization, the OpenID Connect login and provider through Zitadel, Sentry
  and federation, each behind a feature of the facade.
- The command line: `cargo aosabi build` with the built in cards `server`, `server-legacy`,
  `docker`, `worker` and `esp32c3-devkitm1`, `cargo aosabi dev`, and `aosabi new`, `inspect`,
  `verify`, `mcp` and `pull`.
- Embedded targets: the kernel crate without the standard library, the core crates for the ESP32-C3,
  and one static binary for `mipsel-unknown-linux-musl`.
- The test suite: Jupyter notebooks run by papermill, with `aosabi-devtool` as the seam to the
  engine and graphql-js 16.12.0 as the oracle.
- The documentation under `Documentation/` and the public roadmap under `roadmap/`.

### Security

- TLS 1.3 alone on the server and on the outbound client of a native build.
- `unsafe_code` forbidden in the whole workspace, with no unsafe block in the project's own code,
  and the exports and imports of the WebAssembly modules as the named exceptions.
- Every optional feature off until a service names it, and no telemetry of any kind.

[0.1.0]: https://github.com/netsnek/aosabi/releases/tag/v0.1.0
