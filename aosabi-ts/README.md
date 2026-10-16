# aosabi

The package `aosabi` runs an app written in TypeScript on the Aosabi engine. An app written for
Pylon v3 keeps its source byte for byte and changes its import specifiers and a few names, which
`aosabi migrate` rewrites: its root becomes the entry `aosabi/pylon` and its class becomes `App`,
`graphql-yoga` becomes the root `aosabi`, every other subpath and every package pulled in that has
an entry becomes the same path under `aosabi/`, and the configuration of the app and its kin take
Aosabi's own names, `AppConfig` among them. An import of `hono` or of `@hono/node-server` stays as
it is. The class `App` is a Hono of the real hono, the peer of the package in the range `^4.13.14`,
with the base pipeline in hono's own code, so the routes, the middleware and the context `c` of an
app are hono's. The engine under it is the module `core.wasm`, built from the Rust
crates of the Aosabi workspace, which answers the GraphQL endpoint and the hooks of the execution,
with a thin wrapper of plain ES modules in `lib/`, and there is no GraphQL Yoga and no graphql-js
under it.

## The quick start

This is the quick start of the [TypeScript guide](../Documentation/guides/typescript.md). An app is
an `App` with its GraphQL roots, in one file:

```ts
// SPDX-License-Identifier: MIT
/*
 * quick-start: the first app of the TypeScript guide, one query that greets.
 */
import {App} from 'aosabi/pylon'

export default new App({
  graphql: {
    Query: {
      hello: (): string => {
        return 'Hello, world!'
      }
    },
    Mutation: {}
  }
})
```

With this package, its peer hono, the dev package `aosabi-build` and Bun 1.3.14, the app builds and
serves in two commands:

```sh
npx aosabi build
PORT=3000 bun --preload aosabi/bun .aosabi/server.mjs
```

The first request asks for `hello`:

```http
POST /graphql HTTP/1.1
Content-Type: application/json

{"query":"{ hello }"}
```

It answers:

```http
HTTP/1.1 200 OK
content-type: application/json; charset=utf-8

{"data":{"hello":"Hello, world!"}}
```

## How an app imports the entries

```ts
import {App, type AppConfig} from 'aosabi/pylon'  // the app imported '@getcronit/pylon'
import {Repeater} from 'aosabi'                    // the app imported 'graphql-yoga'
import {cors} from 'hono/cors'                     // kept, the real hono, the peer of the package
```

A host hands the app its engine. On a Cloudflare Worker the entry `aosabi/worker` is the fetch
handler over an app and the module `core.wasm`, and on Bun, the host of the Linux containers, the
entry `aosabi/bun` loads the native library of the engine or `core.wasm`, hands every app of the
process that engine when it loads and serves an app over `Bun.serve`. The step `aosabi build`
writes the folder `.aosabi/` of the app, among its files the glue `server.mjs`, which exports the
app and exposes the IR of the build as `globalThis.__AOSABI_IR__`, the IR itself as `ir.json` and
`ir.mjs`, and the Worker entry `worker.mjs`, which imports `aosabi/worker` before the glue and
exports its fetch handler over the app and the IR the glue exposes. So a Worker names
`.aosabi/worker.mjs` as the `main` of its configuration of Wrangler, and a container runs
`bun --preload aosabi/bun .aosabi/server.mjs`, as the quick start does. The glue reads
`import.meta.url`, which workerd leaves undefined, so the configuration of Wrangler defines it, as
section 12 of the guide shows for its example `hosts`.

An entry the app writes itself takes the same files of the build. On a Worker it hands `worker()`
the IR of `ir.json`:

```ts
import {worker} from 'aosabi/worker'    // loaded first, so the glue below finds the engine
import app from './.aosabi/server.mjs'  // the glue of the build
import ir from './.aosabi/ir.json'      // the IR of the build, which aosabi build writes beside it

export default worker(app, {ir})
```

On Bun it serves the app with `serve()`, which hands the app the IR the glue exposes, where the
configuration of the app names no serving plugin:

```ts
import {serve} from 'aosabi/bun'        // loaded first, so the glue below finds the engine
import app from './.aosabi/server.mjs'  // the glue of the build, which exposes the IR of its build

serve(app, {port: 3000})                // over Bun.serve
```

The judge mode of the harness of the Aosabi repository serves the glue of the reference build the
same way, its `server.mjs` with its specifiers rewritten by `aosabi migrate`, which holds no IR, so
the harness writes the IR its sidecar answers for the same source as `ir.json` beside that glue and
hands it to `worker(app, {ir})` on workerd and to `host({ir})` on Bun.

