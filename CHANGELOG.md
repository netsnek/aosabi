# Changelog

All notable changes to Aosabi are written down in this file. The format is based on [Keep a
Changelog](https://keepachangelog.com/en/1.1.0/), and Aosabi follows [Semantic
Versioning](https://semver.org/spec/v2.0.0.html), under which a version before 1.0 may change its
API between minor versions.

## [0.1.1] - 2026-11-16

### Added

- `aosabi::embed`, the engine and the roots of an app inside the program of a host. A host that
  links the app into its own program, such as a module of WebAssembly that a Web Worker of a page
  runs, boots it with `Embedded::boot` and hands each request over as the JSON body of a GraphQL
  request over HTTP, with no socket, no clock and no thread. On `wasm32-unknown-unknown` the host
  installs a source of random bytes with `install_randomness` before its first answer, and the
  facade hands it `Randomness` and `RandomError` for that. The entry below it is
  `aosabi_core::embed::answer`, which the operation `embed` of `aosabi-devtool execute` reaches, and
  `tests/notebooks/80_embed.ipynb` holds it byte for byte to the executor over the capture corpus
  and a complete host to the served binary of its app. The guide
  [Documentation/guides/embedding.md](Documentation/guides/embedding.md) says when to embed and
  shows the build of a host and that complete host.
- `aosabi::embed::Embedded` reads the key `maskedErrors` of the app as the HTTP road does, and its
  boot answers an error for a key that sets a mask function of its own, `mask_error`.
- `aosabi::embed::Embedded` reads the key `introspection` of the app as the HTTP road does, and
  `aosabi_core::embed::answer` takes the keys `maskedErrors` and `introspection` as its parameters.
- The key `introspection` of the configuration, `.introspection(false)` on the Rust layer, which
  turns introspection off. Introspection stays on where an app sets no key.
- A cap on the tokens of a request document, 120,008 by default, which the key `maxTokens` of the
  configuration, `.max_tokens(..)` on the Rust layer, raises or lowers. A document past the cap
  answers the syntax error of graphql-js that names the cap.
- The option `cookie_name` of `ZitadelLoginOptions` and `ZitadelAuthOptions` and the key
  `cookieName` of `zitadelLogin`, `zitadelAuth` and `useAuth`, which name the session cookie of the
  login, `aosabi-auth` where they name none. A name that is no valid cookie name fails the setup of
  the login routes on the Rust layer and the call of `zitadelLogin`, `zitadelAuth` or `useAuth` in
  the TypeScript layer, each with a `TypeError` that names the option, and the logout deletes a name
  with the prefix `__Host-` or `__Secure-` with `Secure`.
- The guides [Documentation/guides/hardening.md](Documentation/guides/hardening.md), which lists
  each step by which a service hardens itself for production with its line of code on both layers,
  and [Documentation/guides/upgrading.md](Documentation/guides/upgrading.md), which says what a
  service changes to move to this release.
- A project that `aosabi new` writes points at the hardening guide in the first line of its
  `Cargo.toml` and in its file `AGENTS.md`.
- A project that `aosabi new` writes registers `cors` for `https://aosabi.org` on `/graphql` as
  visible lines of its `src/lib.rs`, so the hosted explorer reads a new service, and the comment
  above them points at the hardening guide.

### Changed

- The endpoint is the path `/graphql` alone, and `/graphql/` takes the steps of any other path.
- An unexpected error keeps its own message in every build until the app sets the key
  `maskedErrors`, whatever `NODE_ENV` says, and the mask answers `Unexpected error.` with the code
  `INTERNAL_SERVER_ERROR`.
- The logger of graphql-over-http writes a thrown error on standard error as the first line of its
  stack, the name and the message of the error a resolver threw up to the first line break.
- `cargo aosabi build` and `aosabi build` write the current schema to `schema.graphql` in the folder
  of the project as well, beside `Cargo.toml` or `package.json`, as the last step of a build and
  only where its text changed, and a new project keeps the file in git.
- Every parse of a request document stops at 1,000 levels of nesting with a syntax error, the levels
  of the fragments that a spread names counted.
- `cors(options)`, `compress()` and `body_limit(options)` of the facade are middleware that
  `.middleware(path, ..)` of the chain registers.
- A `csrf(options)` that the app registers takes the place of the default csrf check of a Rust
  service only on a request where it stands ahead of every handler of the routes the request
  matches, the GraphQL endpoint among them. A middleware counts as a handler through the new
  provided method `MiddlewareFn::is_handler`.
- `csrf()` reads `text/ping`, the type of the ping of a link, as a type that a form sends.
- An app's own `body_limit(options)` reads a body with no usable `Content-Length` through its bound
  before the routes run.
- `compress()` adds `Accept-Encoding` to the field `Vary` of every answer it may code, whether the
  request accepts a coding or not.
- With an option `origin` other than `*`, the preflight of `cors()` appends `Origin` to a field
  `Vary` that a middleware before it set.
- `serve_static` names a file by the routed path, decoded once, and serves no file for a path that
  holds a `%`, a segment of one or two dots, a run of two or more `/` or a `\`.
- `serve_static` answers `GET` and `HEAD` alone and hands a request of any other method to the next
  handler.
- The query of a URL ends at its first `#`.
- A query key that holds `&` or `=` matches the name of a parameter.
- The native server answers 400 to a request whose path holds a `\`.
- A route whose parameter carries a constraint, such as `:id{[0-9]+}`, panics at its registration
  with a message that names the route.
- The writer of a cookie refuses a name that is no valid cookie name, and a `domain` or a `path`
  that holds `;`, a carriage return or a line feed, with an error in place of the cookie.
- The reader of the `Cookie` field trims the space and the tab alone.
- The reader of the `Cookie` field answers the first value of a name it meets twice.
- The Rust layer gives the rules of the cookie prefixes `__Secure-` and `__Host-` to a name in any
  case of their letters.
- The login cookies carry `SameSite=Lax`, and `Secure` where the request reached the service over
  HTTPS.
- The callback of the login checks its state against the cookie of the verifier.
- The callback of the login sends the token endpoint the parameters of the code grant alone.
- The request data of an event of the Sentry plugin leaves out the fields that hold a credential
  and the query parameter `token`.
- An event of the Sentry plugin carries the variables of an operation only where the app sets
  `includeExecuteVariables`, `include_execute_variables` on the Rust layer.
- An event of the Sentry plugin carries no field that holds the address of the client.
- `init` of `aosabi/sentry/node` with `enabled: false` sets up no client, so a TypeScript app sends
  no event.
- On `wasm32-unknown-unknown`, `xous` and `vexos` the engine answers no request until the host
  installs a provider of random bytes with `install_randomness`, with 503 over HTTP and 503.0 over
  GraphTP.
- The TypeScript layer takes the router of its apps in the range `^4.13.16` as a peer dependency.
- `aosabi new` writes a project only with `--aosabi-path`, the path of a checkout of Aosabi whose
  crates the project names by path, and refuses with `AO-C0405` before it writes a file without it.
- Every command that loads a project refuses with `AO-C0108` a crate of Aosabi from a registry, in
  any spelling of its name, that a manifest names, a lock pins or the resolved graph holds, and the
  hints of the command line show the path form of a checkout.
- The refusal `AO-C0103` of a service that is a member of a workspace above it, which lists
  members beside it, names the manifest of that workspace, and its hint says how the service stays
  the only member of that workspace or leaves it. The hint for a service whose own `[workspace]`
  table lists members also names the `exclude` of that table.
- Under `--locked` or `--frozen`, `AO-C0302` also refuses a missing lock of the checkout of a path
  facade that a card builds the crates of its kernel in.
- The watcher of `aosabi dev` also watches the files the last good load lists, the manifests of the
  path packages, the root manifest and the lock of a workspace above the project and the
  configuration files of cargo, and after a reload that failed it reads the project again before
  every build until a reload succeeds.
- The worker role of `aosabi dev`, `--worker`, watches `Cargo.toml` and `Cargo.lock` of the project
  as well.
- The package `aosabi` names no platform package. An app installs the platform package of its
  machine from a checkout by the path of its folder under `aosabi-ts-native`, or as the tarball
  `npm pack` makes of it, and `aosabi/bun` looks for it in the folder `node_modules` that holds
  `aosabi` and in the one of the root of the app.
- Where a crate of the workspace or an npm package holds code under the Apache License 2.0, its
  manifest declares `MIT AND Apache-2.0`, and the manifest of the npm package `aosabi-build`
  declares `Unicode-3.0` as well.

### Removed

- The pages for development of the server, with the calls and the keys that turned them on, and its
  health check. The explorer of a schema runs on the site of Aosabi.
- The key `graphiql` and the builder call `.graphiql(..)` are gone, so an app turns introspection
  off with `introspection: false`, or `.introspection(false)` on the Rust layer.

### Fixed

- The native server and the client of aosabi-fetch send the TLS bytes that rustls holds, an alert
  among them, before a connection that fails reports its error.
- An operation that names a persisted document by its id gets the one error
  `The engine accepts no persisted document.`
- `cargo aosabi build` builds a service that names the facade from a repository by a tag, a branch,
  a short revision or the default branch, as it builds one that names the whole commit.
- `cargo aosabi build` derives the lock of its generated workspace anew where that lock names a
  package the workspace does not reach, such as a crate a newer facade dropped.

### Security

- This release carries security fixes.

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

[0.1.1]: https://github.com/netsnek/aosabi/releases/tag/v0.1.1
[0.1.0]: https://github.com/netsnek/aosabi/releases/tag/v0.1.0
