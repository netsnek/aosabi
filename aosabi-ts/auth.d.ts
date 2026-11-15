// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * auth.d.ts: the types of the entry aosabi/auth of the package, the gates and the helpers of
 * lib/auth.js.
 *
 * Derived from Pylon 3.0.0-canary by Nico Schett, its source, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The declarations are Pylon's: the folder types/pylon/auth/ holds the declaration files of the
 * subpath auth of Pylon as the declaration build of its package emits them, as vendored literals.
 * This file re-exports the names lib/auth.js exports and the types of their shapes. The type
 * IdentityProvider defaults its context to the Context of the declarations of the root, which they
 * import from @getcronit/pylon by its bare specifier, as Pylon's declarations do, so the author
 * mode judges how its checker reads them.
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
