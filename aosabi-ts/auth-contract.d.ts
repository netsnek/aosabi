// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * auth-contract.d.ts: the types of the entry aosabi/auth/contract of the package, the auth
 * contract of lib/auth-contract.js.
 *
 * Derived from Pylon 3.0.0-canary by Nico Schett, its source, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The declarations are Pylon's, from the declaration files of the subpath auth/contract under
 * types/pylon/auth/, as vendored literals. This file re-exports the names lib/auth-contract.js
 * exports and the types of their shapes.
 */

export { PRINCIPAL_KEY, hasRole, hasPermission, ForbiddenError } from './types/pylon/auth/contract.js'

export type { Principal, IdentityProvider } from './types/pylon/auth/contract.js'
