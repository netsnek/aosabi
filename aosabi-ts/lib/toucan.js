// SPDX-License-Identifier: MIT
/*
 * toucan.js: the client of Sentry of a request of an app of the TypeScript
 * option, the Toucan the middleware of @hono/sentry sets under the key
 * sentry, which c.get('sentry') answers, over the client of aosabi-sentry the
 * engine keeps for the request, and the options of the last Sentry.init of
 * aosabi/sentry/node, which the boot of the engine takes.
 *
 * Derived from toucan-js 4.1.1, ., MIT.
 * Derived from @sentry/core 8.9.2, ., MIT.
 * Derived from @sentry/node 8.55.0, ., MIT.
 */

// The calls of the op SENTRY of the export ctx, crates/aosabi-js/src/sentry.rs.
const ACT = { FORK: 0, SET: 2, MESSAGE: 3, EXCEPTION: 4 };

// The options of the last init, undefined before the first.
let initialized;

// The variables of the process getSentryRelease of @sentry/node 8.55.0 reads
// after SENTRY_RELEASE and the release of sentry-webpack-plugin, in its
// order: those of the git providers, of the providers of CI with a variable
// of their own and of the providers of CI with a generic variable, the first
// that holds a text naming the release.
const RELEASE_VARIABLES = [
  "GITHUB_SHA", "CI_MERGE_REQUEST_SOURCE_BRANCH_SHA", "CI_BUILD_REF", "CI_COMMIT_SHA", "BITBUCKET_COMMIT",
  "APPVEYOR_PULL_REQUEST_HEAD_COMMIT", "APPVEYOR_REPO_COMMIT", "CODEBUILD_RESOLVED_SOURCE_VERSION", "AWS_COMMIT_ID",
  "BUILD_SOURCEVERSION", "GIT_CLONE_COMMIT_HASH", "BUDDY_EXECUTION_REVISION", "BUILDKITE_COMMIT", "CIRCLE_SHA1",
  "CIRRUS_CHANGE_IN_REPO", "CF_REVISION", "CM_COMMIT", "CF_PAGES_COMMIT_SHA", "DRONE_COMMIT_SHA", "FC_GIT_COMMIT_SHA",
  "HEROKU_TEST_RUN_COMMIT_VERSION", "HEROKU_SLUG_COMMIT", "RAILWAY_GIT_COMMIT_SHA", "RENDER_GIT_COMMIT",
  "SEMAPHORE_GIT_SHA", "TRAVIS_PULL_REQUEST_SHA", "VERCEL_GIT_COMMIT_SHA", "VERCEL_GITHUB_COMMIT_SHA",
  "VERCEL_GITLAB_COMMIT_SHA", "VERCEL_BITBUCKET_COMMIT_SHA", "ZEIT_GITHUB_COMMIT_SHA", "ZEIT_GITLAB_COMMIT_SHA",
  "ZEIT_BITBUCKET_COMMIT_SHA", "CI_COMMIT_ID", "SOURCE_COMMIT", "SOURCE_VERSION", "GIT_COMMIT", "COMMIT_REF",
  "BUILD_VCS_NUMBER",
];

// Returns the release getSentryRelease of @sentry/node 8.55.0 reads of the
// variables `env` of the process: SENTRY_RELEASE, then the member id of
// globalThis.SENTRY_RELEASE, which sentry-webpack-plugin writes, then the
// first of RELEASE_VARIABLES, each taken where it is truthy, or undefined.
function sentryRelease(env) {
  if (env.SENTRY_RELEASE) return env.SENTRY_RELEASE;
  const injected = globalThis.SENTRY_RELEASE;
  if (injected && injected.id) return injected.id;
  for (const name of RELEASE_VARIABLES) {
    if (env[name]) return env[name];
  }
  return undefined;
}

// Throws the TypeError of a function under the key `key` of the options of
// init, which the engine runs none of, as useSentry of lib/index.js fails the
// boot for one, so a beforeSend of the app never goes unapplied without a word.
function refuseFunction(key, value, seen) {
  if (typeof value === "function") {
    throw new TypeError("the option " + key + " of init of aosabi/sentry/node holds a function, which aosabi runs "
      + "none of until a fixture passes one, and the options dsn, environment, release and sampleRate cross");
  }
  if (value === null || typeof value !== "object" || seen.has(value)) return;
  seen.add(value);
  for (const member of Object.values(value)) refuseFunction(key, member, seen);
}

