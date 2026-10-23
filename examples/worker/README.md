# Worker

The smallest service in TypeScript, one query in [src/index.ts](src/index.ts), served on the engine
`core.wasm` by Bun and, as a Cloudflare Worker, by workerd. The service is an instance of the class
`App` of the entry `aosabi/pylon`, the compatibility layer, so a Pylon v3 app runs the same way with
its import specifiers changed alone.

From this folder, install the packages `aosabi` and `aosabi-build` of this repository as copies,
with the TypeScript compiler the build needs, then build the service and serve it on Bun:

```sh
npm install --install-links --omit=optional --no-audit
npx aosabi build
PORT=4000 bun --preload aosabi/bun .aosabi/server.mjs
```

The install leaves out the optional packages of the native engine, so Bun runs `core.wasm` as a
Worker does, and the build writes the schema it inferred to `.aosabi/schema.graphql`. Ask the
service from a second terminal:

```sh
curl -s localhost:4000/graphql -H 'content-type: application/json' \
  -d '{"query":"{ hello(name: \"Bun\") }"}'
```

```json
{"data":{"hello":"Hello, Bun"}}
```

Wrangler serves the same build as a Worker on workerd on this machine with
[wrangler.toml](wrangler.toml). It deploys nothing, and `WRANGLER_SEND_METRICS=false` keeps it from
sending usage data. Ask the Worker on port 8787:

```sh
WRANGLER_SEND_METRICS=false npx wrangler@3.114.17 dev
```

```sh
curl -s localhost:8787/graphql -H 'content-type: application/json' \
  -d '{"query":"{ hello(name: \"Worker\") }"}'
```

```json
{"data":{"hello":"Hello, Worker"}}
```
