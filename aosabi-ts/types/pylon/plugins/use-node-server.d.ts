import { type Plugin } from '../core/index';
export interface NodeServerOptions {
    /** Port to bind. Defaults to `process.env.PORT` then `3000`. */
    port?: number;
}
/**
 * Is this a genuine Node runtime (so `@hono/node-server`'s `node:http` serve applies)?
 *
 * The SAME built artifact + `pylon.config` is meant to run on Node AND on Bun / Deno /
 * workerd — which auto-serve the `export default app` themselves. On those, `useNodeServer`
 * must NO-OP, or it double-binds (a second listener next to the host's) or crashes reaching
 * for `node:http`. Detection is by the tells each runtime sets, not a positive Node check
 * alone: Bun sets `process.versions.bun`, Deno exposes a `Deno` global, workerd reports
 * `navigator.userAgent === 'Cloudflare-Workers'` (and has no real Node `process`).
 */
export declare function isNodeRuntime(): boolean;
/**
 * Serve the Pylon app on Node via `@hono/node-server`.
 *
 * Serving is an EXPLICIT, app-owned capability — not a hidden side effect of importing
 * the built entry. The generated entry is pure (`export default app`); an app opts into
 * Node serving by adding this `'last'`-strategy plugin to its `pylon.config` (put it LAST
 * so the port binds only after every route — incl. the usePages catch-all — is mounted).
 * Bun / workerd / Deno need no plugin: they auto-serve the default-exported app, and this
 * plugin no-ops there (see `isNodeRuntime`).
 *
 * In dev (`NODE_ENV==='development'`, set by `pylon dev`) this also NO-OPs: `pylon dev` owns
 * serving (plainly for an API, or Vite-fronted for usePages), so binding here would
 * double-bind / fight the dev server. `@hono/node-server` is imported lazily so it never
 * enters the static graph traced for non-Node runtimes.
 */
export declare function useNodeServer(options?: NodeServerOptions): Plugin;
