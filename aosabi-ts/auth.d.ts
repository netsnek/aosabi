// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * auth.d.ts: the types of the entry aosabi/auth of the package, the gates and the helpers of
 * lib/auth.js.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, its source, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * This file re-exports the names lib/auth.js exports and the types of their shapes.
 */

export {
  ForbiddenError,
  hasPermission,
  hasRole,
  getPrincipal,
  authorize,
  requireRole,
} from './types/pylon/auth/index.js'

export type { Principal, IdentityProvider } from './types/pylon/auth/index.js'
