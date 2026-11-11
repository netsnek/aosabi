// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * zitadel.d.ts: the types of the entry aosabi/auth/zitadel of the package, the three exports of
 * lib/zitadel.js.
 *
 * Derived from Pylon 3.0.0-canary by Nico Schett,
 * packages/pylon/src/auth/zitadel.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The options keep the shapes and the names of the options of Pylon's subpath auth/zitadel, so an app
 * that imports them keeps its types, and they add the key cookieName, the name of the session cookie
 * the callback of zitadelLogin sets and zitadelAuth reads, aosabi-auth where the options name none.
 * The principal and the provider are the types of the entry aosabi/auth and the plugin the type of
 * the root.
 */

import type { IdentityProvider, Principal } from './types/pylon/auth/index.js'
import type { Plugin } from './types/pylon/core/index.js'

/** The user info the issuer answers for a token, its claims by name. */
export interface OidcUser {
  readonly sub: string
  readonly [claim: string]: unknown
}

/** The options both halves take. */
export interface ZitadelOptions {
  /** The URL of the issuer, such as `https://acme.zitadel.cloud`. */
  issuer: string
  /**
   * The key file of the service account, a PKCS#1 PEM in JSON, `key.json` where the options name
   * none, which the variable AUTH_KEY replaces where it is not empty.
   */
  keyPath?: string
  /**
   * The name of the session cookie, which the callback of zitadelLogin sets, its logout deletes and
   * zitadelAuth reads, `aosabi-auth` where the options name none or the empty name. An app hands
   * both halves the same name. A name outside the token that RFC 6265 takes as a cookie name throws
   * a TypeError that names the option when zitadelAuth or zitadelLogin is called. A name with the
   * prefix `__Secure-` or `__Host-` is deleted with `Secure` and needs https at the callback, its
   * URL of `https:` or a first `x-forwarded-proto` of `https`.
   */
  cookieName?: string
}

/** The options of zitadelAuth, those of both halves with the four mappers of the claims. */
export interface ZitadelAuthOptions extends ZitadelOptions {
  /** The id of the principal, the `sub` of the token where the options name no mapper. */
  id?: (user: OidcUser) => string | number
  /** The tenant of the principal, such as the organisation, none where the options name no mapper. */
  tenant?: (user: OidcUser) => string | number | undefined
  /** The permissions of the principal, where the claims carry them, none by default. */
  permissions?: (user: OidcUser) => readonly string[]
  /** The attributes of the principal for its rules, every claim by default. */
  attributes?: (user: OidcUser) => Record<string, unknown>
}

/** Maps the claims of a token to its principal, the mappers of `options` over them. */
export declare function zitadelPrincipal(
  sub: string,
  roles: string[],
  user: OidcUser,
  options: ZitadelAuthOptions,
): Principal

/**
 * Returns the identity provider that reads the token of a request and answers its principal.
 */
export declare function zitadelAuth(options: ZitadelAuthOptions): IdentityProvider

/**
 * Returns the plugin of the routes of the login of a browser, `<endpoint>/login`,
 * `<endpoint>/logout` and `<endpoint>/callback`, under the endpoint `/auth` where the options name
 * none.
 */
export declare function zitadelLogin(options: ZitadelOptions & { endpoint?: string }): Plugin
