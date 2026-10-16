// SPDX-License-Identifier: MIT
/*
 * pylon.js: the entry aosabi/pylon of the package aosabi, in place of
 * @getcronit/pylon for an app of the TypeScript option, an app of the class
 * App over the real hono, the peer of the package.
 *
 * The entry holds the names of vendor/pylon/packages/pylon/src/core/index.ts
 * whose engine item exists, each the shape of Pylon's under Aosabi's own names:
 * the class App and the app of
 * vendor/pylon/packages/pylon/src/app/index.ts:27-424, the class Pylon there, a
 * Hono with Pylon's base pipeline in hono's own code, executeConfig, handler
 * and currentRole of src/app/pylon-handler.ts:82-188 and :190-386, getContext
 * and setContext of src/core/context.ts:30-44, getEnv of
 * src/core/get-env.ts:3-24, getResolveInfo of src/core/resolve-info.ts:16-23,
 * getLocale and getInContext of src/core/in-context.ts:80-95, createDecorator
 * of src/core/create-decorator.ts:1-59, mutation of src/core/mutation.ts:62-80,
 * ServiceError of src/core/define-pylon.ts:488-506, the logger calls of
 * src/core/logger.ts:192-296, the serving plugin useNodeServer of
 * src/plugins/use-node-server.ts:42 and the plugin useSentry of
 * src/plugins/use-sentry.ts:96-276 as markers over the engine, defineConfig,
 * the cookie helpers Pylon re-exports from hono/cookie, and
 * experimentalCreatePubSub, which Pylon re-exports from graphql-yoga, beside
 * Repeater, createGraphQLError and createPubSub, the names of graphql-yoga an
 * app imports beside Pylon, and createGateway, pass and passthrough of
 * src/core/gateway.ts:519-1018 as markers over the federation of the engine,
 * lib/federation.js. Beside them stand requireAuth, authMiddleware and useAuth
 * of the root of Pylon's canary 3.0.0-canary-20260206115650, lib/canary.js. The
 * root aosabi re-exports this entry.
 */

// The app is a Hono of the real hono, the peer of the package, so its routes,
// its middleware, the context c of a request, HTTPException and the helpers of
// hono run in hono's own code, and the boot of Pylon installs Pylon's base
// pipeline on it there: compress(), the bind of the async context, the
// structured request logger behind except of the internal prefix /__aosabi/*,
// the dispatch of the middleware of the plugins and the error answer, then the
// children at their base paths, the routes the setups of the plugins register
// and the GraphQL handler of the glue. The engine in core.wasm, or the native
// library of the engine on Bun, answers the GraphQL endpoint and the execute
// hooks of the plugins, and the routes of the plugins of the engine: the viewer
// where the configuration asks for it with viewer true, the landing page with
// landingPage true and the login of Zitadel. A host hands the app its engine,
// aosabi/worker on a Worker and aosabi/bun on Bun, and the core boots at the
// first request, or where aosabi/worker boots it in the global scope of a
// Worker before that request (lib/early.js), from the SDL, the graphql object
// and the type resolvers the glue of Pylon's build hands handler(), the
// configuration of executeConfig and the IR the host hands.
//
// The context c of a request is the c of hono, which getContext() answers
// through the store the bind of the async context puts in place, and the
// engine reads its own variables of the request, the bag of @inContext and
// the info a root resolver stores under graphqlResolveInfo, through c.get and
// c.var for a key the app did not set, as Pylon's GraphQL layer sets them on
// the same c (lib/core.js, the variables of the core).
import { Hono } from "hono";
import { except } from "hono/combine";
import { compress } from "hono/compress";
import { HTTPException } from "hono/http-exception";
import {
  bodyOf, bridge, context, createCore, LOG, loopTurned, OTHER_HOOKS, primeCore, record, TO, writeJson, writeLoggerOption,
} from "./core.js";
import { ServiceError } from "./errors.js";
import { typeResolversOf } from "./canary.js";
import { bootClock, sameVariables } from "./early.js";

export { ServiceError };
export { deleteCookie, getCookie, getSignedCookie, setCookie, setSignedCookie } from "hono/cookie";
export { authMiddleware, requireAuth, useAuth } from "./canary.js";

export { createGateway, pass, passthrough } from "./federation.js";

// Pylon exports createPubSub of graphql-yoga as experimentalCreatePubSub, whose
// topics live in the pub sub of the engine here, aosabi-ts/lib/pubsub.js.
export { createPubSub as experimentalCreatePubSub } from "./pubsub.js";

// The names of graphql-yoga 5.18.0 an app imports beside Pylon, Repeater of
// @repeaterjs/repeater 3.0.6, which graphql-yoga exports through
// @graphql-yoga/subscription 5.0.5, createGraphQLError of @graphql-tools/utils
// 10.11.0 and createPubSub of @graphql-yoga/subscription, where aosabi migrate
// points the specifier graphql-yoga, the root, which re-exports this entry.
export { Repeater } from "./repeater.js";
export { createGraphQLError } from "./errors.js";
export { createPubSub } from "./pubsub.js";

// The method a host hands the app its engine through, aosabi/worker and
// aosabi/bun, which takes the compiled module or the engine, the JSON text of
// the IR of the build or its schema record and the two modes. The class App
// holds it as well, through which a host hands every app of the process its
// engine before the glue of the app runs.
const HOST = Symbol.for("aosabi.host");

// The engine a host handed every app of the process through the class App,
// before the glue imports the app.
let hosted;

// The instance of the engine whose logger primeCore booted before the first
// request of the app, which the first app to boot takes, and the option
// `logger` of the pass of executeConfig that runs, which applies to the
// logger of that instance.
let primed = null;
let running = null;

// The method that answers the core of the app, made at its first call, which
// a host reads its instances, its engine and the time of its boot from.
const CORE = Symbol.for("aosabi.core");

// The method that boots the app before its first request, through which
// aosabi/worker boots it in the global scope of a Worker, lib/early.js.
const EARLY = Symbol.for("aosabi.early");

