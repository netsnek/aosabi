// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * zitadel.js: the entry aosabi/auth/zitadel of the package aosabi, the Zitadel
 * integration: zitadelPrincipal,
 * zitadelAuth and zitadelLogin over the crate aosabi-auth-zitadel of the
 * engine.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/zitadel.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The reading of the token, the bootstrap of the issuer with its cache, the
 * discovery, the introspection, the user info and the roles run in the engine,
 * in the OIDC client of the crate, whose outbound requests go through the
 * fetch of the host and whose client assertion the crypto of the host signs,
 * so the module and the native library carry no client and no crypto stack of
 * their own. The mapping of the claims to the principal, zitadelPrincipal,
 * runs here, since its mappers are functions of the app.
 */
import { context, coreOf } from "./core.js";

// The pure mapping of the claims to the principal, zitadelPrincipal of
// zitadel.ts:118-131, exported for testing without an OIDC server.
export function zitadelPrincipal(sub, roles, user, options) {
  return {
    id: options.id?.(user) ?? sub,
    tenant: options.tenant?.(user),
    roles,
    permissions: options.permissions?.(user) ?? [],
    attributes: options.attributes?.(user) ?? user,
  };
}

// The identity provider of zitadel.ts:133-149: a function of the context c of
// a request that answers its principal, or undefined for a request with no
// token, an inactive token or one with no sub. The engine reads the token of
// the header Authorization, of the query token or of the cookie aosabi-auth,
// introspects it with the scope openid email profile and fetches the user
// info for its sub, and answers the sub, the roles of the claim
// urn:zitadel:iam:org:projects:roles and the user info, which the provider
// maps through zitadelPrincipal and the mappers of `options`. The provider
// holds no state beyond its options: the engine keeps one provider
// of its own per issuer and key path, which the bootstrap of the issuer
// shares, so providers answer in any order they are made and called in,
// crates/aosabi-js/src/zitadel.rs. The provider asks the core of the app the
// bind of the async context of the base pipeline put in the store of the
// request, or the core whose base pipeline saw c where the call runs outside
// that store, from the stream of a body after the answer, on the request of
// c, which the core opens in the engine where no step opened it before.
export function zitadelAuth(options) {
  const keyPath = options.keyPath ?? "key.json";
  return async (c) => {
    const core = context.getStore()?.core ?? coreOf(c);
    if (core === undefined) {
      throw new TypeError("a provider of aosabi/auth/zitadel runs behind the base pipeline of an app of aosabi/pylon");
    }
    const claims = await core.claims(c, options.issuer, keyPath);
    if (claims === undefined) return undefined;
    return zitadelPrincipal(claims.sub, claims.roles, claims.user, options);
  };
}

// The plugin of the browser login routes of zitadel.ts:151-202 as a marker,
// which the boot of the engine replaces by the plugin of aosabi-auth-zitadel
// with the issuer, the key path and the endpoint of `options`, whose setup
// registers GET <endpoint>/login, GET <endpoint>/logout and GET
// <endpoint>/callback, the endpoint /auth by default, on the app of the
// engine, and whose setup in the pass of executeConfig of aosabi/pylon
// registers the same three routes on the hono of the app, each answered by
// the engine.
export function zitadelLogin(options) {
  return {
    marker: "zitadelLogin",
    options: { issuer: options.issuer, keyPath: options.keyPath, endpoint: options.endpoint },
  };
}
