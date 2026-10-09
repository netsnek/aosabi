// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: the patches of a configured federation, each keyed by the
 * remote type it takes.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The option patches of configure, and of passthrough.
 */

//! The patches of a configured federation, keyed by the name of a remote type.
//!
//! The reference types the patch map as an object whose keys are
//! `keyof TRegistry['types']` and whose values take the type of their key, so
//! a key the registry lacks and a patch over another type both fail to
//! compile. [`Patches`] is that map over a pulled registry, and the literal
//! [󰀂](crate::patches!) writes it as the reference writes the object. Each
//! entry expands into a block with a key type of its own, which implements
//! [`PatchKey`] with the text of the key, and a call of [`Patches::insert`],
//! whose bound asks that the patch take a type of the registry and whose
//! inline constant asks that the name of that type be the key. The bare
//! `passthrough()` expands into [`Patches::insert_passthrough`], whose inline
//! constant looks the key up in [`Registry::TYPES`]. A refused key is an error
//! of constant evaluation, which `cargo build` reports where the federation is
//! configured.
//!
//! A patch is a closure in one of the forms [`IntoPatch`] lists, told apart
//! by a marker type: [`OneArg`] for `data => value`, [`TwoArgs`] for
//! `(data, api) => value`, either with [`Fallible`] when the closure answers
//! a `Result` and may throw, and any of them under `pass(patch, policy)`.
//! The table keeps every patch as the untyped [`PatchFn`] the patch transform
//! calls, beside its policy.

use std::{
    fmt,
    marker::PhantomData,
    rc::Rc, //
};

use aosabi_core::{
    Thrown,
    glue::IntoValue,
    remote::{
        FromDelegated,
        Registry,
        RemoteType, //
    }, //
};
use aosabi_execute::value::Value;

use crate::{
    gateway::Gateway,
    policy::{
        Pass,
        PatchPolicy, //
    },
    transforms::patch::{
        PatchApi,
        PatchFn,
        PatchTable, //
    },
    utils::same_bytes, //
};

/// The patches of one federation, each keyed by the remote type it takes.
///
/// The twin of the option `patches` of `configure`, over the registry `R`.
pub struct Patches<R: Registry> {
    /// The patches with their policies, in insertion order.
    table: PatchTable,
    /// The registry whose types key the patches.
    registry: PhantomData<fn() -> R>,
}

/// The key of one entry of a [`patches!`](crate::patches!) literal, the text of the key.
pub trait PatchKey {
    /// The name of the remote type the entry is keyed by.
    const KEY: &'static str;
}

/// The marker of a patch of one argument, `data => value`.
pub enum OneArg {}

/// The marker of a patch of two arguments, `(data, api) => value`, whose `api` is the federation.
pub enum TwoArgs {}

/// The marker of a patch that answers a `Result`, a patch that may throw.
pub enum Fallible {}

/// The identity patch `passthrough()` writes, keyed by name alone.
///
/// It states that a remote type is published in full on purpose, which
/// `strict` asks of every type that has no patch.
#[derive(Clone, Copy, Debug, Default)]
pub struct Passthrough;

/// A patch in one of its forms, `data => value`, `(data, api) => value`, either returning `Result<value>` when it throws, or `pass(...)` of one.
///
/// `S` is the remote type the patch takes, `O` what it answers, `R` the
/// registry of the federation and `M` the marker of the form.
pub trait IntoPatch<S, O, R: Registry, M> {
    /// Returns the patch as the patch transform calls it, with the policy `pass` attached.
    fn into_patch(self) -> (PatchFn, Option<PatchPolicy>);
}

impl<S, O, R, F> IntoPatch<S, O, R, OneArg> for F
where
    S: FromDelegated<S> + 'static,
    O: IntoValue + 'static,
    R: Registry,
    F: Fn(S) -> O + 'static,
{
    fn into_patch(self) -> (PatchFn, Option<PatchPolicy>) {
        let patch: PatchFn = Rc::new(move |data: Value, _: &PatchApi| {
            Ok(self(S::from_delegated(data)?).into_value())
        });
        (patch, None)
    }
}

impl<S, O, R, F> IntoPatch<S, O, R, TwoArgs> for F
where
    S: FromDelegated<S> + 'static,
    O: IntoValue + 'static,
    R: Registry,
    F: Fn(S, Gateway<R>) -> O + 'static,
{
    fn into_patch(self) -> (PatchFn, Option<PatchPolicy>) {
        let patch: PatchFn = Rc::new(move |data: Value, api: &PatchApi| {
            Ok(self(S::from_delegated(data)?, api_of::<R>(api)?).into_value())
        });
        (patch, None)
    }
}

impl<S, O, R, F> IntoPatch<S, O, R, (OneArg, Fallible)> for F
where
    S: FromDelegated<S> + 'static,
    O: IntoValue + 'static,
    R: Registry,
    F: Fn(S) -> Result<O, Thrown> + 'static,
{
    fn into_patch(self) -> (PatchFn, Option<PatchPolicy>) {
        let patch: PatchFn = Rc::new(move |data: Value, _: &PatchApi| {
            Ok(self(S::from_delegated(data)?)?.into_value())
        });
        (patch, None)
    }
}

