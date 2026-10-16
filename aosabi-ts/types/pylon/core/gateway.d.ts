import { Transform } from '@graphql-tools/delegate';
import type { Context } from './context';
type Primitive = string | number | boolean | symbol | bigint | undefined | null | Date;
/**
 * Recursively maps a GraphQL return type to a selection map for the `needs` object.
 * Allows boolean flags for selection and a special `__args` property for nested arguments.
 */
export type NeedsMap<T> = T extends Primitive ? boolean : T extends (...args: any[]) => infer Ret ? NeedsMap<Awaited<Ret>> : T extends Array<infer U> ? NeedsMap<U> : {
    [K in keyof T]?: NeedsMap<T[K]> | boolean;
} & {
    __args?: Record<string, any>;
};
/**
 * The fields a `needs` selection actually fetched, as a type.
 *
 * `NeedsMap` describes what you MAY ask for; this is the mirror — what you asked
 * for, so it can be read back. Without it `needs` is invisible to the checker:
 * the fields are fetched at runtime, are usually (deliberately) absent from the
 * patched type, and the only way to reach them is an unchecked cast that
 * re-states the selection by hand.
 *
 * Deliberately shallow-ish and forgiving: anything it cannot resolve degrades to
 * `unknown` rather than widening the whole result, so a `needs` shape it does not
 * understand never makes the patched half of the return worse.
 */
type NeedsResult<N, T> = T extends (...args: any[]) => infer R ? NeedsResult<N, Awaited<R>> : T extends Array<infer U> ? Array<NeedsResult<N, U>> : N extends true ? T : N extends object ? {
    [K in Exclude<keyof N, '__args'> & keyof NonNullable<T> as N[K] extends false | undefined ? never : K]: N[K] extends true ? NeedsField<NonNullable<T>[K]> : NeedsResult<N[K], NonNullable<T>[K]>;
} : unknown;
/** A leaf `needs: true` — unwrap a callable field to what it returns. */
type NeedsField<T> = T extends (...args: any[]) => infer R ? Awaited<R> : T;
/**
 * Maps over the properties of the resolved type within the registry.
 * Intercepting the lookup at the property level mitigates TS2589
 * (Type instantiation is excessively deep and possibly infinite) during recursive type inference.
 */
type MapValue<V, P, R> = V extends Primitive ? V : V extends Array<infer U> ? Array<PatchSchema<U, P, R>> : V extends {
    __typename: infer Name;
} ? Name extends keyof P ? P[Name & keyof P] extends (...args: any) => infer Res ? {
    [Prop in keyof Res]: PatchSchema<Res[Prop], P, R>;
} : never : {
    [Prop in keyof V]: PatchSchema<V[Prop], P, R>;
} : {
    [Prop in keyof V]: PatchSchema<V[Prop], P, R>;
};
type MappedRegistry<R, P> = {
    [K in keyof R]: MapValue<R[K], P, R>;
};
type PatchSchema<T, P, R> = T extends Primitive ? T : T extends (...args: infer Args) => infer Ret ? (...args: Args) => Ret extends Promise<any> ? Promise<PatchSchema<Awaited<Ret>, P, R>> : PatchSchema<Awaited<Ret>, P, R> : T extends Array<infer U> ? Array<PatchSchema<U, P, R>> : T extends {
    __typename: infer Name;
} ? Name extends keyof R ? MappedRegistry<R, P>[Name & keyof R] : {
    [K in keyof T]: PatchSchema<T[K], P, R>;
} : T extends object ? {
    [K in keyof T]: PatchSchema<T[K], P, R>;
} : T;
/**
 * The remote schema, introspected once per URL.
 *
 * The cache holds the PROMISE, which is what makes eviction on failure
 * essential: without it a single rejected introspection — the remote being down
 * when the first request happens to arrive — is replayed to every later request
 * for the life of the process, so the gateway never recovers from a remote
 * restart and reports a connection error for a remote that is demonstrably up.
 *
 * The delete is guarded on identity so a late rejection from a superseded
 * attempt cannot evict the entry a newer one already installed.
 */
