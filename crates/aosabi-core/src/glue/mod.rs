// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue: the traits and functions the generated code calls.
 *
 * No analogue in the reference, where a resolver is a JavaScript function
 * and its values are JavaScript values already.
 */

//! The seam between a Rust type and the value of the executor.
//!
//! The macros of `aosabi-macros` describe a type for the schema build, and
//! they also make it usable at run time: a derived struct answers its fields
//! and its methods as an object of the executor, a method of a
//! `#[aosabi::resolvers]` block becomes a function over positional arguments,
//! and an entry of `graphql!` becomes a function of a root object. The
//! generated code names the items of this module and nothing else of the run
//! time, through `::aosabi::__private::glue::`, so the module re-exports the
//! few items of the executor crate and of this crate that code needs.
//!
//! | Module | Holds |
//! | --- | --- |
//! | [`into_value`] | [`IntoValue`], a Rust value as a value of the executor |
//! | [`from_argument`] | [`FromArgument`], [`ArgumentMismatch`] and [`argument`], a positional argument as a Rust value |
//! | [`completion`] | [`IntoThrown`], [`IntoCompletion`], [`ResolverMethods`], [`complete`] and [`complete_future`] |
//! | [`unchecked`] | [`Unchecked`], a value the schema trusts and the run time does not check |
//! | [`thrown`] | [`Thrown`], what a resolver throws, and [`Error`], the error of a resolver of the native layer |
//! | [`stream`] | [`Stream`], the stream a subscription field of the native layer answers, [`task`], and [`complete_stream`] with [`StreamStart`] and [`IntoStream`], the completion of a method that answers a stream |
//! | [`entry`] | the run time of `graphql!`, [`EntryFunction`], [`Part`], [`root_object`] and [`factory`] |
//! | [`decorate`] | the run time of a decorated site, [`decorated`] and [`decorators`] |
//! | [`mutation`] | the run time of a mutation entry, [`mutation_wrapper`], and [`MutationPayload`], the answer of a mutation method of the native layer |
//! | [`embedded`] | the member lookup through an embedded base |
//! | [`shared`] | [`Shared`], [`SharedObject`] and [`FieldValue`], a public field read through a shared reference, and the static shape of a struct with its member writer and [`member_of`] |
//! | [`methods`] | [`ResolversFallback`], the empty method table a resolvers block shadows |
//! | [`resolve_type`] | the type resolution of an abstract type, [`ResolveTypeRule`] and [`ResolveTypeCheck`] |
//!
//! A type the glue cannot carry refuses at compile time with a trait bound
//! error. A Rust panic is no throw and is never caught.

pub mod completion;
pub mod decorate;
pub mod embedded;
pub mod entry;
pub mod from_argument;
pub mod into_value;
pub mod methods;
pub mod mutation;
pub mod resolve_type;
pub mod shared;
pub mod stream;
pub mod thrown;
pub mod unchecked;

pub use std::rc::Rc;

pub use aosabi_execute::{
    execute::MemberSink,
    promise::Agent,
    value::{
        Completion,
        Function,
        MemberKind,
        MemberShape,
        MemberSlot,
        ObjectLike,
        Shape,
        Value, //
    }, //
};

pub use self::{
    completion::{
        IntoCompletion,
        IntoThrown,
        ResolverMethods,
        complete,
        complete_future, //
    },
    decorate::{
        DecoratorCell,
        decorated,
        decorators, //
    },
    entry::{
        EntryFunction,
        Part,
        Run,
        factory,
        root_object, //
    },
    from_argument::{
        ArgumentMismatch,
        FromArgument,
        argument, //
    },
    into_value::IntoValue,
    methods::ResolversFallback,
    mutation::{
        MutationPayload,
        mutation_wrapper, //
    },
    resolve_type::{
        ResolveTypeCheck,
        ResolveTypeRule, //
    },
    shared::{
        FieldValue,
        Reached,
        Shared,
        SharedObject,
        member_of,
        unreached, //
    },
    stream::{
        IntoStream,
        Stream,
        StreamStart,
        complete_stream,
        task, //
    },
    thrown::{
        Error,
        Thrown, //
    },
    unchecked::Unchecked, //
};
pub use crate::{
    app::entry::Fragment,
    bindings::{
        Bindings,
        FromBinding,
        Key,
        Variables,
        binding, //
    },
    decorator::Decorator,
    plugin::{
        PluginList,
        PluginObject,
        PluginSlot,
        object::__wrap, //
    },
    resolvers::Resolvers,
};
