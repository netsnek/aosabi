// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * canary.js: the names of the root of the canary 3.0.0-canary-20260206115650
 * that the root at 2653a81 does not export, requireAuth,
 * authMiddleware and useAuth, which the entry aosabi exports for an app that
 * the canary builds, and the reader of the type resolvers of the form the
 * generator of the canary writes into the glue of its build.
 *
 * Derived from Pylon 2.0.0-canary-20260206115650 by Nico Schett,
 * @getcronit/pylon-dev 2.0.0-canary-20260206115650, Apache-2.0.
 * Changed by the Aosabi authors.
 */

// The canary keeps the auth state of a request in the context under the key
// auth, which a plugin of the app sets and requireAuth and authMiddleware read,
// the user and its roles among them.
import { deleteCookie, getCookie } from "hono/cookie";
import { HTTPException } from "hono/http-exception";
import { ServiceError } from "./errors.js";
import { createDecorator, getContext } from "./pylon.js";

// The waiting the steps of the OIDC path of useAuth name.
const OIDC = "the OIDC path of useAuth of the canary, the introspection of a token of the header " +
  "Authorization, of the cookie aosabi-auth or of the query parameter token and the routes of the login and of " +
  "the callback, waits for the connection of round TS1T over the OIDC client of the engine, which the lead takes " +
  "after item T9 of TS1";

// Returns the runtime of the host, getRuntimeKey of hono at
// dist/helper/adapter/index.js, which the entry aosabi/pylon reads as well,
// read at the first call and kept, since the globals it reads do not change
// while the isolate runs.
let runtime = null;
function runtimeKey() {
  return (runtime ??= readRuntimeKey());
}
function readRuntimeKey() {
  const known = { deno: "Deno", bun: "Bun", workerd: "Cloudflare-Workers", node: "Node.js" };
  if (typeof navigator !== "undefined" && typeof navigator.userAgent === "string") {
    for (const [key, agent] of Object.entries(known)) {
      if (navigator.userAgent.startsWith(agent)) return key;
    }
  }
  if (typeof globalThis?.EdgeRuntime === "string") return "edge-light";
  if (globalThis?.fastly !== undefined) return "fastly";
  if (globalThis?.process?.release?.name === "node") return "node";
  return "other";
}

// Returns the environment of the runtime for the context `c`, env of hono's
// adapter at the same file, lines 2-19, which authMiddleware of the canary
// reads AUTH_PROJECT_ID from: the process on Node and Bun and the bindings of
// the context on workerd.
function adapterEnv(c) {
  const globalEnv = globalThis?.process?.env;
  switch (runtimeKey()) {
    case "bun":
    case "node":
    case "edge-light":
      return globalEnv;
    case "deno":
      return Deno.env.toObject();
    case "workerd":
      return c.env;
    default:
      return {};
  }
}

// Returns the middleware that checks the auth state of the context against
// `checks`, authMiddleware of the canary's dist/index.js:659-690: no state
// and a state with no user throw an HTTPException of hono of the status 401
// and the message "Authentication required", and a user whose roles hold
// none of `checks.roles`, by its name or by the name after the project
// AUTH_PROJECT_ID of the environment and a colon, throws one of the status
// 403 over a response "Forbidden" whose fields Missing-Roles and
// Obtained-Roles list both. The canary checks the roles only where a user is
// set, so a state with no user passes, and the refusal of a state with no
// user is divergence D50 of docs/byte-equality.md of the Aosabi repository,
// which closes flaw P1 of Appendix A of the security review.
export const authMiddleware = (checks = {}) => {
  const middleware = async (ctx, next) => {
    const AUTH_PROJECT_ID = adapterEnv(ctx).AUTH_PROJECT_ID;
    const auth = ctx.get("auth");
    if (!auth) {
      throw new HTTPException(401, { message: "Authentication required" });
    }
    // No user is refused like no auth state, divergence D50 (P1).
    if (!auth.user) {
      throw new HTTPException(401, { message: "Authentication required" });
    }
    if (checks.roles) {
      const roles = auth.user.roles;
      const hasRole = checks.roles.some((role) => roles.includes(role) || roles.includes(`${AUTH_PROJECT_ID}:${role}`));
      if (!hasRole) {
        const resError = new Response("Forbidden", {
          status: 403,
          statusText: "Forbidden",
          headers: { "Missing-Roles": checks.roles.join(","), "Obtained-Roles": roles.join(",") },
        });
        throw new HTTPException(resError.status, { res: resError });
      }
    }
    return next();
  };
  return middleware;
};

