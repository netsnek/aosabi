// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * auth-plugin.js: the entry aosabi/auth/plugin of the package aosabi, the
 * identity plugin useIdentity.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/plugin.ts
 * and packages/pylon/src/auth/authz.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */
import { ENGINE_HOOK } from "./core.js";
import { PRINCIPAL_KEY } from "./auth-contract.js";

// The hook onExecute of every identity plugin, whose member ENGINE_HOOK names
// it the map of the engine, so the boot record carries no function of it. A
// call of the app answers the hooks with no done hook, since the map runs in
// the engine over the errors the engine holds.
function onExecute() {
  return {};
}
Object.defineProperty(onExecute, ENGINE_HOOK, { value: "useIdentity" });

// Returns the identity plugin over the provider `provider` of the app, a
// function of the context c that answers the principal of the request, or
// undefined for public access, or a promise of either.
export function useIdentity(provider) {
  return {
    name: "identity",
    async middleware(c, next) {
      c.set(PRINCIPAL_KEY, await provider(c));
      await next();
    },
    onExecute,
  };
}
