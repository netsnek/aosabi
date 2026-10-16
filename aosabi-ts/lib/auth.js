// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * auth.js: the entry aosabi/auth of the package aosabi, the gates of the
 * capability tier on the principal of a request.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/index.ts and
 * packages/pylon/src/auth/authz.ts, Apache-2.0. Changed by the Aosabi authors.
 *
 * The functions as the build of the reference writes them. The principal is the value
 * the provider of the identity plugin answered, which its middleware set on
 * the context c of the request, so getPrincipal() answers that value as the
 * app made it, and a gate that refuses throws the ForbiddenError of the
 * package, whose class the app imports from this entry.
 */
import { getContext } from "./pylon.js";
import { ForbiddenError, hasPermission, hasRole, PRINCIPAL_KEY } from "./auth-contract.js";

export { ForbiddenError, hasPermission, hasRole };

// The principal of the running request, which the identity plugin bound, or
// undefined outside a request and where none is bound.
export function getPrincipal() {
  try {
    return getContext().get(PRINCIPAL_KEY);
  } catch {
    return;
  }
}

// Throws a ForbiddenError unless `check` accepts the principal of the running
// request.
export function authorize(check) {
  if (!check(getPrincipal())) throw new ForbiddenError();
}

// Throws a ForbiddenError unless the principal of the running request holds
// any of the roles `roles`, so a call with no role refuses every caller.
export function requireRole(...roles) {
  authorize((p) => hasRole(p, ...roles));
}