// The member that keeps the options the glue hands the GraphQL handler,
// handler(), on the app it mounts the handler on.
const HANDLER = Symbol("aosabi.handler");

// The method that sets the middleware of Sentry of Pylon's canary first on
// the default app, which the first pass of the glue of a build of the canary
// asks for.
const CANARY = Symbol("aosabi.canary");

// The roots of a graphql fragment, GRAPHQL_KINDS of app/index.ts:50.
const GRAPHQL_KINDS = ["Query", "Mutation", "Subscription"];

// Merges the roots of the fragment `from` into `into`, a member of both taking
// the value of `from` and the place of `into`, mergeFragment of
// app/index.ts:52-59.
function mergeFragment(into, from) {
  if (!from) return;
  for (const kind of GRAPHQL_KINDS) {
    const map = from[kind];
    if (!map) continue;
    into[kind] = { ...into[kind], ...map };
  }
}

// Returns the roots of `resolvers` with every member awaiting `gate` first,
// gateResolvers of app/index.ts:86-102.
function gateResolvers(resolvers, gate) {
  const out = {};
  for (const kind of GRAPHQL_KINDS) {
    const map = resolvers[kind];
    if (!map) continue;
    const wrapped = {};
    for (const key of Object.keys(map)) {
      const fn = map[key];
      wrapped[key] = async (...args) => {
        await gate();
        return fn(...args);
      };
    }
    out[kind] = wrapped;
  }
  return out;
}

// Returns the runtime of the host, getRuntimeKey of hono at
// dist/helper/adapter/index.js, read at the first call and kept, since the
// globals it reads do not change while the isolate runs.
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
// adapter: the process on Node and Bun and the bindings of the context on
// workerd.
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

// Returns the string bindings of the environment `env` a Worker hands its
// fetch, which the engine takes beside the variables of the process, so a
// setting of NODE_ENV reaches the engine with the bindings of c.env, and none
// on any other host, whose fetch takes no bindings.
function bindingsOf(env) {
  if (runtimeKey() !== "workerd" || env === null || typeof env !== "object") return {};
  return Object.fromEntries(Object.entries(env).filter(([, value]) => typeof value === "string"));
}

// Returns the variables the engine boots with for the bindings `env`: the
// variables of the process and the string bindings of a Worker.
function variablesOf(env) {
  return { ...(globalThis.process?.env ?? {}), ...bindingsOf(env) };
}

// Returns whether the URL `url` names the GraphQL endpoint, the test of the
// unhandled route of Yoga with the endpoint /graphql that Pylon's handler sets:
// a URL or a path that ends in /graphql or /graphql/. The engine answers each
// such request with every method, and the hono of the app every other one.
function isEndpoint(url) {
  if (url.endsWith("/graphql") || url.endsWith("/graphql/")) return true;
  const path = new URL(url).pathname;
  return path === "/graphql" || path === "/graphql/";
}

// The class Pylon of app/index.ts:140, under Aosabi's name, a Hono of the real
// hono with the graphql fragment of the app, its children, the middleware of
// its plugins and its boot.
export class App extends Hono {
  #host = undefined;
  #core = null;
  // The variables of an early boot, until the first request holds them to
  // its own.
  #seen = null;
  #pipeline = false;
  #realized = false;
  #canary = false;