// Returns the decorator that runs authMiddleware of `checks` over the context
// of the running request before the function or the member it decorates,
// requireAuth of the canary's dist/index.js:617-651: the HTTPException of the
// status 401 becomes a ServiceError of its message with the code
// AUTH_REQUIRED and the status 401, and the one of the status 403 a
// ServiceError of the status text of the response of getResponse(), which
// that response leaves empty, with the code AUTHORIZATION_REQUIRED, its
// status and the roles the fields list as details.
export function requireAuth(checks) {
  const checkAuth = async (c) => {
    const ctx = await c;
    try {
      await authMiddleware(checks)(ctx, async () => {});
    } catch (e) {
      if (e instanceof HTTPException) {
        if (e.status === 401) {
          throw new ServiceError(e.message, { statusCode: 401, code: "AUTH_REQUIRED" });
        } else if (e.status === 403) {
          const res = e.getResponse();
          throw new ServiceError(res.statusText, {
            statusCode: res.status,
            code: "AUTHORIZATION_REQUIRED",
            details: {
              missingRoles: res.headers.get("Missing-Roles")?.split(","),
              obtainedRoles: res.headers.get("Obtained-Roles")?.split(","),
            },
          });
        } else {
          throw e;
        }
      }
      throw e;
    }
  };
  return createDecorator(async () => {
    const ctx = getContext();
    await checkAuth(ctx);
  });
}

// Returns the auth state useAuth of the canary sets for a request with no
// token, whose member openidConfig, the configuration of the identity
// provider that its discovery made, throws the error of the OIDC path.
function stateWithoutToken() {
  const state = {};
  Object.defineProperty(state, "openidConfig", {
    get() {
      throw new Error(OIDC);
    },
    enumerable: true,
    configurable: true,
  });
  return state;
}

// Returns the plugin useAuth of the canary's dist/index.js:372-506 for the
// options `args`: its middleware sets the auth state of a request with no token
// and goes on, and throws the error of the OIDC path for a request with a token
// in the header Authorization, the cookie aosabi-auth or the query parameter
// token, which the canary introspects, and its setup mounts the routes of the
// login, the logout and the callback under `endpoint`, /auth by default, where
// the logout deletes the cookie aosabi-auth and redirects to /, as the canary's
// does, and the two others throw the error of the OIDC path. The middleware
// calls next where the canary's returns nothing, which the loader of the canary
// takes as going on and the chain of the plugins of the base pipeline as the
// end of the chain.
export function useAuth(args) {
  const { endpoint = "/auth" } = args;
  const loginPath = `${endpoint}/login`;
  const logoutPath = `${endpoint}/logout`;
  const callbackPath = `${endpoint}/callback`;
  return {
    middleware: async (ctx, next) => {
      const authCookieToken = getCookie(ctx, "aosabi-auth");
      const authHeader = ctx.req.header("Authorization");
      const authQueryToken = ctx.req.query("token");
      if (authCookieToken || authHeader || authQueryToken) {
        throw new Error(OIDC);
      }
      ctx.set("auth", stateWithoutToken());
      return next();
    },
    setup(app) {
      app.get(loginPath, async () => {
        throw new Error(OIDC);
      });
      app.get(logoutPath, async (ctx) => {
        deleteCookie(ctx, "aosabi-auth");
        return ctx.redirect("/");
      });
      app.get(callbackPath, async () => {
        throw new Error(OIDC);
      });
    },
  };
}

// The tokens of the text of a type resolver: names, strings and the
// punctuators the form of the canary is written in.
const NAME = /[A-Za-z_$][\w$]*/y;
const PUNCTUATORS = ["===", "==", "&&", "(", ")", "{", "}", ";"];

