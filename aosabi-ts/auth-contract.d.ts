// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * auth-contract.d.ts: the types of the entry aosabi/auth/contract of the package, the auth
 * contract of lib/auth-contract.js.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, its source, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * This file re-exports the names lib/auth-contract.js exports and the types of their shapes.
 */

export { PRINCIPAL_KEY, hasRole, hasPermission, ForbiddenError } from './types/pylon/auth/contract.js'

export type { Principal, IdentityProvider } from './types/pylon/auth/contract.js'
