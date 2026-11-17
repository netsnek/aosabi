# Upgrading between releases

This page says what a service written for one release of Aosabi changes to run on the next.
[CHANGELOG.md](../../CHANGELOG.md) lists every change of a release, [pylon.md](pylon.md) section 3
lists where Aosabi answers otherwise than Pylon v3, and [hardening.md](hardening.md) gives each step
before production with its line of code.

## 1. Moving to 0.1.1

### 1.1 Every service

- The server holds no page for development and no health check of its own. A request to `/health`
  takes the steps of any other path and gets the not found answer of the app, so a service that a
  monitor checks there writes that route itself. The explorer of a schema runs on the site of
  Aosabi, and a project that `aosabi new` writes registers `cors()` for `https://aosabi.org` on
  `/graphql` so that the explorer reads it. An existing app adds the same `cors()` lines to its
  entry to use the hosted explorer, and before production a service removes or narrows them as
  [hardening.md](hardening.md) section 4 says.
- The endpoint is the path `/graphql` alone. A client that sends its requests to `/graphql/` sends
  them to `/graphql`, or the app adds a route that redirects the form with the slash with 308.
- An unexpected error keeps its own message in every build, whatever `NODE_ENV` says, until the app
  sets the key `maskedErrors`. A service that masks such errors in production sets
  `maskedErrors: true`, or `.masked_errors(MaskedErrors::On)` on the Rust layer.
- Introspection stays on until the app sets `introspection: false`, or `.introspection(false)` on
  the Rust layer. An app that set `graphiql: false` in 0.1.0 had introspection refused, and in 0.1.1
  it sets `introspection: false` to keep refusing it, since the key `graphiql` is gone. On the Rust
  layer `.introspection(false)` takes the place of `.graphiql(false)` in the same way.
- A request document of more than 120,008 tokens, or of more than 1,000 levels of nesting, answers
  a syntax error before its validation. A service whose clients send longer documents raises the cap
  of the tokens with `maxTokens`, or `.max_tokens(..)` on the Rust layer.
- `cargo aosabi build` and `aosabi build` write the current schema to `schema.graphql` in the folder
  of the project as well, beside `Cargo.toml` or `package.json`, as the last step of a build and
  only where its text changed. A file of that name that the project holds is overwritten. A new
  project keeps the file in git, so a change of the schema shows in the diff of the change, and a
  project that keeps it out of git names it in its ignore file.
- The session cookie of the login keeps the name `aosabi-auth` unless the service names another,
  with the option `cookie_name` of `ZitadelLoginOptions` and `ZitadelAuthOptions` on the Rust layer
  or the key `cookieName` of `zitadelLogin`, `zitadelAuth` and `useAuth` in the TypeScript layer, on
  both halves alike. A name that is no valid cookie name fails the setup of the login routes on the
  Rust layer and the call of `zitadelLogin`, `zitadelAuth` or `useAuth` in the TypeScript layer,
  each with a `TypeError` that names the option, and a name with the prefix `__Host-` or `__Secure-`
  needs HTTPS at the callback of the login, as [hardening.md](hardening.md) section 8 says.
- An event of the Sentry plugin carries the variables of an operation only where the app sets
  `includeExecuteVariables`, or `include_execute_variables` on the Rust layer, and no field that
  carries the address of the client. A service that reads the variables in its events sets the
  option.

### 1.2 The Rust layer

- A project names the crates of Aosabi by the path of a checkout or from the repository of Aosabi,
  never from a registry. Every command that loads a project refuses with `AO-C0108` a crate of
  Aosabi that a manifest, a pin of the lock or the resolved graph names from a registry, and its
  hint shows the entry in the path form. `aosabi new` writes a project only with
  `--aosabi-path <checkout>` and refuses with `AO-C0405` before it writes a file without it. The
  publish workflow of a new project installs the command line from that checkout, so a project
  whose checkout lies outside its repository adds it, as a submodule for example, before its first
  push.
- `cors(options)` and `compress()` of the facade are middleware that the chain takes with
  `.middleware(path, ..)`, as it takes `body_limit(options)`.
- A `csrf(options)` of the service takes the place of the default csrf only on a request where it
  stands ahead of the handlers of the routes the request matches, and the GraphQL endpoint counts as
  such a handler. So a `csrf(options)` that a plugin registers with `Strategy::Last`, or that stands
  behind a route handler, leaves the default in place. A service registers it with `use_boxed` in a
  plugin that runs first, as [hardening.md](hardening.md) section 3 shows. A middleware of `use`
  that answers its requests as a handler does returns `true` from `MiddlewareFn::is_handler`.
- An app's own `body_limit(options)` reads a body with no usable `Content-Length` through its bound
  before the routes run, so a body past the bound answers `on_error` also where the route never
  reads its body.
- A route that writes a cookie whose name is no valid cookie name, or whose `domain` or `path` holds
  `;`, a carriage return or a line feed, answers the error of the writer, and a name with the prefix
  `__Secure-` or `__Host-` in any case of its letters takes the rules of that prefix.
- `serve_static` answers `GET` and `HEAD` alone and serves no file for a path that holds a `%`, a
  segment of one or two dots, a run of two or more `/` or a `\`, so a file whose name holds such a
  part is served under another name.
- A route whose parameter carries a constraint, such as `:id{[0-9]+}`, panics at its registration
  with a message that names the route, so a service checks such a parameter in its handler.
- The query of a URL ends at its first `#`, a query key that holds `&` or `=` matches the name of a
  parameter, and the native server answers 400 to a request whose path holds a `\`.
- A host on `wasm32-unknown-unknown`, `xous` or `vexos` installs a provider of random bytes with
  `install_randomness` before its first request. Until it does, its app answers every request with
  503 and a text that names `install_randomness`, and over GraphTP with 503.0 and an empty body, as
  [hardening.md](hardening.md) section 11 says.

### 1.3 The TypeScript layer

- An app installs `hono` in the range `^4.13.16`, which the package `aosabi` names as its peer.
- The package `aosabi` names no platform package. An app installs the platform package of its
  machine from a checkout of Aosabi, by the path of its folder under `aosabi-ts-native` or as the
  tarball `npm pack` makes of it, beside `aosabi` and `aosabi-build` from the same checkout, as
  [typescript.md](typescript.md) section 2 shows. `aosabi/bun` looks for it in the folder
  `node_modules` that holds `aosabi` and in the one of the root of the app, and runs `core.wasm`
  with a reason that names the folder to install it from where neither holds it.
- An app that calls `init` of `aosabi/sentry/node` with `enabled: false` sends no event.
