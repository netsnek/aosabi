// SPDX-License-Identifier: MIT
/*
 * bun.js: the entry aosabi/bun of the package aosabi, the host of an app on
 * Bun, the container host of the TypeScript option, over the native library
 * of the engine through bun:ffi or core.wasm.
 *
 * Bun serves the app as the default export of the glue of a build, while the
 * serving plugin useNodeServer binds on Node alone.
 */

// The library is the crate aosabi-js built as a cdylib from the code of
// core.wasm, with the same exports, and its boot takes the fifteen functions
// of the host as typed function pointers in place of the import block of the
// module. Each function is the one core.js hands the module as an import,
// wrapped as a JSCallback of bun:ffi, so the bridge of core.js runs unchanged
// over both engines. The three conditions of the import block hold for the
// callbacks: every function of core.js runs the JavaScript of the app inside a
// try and answers a throw as a tagged record, and the callback catches what
// would still escape, since a JSCallback that throws hands the library a value
// of no meaning in place of an answer. A panic of the library ends the process
// under the panic strategy abort of its profile, so the container restarts
// where a wasm instance is replaced. And the host writes only into the range
// alloc has just answered, through a view of toArrayBuffer made for that one
// write, and keeps no address and no view across calls.
//
// The library holds one engine per thread, its state in the thread locals
// of the crate, so a thread boots it once and a second core of the same
// thread takes core.wasm. The JSCallbacks stay alive as long as the thread,
// since the library may call them at any later call.
import { dlopen, JSCallback, read, toArrayBuffer } from "bun:ffi";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { wasmEngine } from "./core.js";
import { App } from "./pylon.js";

// The environment variable that picks the engine: wasm forces core.wasm for
// the comparison, native asks for the native library and fails where none
// loads, and an unset or empty variable takes the native library where one
// loads and core.wasm otherwise.
export const ENGINE_VARIABLE = "AOSABI_ENGINE";

// The file of the library in a platform package.
export const LIBRARY = "libaosabi.so";

// The architectures of Bun with a platform package, by process.arch.
const ARCHES = new Set(["x64", "arm64"]);

// The symbols of the library and their types, the export lines of
// crates/aosabi-js/src/abi.rs: an address is a pointer, null where the
// library answers 0, and every other number a u32.
const SYMBOLS = {
  alloc: { args: ["u32"], returns: "ptr" },
  dealloc: { args: [], returns: "void" },
  boot: {
    args: ["u32", "u32", "function", "function", "function", "function", "function", "function", "function",
      "function", "function", "function", "function", "function", "function", "function", "function"],
    returns: "ptr",
  },
  fetch: { args: ["u32", "u32"], returns: "ptr" },
  resume: { args: ["u32"], returns: "void" },
  settle: { args: ["u32", "u32", "u32", "u32"], returns: "void" },
  run_job: { args: ["u32"], returns: "void" },
  release: { args: ["u32"], returns: "void" },
  ctx: { args: ["u32", "u32"], returns: "ptr" },
  logger: { args: ["u32"], returns: "ptr" },
  wake: { args: ["u32"], returns: "void" },
  publish: { args: ["u32"], returns: "void" },
};

// The type of js_project, js_get, js_call, js_error and iter_next: the
// table, the handle, the address and the length of a record, and the
// request.
const RECORD = { args: ["u32", "u32", "ptr", "u32", "u32"], returns: "u32" };

// The fifteen functions of the host in the order the boot takes them, the
// order of the import block of the module, each with its type.
const FUNCTIONS = [
  ["js_project", RECORD, [2]],
  ["js_get", RECORD, [2]],
  ["js_call", RECORD, [2]],
  ["js_error", RECORD, [2]],
  ["iter_next", RECORD, [2]],
  ["host_enqueue", { args: ["u32"], returns: "void" }, []],
  ["host_done", { args: ["u32", "ptr"], returns: "void" }, [1]],
  ["host_write", { args: ["u32", "ptr", "u32"], returns: "void" }, [1]],
  ["js_next", { args: ["u32", "u32", "ptr", "u32"], returns: "void" }, [2]],
  ["host_random", { args: ["u32"], returns: "u32" }, []],
  ["host_push", { args: ["u32", "u32", "ptr", "u32"], returns: "void" }, [2]],
  ["host_now", { args: [], returns: "f64" }, []],
  ["host_timer", { args: ["u32", "u32"], returns: "void" }, []],
  ["host_fetch", { args: ["u32", "ptr", "u32"], returns: "u32" }, [1]],
  ["host_crypto", { args: ["ptr", "u32"], returns: "u32" }, [0]],
];

// Whether this thread has booted the native library, and its callbacks.
let booted = false;
const kept = [];

// Returns the C library of the running process, musl where its maps name
// the dynamic linker of musl and glibc otherwise.
function libc() {
  try {
    return readFileSync("/proc/self/maps", "utf8").includes("/ld-musl-") ? "musl" : "gnu";
  } catch {
    return "gnu";
  }
}

