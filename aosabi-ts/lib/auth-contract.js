// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * auth-contract.js: the entry aosabi/auth/contract of the package aosabi, the
 * auth contract with no dependency.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/contract.ts
 * and packages/pylon/src/auth/principal.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The functions as the build of the reference writes them, with the class
 * ForbiddenError of lib/errors.js, whose brand the engine reads, so the map of
 * the identity plugin finds the error a gate of the app throws.
 */

// The key of the context the bound principal lives under, which a layer below
// the web runtime reads without the entry of the gates.
export const PRINCIPAL_KEY = "principal";

// Answers whether the principal `p` holds any of the roles `roles`, false for
// no principal.
export function hasRole(p, ...roles) {
  return !!p?.roles && roles.some((r) => p.roles.includes(r));
}

// Answers whether the principal `p` holds any of the permissions
// `permissions`, false for no principal.
export function hasPermission(p, ...permissions) {
  return !!p?.permissions && permissions.some((x) => p.permissions.includes(x));
}

export { ForbiddenError } from "./errors.js";
