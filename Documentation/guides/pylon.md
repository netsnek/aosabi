# Compatibility with Pylon v3

Pylon is a full stack web framework for TypeScript, and Aosabi is a GraphQL server. The
compatibility layer `aosabi/pylon` gives an app written for Pylon v3 the API it was written against,
served by the Aosabi engine, so a Pylon v3 app runs on Aosabi through the TypeScript layer of
[typescript.md](typescript.md). This page says how an app moves over, what the layer keeps, where
Aosabi answers otherwise on purpose, and what the layer does not cover.

The reference is Pylon at commit `2653a81` of its branch `feat/v3-fullstack`, published as
[getcronit/pylon](https://github.com/getcronit/pylon), whose package carries the version 2.9.5 at
that commit. Pylon is licensed Apache-2.0, and the files of the layer that derive from it say so, as
[licensing.md](../licensing.md) describes.

## 1. Moving an app

The command `aosabi migrate <dir>` of the dev package `aosabi-build` turns a Pylon v3 app into an
app of the package `aosabi` by its import specifiers:

- It reads every `.ts`, `.mts`, `.js` and `.mjs` file under `<dir>`, outside `node_modules` and the
  directories whose name starts with a dot, and rewrites the module specifier of an import, of an
  export that names a module, of a call of `import()` with a literal and of the name of a
  `declare module` block. A specifier in a string or a comment is never touched.
- It maps each specifier by one table, `aosabi-ts-build/migrate-map.json`, swaps the dependency of
  the app's `package.json` and its build command, and logs every rewritten line in
  `.aosabi/migrate.json`. The flag `--reverse` restores every file byte for byte.
- It stops before it writes anything when a specifier, a name or a plugin hook has no counterpart in
  the engine, and names the file and the line of each.

## 2. What the layer keeps

The layer keeps the surface an app was written against:

- The schema the build writes, `schema.graphql`, with its type order, its descriptions and its
  scalar declarations, and the `resolvers.js` and `ir.json` beside it.
- The HTTP answers of the app: the status, the headers that matter and the body, the errors of every
  layer with their messages, and the frames of a subscription over server sent events.
- The plugins of an app with their members and their `onExecute` hooks, the context of a request
  through `getContext()`, the logger, `ServiceError` and the auth entries `aosabi/auth`,
  `aosabi/auth/plugin`, `aosabi/auth/contract` and `aosabi/auth/zitadel`.

The parity suite of the project runs its fixture apps of Pylon v3 on the layer, on workerd and on
Bun, on `core.wasm` and on the native library, with Pylon v3 at a pinned commit. It reads recorded
answers and parts of the test harness that this repository does not carry, so it runs in the
project's own pipeline. Section 3 lists the differences an app of Pylon v3 meets in its code.

## 3. Where Aosabi answers otherwise

Each of these differences is deliberate, small and named, and the test suite holds it apart from the
comparison by name. [security.md](security.md) gives the defaults of Aosabi in full.

The endpoint and the server:

1. The endpoint is the path `/graphql` alone. A request to `/graphql/` takes the steps of any other
   path and gets the not found answer of the app, so an app that serves the form with the slash adds
   a route that redirects it with 308.
2. The server answers no health check and serves no page for development. A request to `/health`
   takes the steps of any other path, and the configuration has no key `healthCheckEndpoint`. The
   explorer of a schema runs on the site of Aosabi.
3. The engine adds no CORS header until the app writes `cors()` with the origins it allows. The
   preflight of `cors()` with an option `origin` other than `*` appends `Origin` to a field `Vary`
   that a middleware before it set.
4. On the Rust layer the base pipeline codes no answer, and `compress()` codes the answers of the
   routes after it as deflate where the client names deflate and leaves them plain otherwise, since
   the native server codes no gzip, and adds `Accept-Encoding` to the field `Vary` of each answer it
   may code. A coded answer carries the same body after decoding. The TypeScript layer codes its
   answers as `compress()` of `hono` codes them.

Errors and the schema:

5. An unexpected error keeps its own message in every build and whatever `NODE_ENV` says, until the
   app sets `maskedErrors: true`, or `.masked_errors(MaskedErrors::On)` on the Rust layer. The mask
   then answers `Unexpected error.` with the code `INTERNAL_SERVER_ERROR`.
6. Introspection stays on until the app sets `introspection: false`, a key of Aosabi's own, or
   `.introspection(false)` on the Rust layer.
7. A build writes the current schema to `schema.graphql` in the folder of the project as well,
   beside `package.json` or `Cargo.toml`, as its last step and only where the text changed, and a
   new project keeps that file in git.

Requests:

8. A request document stops at 1,000 levels of nesting with the syntax error `Document nests deeper
   than 1000 levels. Parsing aborted.`, and at 120,008 tokens with the syntax error of graphql-js
   that names the cap, which the key `maxTokens`, or `.max_tokens(..)` on the Rust layer, raises or
   lowers.
9. On the Rust layer the base pipeline bounds a request body at 1 MiB and answers 413 `Payload Too
   Large` past it. An app's own `body_limit` takes its place on its path and reads a body with no
   usable `Content-Length` through its bound before the routes run.
10. On the Rust layer the base pipeline refuses with 403 a form that another site posts with a
    cookie, unless a `csrf(options)` of the app stands ahead of the handlers of the routes the
    request matches, the GraphQL endpoint among them. The TypeScript layer registers no csrf of its
    own, and an app there writes `csrf()`.
11. `csrf()` reads `text/ping`, the type of the ping of a link, as a type that a form sends.
12. On the Rust layer uploads are accepted once the app sets `.multipart(true)`.
13. The query of a URL ends at its first `#`. On the Rust layer a query key that holds `&` or `=`
    matches the name of a parameter, the native server answers 400 to a request whose path holds a
    `\`, and a route whose parameter carries a constraint, such as `:id{[0-9]+}`, is refused at its
    registration with a panic that names the route.
14. On the Rust layer `serve_static` names a file by the routed path, decoded once, serves no file
    for a path that holds a `%`, a segment of one or two dots, a run of two or more `/` or a `\`,
    and answers `GET` and `HEAD` alone.

Cookies and the login:

15. The writer of a cookie refuses a name that is no valid cookie name, and a `domain` or a `path`
    that holds `;`, a carriage return or a line feed, with an error in place of the cookie. The
    reader of the `Cookie` field trims the space and the tab alone and answers the first value of a
    name the field holds twice. On the Rust layer the prefixes `__Secure-` and `__Host-` hold in any
    case of their letters.
16. The login routes name the session cookie `aosabi-auth` and the cookie of the verifier
    `aosabi_code_verifier`. The option `cookieName` of `zitadelAuth`, `zitadelLogin` and `useAuth`,
    `cookie_name` on the Rust layer, names the session cookie, so an app that moves over keeps the
    name its sessions carry.
17. The helper `requireAuth` refuses a request with no user with 401.

Sentry and the rest:

18. An event of the Sentry plugin carries the variables of an operation only where the app sets
    `includeExecuteVariables`, and an app that calls `init` with `enabled: false` sends no event.
19. Aosabi sends no telemetry.
20. An outbound call of a native Rust build speaks TLS 1.3 alone and ends after 30 seconds. A
    TypeScript app takes its outbound calls from its host.

[upgrading.md](upgrading.md) says what a service written for an earlier release of Aosabi changes.

## 4. What the layer does not cover yet

- The subpaths `db`, `db/plugin`, `queues`, `query` and `ir` of the package, which the
  [roadmap](../../roadmap/compatibility.md) holds.
- The subpaths `pages` and `pages/plugin`, since server side rendering of React pages is out of
  scope for good: Aosabi serves APIs to headless frontends, and an app moves its pages into a
  frontend of its own.
- The watch and reload of `pylon dev` for a TypeScript app.
- Node.js and Deno as hosts of an app, beside Bun and Cloudflare Workers.
- The signed forms of the cookie helpers.
- Uploads in the TypeScript layer, whose configuration has no key `multipart` yet, so an app there
  takes no upload of the GraphQL multipart request form.

The command `aosabi migrate` stops on a specifier or a name of one of these with its file and its
line, so an app never reaches a missing part at run time.

## 5. A Pylon service in Rust

A Pylon service can also be ported to the Rust layer of [rust.md](rust.md), where it answers the
same schema as long as the port keeps the names and the shapes. The forms map construct by
construct:

| Pylon v3 | Rust layer |
| --- | --- |
| `export default new Pylon({graphql})` | The entry `pub fn app() -> impl IntoApp` with its chain |
| A class with properties and methods | A struct and its inherent `impl` blocks |
| `class Image extends Media` | The shared fields written out, and an enum for a mixed list |
| `@audited(..)` of `createDecorator` | A guard function the method calls first |
| `x?: T` and a default | `Option<T>` and `unwrap_or` in the body |
| A TypeScript `number` | The marker `Number` |
| JSDoc | Doc comments, with `@param` lines for the arguments |
| `pylon.config.ts` | Calls of the chain in the index |

A construct that only JavaScript has, such as inheritance or a decorator, has no Rust form, and the
TypeScript layer keeps Pylon's own behaviour for it.
