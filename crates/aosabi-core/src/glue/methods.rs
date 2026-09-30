// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue methods: the fallback of the method table a resolvers
 * block shadows.
 *
 * No analogue on the reference side, where the methods of a class are the
 * members of its prototype whether the class declares any or not.
 */

//! [`ResolversFallback`], the empty method table every type has.
//!
//! A `#[aosabi::resolvers]` block emits `__AOSABI_METHODS`, `__AOSABI_NAMES`
//! and `__aosabi_method` as inherent items of its type, and the derive reads
//! them by path with this trait in scope. A path resolves an inherent item
//! before an item of a trait, so the derive reads the table of the block
//! where one exists and the empty table of this trait where none does, and a
//! struct states no key to tell the two apart.

use aosabi_execute::value::Function;

use crate::{
    descriptor::MethodInfo,
    glue::shared::Shared, //
};

/// The empty method table, which the inherent items of a resolvers block
/// shadow.
pub trait ResolversFallback {
    /// The descriptors of the methods, none.
    const __AOSABI_METHODS: &'static [MethodInfo] = &[];

    /// The names of the methods, none.
    const __AOSABI_NAMES: &'static [&'static str] = &[];

    /// Returns no method.
    #[must_use]
    fn __aosabi_method(this: &Shared<Self>, key: &str) -> Option<Function>
    where
        Self: Sized + 'static,
    {
        let _ = (this, key);
        None
    }
}

impl<T: ?Sized> ResolversFallback for T {}