`worker()` boots the app at its call, so an entry that calls it in the global scope of the Worker
once the glue has loaded, as above, boots the app before its first request, and Cloudflare starts to load a Worker deployed to a root host name at the
first packet of the TLS handshake of a cold request. The boot takes what the global scope holds when
the entry calls `worker()`: the GraphQL handler, the plugins and the configuration the glue
recorded, the options of `Sentry.init` of `aosabi/sentry/node`, the IR, and the variables of the
process with the text bindings the Worker exports to that scope. So `Sentry.init` and every other
input of the boot must come before that call, as the glue and the configuration it imports make them
at their load, and the instances of the boot keep what it took. The first request holds the
variables alone to those of the boot: a first request whose variables differ boots the app again
with its own, as it boots with no early boot (`lib/early.js`). The default export of
`aosabi/worker`, the handler for the glue of `aosabi build`, calls `worker()` at the first request,
so its app boots there. The option `ir` takes in place of the IR the schema record of the build that
`lib/schema-record.js` of `aosabi-build` writes, from which the core builds the schema of the app
with no parse of its SDL, and a record of another SDL than the build hands the boot fails the boot
with a message that asks the build to write it again.

The command `aosabi migrate` of the dev package `aosabi-build`, whose source is the folder
`aosabi-ts-build/`, writes the change of the specifiers at the top of this section. This folder is
the source of the package until the owner publishes it, and nothing of it is published.

## The entries

| Entry | Replaces | Lands with |
| --- | --- | --- |
| `aosabi/pylon` |  |  |
| `aosabi` |  |  |
| `aosabi/auth`, `aosabi/auth/plugin`, `aosabi/auth/contract` | The same subpaths of `@getcronit/pylon` |  |
| `aosabi/auth/zitadel` | `@getcronit/pylon/auth/zitadel` |  |
| `aosabi/graphql` | `graphql` |  |
| `aosabi/sentry/node` | `@sentry/node` |  |
| `aosabi/worker` | The Wrangler entry |  |
| `aosabi/bun` | The app on Bun, the native engine over `bun:ffi` with `core.wasm` as its fallback, and the serving of an app over `Bun.serve` |  |
| `aosabi/unsupported/*` |  | T2 |

A name of a replaced package whose engine item is missing is not exported.

## The entry `aosabi/unsupported/*`

Only the harness reaches it, through `aosabi migrate --mark-unsupported`, so that a fixture which
imports such a name without calling it still loads. Every other run of `aosabi migrate` stops on the
specifier.

## The native engine on Bun

On Linux `aosabi/bun` runs the engine as a native library over `bun:ffi` in place of `core.wasm`.
The library comes in one platform package per target, `aosabi-linux-x64-gnu`,
`aosabi-linux-arm64-gnu`, `aosabi-linux-x64-musl` and `aosabi-linux-arm64-musl`, whose source
folders stand under `aosabi-ts-native/` and which this package names as its `optionalDependencies`,
so a package manager installs the one whose `os`, `cpu` and `libc` match. Where none is installed
`aosabi/bun` takes `core.wasm`, and the variable `AOSABI_ENGINE` of the environment forces either,
`wasm` for `core.wasm` and `native` for the library, which then must load.

## The host of the containers on Bun

The entry `aosabi/bun` hands the class `App` the engine of its thread when it loads, made at the
first request or the first logger call, so every app of the process boots on it, and
`host({ir, mode, read})` hands every app the IR of its build and the two modes beside it, where the
glue of `aosabi build` exposes the IR as `globalThis.__AOSABI_IR__` otherwise. `serve(app, options)`
serves an app over `Bun.serve` with the options of `Bun.serve`, the fetch of the server the fetch of
the app, `served(app)` answers the server of an app and `hostedEngine()` the engine with its kind
and the reason the native library was not taken. An app served so holds the member `stop` of its
server, so Bun, which serves the default export of an entry that holds a `fetch` and no `stop`,
serves the glue of a build it runs as its entry no second time. The server binds every address of
IPv6 and of IPv4 where the system gives IPv6, the default of `Bun.serve` and the address `node:http`
binds, and closes a connection that stayed silent for 30 s, above the 12 s between two pings of a
stream of events, where the default of Bun closes one after 10 s. The wrapper and the library hold
no HTTP code and no deflate code of their own: `compress()` of hono in the base pipeline codes a
body through the `CompressionStream` of Bun, as on a Worker.

The harness serves the fixture of its files with `@hono/node-server` 2.1.5, where the reference
lockfile pins 1.19.9, and names the changes of 2.1.5 that an answer shows with their cases, HN1 to HN5. A
Worker gives no file system, so there it serves no file.

## The entry `aosabi/pylon`

The root `aosabi`, the file `lib/index.js`, re-exports the entry, so an import of the root keeps
working.

