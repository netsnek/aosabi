import { Context as HonoContext } from 'hono';
import type { Toucan } from 'toucan-js';
import { AsyncLocalStorage } from 'async_hooks';
import type { GraphQLResolveInfo } from 'graphql';
export interface Bindings {
    NODE_ENV: string;
}
export interface Variables {
    sentry: Toucan;
    graphqlResolveInfo?: GraphQLResolveInfo;
}
export type Env = {
    Bindings: Bindings;
    Variables: Variables;
};
export type Context = HonoContext<Env, string, {}>;
export declare const asyncContext: AsyncLocalStorage<Context>;
export declare const getContext: () => Context;
export declare const setContext: (context: Context) => void;
