# Documentation

This folder is the documentation of Aosabi, a very small, very fast GraphQL server that runs from
microcontrollers to the edge. Every page is plain Markdown, every statement holds for the code of
the same revision, and every measured number names the notebook that measured it.

## The design

- [principles.md](principles.md) holds the five principles: single pass, speed, security over
  features, boilerplate over magic and embedded first.
- [architecture.md](architecture.md) shows the path of a request, the layers and their crates, and
  how one engine serves two languages.
- [coding-style.md](coding-style.md) is the coding style, the Linux kernel's, as it holds for Rust
  here.
- [licensing.md](licensing.md) names the license of every part of the repository.

## Guides

- [guides/rust.md](guides/rust.md) writes a service in plain Rust.
- [guides/typescript.md](guides/typescript.md) builds and serves a TypeScript app on Bun and on
  Cloudflare Workers.
- [guides/graphtp.md](guides/graphtp.md) describes GraphTP, the protocol for links where HTTP does
  not fit.
- [guides/embedded.md](guides/embedded.md) says what runs on small Linux devices and on boards
  without an operating system.
- [guides/security.md](guides/security.md) lists the security defaults with the code that sets them.
- [guides/pylon.md](guides/pylon.md) covers compatibility with Pylon v3 through `aosabi/pylon`.
- [guides/conformance.md](guides/conformance.md) shows how the test suite holds the engine to
  graphql-js.

## Elsewhere

- [README.md](../README.md) at the root of the repository is the start.
- [roadmap/](../roadmap/README.md) is the public roadmap.
- [CONTRIBUTING.md](../CONTRIBUTING.md) says how to contribute, and [SECURITY.md](../SECURITY.md)
  how to report a vulnerability.
- [examples/](../examples/) holds complete services.