The class `App` is a Hono of the real hono, and the first pass of `executeConfig` installs the
base pipeline on it in hono's own code: `compress()`,
the bind of the async context, the request logger behind `except` of the internal prefix
`/__aosabi/*`, the dispatch of the middleware of the plugins and the error answer. So the routes,
the middleware, the error handlers, the children, the context `c` of a request, `HTTPException` and
the helpers of an app run in hono's own code and in the order of the base pipeline. The engine
answers the landing page where the configuration sets `landingPage: true` and the viewer where it
sets `viewer: true`, divergences D47 and D48, and the three routes of the login of
`zitadelLogin`. The engine reads its own variables of a request, the bag of `@inContext` and the
info a root resolver stores under `graphqlResolveInfo`, through `c.get` and `c.var` of the same `c`
for a key the app did not set, and the request of
the engine lives until the chain of hono of the request has run and the fetch of the app has ended,
so a middleware reads them after `next()`. Where the resolvers the glue hands `handler()` hold a
type resolver of the form of the canary, the engine resolves its union or its interface by its
checks over every property name of the value. The GraphQL handler of the engine takes the body of
its request from `c.req.raw` at its first read, after any clone the app made of the request, through
the import `iter_next` into one buffer of the length its `Content-Length` names once a quarter of
that length has arrived, whose files the host reads at their place, or in the bytes of its own a
file of a part in base64 holds. Each change of hono 4.13.14 against the 4.11.9 of the reference
lockfile
that an app sees is named with its case, HV1 to HV129, from a read of every ES module of both
versions, and the changes of `@hono/node-server` 2.1.5 that reach an app of the side ts, HN1 to HN6.

CORS and CSRF of an app are hono's own, `cors()` of `hono/cors` and `csrf()` of `hono/csrf`, which
the app writes once for every route with the one list of its sites, before its routes and the
GraphQL handler of the glue, and the engine adds no check of its own. The engine parses a form at
`/graphql` as Yoga parses it, so a page of a listed site keeps working, while an app without
`csrf()` takes a form of every site there, and `csrf()` refuses a form, a multipart
body or a text body that a page of another site sends with a method that is not safe, with the
status 403 before the endpoint runs.

The marker `useSentry` crosses into the engine with its options as JSON and its option `skip` as a
function of the app, an option that holds any other function failing the boot, and its middleware in
the plugin chain of the base pipeline sets `c.get('sentry')`, the client of Sentry of the request,
the Toucan of `@hono/sentry`, through the facade of `lib/toucan.js`, whose setters, captures and
`withScope` call the client the engine keeps for the request, and whose envelopes leave through the
`fetch` of the host. A logger call writes through the export `logger` of the module, which renders
the line and hands it back through the import `host_write`, a call below the level of its logger
returns before its fields cross, and the bindings of `child()` and `createLogger()` stay on the
side of the host until a record is written. The access line `INFO [http] request` of every
request drops
with a level above `info` for its tag `http`, `LOG_LEVEL=info,http=warn` in the variables of the
process or the text bindings of a Worker, which saves the time of the line where an app needs none.
A plugin of the app crosses into the engine with its name, its strategy, its roles, its dependencies
and its hook `onExecute`, while its setup and its middleware run in the hono of the app, and the
engine calls the done hook `onExecuteDone` with the result of the execution and a `setResult`. Each
error of that result carries the nodes, the source and the positions of the error of the engine, a
node a stand in with its kind and its location, so a remap that hands `err.nodes` to a new
`GraphQLError` keeps its locations, and once the hook returned the engine takes the result the hook
leaves, the one `setResult` set last or the handed result as the hook left it, with its members in
their order, at any depth. The payloads of `onExecute` and of `onExecuteDone` hand the one view of
the arguments of the execution under `args`, its document, its variables and its operation name, and
the info a root resolver stores under `graphqlResolveInfo`, the store of `getResolveInfo()` and the
nodes of the document are views of `lib/nodes.js` that answer each member of graphql-js at its first
read, a member the engine holds no form for, such as the location of a node or the schema, throwing
a `TypeError` that names the waiting. A hook that throws rejects the execution as Envelop's does,
and the engine reads the value it threw as Yoga reads it, so a `GraphQLError` keeps its message, its
extensions and its status, and a done hook that answers a promise is awaited before the next one
runs. A hook `onExecute` that answers a promise throws a `TypeError` that names the waiting, and so
does a result whose errors the engine holds no form for, an error changed in place among them. A
plugin that holds any other hook of Envelop or Yoga fails the boot with the name of the hook. The
values `asyncContext`, `renderLine`, `jobLogLevel` and `IN_CONTEXT_SDL` of the root are not
exported, since no app of the harness and no example of the compatibility corpus imports one.

