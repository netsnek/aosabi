// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * Derived from Pylon 3.0.0-canary by Nico Schett,
 * packages/pylon/src/auth/index.ts, Apache-2.0. Changed by the Aosabi authors.
 *
 * This file holds the module list and the re-exports of the root, the export
 * list of index.ts:19-36 plus the Rust items its calls and its seam need,
 * PrincipalId, PrincipalRef, IntoIdentity, SyncProvider, AsyncProvider and
 * PRINCIPAL. The re-exports below carry the names of that export list and
 * those Rust items.
 */

//! The auth root of Aosabi.
//!
//! The crate owns the actor of a request and the capability gates on it: the
//! identity seam over the request context, the plugin that binds the
//! principal of every request, and the calls a resolver or a route gates
//! itself with. The principal and its checks live in `aosabi-auth-contract`,
//! which a layer below the web framework names alone.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `provider` | `auth/index.ts` | the identity seam with the request context as its context |
//! | `authz` | `auth/authz.ts` | the principal of the current request, the gates, the identity plugin and the forbidden mapping |
//! | `plugin` | `auth/plugin.ts` | the subpath of the identity plugin |
//!
//! The root exports the authoring calls and the identity seam, and the
//! identity plugin stands under [`plugin`] alone, as the root of the
//! reference exports no `useIdentity`.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3.

pub mod authz;
pub mod plugin;
pub mod provider;

pub use aosabi_auth_contract::{
    AsyncProvider,
    ForbiddenError,
    IntoIdentity,
    Principal,
    PrincipalId,
    PrincipalRef,
    SyncProvider,
    has_permission,
    has_role, //
};

pub use crate::{
    authz::{
        PRINCIPAL,
        authorize,
        get_principal,
        require_role, //
    },
    provider::IdentityProvider,
};
