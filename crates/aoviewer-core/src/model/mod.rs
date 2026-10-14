// SPDX-License-Identifier: MIT
/*
 * aoviewer-core model: the arena model of a schema, its builder and its
 * accessors.
 *
 * Both readers fill one model of types, fields, arguments, enum values,
 * directives, descriptions and interned names.
 */

//! The schema model.

mod arena;
mod builder;
mod expression;
mod query;

pub use arena::{
    Arg,
    ArgId,
    Deprecation,
    Directive,
    DirectiveId,
    EnumValue,
    Field,
    FieldId,
    Kind,
    Model,
    Name,
    RootKind,
    Roots,
    Type,
    TypeId,
    TypeRef,
    ValueId, //
};
pub use builder::{
    Builder,
    ModelError,
    TypeRefSpec, //
};
pub use expression::{
    Parts,
    TypePart, //
};
pub use query::RefKind;