// Keeps the options of a call of init of aosabi/sentry/node as init of
// @sentry/node 8.55.0 merges them, build/cjs/sdk/index.js:193-265 there: the
// variables SENTRY_DSN and SENTRY_ENVIRONMENT of the process first, then the
// options of the call, an own member that is undefined among them, so
// init({dsn: undefined}) names no DSN, and the release of the options where
// they name one and the release getSentryRelease reads otherwise. The engine
// takes the DSN, the environment, the release and the sample rate at its
// boot, and an option that is a function or holds one throws.
export function setInit(options = {}) {
  for (const [key, value] of Object.entries(options)) refuseFunction(key, value, new Set());
  const env = globalThis.process?.env ?? {};
  const merged = {};
  if (env.SENTRY_DSN !== undefined) merged.dsn = env.SENTRY_DSN;
  if (env.SENTRY_ENVIRONMENT !== undefined) merged.environment = env.SENTRY_ENVIRONMENT;
  Object.assign(merged, options);
  const release = options.release !== undefined ? options.release : sentryRelease(env);
  if (release !== undefined) merged.release = release;
  initialized = {
    dsn: typeof merged.dsn === "string" ? merged.dsn : undefined,
    environment: typeof merged.environment === "string" ? merged.environment : undefined,
    release: typeof merged.release === "string" ? merged.release : undefined,
    sampleRate: typeof merged.sampleRate === "number" ? merged.sampleRate : undefined,
  };
}

// Returns the options of the last init, or undefined before the first.
export function initOptions() {
  return initialized;
}

// Returns the type and the value of the exception of a captured value: the
// name and the message of an Error, No error message for an empty one, as
// exceptionFromError of Toucan reads them, and the text of any other value
// under the type Error. Toucan writes a plain object as "Non-Error exception
// captured with keys" with the extra __serialized__ on its scope and any
// other value that is no Error as the value itself (dist/index.cjs.js:86-124
// there), a difference section 14 of docs/ts1-surface.md names.
function exceptionOf(exception) {
  const isError = exception instanceof Error;
  const type = isError ? String(exception.name) : "Error";
  const value = isError ? String(exception.message) : String(exception);
  return { type, value: value === "" ? "No error message" : value };
}

// The client of Sentry of a request, or the clone of it withScope hands its
// callback, over the scope `number` the engine keeps, 0 for the scope of the
// request. `call(act, number, write)` calls the op SENTRY of the export ctx
// for the request and answers its value, and `json(out, value)` writes the
// JSON copy of a value, as JSON.stringify reads it. Each setter answers the
// client, as a setter of the scope of @sentry/core does, and each capture the
// id of its event.
export class Toucan {
  #call;
  #number;
  #json;

  constructor(call, json, number = 0) {
    this.#call = call;
    this.#json = json;
    this.#number = number;
  }

  #set(name, args) {
    this.#call(ACT.SET, this.#number, (out) => {
      out.text(name);
      this.#json(out, args);
    });
    return this;
  }

  setUser(user) {
    return this.#set("setUser", [user ?? null]);
  }

  setTags(tags) {
    return this.#set("setTags", [tags]);
  }

  setTag(key, value) {
    return this.#set("setTag", [key, value]);
  }

  setExtras(extras) {
    return this.#set("setExtras", [extras]);
  }

  setExtra(key, extra) {
    return this.#set("setExtra", [key, extra]);
  }

  setLevel(level) {
    return this.#set("setLevel", [level]);
  }

  setFingerprint(fingerprint) {
    return this.#set("setFingerprint", [fingerprint]);
  }

  setContext(key, context) {
    return this.#set("setContext", [key, context ?? null]);
  }

  setTransactionName(name) {
    return this.#set("setTransactionName", [name]);
  }

  // A breadcrumb takes the time of the clock of the host where it names
  // none, as addBreadcrumb of the scope stamps it.
  addBreadcrumb(breadcrumb) {
    return this.#set("addBreadcrumb", [{ timestamp: Date.now() / 1000, ...breadcrumb }]);
  }

  captureException(exception) {
    const { type, value } = exceptionOf(exception);
    return this.#call(ACT.EXCEPTION, this.#number, (out) => {
      out.text(type);
      out.text(value);
    });
  }

  captureMessage(message, level) {
    return this.#call(ACT.MESSAGE, this.#number, (out) => {
      out.text(String(message));
      if (level === undefined) {
        out.byte(0);
      } else {
        out.byte(1);
        out.text(String(level));
      }
    });
  }

  // Hands `callback` a clone of the client with a copy of its scope and
  // answers what the callback answers, withScope of Toucan, which keeps the
  // clone as long as the callback holds it.
  withScope(callback) {
    const number = this.#call(ACT.FORK, this.#number);
    return callback(new Toucan(this.#call, this.#json, number));
  }
}
