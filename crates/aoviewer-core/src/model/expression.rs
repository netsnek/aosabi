// SPDX-License-Identifier: MIT
/*
 * aoviewer-core model expression: the type expression of a type reference,
 * read from the outside in.
 *
 * One walk serves the printer and the comparison of a reference of the model
 * and of a reference by name, so neither is written twice.
 */

//! The type expression.
//!
//! A type expression wraps a named type in list and non null wrappers, which
//! GraphQL writes as `[Post!]!`. [`TypeRef::parts`] reads a reference of the
//! model and [`TypeRefSpec::parts`] a reference by name the same way, its
//! wrappers from the outside in and the named type last. So [`Parts::text`]
//! writes both as GraphQL writes them, and two references compare by their parts
//! whatever model holds them, as [`Iterator::eq`] compares two walks.
//!
//! Each step of [`Parts`] strips one wrapper and the named type ends the
//! walk, so a walk takes one step more than its reference has wrappers. The
//! builder caps the wrappers of a reference at
//! [`ReadLimits::max_depth`](crate::limits::ReadLimits::max_depth).
//!
//! [specification]: https://spec.graphql.org/October2021/

use alloc::{
    string::String,
    vec::Vec, //
};

use super::{
    arena::{
        Model,
        TypeRef, //
    },
    builder::TypeRefSpec,
};

/// One part of a type expression, read from the outside in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypePart<'a> {
    /// A list of what the parts after it name.
    List,
    /// What the parts after it name, never null.
    NonNull,
    /// The named type, the last part.
    Named(&'a str),
}

/// The parts of a type expression, from the outside in.
#[derive(Clone, Debug)]
pub struct Parts<'a> {
    /// The reference the walk stands at, until the named type ends it.
    next: Option<Step<'a>>,
}

/// The reference a walk of [`Parts`] stands at.
#[derive(Clone, Copy, Debug)]
enum Step<'a> {
    /// A reference of the model, whose names the model holds.
    Ref(&'a TypeRef, &'a Model),
    /// A reference by name.
    Spec(&'a TypeRefSpec),
}

impl TypeRef {
    /// Returns the parts of the reference, its named type by its name in
    /// `model`.
    #[must_use]
    pub fn parts<'a>(&'a self, model: &'a Model) -> Parts<'a> {
        Parts {
            next: Some(Step::Ref(self, model)),
        }
    }
}

impl TypeRefSpec {
    /// Returns the parts of the reference.
    #[must_use]
    pub fn parts(&self) -> Parts<'_> {
        Parts {
            next: Some(Step::Spec(self)),
        }
    }
}

impl<'a> Iterator for Parts<'a> {
    type Item = TypePart<'a>;

    fn next(&mut self) -> Option<TypePart<'a>> {
        let (part, inner) = match self.next.take()? {
            Step::Ref(type_ref, model) => match type_ref {
                TypeRef::Named(id) => (TypePart::Named(model.name(model.ty(*id).name)), None),
                TypeRef::List(inner) => (TypePart::List, Some(Step::Ref(inner, model))),
                TypeRef::NonNull(inner) => (TypePart::NonNull, Some(Step::Ref(inner, model))),
            },
            Step::Spec(spec) => match spec {
                TypeRefSpec::Named(name) => (TypePart::Named(name), None),
                TypeRefSpec::List(inner) => (TypePart::List, Some(Step::Spec(inner))),
                TypeRefSpec::NonNull(inner) => (TypePart::NonNull, Some(Step::Spec(inner))),
            },
        };
        self.next = inner;
        Some(part)
    }
}

impl Parts<'_> {
    /// Returns the type expression as GraphQL writes it, such as
    /// `[Project!]!`.
    #[must_use]
    pub fn text(self) -> String {
        let mut out = String::new();
        let mut close = Vec::new();
        for part in self {
            match part {
                TypePart::List => {
                    out.push('[');
                    close.push(']');
                }
                TypePart::NonNull => close.push('!'),
                TypePart::Named(name) => out.push_str(name),
            }
        }
        out.extend(close.iter().rev());
        out
    }
}