// Returns the name of the platform package of this process, or null with
// the reason where the platform has none.
export function platformPackage() {
  if (process.platform !== "linux") {
    return { name: null, reason: "the native engine is built for Linux alone, not " + process.platform };
  }
  if (!ARCHES.has(process.arch)) {
    return { name: null, reason: "no platform package for the architecture " + process.arch };
  }
  return { name: "aosabi-linux-" + process.arch + "-" + libc(), reason: null };
}

// Returns the path of the library of the platform package of this process as
// the package aosabi resolves it, or null with the reason where it is not
// installed.
export function nativeLibrary() {
  const { name, reason } = platformPackage();
  if (name === null) return { path: null, reason };
  try {
    return { path: fileURLToPath(import.meta.resolve(name + "/" + LIBRARY)), reason: null };
  } catch {
    return { path: null, reason: "the platform package " + name + " is not installed" };
  }
}

// Wraps the function `fn` of the host as the body of a JSCallback, which
// turns a null pointer at an index of `pointers` into the address 0 and
// throws nothing: a throw answers 0 for a function that answers a length,
// a record of no length, and nothing for one that answers none.
function callback(fn, type, pointers) {
  const nulls = (args) => {
    for (const index of pointers) args[index] ??= 0;
    return args;
  };
  if (type.returns === "void") {
    return (...args) => {
      try {
        fn(...nulls(args));
      } catch {
        // The function of core.js catches every throw of the app, so this
        // throw is the wrapper's own, which reaches the library as nothing.
      }
    };
  }
  return (...args) => {
    try {
      return fn(...nulls(args));
    } catch {
      return 0;
    }
  };
}

// Returns the engine of the library at `path`, opened through dlopen of
// bun:ffi. Its open boots the library with the functions of the host and
// answers the exports of the library and the reads and the write of the
// memory of the process at the addresses the library hands out.
function nativeEngine(path, library) {
  return {
    kind: "native",
    path,
    reason: null,
    open(host) {
      if (booted) throw new Error("the native library of the engine boots once per thread");
      booted = true;
      const pointers = FUNCTIONS.map(([name, type, nullable]) => {
        const made = new JSCallback(callback(host[name], type, nullable), type);
        kept.push(made);
        return made.ptr;
      });
      const native = library.symbols;
      const exports = {
        alloc: (len) => native.alloc(len) ?? 0,
        dealloc: () => native.dealloc(),
        boot: (len, graphql) => native.boot(len, graphql, ...pointers) ?? 0,
        fetch: (len, request) => native.fetch(len, request) ?? 0,
        resume: (request) => native.resume(request),
        settle: (request, handle, ok, len) => native.settle(request, handle, ok, len),
        run_job: (request) => native.run_job(request),
        release: (request) => native.release(request),
        ctx: (request, len) => native.ctx(request, len) ?? 0,
        logger: (len) => native.logger(len) ?? 0,
        wake: (request) => native.wake(request),
        publish: (len) => native.publish(len),
      };
      return {
        exports,
        write: (at, bytes) => new Uint8Array(toArrayBuffer(at, 0, bytes.length)).set(bytes),
        copy: (at, length) => new Uint8Array(toArrayBuffer(at, 0, length)).slice(),
        word: (at) => read.u32(at, 0),
      };
    },
  };
}

// Returns the engine of core.wasm, the compiled module `module` or the file
// core.wasm of the package beside lib/, with the reason the native library
// was not taken.
function moduleEngine(module, reason) {
  const compiled = module ?? new WebAssembly.Module(readFileSync(new URL("../core.wasm", import.meta.url)));
  return { ...wasmEngine(compiled), reason };
}

// Returns the engine an app runs on in this thread of Bun, which createCore
// of core.js takes as its engine. The variable `choice`, ENGINE_VARIABLE of
// the environment when left out, picks it: wasm forces core.wasm, native the
// native library, which must load, and an unset or empty one the native
// library where the platform package of the process is installed and loads,
// and core.wasm otherwise, whose engine names the reason in `reason`.
// `module` is the compiled core.wasm, the file of the package when left out.
export function engineFor({ module, choice = process.env[ENGINE_VARIABLE] } = {}) {
  const wanted = choice ?? "";
  if (!["", "native", "wasm"].includes(wanted)) {
    throw new Error(ENGINE_VARIABLE + " takes native or wasm, not " + wanted);
  }
  if (wanted === "wasm") return moduleEngine(module, ENGINE_VARIABLE + " forces core.wasm");
  let reason;
  if (booted) {
    reason = "the native library of this thread serves another core";
  } else {
    const found = nativeLibrary();
    reason = found.reason;
    if (found.path !== null) {
      try {
        return nativeEngine(found.path, dlopen(found.path, SYMBOLS));
      } catch (error) {
        reason = "the library " + found.path + " does not load: " + String(error?.message ?? error);
      }
    }
  }
  if (wanted === "native") throw new Error("no native engine: " + reason);
  return moduleEngine(module, reason);
}

