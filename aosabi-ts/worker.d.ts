// SPDX-License-Identifier: MIT
/*
 * worker.d.ts: the types of the entry aosabi/worker of the package, the fetch handler of a
 * Cloudflare Worker over an app of the entry aosabi, for lib/worker.js.
 *
 * The declarations are the project's own, of the two exports of lib/worker.js.
 */

import type { App } from './types/pylon/core/index.js'

/** The fetch handler of a Worker, which answers each request on the engine. */
export interface WorkerHandler {
  fetch(request: Request, env?: unknown, ctx?: unknown): Promise<Response>
}

/**
 * The options of `worker`: the IR of the build of the app, its JSON text or the parsed object,
 * which the core boots from, so the boot throws without it, or in its place the schema record of
 * the build that `lib/schema-record.js` of `aosabi-build` writes, which the core boots from with no
 * parse of the SDL of the app, and the job mode and the read mode, `faithful` and `get` when left
 * out.
 */
export interface WorkerOptions {
  ir: string | object
  mode?: 'faithful' | 'fast'
  read?: 'get' | 'project'
}

/**
 * Returns the fetch handler of a Worker over `app`, which hands the app the module `core.wasm` and
 * the IR of `options` and boots it at the call, with the variables of the process and the text
 * bindings the Worker exports to its global scope, so an entry that calls it in its global scope
 * after the glue has loaded boots the app before its first request. The boot takes what the global
 * scope holds at the call, so `Sentry.init` and every other input of the boot come before it. A
 * first request whose variables differ from those of that boot boots the app again with its own.
 */
export declare function worker(app: App, options: WorkerOptions): WorkerHandler

/**
 * The fetch handler of a Worker over the app the glue of `aosabi build` exposes as
 * `globalThis.__AOSABI_APP__` with the IR it exposes as `globalThis.__AOSABI_IR__`, which boots the
 * app at its first request.
 */
declare const handler: WorkerHandler
export default handler