impl<S, O, R, F> IntoPatch<S, O, R, (TwoArgs, Fallible)> for F
where
    S: FromDelegated<S> + 'static,
    O: IntoValue + 'static,
    R: Registry,
    F: Fn(S, Gateway<R>) -> Result<O, Thrown> + 'static,
{
    fn into_patch(self) -> (PatchFn, Option<PatchPolicy>) {
        let patch: PatchFn = Rc::new(move |data: Value, api: &PatchApi| {
            Ok(self(S::from_delegated(data)?, api_of::<R>(api)?)?.into_value())
        });
        (patch, None)
    }
}

impl<S, O, R, M, P> IntoPatch<S, O, R, M> for Pass<P>
where
    R: Registry,
    P: IntoPatch<S, O, R, M>,
{
    fn into_patch(self) -> (PatchFn, Option<PatchPolicy>) {
        let (patch, policy) = self.into_parts();
        (patch.into_patch().0, Some(policy))
    }
}

/// Returns the federation the patch transform hands a patch as its `api`.
fn api_of<R: Registry>(api: &PatchApi) -> Result<Gateway<R>, Thrown> {
    api.downcast_ref::<Gateway<R>>()
        .cloned()
        .ok_or_else(|| Thrown::error("the api of a patch is the federation that configured it"))
}

/// Returns whether `names` holds `name`, which a constant can evaluate.
const fn lists(names: &[&str], name: &str) -> bool {
    let mut index = 0;
    while index < names.len() {
        if same_bytes(names[index].as_bytes(), name.as_bytes()) {
            return true;
        }
        index += 1;
    }
    false
}

impl<R: Registry> Default for Patches<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Registry> Patches<R> {
    /// Returns patches that hold no entry, the object literal `{}`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            table: PatchTable::new(),
            registry: PhantomData,
        }
    }

    /// Adds `patch` under the key `K`, refused at compile time unless `S` is the type of that name in the registry `R`.
    #[must_use]
    pub fn insert<K, S, O, M, P>(mut self, patch: P) -> Self
    where
        K: PatchKey,
        S: RemoteType<Registry = R> + FromDelegated<S>,
        O: IntoValue + 'static,
        P: IntoPatch<S, O, R, M>,
    {
        const {
            assert!(
                same_bytes(S::TYPENAME.as_bytes(), K::KEY.as_bytes()),
                "a patch is keyed by the name of the remote type it takes"
            );
        }
        let (patch, policy) = patch.into_patch();
        self.table.insert(K::KEY, patch, policy);
        self
    }

    /// Adds the identity patch under the key `K`, refused at compile time unless `R::TYPES` holds `K::KEY`.
    #[must_use]
    pub fn insert_passthrough<K: PatchKey>(mut self) -> Self {
        const {
            assert!(
                lists(R::TYPES, K::KEY),
                "a passthrough is keyed by a type the registry of the federation lists"
            );
        }
        let patch: PatchFn = Rc::new(|data: Value, _: &PatchApi| Ok(data));
        self.table.insert(K::KEY, patch, None);
        self
    }

    /// Returns the untyped table the patch transform runs over.
    #[must_use]
    pub fn into_table(self) -> PatchTable {
        self.table
    }
}

impl<R: Registry> fmt::Debug for Patches<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.table, f)
    }
}

/// Builds the [`Patches`] of a federation from a literal of remote type names and patches, the object literal of the option `patches`.
///
/// A key is the name of a remote type, and a patch is a closure that takes
/// that type, `pass(closure, policy! { .. })`, or the bare `passthrough()`.
/// A key that names another type than the closure takes, or a type the
/// registry lacks, fails to compile. A key written as a raw identifier names
/// the type without its prefix, as the derive names the type.
///
/// # Examples
///
/// ```ignore
/// patches! {
///     User: pass(|u: users::User| UserPatch::from(u), policy! { orders: { args: [] } }),
///     Money: passthrough(),
/// }
/// ```
#[macro_export]
macro_rules! patches {
    ($($body:tt)*) => {
        $crate::__federation_patches!($crate::patches::Patches::new(); $($body)*)
    };
}

/// Adds the entries of a [`patches!`](crate::patches!) literal, one at a time.
#[doc(hidden)]
#[macro_export]
macro_rules! __federation_patches {
    ($patches:expr;) => {
        $patches
    };
    ($patches:expr; $key:ident : passthrough() $(, $($rest:tt)*)?) => {
        $crate::__federation_patches!(
            {
                let _: $crate::patches::Passthrough = passthrough();
                enum PatchEntryKey {}
                impl $crate::patches::PatchKey for PatchEntryKey {
                    const KEY: &'static str = $crate::headers::__unraw(::core::stringify!($key));
                }
                $patches.insert_passthrough::<PatchEntryKey>()
            };
            $($($rest)*)?
        )
    };
    ($patches:expr; $key:ident : $patch:expr $(, $($rest:tt)*)?) => {
        $crate::__federation_patches!(
            {
                enum PatchEntryKey {}
                impl $crate::patches::PatchKey for PatchEntryKey {
                    const KEY: &'static str = $crate::headers::__unraw(::core::stringify!($key));
                }
                $patches.insert::<PatchEntryKey, _, _, _, _>($patch)
            };
            $($($rest)*)?
        )
    };
}
