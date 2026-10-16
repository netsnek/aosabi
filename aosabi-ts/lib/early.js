// SPDX-License-Identifier: MIT
/*
 * early.js: the boot of the app of a Worker in the global scope of its
 * isolate, which the entry aosabi/worker runs once the glue of the app has
 * loaded.
 *
 * The glue of a build calls handler() in the global scope of a Worker, and
 * handler() builds the schema at its call.
 */

// Cloudflare starts to load a Worker deployed to a root host name at the
// first packet of the TLS handshake of a cold request, and the evaluation of
// its global scope runs in that load, so a boot of the app there leaves the
// first request a booted engine, within the limit of 1 second on the global
// scope. The boot takes what the global scope holds when the entry calls
// worker(): the record of the routes, the plugins and the configuration the
// glue made, the options of init of aosabi/sentry/node, the IR the host hands
// and the variables of the process with the string bindings of the Worker,
// which the global scope reads through process.env and the bindings that
// cloudflare:workers exports there. So Sentry.init and every other input of
// the boot record must come before that call, as the glue of a build and the
// configuration it imports make them at their load, and the instances of the
// boot keep what it took. The first request holds the variables alone to
// those of the boot: a first request whose variables differ boots the app
// again with its own, as it boots with no early boot.

// The method of an app of the entry aosabi that boots it before its first
// request, lib/index.js.
const EARLY = Symbol.for("aosabi.early");

// Returns the members of `variables` the boot record writes, each string
// member as its name and its value in the order of its object, lib/core.js.
function written(variables) {
  return Object.entries(variables).filter(([, value]) => typeof value === "string");
}

// Returns whether the variables `seen`, which an early boot took, are the
// variables `variables` of a first request, member for member in the same
// order, so both boots write the same record.
export function sameVariables(seen, variables) {
  const left = written(seen);
  const right = written(variables);
  if (left.length !== right.length) return false;
  return left.every(([name, value], index) => right[index][0] === name && right[index][1] === value);
}

// Returns the milliseconds a boot is timed by: performance.now() of the host,
// or Date.now() in the global scope of a Worker, where workerd holds
// performance.now() at 0 while Date.now() goes on.
export function bootClock() {
  return performance.now() || Date.now();
}

// Boots `app`, an app of the entry aosabi whose host handed it its engine,
// with the bindings `env` of the global scope of a Worker. A boot that throws
// leaves the app to boot at its first request, as it boots with no early
// boot, so a Worker whose boot fails starts and answers as before.
export function bootEarly(app, env) {
  try {
    app[EARLY](env);
  } catch {
    // The first request boots the app, as it boots with no early boot.
  }
}
