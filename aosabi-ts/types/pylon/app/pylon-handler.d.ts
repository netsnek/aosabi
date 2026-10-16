import { MiddlewareHandler } from 'hono';
import { App } from '.';
import { AppConfig, Role } from '../core';
interface AppHandlerOptions {
    graphql: {
        Query: Record<string, any>;
        Mutation?: Record<string, any>;
        Subscription?: Record<string, any>;
    };
    config?: AppConfig;
}
/**
 * Strip a Shopify-style global id (`gid://namespace/Type/localId`) back to its
 * raw local id; any non-gid value (a raw id, a number) passes through untouched,
 * so a client may hand back either form. This is the SINGLE place a gid is
 * decoded: the `ID` scalar below runs it on every `ID`-typed input (primary
 * keys, foreign keys, `node(id)`), so the ORM never sees a gid and needs no
 * per-write-path decoding. Output encoding is the symmetric per-type `id` field
 * resolver the compiler attaches. Decode is intentionally lenient — a scalar
 * can't know a field's expected model type, and globally-unique ids (snowflake,
 * cuid, uuid) make a wrong-type gid a harmless "not found" rather than a
 * cross-table collision.
 */
export declare const decodeGidInput: (value: unknown) => unknown;
/**
 * Merge two resolver maps ONE level deep: for a type present in both, combine its
 * field resolvers (`b` wins on conflict) rather than letting `b`'s type object
 * replace `a`'s wholesale. Lets build-side additions (interface `__resolveType`,
 * the `Node` layer's `Query.node` + per-type `id`) coexist with the app's own
 * `Query`/`Mutation`/entity resolvers instead of clobbering them.
 */
export declare const mergeResolverMaps: (a?: Record<string, any>, b?: Record<string, any>) => Record<string, any>;
/**
 * The current process run-role, from `PYLON_ROLE` (default `web`). `server.mjs` leaves it
 * unset (→ web, or `all` if the operator opts a single process into consuming too); the
 * generated `worker.mjs` and `pylon dev --worker` set `worker`.
 */
export declare const currentRole: () => Role;
export declare const executeConfig: (config: AppConfig, args?: {
    pluginsStrategy?: "first" | "last";
}, target?: App<any>) => Promise<void>;
export declare const handler: (options: AppHandlerOptions, target?: App<any>) => MiddlewareHandler;
export {};
