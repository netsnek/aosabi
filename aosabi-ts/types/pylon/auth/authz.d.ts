/**
 * Capability-tier authorization — "may this principal perform this operation?".
 * Needs ONLY the Principal (no ORM), so it secures resolvers and routes on bare
 * core + an identity provider. pylon-app extends `authorize` with the resource
 * tier (rows/instances/fields via WhereInput).
 */
import type { Context, Plugin } from '@getcronit/pylon';
import { type IdentityProvider, type Principal } from './principal.js';
/** The current request's Principal (set by `useIdentity`), or undefined. */
export declare function getPrincipal(): Principal | undefined;
/** Capability gate: throws `ForbiddenError` if the check fails. Works on bare core. */
export declare function authorize(check: (principal: Principal | undefined) => boolean): void;
/** Require ANY of the given roles on the current principal (sugar over `authorize`). */
export declare function requireRole(...roles: string[]): void;
/**
 * Bind the request's Principal from any identity provider, and map authz errors.
 * The principal lives on the request context, so resolvers/routes read it via
 * `getPrincipal()`. `ForbiddenError` thrown anywhere downstream is mapped to a
 * `FORBIDDEN` GraphQL error (not masked).
 */
export declare function useIdentity(provider: IdentityProvider<Context>): Plugin;
