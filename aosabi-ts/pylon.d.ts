// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * pylon.d.ts: the types of the entry aosabi/pylon of the package, Pylon's names and shapes under
 * Aosabi's own names for the names of lib/pylon.js.
 *
 * Derived from Pylon 3.0.0-canary by Nico Schett, its source, Apache-2.0.
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
import type { AppConfig as PylonAppConfig, Env, Plugin } from './types/pylon/core/index.js'

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

// The names of graphql-yoga an app imports beside Pylon, which the root aosabi re-exports, where aosabi
// migrate points the specifier graphql-yoga. The class Repeater comes from the declaration file of
// @repeaterjs/repeater 3.0.6 under types/@repeaterjs/repeater/, a vendored literal of
// @repeaterjs/repeater, MIT, so the checker of the step aosabi build finds the type a subscription of an
// app annotates in a file whose name holds @repeaterjs, the test of Pylon's inference for a stream, and
// the values createGraphQLError and createPubSub and the type YogaInitialContext, the context Yoga hands
// a plugin, are re-exported from graphql-yoga by its bare specifier, as the declarations of Pylon under
// types/pylon/ import it.
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
  ID,
  Int,
  Float,
  FieldPolicy,
  PatchPolicy,
} from './types/pylon/core/index.js'

/**
 * The configuration of an app, the keys of the AppConfig of types/pylon/core and maxTokens, a key of
 * Aosabi: the cap on the tokens of a request document, which every parse of a request document of
 * the app takes, 120,008 where the configuration leaves it out. A document of more tokens answers
 * the syntax error the parser of graphql-js answers under a maxTokens of the same value. The cap is a
 * whole number from 0 to 4294967295, and any other value fails the boot of the app, null among them,
 * since no value lifts the cap.
 */
export type AppConfig = PylonAppConfig & {
  maxTokens?: number
}

/** A factory of the configuration, which may answer a promise of it. */
export type AppConfigFactory = () => AppConfig | Promise<AppConfig>

/** Answers the configuration or the factory of one as it is, for aosabi.config.ts. */
export declare function defineConfig(config: AppConfig | AppConfigFactory): AppConfig | AppConfigFactory

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

/**
 * The plugin of Pylon's canary that sets the auth state of a request and mounts the routes under
 * `endpoint`. The key `cookieName` names the session cookie its middleware reads and its logout deletes,
 * `aosabi-auth` where the options name none or the empty name, as the options of aosabi/auth/zitadel name
 * it. A name outside the token that RFC 6265 takes as a cookie name throws a TypeError that names the key
 * when useAuth is called, and the logout deletes a name with the prefix `__Secure-` or `__Host-` with
 * `Secure`.
 */
export declare function useAuth(args: {
  issuer: string
  endpoint?: string
  keyPath?: string
  cookieName?: string
}): Plugin