// The method a host hands the app its engine through, lib/index.js.
const HOST = Symbol.for("aosabi.host");

// The port useNodeServer binds where neither its options nor PORT name one,
// use-node-server.ts:52.
const DEFAULT_PORT = 3000;

// The seconds a connection of a server of this entry may stay silent before
// Bun closes it, its idleTimeout, above the 12 s between two pings of a stream
// of events with room for a late tick, so a stream outlives its silences as on
// Node, while the default of Bun.serve closes one after 10 s.
const IDLE_TIMEOUT_S = 30;

// The engine of this thread, made by the first app or the first logger call
// that needs one, and what the host hands every app beside it: the JSON text
// of the IR of the build and the two modes, which host() sets.
let threadEngine = null;
let handed = {};

// The servers of the apps this entry serves, by the app.
const servers = new WeakMap();

// Returns the engine of this thread, the one engineFor answers, made once.
function engineOfThread() {
  threadEngine ??= engineFor();
  return threadEngine;
}

// Returns the JSON text of the IR `ir`, its text or the parsed object, or
// undefined for none.
function irText(ir) {
  if (ir === undefined || ir === null) return undefined;
  return typeof ir === "string" ? ir : JSON.stringify(ir);
}

// The host this entry hands the class App, which every app of the process
// takes that no host handed an engine of its own: the engine of this thread,
// made at its first read, the IR host() handed or the one the glue of aosabi
// build exposes as globalThis.__AOSABI_IR__, the two modes, and the serving
// of the plugin useNodeServer, serveNode.
const processHost = {
  get engine() {
    return engineOfThread();
  },
  get ir() {
    return handed.ir ?? irText(globalThis.__AOSABI_IR__);
  },
  get mode() {
    return handed.mode;
  },
  get read() {
    return handed.read;
  },
  serve: (app, options) => serveNode(app, options),
};

App[HOST](processHost);

// Hands every app of the process the JSON text of the IR of its build `ir`,
// its text or the parsed object, and the job mode and the read mode `mode`
// and `read`, faithful and get when left out, beside the engine of this
// thread, before the glue of the app runs, as a host of the judge mode hands
// the IR the sidecar of the harness answers.
export function host({ ir, mode, read } = {}) {
  handed = { ir: irText(ir), mode, read };
}

// Serves `app`, the app of the entry aosabi, over Bun.serve with the options
// `options` of Bun.serve, beside the IR `ir` of its build and the modes `mode`
// and `read`, and the engine `engine`, the engine of this thread when left
// out, and returns the server, serve of @hono/node-server over Bun.serve. The
// fetch of the server is the fetch of the app.
export function serve(app, { ir, mode, read, engine, ...options } = {}) {
  app[HOST]({
    engine: engine ?? engineOfThread(),
    ir: irText(ir) ?? processHost.ir,
    mode: mode ?? handed.mode,
    read: read ?? handed.read,
  });
  return listen(app, { ...options, fetch: app.fetch });
}

// Returns the server this entry serves `app` on, or undefined for none.
export function served(app) {
  return servers.get(app);
}

// Returns the engine this entry hands every app of the process, the one
// engineFor answers, made at the first call, with its kind and the reason the
// native library was not taken.
export function hostedEngine() {
  return engineOfThread();
}

// Serves `app` over Bun.serve with `options` and returns the server, with the
// idleTimeout of IDLE_TIMEOUT_S where the options name none. The app takes the
// member stop of its server, so Bun, which serves the default export of an
// entry that holds a fetch and no stop, serves the app of a glue it runs as
// its entry no second time.
function listen(app, options) {
  const server = Bun.serve({ idleTimeout: IDLE_TIMEOUT_S, ...options });
  servers.set(app, server);
  Object.defineProperty(app, "stop", {
    value: (closeActiveConnections) => server.stop(closeActiveConnections),
    configurable: true,
    writable: true,
  });
  return server;
}

// Serves `app` as the plugin useNodeServer of use-node-server.ts:42-69 serves
// an app on Node through @hono/node-server, with Bun.serve in place of
// node:http: on the port of `options`, of the variable PORT where Number reads
// a port of it, or 3000, as the rule of the plugin reads them, and once the
// server listens it writes the line
// `Aosabi running at http://localhost:<port>` through the console,
// divergence D53 of docs/byte-equality.md section 3. The pass of executeConfig calls it in the role web alone and
// never under NODE_ENV=development, lib/index.js. The server binds every
// address of IPv6 and of IPv4 where the system gives IPv6, the default of
// Bun.serve, as node:http binds.
function serveNode(app, options = {}) {
  const port = options.port ?? (Number(globalThis.process?.env?.PORT) || DEFAULT_PORT);
  const server = listen(app, { port, fetch: app.fetch });
  console.log(`Aosabi running at http://localhost:${server.port}`);
  return server;
}