  constructor(opts) {
    super();
    this.config = undefined;
    this.graphql = {};
    this.children = [];
    this.pluginsMiddleware = [];
    this.sourceDir = undefined;
    this.appOptions = opts ?? {};
    this.name = opts?.name;
    this.gate = opts?.gate;
    this.routePrefix = opts?.basePath;
    if (opts?.graphql) {
      const fragment = this.gate ? gateResolvers(opts.graphql, this.gate) : opts.graphql;
      mergeFragment(this.graphql, fragment);
    }
    // The fetch of hono, through which the core holds the request of the
    // engine of each request it dispatches until its whole chain has run, so
    // a middleware the app registered before the base pipeline reads after
    // next() the variables the engine set, as it reads those Pylon's Yoga set
    // on c. Once executeConfig has installed the base pipeline the first
    // fetch boots the core, whose refusal of the configuration rejects the
    // fetch.
    const dispatch = this.fetch;
    this.fetch = (request, ...rest) => {
      if (!this.#pipeline) return dispatch(request, ...rest);
      let core;
      try {
        core = this[CORE](rest[0]);
      } catch (error) {
        return Promise.reject(error);
      }
      core.dispatch(request);
      let answered;
      try {
        answered = dispatch(request, ...rest);
      } catch (error) {
        core.dispatched(request);
        throw error;
      }
      if (typeof answered?.then !== "function") {
        core.dispatched(request);
        return answered;
      }
      return answered.then((response) => {
        core.dispatched(request);
        return response;
      }, (error) => {
        core.dispatched(request);
        throw error;
      });
    };
  }

  // Records the children `children` and merges their fragments, compose of
  // app/index.ts:187-195.
  compose(...children) {
    for (const child of children) {
      this.children.push(child);
      mergeFragment(this.graphql, child.graphql);
    }
    return this;
  }

  // Installs the base pipeline of the served root once, installBasePipeline
  // of app/index.ts:292-363, in hono's own code: compress(), the bind of the
  // async context, the structured request logger behind except of the
  // internal prefix, the dispatch of the middleware of the plugins and the
  // error answer. The bind holds the request of the engine until its chain
  // has run, and the logger opens it with the logger of the request, whose
  // access line the end of the request writes, lib/core.js.
  installBasePipeline() {
    if (this.#pipeline) return;
    this.#pipeline = true;

    this.use("*", compress());

    this.use("*", async (c, next) => {
      const core = this[CORE](c.env);
      core.hold(c);
      return new Promise((resolve, reject) => {
        context.run({ context: c, core }, async () => {
          try {
            resolve(await next());
          } catch (error) {
            reject(error);
          } finally {
            core.release(c);
          }
        });
      });
    });

    this.use("*", except(["/__aosabi/*"], (c, next) => {
      const start = Date.now();
      const core = this[CORE](c.env);
      const request = core.open(c, true);
      const at = { ref: request.logger, tag: undefined, bindings: undefined };
      const store = { ...context.getStore(), request: request.id, bridge: request.bridge, logger: at.ref,
        loggerBindings: undefined };
      return context.run(store, async () => {
        await next();
        core.note(c, c.res.status, Date.now() - start);
      });
    }));

    // The middleware of the plugins in their order, each around the rest of
    // the chain. A middleware that returns nothing once the rest of the
    // chain has answered with a Response hands that Response on, so the
    // answer of a later middleware reaches hono behind middlewares that
    // await the rest and return nothing, where Pylon's dispatch loses it and
    // hono answers 500 with the records of the error logger, divergence D80
    // of docs/byte-equality.md of the Aosabi repository (row QD-003 of the
    // rated table). A middleware that returns a value hands hono that value,
    // and one that returns nothing over a rest that has not answered or has
    // answered no Response hands on nothing, each as on Pylon.
    this.use("*", (c, next) => {
      const dispatch = (i) => {
        const middleware = this.pluginsMiddleware[i];
        if (!middleware) return Promise.resolve(next());
        let answered;
        const rest = () => dispatch(i + 1).then((value) => {
          if (value instanceof Response) answered = value;
          return value;
        });
        return Promise.resolve(middleware(c, rest)).then((value) => (value === undefined ? answered : value));
      };
      return dispatch(0);
    });

    this.onError((err, c) => {
      if (err instanceof HTTPException) return err.getResponse();
      const status = err.statusCode;
      if (typeof status === "number") {
        return c.json({ error: err.message }, status);
      }
      getLogger().error("unhandled route error", { err });
      return c.json({ error: "Internal Server Error" }, 500);
    });
  }

  // Mounts the composed child tree once, depth first, realize of
  // app/index.ts:376-383.
  realize() {
    if (this.#realized) return;
    this.#realized = true;
    for (const child of this.children) {
      child.realize();
      this.route(child.routePrefix ?? "/", child);
    }
  }

  // Sets the middleware of the base pipeline of Pylon's canary that sets the
  // client of Sentry of a request where a DSN is set, once and first, which the
  // first pass of the glue of a build of the canary asks for before the module
  // of the app registers its routes. It holds the request of the engine until
  // its own end, since it stands before the bind of the async context.
  [CANARY]() {
    if (this.#canary) return;
    this.#canary = true;
    this.use("*", async (c, next) => {
      const core = this[CORE](c.env);
      core.hold(c);
      try {
        const client = core.sentry(c);
        if (client === undefined) return await next();
        c.set("sentry", client);
        await next();
        if (c.error) core.capture(c, c.error);
      } finally {
        core.release(c);
      }
    });
  }

  // Hands the app the engine of its host: the compiled module core.wasm or
  // an engine of aosabi/bun, the JSON text of the IR of the build or its schema
  // record and the two modes.
  [HOST](host) {
    this.#host = host;
  }

  // Hands every app of the process the engine of its host before the glue
  // imports the app, so a logger call at the load of a module or in the setup
  // of a plugin writes through the engine.
  static [HOST](host) {
    hosted = host;
  }

  // Returns the core of the app, which the first call makes from the host,
  // the options of the GraphQL handler of the glue and the configuration of
  // executeConfig, the bindings `env` of the first request beside the
  // variables of the process. The core of an early boot serves a first
  // request whose variables are those it booted with, and a first request
  // whose variables differ boots the app again with its own.
  [CORE](env) {
    if (this.#seen !== null) {
      const seen = this.#seen;
      this.#seen = null;
      if (!sameVariables(seen, variablesOf(env))) this.#core = null;
    }
    if (this.#core !== null) return this.#core;
    return this.#boot(variablesOf(env));
  }

  // Boots the app before its first request with the bindings `env` of the
  // global scope of a Worker, which aosabi/worker hands once the glue has
  // loaded (lib/early.js), and holds the first request to the variables of
  // that boot.
  [EARLY](env) {
    if (this.#core !== null) return;
    const variables = variablesOf(env);
    this.#boot(variables).early = true;
    this.#seen = variables;
  }

  // Makes the core of the app with the variables `variables`.
  #boot(variables) {
    const host = this.#host ?? hosted;
    if (host === undefined) {
      throw new Error("no host handed the app its engine: serve it through aosabi/worker or aosabi/bun");
    }
    const options = this[HANDLER];
    if (options === undefined) throw new Error("the glue mounted no GraphQL handler on the app");
    const graphql = typeof options.graphql === "function" ? options.graphql() : options.graphql;
    const config = this.config ?? {};
    const started = bootClock();
    const core = createCore({
      module: host.module,
      engine: host.engine,
      schema: options.typeDefs,
      ir: host.ir,
      graphql,
      config,
      mode: host.mode ?? "faithful",
      read: host.read ?? "get",
      plan: { plugins: pluginsOf(config), types: typeResolversOf(options.resolvers) },
      env: variables,
      primed: takePrimed(host),
    });
    core.bootMs = bootClock() - started;
    this.#core = core;
    return core;
  }
}

// Returns the instance primeCore opened for the logger of the engine `host`
// hands, which the first app to boot takes, or undefined.
function takePrimed(host) {
  const taken = primed;
  if (taken === null || (host.engine ?? null) !== (hosted?.engine ?? null) || host.module !== hosted?.module) {
    return undefined;
  }
  primed = null;
  return taken;
}

// Returns the plugins of `config` as the boot record takes them: a marker by
// its name and its options, and a plugin of the app with the keys the host
// reports of it, its hook onExecute, which Envelop reads as the orchestrator
// destructures a plugin, and the names of the other hooks of Envelop and Yoga
// it holds, each where it is true, which the boot refuses. Its setup and its
// middleware run in the hono of the app.
function pluginsOf(config) {
  return (config.plugins ?? []).map((plugin) => {
    if (plugin.marker !== undefined) {
      return { marker: plugin.marker, options: plugin.options, functions: plugin.functions };
    }
    return {
      name: plugin.name,
      strategy: plugin.strategy,
      roles: plugin.roles,
      dependsOn: plugin.dependsOn,
      onExecute: plugin.onExecute,
      hooks: OTHER_HOOKS.filter((hook) => plugin[hook]),
    };
  });
}

// The default app, app of app/index.ts:414.
export const app = new App();

// The run role of the process, currentRole of pylon-handler.ts:114-117.
export const currentRole = () => {
  const role = globalThis.process?.env?.AOSABI_ROLE;
  return role === "worker" || role === "all" ? role : "web";
};

// Returns whether `plugin` runs under `role`, roleAllows of
// pylon-handler.ts:121-122.
const roleAllows = (role, plugin) => role === "all" || !plugin.roles || plugin.roles.includes(role);

// Returns `plugins` in the order of their dependencies, topoSortPlugins of
// vendor/pylon/packages/pylon/src/app/plugin-order.ts:14-45.
function topoSortPlugins(plugins) {
  const byName = new Map();
  for (const plugin of plugins) if (plugin.name) byName.set(plugin.name, plugin);
  const order = new Map(plugins.map((plugin, index) => [plugin, index]));
  const state = new Map();
  const out = [];
  const visit = (plugin, stack) => {
    const seen = state.get(plugin);
    if (seen === "done") return;
    if (seen === "visiting") {
      throw new Error(`Pylon plugin dependency cycle: ${[...stack, plugin.name ?? "(anonymous)"].join(" → ")}`);
    }
    state.set(plugin, "visiting");
    const deps = (plugin.dependsOn ?? [])
      .map((name) => byName.get(name))
      .filter((dep) => !!dep && dep !== plugin)
      .sort((a, b) => order.get(a) - order.get(b));
    for (const dep of deps) visit(dep, plugin.name ? [...stack, plugin.name] : stack);
    state.set(plugin, "done");
    out.push(plugin);
  };
  for (const plugin of plugins) visit(plugin, []);
  return out;
}

// The built in plugins of the boot of Pylon in their place of the list of
// pylon-handler.ts:153-170, before and after the configured ones: the viewer,
// the error logger and the in context plugin, and the landing page and the
// refusal of introspection where the configuration asks for them. The
// engine runs the hooks of each, and the routes of the viewer and of the
// landing page answer on the engine where the configuration switches them on
// with viewer true and landingPage true, the sides ts of D48 and D47 of
// docs/byte-equality.md of the Aosabi repository.
function builtins(config) {
  const before = [{ builtin: "viewer" }, { builtin: "errors" }, { builtin: "in-context", name: "in-context" }];
  const after = [];
  if (config?.landingPage ?? true) after.push({ builtin: "landing" });
  if (config?.graphiql === false) after.push({ builtin: "introspection" });
  return [before, after];
}

// Runs one pass of the boot over `target`, executeConfig of
// pylon-handler.ts:127-188: the base pipeline installed and the children
// mounted, and the setup of each plugin of the pass awaited in the order of
// the dependencies on the hono of the target, its middleware pushed to the
// plugin chain of the base pipeline. A setup that throws fails the pass with
// the message of Pylon's loader.
export const executeConfig = async (config, args, target) => {
  // The glue of a build of Pylon's canary calls executeConfig with no target,
  // on the default app, before the module of the app, where the glue of the
  // reference names its app. The base pipeline of the canary opens with the
  // middleware that sets the client of Sentry of a request where a DSN is set,
  // which the first pass sets first on that app.
  if (target === undefined && (args?.pluginsStrategy ?? "first") === "first") app[CANARY]();
  target ??= app;
  target.installBasePipeline();
  target.realize();
  configureLogger(config?.logger);
  const before = running;
  running = { logger: config?.logger };
  try {
    await runPass(config, args, target);
  } finally {
    running = before;
  }
};

// Applies the option `logger` of a configuration to the logger of the engine
// at the start of a pass, configureLogger of
// vendor/pylon/packages/pylon/src/core/logger.ts:358-376, where primeCore has
// booted that logger and the app has not booted yet. The boot of the app does
// it otherwise, which writes the same records, since no record came before.
function configureLogger(option) {
  if (primed === null || primed.booted || primed.dead) return;
  logCall(LOG.CONFIGURE, ROOT, (out) => writeLoggerOption(out, option));
}

// Runs the rest of one pass of executeConfig after the logger is configured.
async function runPass(config, args, target) {
  const pluginsStrategy = args?.pluginsStrategy || "first";
  const [before, after] = builtins(config);
  const plugins = [...before, ...(config?.plugins || []), ...after];
  const role = currentRole();
  const pass = plugins.filter((plugin) => {
    if (!plugin.strategy) plugin.strategy = "first";
    if (!roleAllows(role, plugin)) return false;
    return plugin.strategy === pluginsStrategy;
  });
  const sentries = (config?.plugins ?? []).filter((plugin) => plugin.marker === "useSentry");
  for (const [index, plugin] of topoSortPlugins(pass).entries()) {
    if (plugin.builtin !== undefined) {
      builtinSetup(plugin.builtin, config, target);
      continue;
    }
    if (plugin.marker !== undefined) {
      markerSetup(plugin, sentries.indexOf(plugin), target);
      continue;
    }
    try {
      await plugin.setup?.(target);
    } catch (e) {
      const label = plugin.name ? `"${plugin.name}"` : `#${index} (unnamed)`;
      const cause = e instanceof Error ? e : new Error(String(e));
      throw new Error(`Pylon plugin ${label} failed during setup: ${cause.message}`, { cause });
    }
    if (plugin.middleware) target.pluginsMiddleware.push(plugin.middleware);
  }
  target.config = config;
}

// Registers what the built in plugin `kind` of the boot of Pylon registers on
// `target` in its setup where the configuration `config` switches it on: the
// route /viewer of useViewer with viewer true and the not found handler of
// useUnhandledRoute with landingPage true, each answered by the engine, while
// the error logger, the in context plugin and the refusal of introspection hold
// hooks of the engine alone.
function builtinSetup(kind, config, target) {
  if (kind === "viewer" && config?.viewer === true) target.get("/viewer", (c) => answerEngine(target, c));
  if (kind === "landing" && config?.landingPage === true) target.notFound((c) => answerEngine(target, c));
}

// Runs the setup of the marker `plugin` of the engine on `target`: the
// serving of useNodeServer through its host, the three routes of the login
// of zitadelLogin, which the engine answers, and the middleware of
// useSentry, the `index`th of the configuration, which the plugin chain of
// the base pipeline runs in its place.
function markerSetup(plugin, index, target) {
  if (plugin.marker === "useNodeServer") {
    serveNode(target, plugin.options);
  } else if (plugin.marker === "zitadelLogin") {
    const endpoint = plugin.options?.endpoint ?? "/auth";
    for (const route of ["login", "logout", "callback"]) {
      target.get(`${endpoint}/${route}`, (c) => answerEngine(target, c));
    }
  } else if (plugin.marker === "useSentry") {
    target.pluginsMiddleware.push(sentryMiddleware(target, index));
  }
}

// Returns the middleware of the plugin useSentry, the middleware of
// @hono/sentry 1.2.2 Pylon's plugin holds: the client of Sentry of the request,
// which the engine builds from the options of the `index`th marker useSentry
// and keeps for the request, set under the key sentry of c, the rest of the
// chain, and the report of the error hono set on c once it has run.
function sentryMiddleware(target, index) {
  return async (c, next) => {
    const core = target[CORE](c.env);
    const client = core.sentry(c, index);
    if (client !== undefined) c.set("sentry", client);
    await next();
    if (c.error && client !== undefined) core.capture(c, c.error);
  };
}

// Answers the request of `c` on the app of the engine of `target`, a route
// of a plugin of the engine or its not found handler, and returns its
// response over the prepared header fields of c, with its body in the form
// the engine wrote it, the string or the bytes of a whole body, `bodyOf`, as
// the route of Pylon the route of the engine stands for answers the body of
// a helper of c or the response of an HTTPException, so a host writes its
// length and the type of a string where the answer names none. The
// variables the engine sets for the request read on c from then on.
async function answerEngine(target, c) {
  const core = target[CORE](c.env);
  readThrough(c, core);
  const response = await core.answer(c, TO.APP);
  return c.newResponse(bodyOf(response), response);
}

// The contexts whose reads of variables reach the engine for a key the app
// did not set.
const reading = new WeakSet();

// Makes the reads c.get and c.var of the context `c` of hono answer the
// variables the engine `core` sets for the request of c for a key the app did
// not set on c, the bag of @inContext and the info a root resolver stores
// under graphqlResolveInfo, each after the variables of the app in c.var, as
// Pylon's GraphQL layer sets them on c after the middleware of the app.
function readThrough(c, core) {
  if (reading.has(c)) return;
  reading.add(c);
  const own = c.get;
  c.get = (key) => {
    const value = own(key);
    return value === undefined ? core.variable(c, key) : value;
  };
  const vars = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Object.getPrototypeOf(c)), "var")
    ?? Object.getOwnPropertyDescriptor(Object.getPrototypeOf(c), "var");
  Object.defineProperty(c, "var", {
    configurable: true,
    get() {
      const all = vars.get.call(c);
      for (const name of core.variableNames(c)) {
        if (!Object.hasOwn(all, name)) all[name] = core.variable(c, name);
      }
      return all;
    },
  });
}

// Serves `target` with the options `options` of the marker useNodeServer, as
// the setup of the plugin of use-node-server.ts:52-69 serves an app on Node,
// where the host gives a serving of its own, aosabi/bun over Bun.serve: in a
// pass of the strategy last and the role web alone, which the pass has
// filtered by, and never under NODE_ENV=development, where pylon dev owns the
// serving. A Worker, whose host gives none, serves nothing here, since
// workerd serves the app, as the plugin of the reference binds nothing there.
function serveNode(target, options) {
  if (globalThis.process?.env?.NODE_ENV === "development") return;
  hosted?.serve?.(target, options ?? {});
}

// Returns the GraphQL handler of the glue as a middleware the glue mounts with
// use, handler of pylon-handler.ts:190-386, which keeps the SDL typeDefs, the
// graphql object of the app and the type resolvers of resolvers.js on the
// target for its boot. The middleware hands every request of the GraphQL
// endpoint, with every method, to the server of the endpoint in the engine,
// which answers it as Yoga answers it with no router of its own, since this
// middleware has routed it, goes on where the engine answers 404, sets
// X-Pylon-Version where the version global is set, and answers the response of
// the engine over the prepared header fields of c. Every other request goes on
// to the routes of the app, where Pylon's Yoga answers the plugins of its
// server on any path, its health check and its preflight among them, divergence
// D56 of docs/byte-equality.md of the Aosabi repository.
export const handler = (options, target = app) => {
  target[HANDLER] = options;
  return async (c, next) => {
    if (!isEndpoint(c.req.url)) return next();
    const core = target[CORE](c.env);
    readThrough(c, core);
    const response = await core.answer(c, TO.ENDPOINT);
    if (response.status === 404) return next();
    const version = globalThis.__PYLON_VERSION__;
    if (version) c.header("X-Pylon-Version", version);
    return c.newResponse(response.body, response);
  };
};

// The context of the running request, getContext of context.ts:30-40, with
// the environment of the runtime set on it. Outside a request, and inside a
// route that stands before the base pipeline, there is none, and the call
// throws as Pylon's does.
export const getContext = () => {
  const ctx = context.getStore()?.context;
  if (!ctx) throw new Error("Context not defined");
  ctx.env = adapterEnv(ctx);
  return ctx;
};

// Puts `ctx` in place as the context of the running code, setContext of
// context.ts:42-44.
export const setContext = (ctx) => context.enterWith({ ...context.getStore(), context: ctx });

// The environment of the running request or of the process, getEnv of
// get-env.ts:3-24, with NODE_ENV written where it is unset.
export function getEnv() {
  try {
    const own = context.getStore().context;
    const ctx = own.env || globalThis.process?.env || {};
    ctx.NODE_ENV = ctx.NODE_ENV || globalThis.process?.env?.NODE_ENV || "development";
    return ctx;
  } catch {
    return globalThis.process?.env;
  }
}

// The key of the context that holds what @inContext carried, IN_CONTEXT_KEY of
// in-context.ts:57.
const IN_CONTEXT_KEY = "pylonInContext";

// Everything @inContext carried for the running operation, getInContext of
// in-context.ts:88-95, and an empty object outside a request.
export const getInContext = () => {
  try {
    return getContext().get(IN_CONTEXT_KEY) ?? {};
  } catch {
    return {};
  }
};

// The locale the running operation asked for, getLocale of in-context.ts:80.
export const getLocale = () => getInContext().locale;

// The store of the resolver that runs, getResolveInfo of
// resolve-info.ts:16-23: the info of graphql-js of its call and the fields
// selected below it, each its name, its nodes and its type, the one object of
// the call, as the views of lib/nodes.js answer them over what the engine
// holds, the nodes of the document among them, while the schema does not
// cross, which docs/ts1-surface.md of the Aosabi repository names.
export const getResolveInfo = () => {
  const store = context.getStore();
  if (!store?.info) throw new Error("Resolve info is not available");
  const described = store.bridge.info(store.info);
  if (described === undefined) throw new Error("Resolve info is not available");
  return described;
};

// Returns a decorator that runs `callback` before the member or the function
// it decorates, createDecorator of create-decorator.ts:1-59.
export function createDecorator(callback) {
  function MyDecorator(arg1, propertyKey, descriptor) {
    if (descriptor) {
      const originalMethod = descriptor.value;
      descriptor.value = async function (...args) {
        await callback(...args);
        return originalMethod.apply(this, args);
      };
      return descriptor;
    }
    if (propertyKey === undefined) {
      const originalFunction = arg1;
      return async function (...args) {
        await callback(...args);
        return originalFunction(...args);
      };
    }
    let value = arg1[propertyKey];
    Object.defineProperty(arg1, propertyKey, {
      get: function () {
        return async function (...args) {
          await callback(...args);
          if (typeof value === "function") {
            return value(...args);
          }
          return value;
        };
      },
      set: function (newValue) {
        value = newValue;
      },
      enumerable: true,
      configurable: true,
    });
  }
  return MyDecorator;
}

// Returns the issues of a ValidationError of pylon-db, by its shape, or null,
// validationIssues of mutation.ts:44-52.
function validationIssues(err) {
  const issues = err?.issues;
  if (Array.isArray(issues) && issues.every((issue) => issue && typeof issue.message === "string")) {
    return issues;
  }
  return null;
}

// Returns the userErrors of a thrown error, or null for one to throw on,
// toUserErrors of mutation.ts:55-74.
function toUserErrors(err) {
  const issues = validationIssues(err);
  if (issues) {
    return issues.map((issue) => ({
      field: issue.path ? issue.path.split(".") : [],
      message: issue.message ?? "Invalid value",
      code: issue.code ?? "invalid",
    }));
  }
  if (err instanceof ServiceError) {
    const details = err.extensions?.details;
    return [
      {
        field: details?.field ?? [],
        message: err.message,
        code: String(err.extensions?.code ?? "ERROR"),
      },
    ];
  }
  return null;
}

// Wraps a resolver of a mutation so an error the user can act on answers as
// userErrors, mutation of mutation.ts:62-80.
export function mutation(fn) {
  return async (...args) => {
    try {
      const result = await fn(...args);
      return { ...result, userErrors: [] };
    } catch (err) {
      const userErrors = toUserErrors(err);
      if (userErrors) return { userErrors };
      throw err;
    }
  };
}

// The levels of a record, in the order of their ranks, LogLevel of
// logger.ts:20.
const LEVELS = ["trace", "debug", "info", "warn", "error", "fatal"];

// Returns the bridge of the engine a logger call writes through. Before the
// first instance of the engine the call opens one through primeCore with the
// engine a host handed the class Pylon, which boots the logger alone, and
// applies the option logger of the pass of executeConfig that runs, so the call
// writes at once, as the root logger of Pylon writes. With no engine handed yet
// the call throws.
function logBridge() {
  const own = bridge();
  if (own !== null) return own;
  if (hosted === undefined) {
    throw new Error("the logger of aosabi writes through the engine, which a host hands the app through " +
      "aosabi/worker or aosabi/bun");
  }
  primed = primeCore(hosted, { ...(globalThis.process?.env ?? {}) });
  if (running !== null) configureLogger(running.logger);
  return bridge();
}

// Writes the logger `ref` with the tag `tag` into the record of a call of the
// logger: 0 for the root, or 1 and the two numbers of a bound logger of the
// engine, then the tag a logger of logger(tag) resolves with.
function writeLogger(out, ref, tag) {
  if (ref === null || ref === undefined) {
    out.byte(0);
  } else {
    out.byte(1);
    out.word(ref[0]);
    out.word(ref[1]);
  }
  if (tag === undefined) {
    out.byte(0);
  } else {
    out.byte(1);
    out.text(String(tag));
  }
}

// The forms of a member of the fields of a logger call,
// crates/aosabi-js/src/log.rs: a value of JSON, whose line the engine
// writes, and the text of its line beside the value its JSON line writes.
const FIELD = { VALUE: 0, TEXT: 1 };

// Returns the error `error` as a record carries it, {name, message, stack},
// read in the order of Pylon's normalize, with the line
// Error.prototype.toString answers for that name and that message in place of a
// stack that is a string, as the engine writes the stack of an error on every
// host. The frames of a stack name the source files of the app and of its
// packages, so no record of the side carries them, divergence D83 of
// docs/byte-equality.md of the Aosabi repository (row QD-272 of the rated
// table). The line is the text before the first frame of a stack V8 formats for
// the error itself, with its name and its message as they were at the first
// read of the stack. A stack the error took from another error, as a
// GraphQLError holds the stack of its originalError, the case copied of D83, or
// a stack first read before the name or the message of its error changed,
// begins with another text, which Pylon's record keeps and the side's does not.
// A stack that is no string stays as the error holds it, so a record leaves out
// the stack of an error that holds none, as Pylon's does.
function normalError(error) {
  const name = error.name;
  const message = error.message;
  const stack = error.stack;
  return { name, message, stack: typeof stack === "string" ? stackLine(name, message) : stack };
}

// Returns the line Error.prototype.toString answers for the name `name` and the
// message `message`: the name, a colon, a space and the message, the one of
// them that is not empty where the other is, the name Error for a name that is
// undefined and the empty message for a message that is undefined. It is the
// text before the first frame of a stack V8 formats for an error with that name
// and that message at the first read of its stack.
function stackLine(name, message) {
  const named = name === undefined ? "Error" : String(name);
  const said = message === undefined ? "" : String(message);
  if (named === "") return said;
  if (said === "") return named;
  return named + ": " + said;
}

// Writes the fields of a record as the engine takes them: the byte 0 for none,
// or the byte 1 and the own keys of their spread, each value an error made
// {name, message, stack} by normalError. The line of Pylon writes a field as
// String(value) and an object as its JSON.stringify, and its JSON line leaves
// out a member that is undefined, a function or a symbol. So a value that is
// undefined, a boolean, a number, a string or null crosses as itself, an object
// whose JSON is an object or an array crosses as that JSON, and a function, a
// symbol and an object whose JSON is no object cross as the text of their line
// beside the value their JSON line writes, undefined for one it leaves out. A
// bigint with no toJSON throws the TypeError of JSON.stringify of the host in a
// record the logger writes, as the JSON line of Pylon throws it, while the line
// of Pylon writes String(value).
function writeLogFields(out, fields) {
  if (fields === undefined || fields === null) {
    out.byte(0);
    return;
  }
  const spread = { ...fields };
  const keys = Object.keys(spread);
  out.byte(1);
  out.word(keys.length);
  for (const key of keys) {
    const value = spread[key];
    const normal = value instanceof Error ? normalError(value) : value;
    out.text(key);
    if (typeof normal === "function" || typeof normal === "symbol") {
      out.byte(FIELD.TEXT);
      out.text(String(normal));
      writeJson(out, undefined);
    } else if (typeof normal === "bigint") {
      const text = JSON.stringify(normal);
      out.byte(FIELD.TEXT);
      out.text(String(normal));
      writeJson(out, text === undefined ? undefined : JSON.parse(text));
    } else if (typeof normal === "object" && normal !== null) {
      const text = JSON.stringify(normal);
      const copy = text === undefined ? undefined : JSON.parse(text);
      if (typeof copy === "object" && copy !== null) {
        out.byte(FIELD.VALUE);
        writeJson(out, copy);
      } else {
        out.byte(FIELD.TEXT);
        out.text(String(text));
        writeJson(out, copy);
      }
    } else {
      out.byte(FIELD.VALUE);
      writeJson(out, normal);
    }
  }
}

// Calls the logger op `op` of the logger `at`, with the arguments `write`
// writes, through the engine. The logger is the handle `ref` of a logger of
// the engine, null for the root, and the tag `tag` the engine resolves it
// with, undefined for none.
function logCall(op, at, write) {
  const own = logBridge();
  const store = context.getStore();
  const out = record();
  out.word(store?.request ?? 0);
  out.byte(loopTurned() ? 1 : 0);
  out.byte(op);
  writeLogger(out, at.ref, at.tag);
  if (write !== undefined) write(out);
  return own.log(out);
}

// The logger a call names for the root of the engine, with no tag and no
// binding of the host.
const ROOT = { ref: null, tag: undefined, bindings: undefined };

// Returns the logger `at` with its tag resolved into a bound logger of the
// engine, the withTag a logger of a tag makes at each call, so the calls that
// follow name that one logger.
function pin(at) {
  if (at.tag === undefined) return at;
  const ref = logCall(LOG.WITH_TAG, { ref: at.ref, tag: undefined }, (out) => out.text(String(at.tag)));
  return { ref, tag: undefined, bindings: at.bindings };
}

// Returns a logger of Pylon's interface over `resolve`, logger.ts:24-45 and
// :192-216. The function `resolve` answers the logger a call writes
// through: the handle of its logger of the engine, the tag the engine
// resolves it with and the bindings it carries on the side of the host.
//
// A record takes two calls of the engine after the one of pin() for a logger of
// a tag. The first reads the level of the logger for its tag, and a record
// below that level returns before its message, its bindings and its fields are
// read, as the emit of Pylon returns at its gate before it builds the record.
// The second writes the record with the bindings of the host spread before its
// fields. The engine builds a record as {time, level, msg..bindings..fields},
// so the bindings of its own logger, then those of the host and then the fields
// give the keys of Pylon's record in Pylon's order, the bindings of a child
// spread after those of its parent. The bindings of child() and createLogger()
// stay on the side of the host as Pylon keeps them, the spread child() makes at
// its call and the object createLogger() is handed, unread, so they reach the
// engine only as the fields of a record the logger writes, and a binding the
// JSON of the host cannot write throws only there, as the sink of Pylon throws
// it.
function makeLogger(resolve) {
  const emit = (level) => (msg, fields) => {
    const at = pin(resolve());
    if (LEVELS.indexOf(level) < LEVELS.indexOf(logCall(LOG.LEVEL, at))) return;
    logCall(LOG.EMIT, at, (out) => {
      out.byte(LEVELS.indexOf(level));
      out.text(String(msg));
      writeLogFields(out, at.bindings === undefined ? fields : { ...at.bindings, ...fields });
    });
  };
  const logger = {
    trace: emit("trace"),
    debug: emit("debug"),
    info: emit("info"),
    warn: emit("warn"),
    error: emit("error"),
    fatal: emit("fatal"),
    // The engine makes a child with no binding of its own, which keeps the
    // logger and the tag `resolve` answers at this call.
    child: (bindings) => {
      const at = resolve();
      const merged = { ...at.bindings, ...bindings };
      const child = { ref: logCall(LOG.CHILD, at, (out) => out.byte(0)), tag: undefined, bindings: merged };
      return makeLogger(() => child);
    },
    withTag: (tag) => {
      const at = resolve();
      const tagged = { ref: logCall(LOG.WITH_TAG, at, (out) => out.text(String(tag))), tag: undefined,
        bindings: at.bindings };
      return makeLogger(() => tagged);
    },
    tee: () => {
      throw new Error("tee of the logger of aosabi takes no sink of the app, a function of the app crosses into " +
        "no logger of the engine");
    },
    get level() {
      return logCall(LOG.LEVEL, resolve());
    },
  };
  Object.defineProperty(logger, REF, { value: resolve });
  return logger;
}

// The member of a logger of this entry that answers the logger a call of it
// writes through.
const REF = Symbol("aosabi.logger");

// The process wide root logger, getRootLogger of logger.ts:255.
export const getRootLogger = () => makeLogger(() => ROOT);

// The scoped logger of the running request, or the root outside a scope,
// getLogger of logger.ts:269. The store of a call holds the handle of the
// scoped logger of the engine and the bindings it carries on the side of the
// host.
export const getLogger = () => {
  const store = context.getStore();
  const at = { ref: store?.logger ?? null, tag: undefined, bindings: store?.loggerBindings };
  return makeLogger(() => at);
};

// Runs `fn` with `log` as the scoped logger of its whole execution,
// runWithLogger of logger.ts:272.
export const runWithLogger = (log, fn) => {
  const resolve = log?.[REF];
  if (resolve === undefined) throw new TypeError("runWithLogger of aosabi takes a logger of aosabi");
  const at = pin(resolve());
  return context.run({ ...context.getStore(), logger: at.ref, loggerBindings: at.bindings }, fn);
};

// A module scoped, lazy, tagged logger, which resolves the scoped logger at
// every call, logger of logger.ts:280-296.
export const logger = (tag) => makeLogger(() => {
  const store = context.getStore();
  return { ref: store?.logger ?? null, tag, bindings: store?.loggerBindings };
});

// Builds a logger with an explicit level, tag and bindings, createLogger of
// logger.ts:219-233. A sink of the app crosses into no logger of the engine,
// and the engine makes the logger with no binding of its own, since the
// bindings stay with the host.
export const createLogger = (opts = {}) => {
  if (opts.sink !== undefined) {
    throw new Error("createLogger of aosabi takes no sink of the app, a function of the app crosses into no " +
      "logger of the engine");
  }
  const ref = logCall(LOG.CREATE, ROOT, (out) => {
    const level = opts.level;
    if (level === undefined) {
      out.byte(0);
    } else if (typeof level === "string") {
      out.byte(1);
      out.text(level);
    } else {
      const entries = Object.entries(level);
      out.byte(2);
      out.word(entries.length);
      for (const [key, value] of entries) {
        out.text(key);
        out.text(String(value));
      }
    }
    if (opts.tag === undefined) {
      out.byte(0);
    } else {
      out.byte(1);
      out.text(String(opts.tag));
    }
    out.byte(0);
  });
  const at = { ref, tag: undefined, bindings: opts.bindings };
  return makeLogger(() => at);
};

// The Sentry plugin of Pylon as a marker, useSentry of
// vendor/pylon/packages/pylon/src/plugins/use-sentry.ts:96-276, which the
// engine makes the plugin of aosabi-sentry of: the values of its options
// cross as JSON data, and its option skip as a function that hands the
// function of the app the member args of the payload of onExecute, the view
// of the arguments of the execution. Its middleware, the one of
// @hono/sentry, runs in the plugin chain of the hono of the app in its place,
// sentryMiddleware, over the client the engine builds for the request. Pylon hands every key outside its twelve
// GraphQL keys to the middleware of @hono/sentry and so to Toucan
// (use-sentry.ts:118-145), beforeSend, beforeBreadcrumb, integrations and
// tracesSampler among them, which the engine runs none of, so every other key
// whose value is a function or holds one in a member at any depth crosses as
// a function the engine runs none of, which fails the boot with the waiting
// it names, and no function of the app goes unapplied without a word.
export function useSentry(options = {}) {
  const functions = {};
  for (const [key, value] of Object.entries(options)) {
    if (key === "skip" && typeof value === "function") {
      functions.skip = (payload) => Boolean(value(payload.args));
    } else if (holdsFunction(value, new Set())) {
      functions[key] = null;
    }
  }
  return { name: "sentry", marker: "useSentry", options, functions };
}

// Returns whether `value` is a function or holds one in an own enumerable
// member of an object or an array at any depth, `seen` the objects walked.
function holdsFunction(value, seen) {
  if (typeof value === "function") return true;
  if (value === null || typeof value !== "object" || seen.has(value)) return false;
  seen.add(value);
  return Object.values(value).some((member) => holdsFunction(member, seen));
}

// The serving plugin of Pylon as a marker, useNodeServer of
// use-node-server.ts:42, with its name, its strategy and its roles, which the
// boot of the engine replaces by no plugin, since the host serves: the pass
// of executeConfig serves the app through the serving of its host, serveNode,
// aosabi/bun over Bun.serve, and a Worker serves nothing.
export function useNodeServer(options = {}) {
  return { name: "node-server", strategy: "last", roles: ["web"], marker: "useNodeServer", options };
}

// The configuration as it is, defineConfig of core/index.ts:182-186.
export function defineConfig(config) {
  return config;
}

