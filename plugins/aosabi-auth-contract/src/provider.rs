// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-auth-contract: the identity seam that turns a request into its
 * principal.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/auth/principal.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `IdentityProvider` and the two forms a provider is written in.
//!
//! The reference's provider is one function type whose answer is a principal, `undefined`
//! or a promise of either, and the middleware awaits whatever it answers. A Rust function
//! is either plain or `async`, so the seam is one trait with a marker parameter and two
//! blanket implementations, one per form. The compiler picks the one a function meets: a
//! plain function meets only the first, because its return is no future, and an
//! `async fn` meets only the second, because its future borrows the context and no answer
//! free of that borrow exists for the first. So one `use_identity(provider)` takes either
//! form and infers the marker from the provider.
//!
//! A provider that can fail answers `Result<Option<Principal>, F>` and throws with `?`,
//! which is a provider of the reference that throws or rejects. Its error is any error
//! that converts into the error of the host, so a provider of the native layer answers
//! the `Result` of the prelude of the facade, whose error converts into the thrown value
//! of the engine. The trait names no host: `Ctx` is the request context of the host and
//! `E` the error it throws, so the contract builds with no standard library and shares no
//! value between threads.

use alloc::boxed::Box;
use core::{
    future::Future,
    pin::Pin, //
};

use crate::principal::Principal;

/// The future a provider answers with, boxed because a plugin holds the provider behind a
/// trait object.
pub type ProviderFuture<'a, E> = Pin<Box<dyn Future<Output = Result<Option<Principal>, E>> + 'a>>;

/// The marker of a provider written as a plain function, `c => principal`.
///
/// It has no value. It only tells the two implementations of
/// [`IdentityProvider`] apart.
pub enum SyncProvider {}

/// The marker of a provider written as an `async fn` or an async closure, `async c => principal`.
///
/// It has no value. It only tells the two implementations of
/// [`IdentityProvider`] apart.
pub enum AsyncProvider {}

/// What a provider answers: a principal or none, or a thrown error.
///
/// `Principal | undefined` is `Option<Principal>`, and a provider that
/// throws answers `Result<Option<Principal>, F>` with an error `F` that
/// converts into the error `E` of the host, so both forms take either answer.
pub trait IntoIdentity<E> {
    /// Returns the answer as the principal or none, or as the thrown error.
    ///
    /// # Errors
    ///
    /// Returns the error the provider threw.
    fn into_identity(self) -> Result<Option<Principal>, E>;
}

impl<E> IntoIdentity<E> for Option<Principal> {
    fn into_identity(self) -> Result<Option<Principal>, E> {
        Ok(self)
    }
}

impl<E, F: Into<E>> IntoIdentity<E> for Result<Option<Principal>, F> {
    fn into_identity(self) -> Result<Option<Principal>, E> {
        self.map_err(Into::into)
    }
}

/// Turns a request into the current principal, or none for public access.
///
/// `Ctx` is the host request context, `E` the error the host throws, and `M` the marker one of the
/// two blanket implementations picks, so one function takes both forms.
///
/// The seam is the one place any auth mechanism plugs in, an OIDC issuer, a
/// custom token, an API key, a session cookie or a client certificate, and
/// swapping the provider never touches a gate, a policy, a model or a
/// resolver.
pub trait IdentityProvider<Ctx: ?Sized, E, M>: 'static {
    /// Returns the future of the principal of `context`, or of the error the provider throws.
    fn identify<'a>(&'a self, context: &'a Ctx) -> ProviderFuture<'a, E>;
}

impl<Ctx: ?Sized, E, F, R> IdentityProvider<Ctx, E, SyncProvider> for F
where
    F: Fn(&Ctx) -> R + 'static,
    R: IntoIdentity<E>,
{
    fn identify<'a>(&'a self, context: &'a Ctx) -> ProviderFuture<'a, E> {
        // The call runs when the future is first polled, which is at once,
        // since the middleware awaits it, so the future holds the two
        // borrows alone and asks nothing of how long `E` lives.
        Box::pin(async move { self(context).into_identity() })
    }
}

impl<Ctx: ?Sized, E, F, R> IdentityProvider<Ctx, E, AsyncProvider> for F
where
    F: AsyncFn(&Ctx) -> R + 'static,
    R: IntoIdentity<E>,
{
    fn identify<'a>(&'a self, context: &'a Ctx) -> ProviderFuture<'a, E> {
        Box::pin(async move { self(context).await.into_identity() })
    }
}
