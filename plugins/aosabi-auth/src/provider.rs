// SPDX-License-Identifier: MIT AND Apache-2.0
// Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/auth/index.ts,
// Apache-2.0. Changed by the Aosabi authors.

//! `IdentityProvider` over the request `Context`.
//!
//! The auth root narrows the generic seam of the contract to the request
//! context of Aosabi, `IdentityProvider<Ctx = Context>`, so a provider written
//! for it reads the request of hono, `c.req().header(...)` and `c.get(...)`,
//! while the contract keeps the seam generic for a layer that names no web
//! framework. The error a provider throws is [`Thrown`], the value a resolver
//! and a route throw, so a provider answers
//! `Result<Option<Principal>, Thrown>` and throws with `?`.
//!
//! The trait is the contract's seam under one more name and adds no method,
//! so every function, `async fn` and closure that meets the contract's seam
//! for this context and this error meets it, with the marker the compiler
//! infers from the provider.

use aosabi_auth_contract::provider as contract;
use aosabi_core::{
    Context,
    Thrown, //
};

/// Turns a request into the current principal, or none for public access.
///
/// The trait is the reference's `IdentityProvider<Ctx = Context>`. `M` is the
/// marker of the form the provider is written in,
/// [`SyncProvider`](aosabi_auth_contract::SyncProvider) for a plain function
/// and [`AsyncProvider`](aosabi_auth_contract::AsyncProvider) for an
/// `async fn` or an async closure, which the compiler picks.
pub trait IdentityProvider<M>: contract::IdentityProvider<Context, Thrown, M> {}

impl<M, P: contract::IdentityProvider<Context, Thrown, M>> IdentityProvider<M> for P {}
