// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * pylon.d.ts: the types of the entry aosabi/pylon of the package, Pylon's names and shapes under
 * Aosabi's own names for the names of lib/pylon.js.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, its source, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * Every declaration is Pylon's. This file re-exports the names lib/pylon.js exports and the types of
 * their shapes, and no other name, so a name whose engine item is missing stays undeclared here:
 * asyncContext, renderLine, jobLogLevel and IN_CONTEXT_SDL. The cookie helpers are hono's own, the
 * signed forms among them, which Pylon's root re-exports from hono/cookie. The declaration of
 * experimentalCreatePubSub is the one of Pylon's root, createPubSub of graphql-yoga, whose topics
 * the engine keeps in its own pub sub.
 *
 * The three names requireAuth, authMiddleware and useAuth of the root of Pylon's canary
 * 3.0.0-canary-20260206115650, which lib/canary.js exports for an app that build makes, are
 * declared at the end of this file in the package's own words after the declarations of the
 * canary's dist/plugins/use-auth/auth-require.d.ts and use-auth.d.ts, which no tree of Aosabi
 * vendors (docs/ts1-surface.md of the Aosabi repository, section 11).
 */

import type { MiddlewareHandler } from 'hono'
import type { Env, Plugin } from './types/pylon/core/index.js'

export {
  App,
  app,
  executeConfig,
  handler,
  currentRole,
  getContext,
  setContext,
  getEnv,
  getResolveInfo,
  getLocale,
  getInContext,
  createDecorator,
  mutation,
  ServiceError,
  getLogger,
  logger,
  runWithLogger,
  getRootLogger,
  createLogger,
  useNodeServer,
  useSentry,
  defineConfig,
  getCookie,
  getSignedCookie,
  setCookie,
  setSignedCookie,
  deleteCookie,
  experimentalCreatePubSub,
  createGateway,
  pass,
  passthrough,
} from './types/pylon/core/index.js'

// The names of graphql-yoga an app imports beside Pylon, which the root aosabi re-exports, where
// aosabi migrate points the specifier graphql-yoga. The class Repeater comes from the declaration
// file of @repeaterjs/repeater 3.0.6 under types/@repeaterjs/repeater/, which tools/provenance
// writes as a vendored literal of @repeaterjs/repeater, MIT, so the checker of the step aosabi build
// finds the type a subscription of an app annotates in a file whose name holds @repeaterjs, the test
// of Pylon's inference for a stream, and the values createGraphQLError and createPubSub and the type
// YogaInitialContext, the context Yoga hands a plugin, are re-exported from graphql-yoga by its bare
// specifier, as the declarations of Pylon under types/pylon/ import it.
export { Repeater } from './types/@repeaterjs/repeater/repeater.js'
export { createGraphQLError, createPubSub } from 'graphql-yoga'
export type { YogaInitialContext } from 'graphql-yoga'

export type {
  Bindings,
  Context,
  Env,
  Variables,
  Logger,
  LogLevel,
  LogFields,
  LogRecord,
  LogSink,
  LoggerConfig,
  UserError,
  InContext,
  OperationContext,
  NodeServerOptions,
  SentryPluginOptions,
  Gate,
  Resolvers,
  AppOptions,
  Role,
  PluginRole,
  Plugin,
  AppConfig,
  AppConfigFactory,
  ID,
  Int,
  Float,
  FieldPolicy,
  PatchPolicy,
} from './types/pylon/core/index.js'

/** The checks of requireAuth and authMiddleware of Pylon's canary, the roles a user must hold one of. */
export type AuthRequireChecks = {
  roles?: string[]
}

/** The middleware of Pylon's canary that checks the auth state of the context against `checks`. */
export declare const authMiddleware: (checks?: AuthRequireChecks) => MiddlewareHandler<Env>

/** The decorator of Pylon's canary that runs authMiddleware over the context of the running request. */
export declare function requireAuth(checks?: AuthRequireChecks): {
  <T extends (...args: any[]) => any>(target: Object, propertyKey: string | symbol): void
  <T>(fn: T): T
}

/** The plugin of Pylon's canary that sets the auth state of a request and mounts the routes under `endpoint`. */
export declare function useAuth(args: {issuer: string; endpoint?: string; keyPath?: string}): Plugin
