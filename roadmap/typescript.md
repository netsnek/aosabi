# TypeScript

The TypeScript layer runs an app on Cloudflare Workers and on Bun today. The work here widens where
it runs and gives it an API of its own beside the compatibility layer.

1. The packages `aosabi`, its four platform packages and `aosabi-build` published on npm.
2. Node.js as a host of an app at run time, beside Bun and Cloudflare Workers, and Deno after it.
3. Watch and reload in development, so a change of a TypeScript source serves again without a manual
   build.
4. A core layer in TypeScript beside the compatibility layer: one handler over a schema and its
   resolvers, with `cors` and `csrf` of its own under the names and the options a web developer
   knows, and safe defaults with bounds on every input.
5. A scaffold for each language, so `aosabi new` writes the smallest service in TypeScript as it
   writes one in Rust.
6. A report of `aosabi migrate` for every default that differs from the one the app was written
   against, so a moved app learns each change before it runs.
