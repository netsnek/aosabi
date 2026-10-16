import { isOriginalGraphQLError, TypedExecutionArgs } from '@envelop/core';
import * as Sentry from '@sentry/node';
import type { TraceparentData } from '@sentry/types';
import { sentry } from '@hono/sentry';
import { Plugin } from '../core';
/** Options accepted by the @hono/sentry HTTP middleware (dsn, environment, …). */
type SentryMiddlewareOptions = NonNullable<Parameters<typeof sentry>[0]>;
export type SentryPluginOptions<PluginContext extends Record<string, any>> = {
    /**
     * Starts a new transaction for every GraphQL Operation.
     * When disabled, an already existing Transaction will be used.
     *
     * @default true
     */
    startTransaction?: boolean;
    /**
     * Renames Transaction.
     * @default false
     */
    renameTransaction?: boolean;
    /**
     * Adds result of each resolver and operation to Span's data (available under "result")
     * @default false
     */
    includeRawResult?: boolean;
    /**
     * Adds operation's variables to a Scope (only in case of errors)
     * @default false
     */
    includeExecuteVariables?: boolean;
    /**
     * The key of the event id in the error's extension. `null` to disable.
     * @default sentryEventId
     */
    eventIdKey?: string | null;
    /**
     * Adds custom tags to every Transaction.
     */
    appendTags?: (args: TypedExecutionArgs<PluginContext>) => Record<string, unknown>;
    /**
     * Callback to set context information onto the scope.
     */
    configureScope?: (args: TypedExecutionArgs<PluginContext>, scope: Sentry.Scope) => void;
    /**
     * Produces a name of Transaction (only when "renameTransaction" or "startTransaction" are enabled) and description of created Span.
     *
     * @default operation's name or "Anonymous Operation" when missing)
     */
    transactionName?: (args: TypedExecutionArgs<PluginContext>) => string;
    /**
     * Produces tracing data for Transaction
     *
     * @default is empty
     */
    traceparentData?: (args: TypedExecutionArgs<PluginContext>) => TraceparentData | undefined;
    /**
     * Produces a "op" (operation) of created Span.
     *
     * @default execute
     */
    operationName?: (args: TypedExecutionArgs<PluginContext>) => string;
    /**
     * Indicates whether or not to skip the entire Sentry flow for given GraphQL operation.
     * By default, no operations are skipped.
     */
    skip?: (args: TypedExecutionArgs<PluginContext>) => boolean;
    /**
     * Indicates whether or not to skip Sentry exception reporting for a given error.
     * By default, this plugin skips all `GraphQLError` errors and does not report it to Sentry.
     */
    skipError?: (args: Error) => boolean;
} & Partial<SentryMiddlewareOptions>;
export declare const defaultSkipError: typeof isOriginalGraphQLError;
export declare const useSentry: <PluginContext extends Record<string, any> = {}>(options?: SentryPluginOptions<PluginContext>) => Plugin<PluginContext>;
export {};