export declare function getRemoteSchema(url: string, makeExecutor: () => any): Promise<any>;
/** Test seam: drop every cached schema. */
export declare function __resetSchemaCache(): void;
export interface GatewayContext<TRegistry extends {
    delegate: any;
    types: any;
}> {
    delegate: <K extends keyof TRegistry['delegate'], TNeeds extends NeedsMap<TRegistry['delegate'][K]['return']> = NeedsMap<TRegistry['delegate'][K]['return']>>(key: K, ...opts: {} extends TRegistry['delegate'][K]['args'] ? [options?: {
        args?: TRegistry['delegate'][K]['args'];
        needs?: TNeeds;
    }] : [options: {
        args: TRegistry['delegate'][K]['args'];
        needs?: TNeeds;
    }]) => Promise<TRegistry['delegate'][K]['return']>;
}
export declare class InlineArgsTransform implements Transform {
    private wrapperArgs;
    constructor(wrapperArgs: Record<string, any>);
    transformRequest(originalRequest: any, delegationContext: any): any;
}
/**
 * The argument policy for one field of a patched type.
 *
 * `args` is an ALLOWLIST: name the arguments a caller may set. Anything else is
 * rejected, so an argument the remote adds later is denied by default — the same
 * rule fields already follow. Omit it to allow everything.
 *
 * `force` is applied to the outgoing request. It is a constraint, not a default,
 * and a caller may NOT supply a forced argument — the value could only ever be
 * overridden, and silently discarding what they sent is the failure this exists
 * to remove. Values are constants, or `(ctx) => value` for per-request ones.
 *
 * `force` and `args` are therefore disjoint: `args` is what the caller may set,
 * `force` is what the gateway sets.
 */
export interface FieldPolicy {
    args?: readonly string[];
    force?: Record<string, unknown | ((ctx: any) => unknown)>;
}
/** Per-field policies for one patched type, keyed by field name. */
export type PatchPolicy = Record<string, FieldPolicy>;
/**
 * Attach an argument policy to a patch.
 *
 * A patch transforms the RESULT of a delegated field, so on its own it cannot
 * constrain what was ASKED FOR: by the time it runs, the caller's arguments have
 * already reached the remote. A filter applied in one resolver therefore does not
 * apply to the same rows reached through a nested field.
 *
 *     patches: {
 *       ProductCollection: pass(
 *         c => ({handle: c.handle, name: c.name, products: c.products}),
 *         {
 *           products: {
 *             args: ['first', 'last', 'after', 'before', 'skip'],
 *             force: {query: 'status:ACTIVE published:true'}
 *           }
 *         }
 *       )
 *     }
 *
 * The type name comes from the patch's own key, so it is never repeated. Root
 * fields need nothing here: a delegated root field is called from your own
 * resolver, which already decides its arguments.
 *
 * The patch is returned unchanged — its signature, and so the schema it
 * generates, is untouched.
 */
export declare function pass<D, A, R>(patch: (data: D, api: A) => R, policy: PatchPolicy): (data: D, api: A) => R;
/**
 * Applies field policies to the outgoing request.
 *
 * Two jobs, both impossible from a result-side patch:
 *
 *   - REJECT an argument outside the allowlist, so a caller cannot reach a knob
 *     the boundary never granted — including one the remote added after this
 *     gateway was written;
 *   - FORCE an argument, overriding whatever the caller sent.
 *
 * Both rewrite the document that goes upstream, so the nested selection still
 * travels inside its parent's single request and nothing here costs a round trip.
 *
 * `InlineArgsTransform` cannot do this job: it matches only the root field
 * (`delegationContext.fieldName`), which is why nested arguments are otherwise
 * out of reach.
 */
