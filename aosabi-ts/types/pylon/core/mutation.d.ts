export interface UserError {
    /** Path to the offending input field (e.g. `['address','zip']`). */
    field: string[];
    message: string;
    /** Stable machine code (validation code, or a business code). */
    code: string;
}
/**
 * Wrap a mutation resolver so user-facing errors surface as `userErrors`. The
 * payload entity is optional (null on failure); `userErrors` is always present.
 */
export declare function mutation<A extends any[], R>(fn: (...args: A) => R | Promise<R>): (...args: A) => Promise<{
    [K in keyof Awaited<R>]: Awaited<R>[K] | null;
} & {
    userErrors: UserError[];
}>;
