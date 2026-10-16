// SPDX-License-Identifier: MIT
/*
 * worker.js: the entry aosabi/worker of the package aosabi, the fetch handler
 * of a Cloudflare Worker over an app on the module core.wasm.
 *
 * The Worker serves the default export of the glue of a build, an app of hono.
 */

// Wrangler bundles this module with the app and uploads core.wasm as a module
// of the Worker, as it uploads the module of the Worker probe of the Aosabi
// repository. When the module loads it hands every app of the isolate the
// module through the class App, so an entry of a Worker that imports this
// module before the glue of its app gives a logger call of the app before its
// boot the engine to write through, so the logger writes at once. The app
// takes the module with the IR of its build when the entry calls worker(),
// after the glue has loaded, and the core boots there, in the global scope of
// the Worker when the entry calls it there, lib/early.js.
import module from "../core.wasm";
import strings from "../strings.wasm";
import * as workers from "cloudflare:workers";
import { STRINGS } from "./core.js";
import { bootEarly } from "./early.js";
import { App } from "./pylon.js";

// The method a host hands the app its engine through, lib/index.js.
const HOST = Symbol.for("aosabi.host");

// The module of the hand over of strings through the JS String Builtins of
// V8 rides on core.wasm, round JSSTRING: workerd compiles it with the builtins
// js-string where the Worker carries the compatibility flag
// wasm_esm_integration_builtins, and lib/core.js takes TextEncoder and
// TextDecoder where it cannot instantiate it.
module[STRINGS] = strings;

App[HOST]({ module });

// Returns the fetch handler of a Worker over `app`, the app of the entry
// aosabi, with the IR of its build `ir`, its JSON text, the parsed object or
// the schema record of the build that lib/schema-record.js of aosabi-build
// writes,
// and the job mode and the read mode `mode` and `read`, faithful and get when
// left out. The call hands the app core.wasm and boots it with the bindings
// the Worker exports to its global scope, and the handler answers each
// request on the engine with the bindings and the execution context of the
// Worker.
export function worker(app, { ir, mode, read } = {}) {
  app[HOST]({ module, ir: typeof ir === "string" ? ir : JSON.stringify(ir), mode, read });
  bootEarly(app, workers.env);
  return {
    fetch(request, env, ctx) {
      return app.fetch(request, env, ctx);
    },
  };
}

// It reads the app at its first request, so the app boots there.
let served = null;

export default {
  fetch(request, env, ctx) {
    if (served === null) {
      const app = globalThis.__AOSABI_APP__;
      if (app === undefined) throw new Error("no glue exposed an app as globalThis.__AOSABI_APP__");
      served = worker(app, { ir: globalThis.__AOSABI_IR__ });
    }
    return served.fetch(request, env, ctx);
  },
};