## The entries of the auth subpaths

The files `lib/auth.js`, `lib/auth-plugin.js` and `lib/auth-contract.js` hold the names of the
subpaths `auth`, `auth/plugin` and `auth/contract`: `getPrincipal`, `authorize`,
`requireRole`, `hasRole`, `hasPermission`, `PRINCIPAL_KEY`, `ForbiddenError` and `useIdentity`.
The middleware of `useIdentity` binds the principal the provider of the app answers on the context
of the request, the gates read it there and throw the `ForbiddenError` of `lib/errors.js`, and its
hook `onExecute` is the map of the engine, which turns every `ForbiddenError` of a result into the
error `FORBIDDEN` with its message and its feature as the app made them, the truth of the feature
tested on the value the app made.

## The types

The `.d.ts` files declare the names and shapes of the entries. The declaration of `HTTPException` is hono's own
since step 2, from the peer. The files `pylon.d.ts`, `graphql.d.ts`, `auth.d.ts`, `auth-plugin.d.ts`
and `auth-contract.d.ts` re-export the names of their entries from there, `pylon.d.ts` among them
the class `Repeater` from the declaration of `@repeaterjs/repeater` and the rest of the names of
graphql-yoga an app imports from graphql-yoga by its bare specifier, `index.d.ts` re-exports
`pylon.d.ts`, `worker.d.ts` declares the fetch handler itself, and `sentry.d.ts` declares `init`
of the entry `aosabi/sentry/node` with the keys of its options the engine reads. The declarations
of the compatibility entry import `hono`, `graphql`, `graphql-yoga` and the other packages they
depend on by their bare specifiers: `hono` resolves to the peer, and a project that installs none of the others reads those
types as unresolved, which `skipLibCheck` passes. The text the declarations derive from Pylon is
under the Apache License 2.0 and the text of graphql and `@repeaterjs/repeater` under the MIT licence, whose notices `NOTICE` of
the Aosabi repository carries.

## The module

The file `core.wasm` is the engine built from the crate `aosabi-js` of the Aosabi workspace in the
profile `wasm`, and `core.wasm.sha256` holds its SHA-256. The row `aosabi_core_wasm` of
`tools/provenance/products.lock` of the Aosabi repository anchors the sources it is built from. The
router and the chain
of the port of hono in the module still answer the routes of the plugins of the engine, the viewer,
the landing page and the routes of the login of `zitadelLogin`, and its not found handler, so the
router stays in the module until those routes leave it.

The file `strings.wasm` is the module through which the wrapper hands the short strings of a record
under the compatibility flag `wasm_esm_integration_builtins` of workerd, which
`tools/jsstring/strings-wasm.mjs` of the Aosabi repository writes, and `strings.wasm.sha256` holds
its SHA-256.

## Dependencies

The app installs the peer and owns its version within the range, the hono its lock resolves. Its optional dependencies are the four platform packages above, the project's own
packages. Its wrapper uses the platform's own `WebAssembly`, `TextEncoder`, `queueMicrotask` and
`AsyncLocalStorage` of `node:async_hooks`, which a Worker reaches under the flag `nodejs_compat_v2`,
and on Bun its `bun:ffi`. Under the compatibility flag
`wasm_esm_integration_builtins` of workerd the wrapper hands the strings of a record through the
module `strings.wasm` of the package and the JS String Builtins of V8, and without that flag it
takes `TextEncoder` and `TextDecoder`, so the flag is an option for a Worker and no requirement. The
serving of `aosabi/bun` uses `Bun.serve`. For the Zitadel layer it uses the `fetch` of the platform,
the outbound requests of the engine, and `node:crypto`, `node:buffer`, `node:fs` and `node:path`
with `process.cwd()`, the signer, the digest and the import of a key and the read of a key file
below the working directory, each taken at the run through `process.getBuiltinModule` and never
imported. A Worker gives no `node:fs`, so there no key file is read and the key comes from the
variable `AUTH_KEY`. The envelopes of Sentry
leave through the same `fetch` of the platform, which the execution context of a Worker keeps alive
past the answer through `waitUntil`, as the transport of Toucan hands its fetch to it.

The entry `aosabi/worker` imports the module `cloudflare:workers` of the platform for its export
`env`, the bindings of the Worker in its global scope, whose text bindings the boot at the call of
`worker()` reads beside `process.env`. So the entry loads only where `cloudflare:workers` resolves,
on workerd or from a bundle whose bundler leaves the module to workerd, as Wrangler leaves it, as it
leaves `node:async_hooks`.

## Where it stands

The entry `aosabi/bun` holds no declaration file either, and the declarations of the entries without
one wait for the lead.