export declare class ForceArgsTransform implements Transform {
    private policies;
    private ctx;
    constructor(policies: Map<string, FieldPolicy> | undefined, ctx: any);
    transformRequest(originalRequest: any, delegationContext: any): any;
}
export declare class PatchTransform<TPatch> implements Transform {
    private patches;
    private api;
    private strict;
    constructor(patches: TPatch, api: any, strict?: boolean);
    transformRequest(originalRequest: any): any;
    transformResult(originalResult: any): any;
    private applyTransforms;
}
declare class Gateway<TRegistry extends {
    delegate: any;
    types: any;
}, TPatch extends Record<string, (data: any, api: any) => any>> {
    private config;
    private apiContext;
    constructor(config: {
        url: string;
        headers?: (ctx: any) => Record<string, string>;
        patches?: TPatch;
        strict?: boolean;
    });
    private policies;
    /**
     * Delegate with a guard — the result is `null` when the guard rejects it.
     *
     * The guard's argument is typed from `needs`, which is the only way to read
     * back a field you fetched purely to decide with. Intersecting those fields
     * into the RETURN type instead does not work: an intersection is a
     * structurally new type, so the schema builder mints a second `Org_1`
     * alongside `Org` and then rejects the duplicate. Keeping them inside the
     * guard leaves the returned type — the one the schema is generated from —
     * exactly as it was.
     */
    delegate<K extends keyof TRegistry['delegate'], TNeeds extends NeedsMap<TRegistry['delegate'][K]['return']> = NeedsMap<TRegistry['delegate'][K]['return']>>(key: K, options: {
        args?: TRegistry['delegate'][K]['args'];
        needs?: TNeeds;
        guard: (data: NeedsResult<TNeeds, TRegistry['delegate'][K]['return']>) => boolean;
    }): Promise<PatchSchema<TRegistry['delegate'][K]['return'], TPatch, TRegistry['types']> | null>;
    delegate<K extends keyof TRegistry['delegate'], TNeeds extends NeedsMap<TRegistry['delegate'][K]['return']> = NeedsMap<TRegistry['delegate'][K]['return']>>(key: K, ...opts: {} extends TRegistry['delegate'][K]['args'] ? [options?: {
        args?: TRegistry['delegate'][K]['args'];
        needs?: TNeeds;
    }] : [options: {
        args: TRegistry['delegate'][K]['args'];
        needs?: TNeeds;
    }]): Promise<PatchSchema<TRegistry['delegate'][K]['return'], TPatch, TRegistry['types']>>;
}
/**
 * Instantiates a factory for configuring a strongly-typed PylonGateway.
 *
 * Utilizes a generated type registry (`TRegistry`) mapping remote GraphQL typenames
 * to TypeScript interfaces, guaranteeing end-to-end type safety for configurations,
 * schema patches, and delegated execution payloads.
 *
 * @template TRegistry - Type map correlating GraphQL typenames to local TypeScript definitions.
 * @returns Gateway configuration interface.
 *
 * @example
 *
 * // Provision remote schema via CLI: pylon pull <url> -n <name>
 *
 * ```typescript
 * import { createGateway } from '@getcronit/pylon';
 * import { RemoteRegistry } from '../generated/remote';
 *
 * const gateway = createGateway<RemoteRegistry>().configure({
 * url: '[https://api.example.com/graphql](https://api.example.com/graphql)',
 * headers: (ctx) => ({ Authorization: ctx.token }),
 * patches: {
 * User: (data, api) => ({ ...data, fullName: `${data.firstName} ${data.lastName}` })
 * }
 * });
 * ```
 */
/**
 * Marks a remote type as deliberately exposed in full.
 *
 * Only meaningful under `strict`, where an unpatched type is an error. Using
 * this says "every field of this type, now and as the remote adds them, is
 * public" — which is a real decision for a leaf like `Money`, and a mistake for
 * anything else.
 */
export declare function passthrough<T>(): (data: T) => T;
export declare function createGateway<TRegistry extends {
    delegate: any;
    types: any;
}>(): {
    configure: <TPatch extends { [K in keyof TPatch]: K extends keyof TRegistry["types"] ? (data: TRegistry["types"][K], api: GatewayContext<TRegistry>) => any : never; }>(config: {
        url: string;
        headers?: (ctx: Context) => Record<string, string>;
        patches?: TPatch;
        /**
         * Fail on a remote type that has no patch, instead of publishing it whole.
         *
         * Off by default: turning it on removes types an existing gateway is
         * already serving, which is the point, but it is a breaking change to
         * adopt. Types that genuinely should pass through say so with
         * `passthrough()`, so it reads as a decision rather than an omission.
         */
        strict?: boolean;
    }) => Gateway<TRegistry, TPatch>;
};
export {};
