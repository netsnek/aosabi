// SPDX-License-Identifier: MIT
/*
 * sentry.js: the entry aosabi/sentry/node of the package aosabi, in place of
 * @sentry/node for an app of the TypeScript option.
 *
 * The entry holds the members of @sentry/node 8.55.0 the apps call,
 * init, whose client the hub of aosabi-sentry in the engine holds, so the
 * plugin useSentry of the entry aosabi sends the errors of an execution
 * through it as the plugin of the reference sends them through the hub of
 * @sentry/node. An event of the engine leaves through the fetch of the host,
 * on a Worker through the import block of the module.
 */
import { setInit } from "./toucan.js";

// Initializes the client of Sentry of the app with `options`, init of
// @sentry/node, whose DSN, environment, release and sample rate the engine
// takes at the boot of the app: on a Worker when the entry calls worker() of
// aosabi/worker, in its global scope where the entry calls it there, and on
// Bun at the first request. So the call must come before that boot: a call
// at the top of the configuration of the app, as the apps of the reference
// call it, which the glue runs before the entry calls worker(), makes the
// client of every instance of the engine the app boots, and the instances of
// a boot keep the options it took. An option that is a function or holds
// one, a beforeSend among them, throws, since the engine runs none of them,
// and every other option takes no effect.
export function init(options = {}) {
  setInit(options);
}
