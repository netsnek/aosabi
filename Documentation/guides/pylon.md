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
Bun, on `core.wasm` and on the native library, with Pylon v3 fetched by `tests/fetch_oracles.py` at
the commit `tests/oracles.toml` pins. It reads recorded answers and parts of the test harness that
this repository does not carry, so it runs in the project's own pipeline. Every difference the suite
allows is one of section 3.

## 3. Where Aosabi answers otherwise

Each of these differences is deliberate, small and named, and the test suite holds it apart from the
comparison by name.

1. An answer travels as deflate where the client names deflate and plain otherwise, since Aosabi
   codes no gzip, and its body is the same after decoding.
2. The route `/viewer` answers a page of Aosabi's own, which loads nothing from a CDN.
3. The engine adds no CORS header until the app writes `cors()` with the origins it allows.
4. The landing page, GraphiQL and `/viewer` answer only when the app turns them on, with
   `landingPage: true`, `graphiql: true` or `viewer: true`.
5. Uploads are accepted once the app sets `multipart: true`.
6. The helper `requireAuth` refuses a request with no user with 401.
7. Aosabi sends no telemetry.
8. An outbound call of a native Rust build speaks TLS 1.3 alone and ends after 30 seconds. A
   TypeScript app takes its outbound calls from its host.

[security.md](security.md) gives the defaults of Aosabi in full.

## 4. What the layer does not cover yet

- The subpaths `db`, `db/plugin`, `queues`, `query` and `ir` of the package, which the
  [roadmap](../../roadmap/compatibility.md) holds.
- The subpaths `pages` and `pages/plugin`, since server side rendering of React pages is out of
  scope for good: Aosabi serves APIs to headless frontends, and an app moves its pages into a
  frontend of its own.
- The watch and reload of `pylon dev` for a TypeScript app.
- Node.js and Deno as hosts of an app, beside Bun and Cloudflare Workers.
- The signed forms of the cookie helpers.

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