// Returns the tokens of `text`, each a name, a string or a punctuator, with
// its whitespace and its comments left out, or null for a text that holds
// any other token.
function tokensOf(text) {
  const tokens = [];
  let at = 0;
  while (at < text.length) {
    const char = text[at];
    if (char === " " || char === "\t" || char === "\n" || char === "\r") {
      at += 1;
    } else if (text.startsWith("//", at)) {
      const end = text.indexOf("\n", at);
      at = end < 0 ? text.length : end + 1;
    } else if (text.startsWith("/*", at)) {
      const end = text.indexOf("*/", at + 2);
      if (end < 0) return null;
      at = end + 2;
    } else if (char === '"' || char === "'") {
      const end = text.indexOf(char, at + 1);
      if (end < 0) return null;
      const value = text.slice(at + 1, end);
      if (value.includes("\\") || value.includes("\n")) return null;
      tokens.push({ kind: "string", value });
      at = end + 1;
    } else {
      NAME.lastIndex = at;
      const name = NAME.exec(text);
      if (name !== null) {
        tokens.push({ kind: "name", value: name[0] });
        at += name[0].length;
        continue;
      }
      const punctuator = PUNCTUATORS.find((candidate) => text.startsWith(candidate, at));
      if (punctuator === undefined) return null;
      tokens.push({ kind: "punctuator", value: punctuator });
      at += punctuator.length;
    }
  }
  return tokens;
}

// Returns the checks of the type resolver whose text is `text` where it has the
// form the generator of the canary writes,
// `function resolveType(node) { if (node && typeof node === 'object') { if ("a" in node && "b" in node) {return 'A'}; ... } }`,
// each member in the order of its check with the fields the check tests with in,
// and null for any other form, the form of the reference among them, which reads
// __typename. The reader takes the text as a bundler prints it as well, with
// either quote, its own breaks of lines and a name of its own for the function.
export function canaryChecks(text) {
  const tokens = tokensOf(text);
  if (tokens === null) return null;
  let at = 0;
  const take = (kind, value) => {
    const token = tokens[at];
    if (token === undefined || token.kind !== kind || (value !== undefined && token.value !== value)) return null;
    at += 1;
    return token;
  };
  const skipEmpty = () => {
    while (take("punctuator", ";") !== null);
  };
  if (take("name", "function") === null) return null;
  if (tokens[at]?.kind === "name") at += 1;
  if (take("punctuator", "(") === null) return null;
  const node = take("name");
  if (node === null || take("punctuator", ")") === null || take("punctuator", "{") === null) return null;
  const guard = take("name", "if") !== null && take("punctuator", "(") !== null && take("name", node.value) !== null &&
    take("punctuator", "&&") !== null && take("name", "typeof") !== null && take("name", node.value) !== null &&
    (take("punctuator", "===") !== null || take("punctuator", "==") !== null) && take("string", "object") !== null &&
    take("punctuator", ")") !== null;
  if (!guard) return null;
  const braced = take("punctuator", "{") !== null;
  const checks = [];
  for (;;) {
    skipEmpty();
    if (take("name", "if") === null) break;
    if (take("punctuator", "(") === null) return null;
    const fields = [];
    for (;;) {
      const field = take("string");
      if (field === null || take("name", "in") === null || take("name", node.value) === null) return null;
      fields.push(field.value);
      if (take("punctuator", "&&") === null) break;
    }
    if (take("punctuator", ")") === null) return null;
    const block = take("punctuator", "{") !== null;
    if (take("name", "return") === null) return null;
    const member = take("string");
    if (member === null) return null;
    take("punctuator", ";");
    if (block && take("punctuator", "}") === null) return null;
    checks.push({ name: member.value, fields });
    if (!braced) break;
  }
  skipEmpty();
  if (braced && take("punctuator", "}") === null) return null;
  skipEmpty();
  if (take("punctuator", "}") === null) return null;
  return at === tokens.length && checks.length > 0 ? checks : null;
}

// Returns the type resolvers of the form of the canary among `resolvers`, the
// resolvers the glue of a build hands handler(), each the name of its union or
// interface and its checks, in the order of the keys of `resolvers`. A build
// of the reference holds none, so the engine resolves each union and
// interface by the rules of the IR of the build.
export function typeResolversOf(resolvers) {
  const types = [];
  if (resolvers === null || typeof resolvers !== "object") return types;
  for (const type of Object.keys(resolvers)) {
    const resolveType = resolvers[type]?.__resolveType;
    if (typeof resolveType !== "function") continue;
    const checks = canaryChecks(Function.prototype.toString.call(resolveType));
    if (checks !== null) types.push({ type, checks });
  }
  return types;
}
