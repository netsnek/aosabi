import { Hono, MiddlewareHandler } from 'hono';
import { Env } from '../core/context';
import type { AppConfig } from '../core/index';
type ResolverMap = Record<string, (...args: any[]) => any>;
/** A GraphQL resolver fragment — the typed surface the compiler introspects. */
export interface Resolvers {
    Query?: ResolverMap;
    Mutation?: ResolverMap;
    Subscription?: ResolverMap;
}
type GraphqlOf<P> = P extends App<infer G> ? G : {};
/**
 * Deep intersection of every child Pylon's `graphql` fragment. `{Query: A} &
 * {Query: B}` collapses to `{Query: A & B}`, so the per-kind resolver maps merge
 * at the type level — the SAME shape `compose()` proves type-introspects.
 */
type MergeGraphql<C extends readonly unknown[]> = C extends readonly [
    infer H,
    ...infer T
] ? GraphqlOf<H> & MergeGraphql<T> : {};
/**
 * An app's CAPABILITY gate — a check that THROWS on deny (e.g. `ForbiddenError`).
 * A plain thunk, so core stays authz-free: the user (or the pylon-db `gate()`
 * sugar) writes it with auth/db helpers, and core only CALLS it — before every
 * resolver of the app. It runs once the per-request context is bound (after
 * `useDatabase`), so `authorize`/`requireFeature` can read the Principal/features.
 * Row/resource authz is a separate, ORM-layer concern (`defineAbilities`).
 */
export type Gate = () => void | Promise<void>;
/**
 * Constructor options for a `Pylon`. Named + AUGMENTABLE on purpose: satellite
 * packages add their own app-level config keys via `declare module` (e.g. pylon-db
 * adds `models?: {...}`), so an app's whole configuration lives in one
 * `new Pylon({...})` call — without core knowing what those keys mean. Core stashes
 * the object on `app.pylonOptions`; each satellite reads its own slice from there.
 */
