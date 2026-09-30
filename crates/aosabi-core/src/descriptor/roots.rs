// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the roots of an application, as types, as fragments of
 * graphql! and as the merge of compose.
 *
 * No analogue on the reference side.
 */

//! The roots of an application.
//!
//! A root reaches the build in one of three forms, each written by
//! `graphql!` or by `compose`. A struct named by its key is a root type, the
//! twin of a class used as a root (`export const graphql = {Query}`). A root
//! with a block of entries is a [`FragmentRoot`], the twin of an object
//! literal. The roots of `compose` are the merge of the parent's and the
//! children's, the twin of the intersection the reference types them with, the
//! parent first and then each child in call order. [`RootInfo`] holds one of
//! the three, and [`RootsInfo`] holds one per root.
//!
//! The build reads the roots from a type and runs nothing, so every form is a
//! constant of a trait: [`Roots`] for the roots of an application,
//! [`AppRoots`] for the entry `default()` answers.
//!
//! A plain crate states its roots in no type. Its entry answers the native
//! builder of the facade, whose calls register the parts of each root as
//! values, and the builder of the source path reads the roots from the source
//! of the crate, the entry and the calls of its chain, with nothing of the
//! crate compiled. So the type of the builder names [`NoRoots`], and the parts
//! reach the app at run time through `App::merge_parts` of the module `app`.

use core::marker::PhantomData;

use super::{
    entry::FragmentRoot,
    link::TypeLink,
    registry::Declaration, //
};

/// One root of an application.
#[derive(Clone, Copy, Debug)]
pub enum RootInfo {
    /// The application has no such root.
    Absent,
    /// A struct used as the root, the twin of a class used as a root.
    Type(TypeLink),
    /// A root of a `graphql!` fragment, the twin of an object literal.
    Fragment(&'static FragmentRoot),
    /// The root of the parent merged with the root of a child, the twin of the
    /// intersection of `compose`.
    Merged(&'static RootInfo, &'static RootInfo),
}

impl RootInfo {
    /// Returns whether the application has the root.
    #[must_use]
    pub const fn is_present(&self) -> bool {
        !matches!(self, Self::Absent)
    }

    /// Returns the root of `parent` merged with the root of `child`.
    ///
    /// A root that one side lacks is the other side's as it stands, because
    /// an intersection with an absent member is that member.
    #[must_use]
    pub const fn merge(parent: &'static Self, child: &'static Self) -> Self {
        match (parent, child) {
            (Self::Absent, _) => *child,
            (_, Self::Absent) => *parent,
            _ => Self::Merged(parent, child),
        }
    }
}

/// The three roots of an application.
#[derive(Clone, Copy, Debug)]
pub struct RootsInfo {
    /// The `Query` root.
    pub query: RootInfo,
    /// The `Mutation` root.
    pub mutation: RootInfo,
    /// The `Subscription` root.
    pub subscription: RootInfo,
}

impl RootsInfo {
    /// The roots of an application with no root.
    pub const ABSENT: Self = Self {
        query: RootInfo::Absent,
        mutation: RootInfo::Absent,
        subscription: RootInfo::Absent,
    };

    /// Returns the roots of `parent` merged with the roots of `child`, root by
    /// root, the parent's first.
    #[must_use]
    pub const fn merged(parent: &'static Self, child: &'static Self) -> Self {
        Self {
            query: RootInfo::merge(&parent.query, &child.query),
            mutation: RootInfo::merge(&parent.mutation, &child.mutation),
            subscription: RootInfo::merge(&parent.subscription, &child.subscription),
        }
    }

    /// Returns whether one root at least is present.
    #[must_use]
    pub const fn has_root(&self) -> bool {
        self.query.is_present() || self.mutation.is_present() || self.subscription.is_present()
    }
}

/// A type that names the roots of an application.
///
/// The build script of a server crate reads an implementor, which carries the
/// roots and the declaration list of the application crate.
pub trait Roots: 'static {
    /// The three roots.
    const ROOTS: RootsInfo;
    /// The declaration list of the crate of the first present root in the
    /// order Query, Mutation, Subscription, which is the application crate of
    /// the inference, or an empty list when no root is present.
    const DECLARATIONS: &'static [Declaration];
}

/// The roots of an application with no fragment, every root absent.
pub struct NoRoots;

impl Roots for NoRoots {
    const ROOTS: RootsInfo = RootsInfo::ABSENT;
    const DECLARATIONS: &'static [Declaration] = &[];
}

/// The roots of `compose`, the parent's first and then the child's.
///
/// A composition of several children nests, `(A, B, C)` merging as
/// `Merged<A, Merged<B, C>>`, so the order is the call order.
pub struct Merged<P, C>(PhantomData<(P, C)>);

impl<P: Roots, C: Roots> Merged<P, C> {
    /// The roots of the parent, as a reference a constant of the merge keeps.
    const PARENT: &'static RootsInfo = &P::ROOTS;
    /// The roots of the child, as a reference a constant of the merge keeps.
    const CHILD: &'static RootsInfo = &C::ROOTS;
}

impl<P: Roots, C: Roots> Roots for Merged<P, C> {
    const ROOTS: RootsInfo = RootsInfo::merged(Self::PARENT, Self::CHILD);
    const DECLARATIONS: &'static [Declaration] = if P::ROOTS.has_root() {
        P::DECLARATIONS
    } else {
        C::DECLARATIONS
    };
}

/// The entry of an application, the type `default()` answers, which names its
/// roots.
///
/// The build reads the roots from the type of the entry without calling it,
/// which is the twin of the reference reading the type of the default export
/// and executing nothing. The trait lives in the descriptor module, so a build
/// script reads the roots with no run time linked, and the trait `App` of the
/// run time extends it.
pub trait AppRoots: 'static {
    /// The roots the build reads, the type of `default().graphql`.
    type Roots: Roots;
}
