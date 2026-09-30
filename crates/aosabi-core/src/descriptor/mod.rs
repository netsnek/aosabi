// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the static description of a Rust type.
 *
 * No analogue on the reference side.
 */

//! The static description of a Rust type.
//!
//! A TypeScript build reads the types of a service through the TypeScript checker and
//! executes nothing. Rust has no checker a build can ask, so the three macros
//! of `aosabi-macros` write what the checker would answer into constants, and
//! the inference walks those constants.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `link` | [`TypeLink`], the lazy reference to a described type, [`TypeExpr`] and [`Primitive`] |
//! | `info` | [`TypeInfo`] and the records it lists, [`FieldInfo`], [`MethodInfo`], [`ParamInfo`], [`VariantInfo`] and [`HeritageInfo`], beside [`Docs`], [`DocTag`], [`Site`], [`TypeKind`], [`HeritageKind`], [`Visibility`] and [`SourceKey`] |
//! | `described` | [`Described`], [`Resolvers`], [`Promise`] and the implementations for the types of `core` and `alloc` |
//! | `roots` | [`Roots`], [`RootsInfo`], [`RootInfo`], [`NoRoots`], [`Merged`] and [`AppRoots`] |
//! | `registry` | [`Declaration`], the record the declaration list of a crate names |
//! | `entry` | [`FragmentRoot`], [`Segment`], [`EntryInfo`] and [`EntryKind`], the descriptors of the entries of `graphql!`, and [`UserError`] |
//! | `facts` | [`DecoratorFact`], [`GuardFact`], [`GuardCall`] and [`GuardKind`], the facts of a decorator and of a guard call the authoring check reads |
//! | `markers` | the scalar markers [`ID`], [`Int`], [`Float`], [`Date`], [`File`], [`JSON`] and [`Any`], with the feature `runtime` |
//! | [`repeater`] | [`Repeater`], the stream a subscription field returns |
//! | [`callable`] | [`CallableInfo`](callable::CallableInfo), the description of a member whose value is a function |
//!
//! # Links are lazy
//!
//! A [`TypeLink`] holds function pointers, and [`TypeLink::of`] evaluates no
//! descriptor. A type that refers to itself and two types that refer to each
//! other therefore compile, because the constant of one type never reads the
//! constant of another while the compiler evaluates it.
//!
//! # Identity
//!
//! The identity of a described type is [`TypeLink::id`] and never the address
//! of a [`TypeInfo`], because a promoted constant has no address a program may
//! rely on.
//!
//! # What has no description
//!
//! A tuple, a `HashMap`, a `BTreeMap`, a closure and a trait object implement
//! [`Described`] nowhere, which is the compile time refusal of
//! `docs/schema-inference.md` section 13. [`TypeExpr`] is `non_exhaustive`,
//! because the scalar markers, the JSON markers, the repeater newtype and the
//! insertion ordered map join it with the inference corpus.

pub mod callable;
mod described;
mod entry;
mod facts;
mod info;
mod link;
// A marker holds a value of the executor, so it exists where the run time
// does, which is every build that links an application through the facade.
#[cfg(feature = "runtime")]
mod markers;
mod registry;
pub mod repeater;
mod roots;

#[cfg(feature = "runtime")]
pub use self::markers::{
    Any,
    Date,
    File,
    Float,
    ID,
    Int,
    JSON,
    Number, //
};

pub use self::{
    described::{
        Described,
        Promise,
        Resolvers, //
    },
    entry::{
        EntryInfo,
        EntryKind,
        FragmentRoot,
        Segment,
        UserError, //
    },
    facts::{
        DecoratorFact,
        GuardCall,
        GuardFact,
        GuardKind, //
    },
    info::{
        DocTag,
        Docs,
        FieldInfo,
        HeritageInfo,
        HeritageKind,
        MethodInfo,
        ParamInfo,
        Site,
        SourceKey,
        TypeInfo,
        TypeKind,
        VariantInfo,
        Visibility, //
    },
    link::{
        Primitive,
        TypeExpr,
        TypeLink, //
    },
    registry::Declaration,
    repeater::Repeater,
    roots::{
        AppRoots,
        Merged,
        NoRoots,
        RootInfo,
        Roots,
        RootsInfo, //
    },
};
