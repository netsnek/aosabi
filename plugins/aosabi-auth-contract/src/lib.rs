// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-auth-contract: the authorization contract of Aosabi.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/contract.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The subpath auth/contract. The re-exports below carry its export list of
 * contract.ts:11-19 plus the Rust items the seam and the checks need,
 * PrincipalId, PrincipalRef, ProviderFuture, SyncProvider, AsyncProvider and
 * IntoIdentity. This file holds the module list and those re-exports.
 */

//! The authorization contract of Aosabi.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `errors` | `auth/errors.ts` | [`ForbiddenError`] |
//! | `principal` | `auth/principal.ts`, the principal and its checks | the principal, its id, the context key and the role and permission checks |
//! | `provider` | `auth/principal.ts`, the identity seam | the identity provider seam in its synchronous and asynchronous forms |
//!
//! The crate builds with no standard library and needs an allocator, so a
//! build for a microcontroller links it to check a principal, and it shares
//! through no atomic operation, because the core of the ESP32-C3 has none.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebooks are `tests/notebooks/30_http_tier1.ipynb`
//! and `12_authoring_parity.ipynb`, whose compile cells `c6-identity-*` hold
//! the provider seam.

#![no_std]

extern crate alloc;

pub mod errors;
pub mod principal;
pub mod provider;

pub use crate::{
    errors::ForbiddenError,
    principal::{
        PRINCIPAL_KEY,
        Principal,
        PrincipalId,
        PrincipalRef,
        has_permission,
        has_role, //
    },
    provider::{
        AsyncProvider,
        IdentityProvider,
        IntoIdentity,
        ProviderFuture,
        SyncProvider, //
    }, //
};
