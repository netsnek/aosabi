# The TypeScript layer

A TypeScript app runs on the Aosabi engine through the npm package `aosabi`. The app's own code runs
in the JavaScript engine of its host, Cloudflare Workers or Bun, and the Rust engine of this
repository parses, validates and executes every GraphQL request and writes the answer to it. The API
an app writes against is the compatibility layer `aosabi/pylon`, so an app is written as a Pylon v3
app is written, and an existing Pylon v3 app moves over through `aosabi migrate`, which rewrites its
import specifiers by one table. [pylon.md](pylon.md) lists what the layer covers and where it
differs.

## 1. The packages

| Package | Folder | What it holds |
| --- | --- | --- |
| `aosabi` | `aosabi-ts/` | The engine as `core.wasm`, a wrapper of ES modules, the types |
| `aosabi-linux-x64-gnu` and three more | `aosabi-ts-native/` | The engine as a native library |
| `aosabi-build` | `aosabi-ts-build/` | The commands `aosabi migrate` and `aosabi build` |

The four platform packages, for x64 and arm64 with glibc or musl, each carry the engine as the
native library `libaosabi.so`, and the package `aosabi` names none of them. The dev package
`aosabi-build` depends on `typescript` 5.9.3 alone, pinned exact, runs on Node.js at build time and
never ships with an app.

Aosabi is released through its repository alone, so an app installs the packages from a checkout of
this repository, as the tarballs that `npm pack` makes of their folders or by the paths of the
folders, as section 2 and the example under [examples/](../../examples/) show. A platform package
carries the native library once the checkout builds it with
`cargo build --locked -p aosabi-js --features native --profile release --target <triple>` and copies
it into the folder of the package as `libaosabi.so`, since the library is a build product the
repository does not hold. The entry `aosabi/bun` looks for the platform package in two folders
alone, the folder `node_modules` that holds `aosabi` and the one of the root of the app, as the
[README of aosabi-ts-native](../../aosabi-ts-native/README.md) says.

## 2. Building an app

An app keeps its index at `src/index.ts`. Eight commands take an app from its folder to a running
server, here on an arm64 machine with glibc. The first three run in the checkout: they build the
native library, copy it into the folder of its platform package and make the tarballs of the three
packages. The others run in the folder of the app:

```sh
cargo build --locked -p aosabi-js --features native --profile release \
  --target aarch64-unknown-linux-gnu
cp target/aarch64-unknown-linux-gnu/release/libaosabi_js.so \
  aosabi-ts-native/aosabi-linux-arm64-gnu/libaosabi.so
npm pack ./aosabi-ts ./aosabi-ts-build ./aosabi-ts-native/aosabi-linux-arm64-gnu
npm install --save-dev <checkout>/aosabi-build-<version>.tgz
npx --no aosabi migrate .
npm install <checkout>/aosabi-<version>.tgz <checkout>/aosabi-linux-arm64-gnu-<version>.tgz
npx --no aosabi build
PORT=4000 bun --preload aosabi/bun .aosabi/server.mjs
```

The app installs `aosabi-build` as a dev dependency before its first `npx`, and `aosabi` with its
platform package once `aosabi migrate` names `aosabi` in its `package.json`. Each `npx --no` runs
the command of the installed `aosabi-build` and fails where none is installed.

1. `aosabi migrate` rewrites the import specifiers of the app by one table, swaps the dependency of
   its `package.json` and the build command of its scripts, points the paths of its scripts into the
   build folder at `.aosabi/`, renames its configuration file to `aosabi.config.ts`, `.mts`, `.js`
   or `.mjs` and its declaration file to `aosabi.d.ts`, and logs every rewritten line and every
   renamed file in `.aosabi/migrate.json`. The flag `--reverse` restores every file the log names
   byte for byte and under its name. A new app that imports `aosabi/pylon` from its first line
   needs no migration.
2. `aosabi build` infers the schema of the default export of `src/index.ts` with the TypeScript
   checker itself, transpiles the sources and writes the build into `.aosabi/`: `schema.graphql`,
   `resolvers.js`, `ir.json`, the transpiled sources, `server.mjs` for a server and `worker.mjs` for
   a Worker. A source with a syntax error, a configuration that throws when it loads and a schema
   that forms no valid GraphQL schema fail the build before the glue is written.
3. The last command serves the build on Bun, which section 3 describes.

The schema comes from the types of the app, as the checker of TypeScript reads them, so the app
declares no schema of its own.

## 3. Hosts

| Host | Entry | Engine |
| --- | --- | --- |
| Bun on Linux | `aosabi/bun`, preloaded before `server.mjs` | Native, or `core.wasm` |
| Cloudflare Workers | `worker.mjs`, the main of the Wrangler configuration | `core.wasm` |

On Bun the entry `aosabi/bun` loads the native library of the platform package over `bun:ffi` and
falls back to `core.wasm` where none is installed. The variable `AOSABI_ENGINE` forces the choice,
`wasm` for `core.wasm` and `native` for the library, which then must load. Bun serves the app that
`server.mjs` exports on the port of `PORT`, or on 3000, and an app whose plugins name the serving
plugin `useNodeServer()` is served over `Bun.serve` by that plugin. An app builds and serves with
Bun 1.3.14, the Bun the README of the package names.

On Cloudflare Workers the build's `worker.mjs` is the fetch handler, and the Worker sets the
compatibility flag `nodejs_compat_v2`, which gives the wrapper `AsyncLocalStorage`, so the context
of a request stays readable after an `await`. The platform serves HTTP and compresses the answer at
its edge, so the Worker carries no HTTP server and no encoder.

Node.js runs the dev package at build time and is no host of an app at run time yet, and Deno waits
as well. Both are on the [roadmap](../../roadmap/typescript.md).

## 4. What runs where

- The engine executes no JavaScript and carries no JavaScript runtime. The app's resolvers, routes,
  middleware and plugins run in the JavaScript engine of the host, and the wrapper hands each value
  across the boundary as a handle, so no JavaScript object is copied whole on the path of a request.
- The engine schedules each of its jobs as one microtask in the host's own queue, so the promises of
  the app's resolvers settle in the order graphql-js gives them on the same host.
- TLS, the outbound `fetch` and the crypto are the host's: those of Bun on Bun and those of the
  platform on a Worker. Neither `core.wasm` nor the native library links a TLS or crypto library of
  its own.
- The package `aosabi` holds no telemetry, and neither does the dev package.

## 5. The engine module

The module `core.wasm` is built from the crate [aosabi-js](../../crates/aosabi-js/) with every
default layer of the engine and no app, and its SHA-256 stands beside it in `core.wasm.sha256`. The
same crate builds the native library under its feature `native` for the four Linux targets of the
platform packages. Both export the same functions. The module imports the functions of the host
through one block of safe functions, and the native library takes the same functions as typed
pointers when it boots, as [the security guide](security.md) describes.
