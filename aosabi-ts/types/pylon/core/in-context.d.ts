/** SDL for the directive. Appended to the emitted schema whenever the app has resolvers. */
export declare const IN_CONTEXT_SDL = "directive @inContext(\n  \"\"\"The locale for this operation, e.g. `en` or `de-AT`.\"\"\"\n  locale: String\n  \"\"\"Per-operation context, as JSON (an app-typed `OperationContext` bag). Carried as a\n  variable so it folds into the client cache key. INERT until the server acts on it \u2014 e.g.\n  `useDatabase({operationContext})` honouring an acting tenant; a bare value grants nothing.\"\"\"\n  context: String\n) on QUERY | MUTATION | SUBSCRIPTION";
/** Hono context key holding the operation's resolved context. */
export declare const IN_CONTEXT_KEY = "pylonInContext";
/**
 * Per-operation context an app carries on a call and reads on the server. Extend it by
 * declaration merging — NO compiler change per key:
 *
 * ```ts
 * declare module '@getcronit/pylon' {
 *   interface OperationContext { previewMode?: boolean }
 * }
 * ```
 *
 * `actingTenant` ships as the flagship key: per-operation tenant impersonation, gated by
 * `useDatabase({operationContext})` (rfcs/ACTING_TENANT.md). Like every key here it is an
 * UNGATED request — it grants nothing until the server acts on it.
 */
export interface OperationContext {
    actingTenant?: string;
}
export interface InContext {
    locale?: string;
    /** The operation's `@inContext(context:)` bag, parsed from its JSON. See OperationContext. */
    context?: OperationContext;
}
/**
 * The locale this operation asked for, or `undefined` when it asked for none.
 *
 * `undefined` is meaningful and should not be papered over with a default: it means the
 * caller did not state a locale, so a resolver should serve whatever it considers neutral
 * rather than guess.
 */
export declare const getLocale: () => string | undefined;
/** Everything `@inContext` carried for this operation. */
export declare const getInContext: () => InContext;
