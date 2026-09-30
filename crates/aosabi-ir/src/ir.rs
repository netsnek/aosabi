// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-ir: the model of the intermediate representation, the
 * normalized description of an application that every generated artifact is
 * projected from.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/ir/ir.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The persistence vocabulary, the migration types and the identifier helpers
 * of :31-225 are not translated here.
 */

//! The model of the intermediate representation.
//!
//! Every type keeps the name and the member order of its interface in
//! `ir.ts`. An optional member of the reference, written `name?: T` there, is
//! an `Option` here, and `None` stands for the absent key and for `undefined`
//! alike, which `JSON.stringify` does not tell apart either. A
//! `Record<string, T>` of the reference is a [`Record`], which keeps the key
//! order a JavaScript object keeps.
//!
//! The three persistence positions, [`Field::column`], [`Field::relation`]
//! and [`Entity::indexes`], are carried as an opaque [`JsValue`]. The printer
//! reads none of them, so an entity that a reference test builds by hand
//! replays without loss, and the database milestone types them when it ports
//! their producer.
//!
//! [stringify]: https://tc39.es/ecma262/#sec-json.stringify

use alloc::{
    boxed::Box,
    string::String,
    vec::Vec, //
};

use aosabi_json::JsValue;

use crate::record::Record;

/// A normalized type reference, the single answer to what shape a position
/// has.
///
/// The `kind` tag of the reference is the variant. `ScalarName` of :20-29 is
/// an open set of strings, so a scalar name is a `String`.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeRef {
    /// A scalar, `kind: 'scalar'`.
    Scalar {
        /// The scalar name, such as `String` or `Number`.
        name: String,
        /// Whether the position accepts null.
        nullable: bool,
    },
    /// A reference to an entity, an object, an interface, a union, an input
    /// or an enum by its name, `kind: 'ref'`.
    Ref {
        /// The name of the type referred to.
        name: String,
        /// Whether the position accepts null.
        nullable: bool,
    },
    /// A list, `kind: 'list'`.
    List {
        /// The element type.
        of: Box<TypeRef>,
        /// Whether the list itself may be null.
        nullable: bool,
    },
}

impl TypeRef {
    /// Returns the `nullable` member, which every variant carries.
    #[must_use]
    pub fn nullable(&self) -> bool {
        match self {
            Self::Scalar { nullable, .. }
            | Self::Ref { nullable, .. }
            | Self::List { nullable, .. } => *nullable,
        }
    }
}

/// The name of a root operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RootName {
    /// The `Query` root.
    Query,
    /// The `Mutation` root.
    Mutation,
    /// The `Subscription` root.
    Subscription,
}

impl RootName {
    /// The three roots in the order the printer groups them by.
    pub const ALL: [RootName; 3] = [Self::Query, Self::Mutation, Self::Subscription];

    /// Returns the string the reference writes for the root.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Query => "Query",
            Self::Mutation => "Mutation",
            Self::Subscription => "Subscription",
        }
    }
}

/// A single member of an entity, an object, an interface or an input, and a
/// single argument.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The name the API shows.
    pub name: String,
    /// The type of the member.
    pub r#type: TypeRef,
    /// Whether the member appears in the generated API.
    pub exposed: bool,
    /// The GraphQL description, inserted verbatim by the printer.
    pub description: Option<String>,
    /// The GraphQL arguments of a callable member, in the shape of
    /// [`Operation::args`].
    pub args: Option<Vec<Field>>,
    /// The `ColumnSpec` of a persisted member, opaque.
    pub column: Option<JsValue>,
    /// The `RelationSpec` of a relation member, opaque.
    pub relation: Option<JsValue>,
}

/// A persisted object type that a table backs.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    /// The type name.
    pub name: String,
    /// The table name.
    pub table: String,
    /// Whether the entity is abstract.
    pub r#abstract: bool,
    /// The members in declaration order.
    pub fields: Vec<Field>,
    /// The property name of the primary key member, if any.
    pub primary_key: Option<String>,
    /// The interface names the type implements.
    pub implements: Vec<String>,
    /// The `IndexSpec` list of the secondary indexes, opaque.
    pub indexes: Option<JsValue>,
    /// Whether the entity shares the physical table of another entity.
    pub shared_table: Option<bool>,
}

/// An object type that is not persisted.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectType {
    /// The type name.
    pub name: String,
    /// The members in declaration order.
    pub fields: Vec<Field>,
    /// The interface names the type implements.
    pub implements: Option<Vec<String>>,
    /// The GraphQL description.
    pub description: Option<String>,
}

/// A GraphQL interface.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceType {
    /// The interface name.
    pub name: String,
    /// The members in declaration order.
    pub fields: Vec<Field>,
    /// The interface names the interface implements.
    pub implements: Option<Vec<String>>,
    /// The GraphQL description.
    pub description: Option<String>,
}

/// A GraphQL enum.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumType {
    /// The enum name.
    pub name: String,
    /// The value names in declaration order.
    pub values: Vec<String>,
    /// The GraphQL description.
    pub description: Option<String>,
}

/// A GraphQL union.
#[derive(Debug, Clone, PartialEq)]
pub struct UnionType {
    /// The union name.
    pub name: String,
    /// The member type names in the order the printer joins them.
    pub members: Vec<String>,
    /// The GraphQL description.
    pub description: Option<String>,
}

/// A GraphQL input object, the shape of a resolver argument.
#[derive(Debug, Clone, PartialEq)]
pub struct InputType {
    /// The input name.
    pub name: String,
    /// The members in declaration order.
    pub fields: Vec<Field>,
    /// The GraphQL description.
    pub description: Option<String>,
}

/// A root resolver.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    /// The root the resolver belongs to.
    pub root: RootName,
    /// The field name on the root type.
    pub name: String,
    /// The arguments in declaration order.
    pub args: Vec<Field>,
    /// The return type.
    pub returns: TypeRef,
    /// The GraphQL description.
    pub description: Option<String>,
}

/// The whole normalized application.
///
/// The reference types `version` as the literal `1`, and
/// [`crate::json::from_js_value`] refuses every other number.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaIr {
    /// The version of the model, which is `1`.
    pub version: u32,
    /// The persisted object types by name.
    pub entities: Record<Entity>,
    /// The object types that are not persisted, by name.
    pub objects: Record<ObjectType>,
    /// The interfaces by name.
    pub interfaces: Record<InterfaceType>,
    /// The unions by name.
    pub unions: Record<UnionType>,
    /// The input objects by name.
    pub inputs: Record<InputType>,
    /// The enums by name.
    pub enums: Record<EnumType>,
    /// The scalar names to emit, custom and built in, in emission order.
    pub scalars: Vec<String>,
    /// The root resolvers in walk order.
    pub operations: Vec<Operation>,
}

/// Returns an empty IR, the starting point a contributor pipeline builds on.
#[must_use]
pub fn empty_ir() -> SchemaIr {
    SchemaIr {
        version: 1,
        entities: Record::new(),
        objects: Record::new(),
        interfaces: Record::new(),
        unions: Record::new(),
        inputs: Record::new(),
        enums: Record::new(),
        scalars: Vec::new(),
        operations: Vec::new(),
    }
}