export interface AppOptions<G extends Resolvers = {}> {
    graphql?: G;
    gate?: Gate;
    basePath?: string;
    name?: string;
}
export declare class App<G extends Resolvers = {}> extends Hono<Env> {
    /** The resolved config for this instance (set by `executeConfig`). */
    config?: AppConfig;
    /**
     * This Pylon's GraphQL resolver fragment. Built by `resolvers()` / `compose()`.
     * `export const graphql = app.graphql` is the typed surface the compiler reads
     * to emit the SDL; at runtime the same object provides the resolver functions.
     */
    graphql: G;
    /** Composed child Pylons (recorded by `compose`, for later route/plugin wiring). */
    readonly children: App<any>[];
    /**
     * The plugin middleware chain for THIS instance. `executeConfig` fills it; the
     * onion-chain loader below composes it so a middleware can WRAP `next()` (e.g.
     * `useDatabase` binds the per-request connection/tenant/principal around resolver
     * execution). A middleware that returns without calling `next` short-circuits.
     */
    pluginsMiddleware: MiddlewareHandler[];
    /** Whether `installBasePipeline()` has already run on this instance (idempotency). */
    private basePipelineInstalled;
    /** Whether `realize()` has mounted this instance's composed child tree (idempotency). */
    private realized;
    /**
     * Fuse child apps into this root: each child's `graphql` fragment merges into
     * this Pylon's `graphql` (type-accumulating, the deep intersection — so the
     * build type-introspects ONE merged schema served at one /graphql), and each
     * child's ROUTES are mounted onto this app. Returns the same instance re-typed
     * with the merged graphql.
     *
     * GraphQL doesn't federate — it merges; routes DO mount (Hono sub-app). This is
     * the single composition primitive: `new Pylon().compose(billing, catalog)`.
     *
     * RECORD-only: `compose` merges the graphql fragment and records each child, but
     * does NOT mount routes or install any middleware. The child tree is mounted once,
     * at boot, by `realize()` — after `installBasePipeline()` — so the once-per-request
     * pipeline precedes the routes and is never duplicated (no "install on the composer,
     * strip when re-composed" dance). A child mounts at its `basePath` (default `/`), so
     * an app's routes can be namespaced under a prefix (`new Pylon({basePath: '/vault'})`
     * → its routes live under `/vault`); the GraphQL fragment still merges to the single
     * root `/graphql`. The prefix is the seam for per-app route middleware (gating `/vault/*`).
     */
    compose<C extends readonly App<any>[]>(...children: C): App<G & MergeGraphql<C>>;
    /**
     * `new Pylon({ graphql: { Query: {...} } })` declares the resolver fragment up
     * front (the generic `G` is inferred from it), so a leaf app needs no `.resolvers()`
     * chaining. `new Pylon()` is `Pylon<{}>`.
     *
     * Overloads matter: making `graphql` REQUIRED in the second signature forces the
     * checker to infer `G` from the argument. A single `(opts?: {graphql?: G})` lets
     * the build's compiler fall back to the default `G = {}` (an optional property +
     * a defaulted type param is a known inference weak spot) — which silently drops
     * the schema. (Verified: that fallback produced an empty `graphql` type.)
     */
    /**
     * A stable identity for this app. Generic and auth/db-free — core only stores it.
     * Satellites use it as the app's key: `pylon-db` groups this app's migrations under
     * it when models bind via `app.model()`, and `pylon-queues` namespaces `app.queue()`.
     * Optional; composition never requires it.
     */
    readonly name?: string;
    /**
     * The raw constructor options, including satellite-augmented keys (e.g. pylon-db's
     * `models`). Core never interprets the extra keys — satellites read their own slice
     * (`app.pylonOptions.models`) so app config can live in one `new Pylon({...})` call.
     */
    readonly appOptions: AppOptions<G>;
    /** This app's capability gate (if any) — wraps its resolvers. */
    readonly gate?: Gate;
    /**
     * Where `compose` mounts THIS app's routes on its parent (default `/`). A
     * composition concern — it prefixes the child's Hono routes only; the GraphQL
     * fragment always merges to the parent's single root `/graphql`.
     */
    readonly routePrefix?: string;
    /**
     * Absolute directory of the SOURCE FILE that constructed this Pylon (captured from
     * the call stack). Generic and satellite-free — core only stores it. Satellites use
     * it to colocate per-app artifacts with the app's source: `pylon-db` defaults an
     * app's migrations to `<sourceDir>/migrations`. `undefined` if the call site can't
     * be determined. Requires the constructing file to run as itself (not bundled) —
     * which the project runner guarantees.
     */
    readonly sourceDir?: string;
    constructor();
    constructor(opts: {
        graphql: G;
    } & AppOptions<G>);
    constructor(opts: AppOptions<G>);
    /**
     * Install the served root's once-per-request middleware: compress, the
     * async-context bind, the request logger, the plugin chain (binds the per-request
     * DB/tenant/principal), and HTTP error mapping for plain routes.
     *
     * A ROOT concern, installed ONCE by `executeConfig` (boot) — BEFORE `realize()`
     * mounts the child tree, so the middleware precedes the routes in Hono's order.
     * Children are never given a pipeline (they're only mounted by `realize`), so
     * there's nothing to duplicate and nothing to strip. Idempotent (boot runs it for
     * both the 'first' and 'last' plugin passes).
     */
    installBasePipeline(): void;
    /**
     * Mount this instance's composed child tree (recorded by `compose`) onto its Hono
     * routes. Depth-first — a child realizes its OWN children before being mounted, so
     * a nested compose (`root.compose(sub)`, `sub = new Pylon().compose(leaf)`) mounts
     * the whole tree. Each child mounts at its `basePath` (default `/`). Idempotent.
     *
     * Called once by boot (`executeConfig`) AFTER `installBasePipeline`, so the
     * base-pipeline middleware is registered before any route it must wrap. Children
     * never install a pipeline (only `realize` touches them), so mounting them can't
     * duplicate it.
     */
    realize(): void;
}
export declare const app: App<{}>;
/**
 * Back-compat alias for the default instance's plugin middleware array. Existing
 * imports of `pluginsMiddleware` keep working (it's the same array `app` composes).
 */
export declare const pluginsMiddleware: MiddlewareHandler[];
export {};
