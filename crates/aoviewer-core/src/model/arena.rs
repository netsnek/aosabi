// SPDX-License-Identifier: MIT
/*
 * aoviewer-core model arena: the ids, the interned names and the element
 * records of the model.
 *
 * Every element lives in an arena addressed by a small integer id, so a view
 * holds ids and no references.
 */

//! The arenas of the model.

use alloc::{
    boxed::Box,
    string::String,
    vec::Vec, //
};

use super::query::RefKind;

/// The id of a named type, its position in schema order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeId(pub(crate) u32);

/// The id of a field or an input field, its position in schema order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FieldId(pub(crate) u32);

/// The id of an argument of a field or of a directive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArgId(pub(crate) u32);

/// The id of an enum value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValueId(pub(crate) u32);

/// The id of a directive definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirectiveId(pub(crate) u32);

/// An interned name, which compares and orders as bytes.
///
/// [`Builder::finish`](super::Builder::finish) numbers the names in the byte
/// order of their UTF-8 text, so the derived order of two names is the order
/// of their bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Name(pub(crate) u32);

/// Implements `index` for an id, the position of its element in its arena.
macro_rules! arena_index {
    ($($id:ident),*) => {$(
        impl $id {
            /// Returns the position of the element in its arena, which a
            /// caller uses to index a table of its own beside the model.
            #[must_use]
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }
    )*};
}

arena_index!(TypeId, FieldId, ArgId, ValueId, DirectiveId, Name);

impl FieldId {
    /// The owner an argument of a directive carries, which names no field.
    ///
    /// The arguments of fields and of directives share one arena, and only
    /// those of fields have a field above them.
    pub const DIRECTIVE: FieldId = FieldId(u32::MAX);
}

/// The kind of a named type, in the order the index lists kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// An object type.
    Object,
    /// An interface type.
    Interface,
    /// A union type.
    Union,
    /// An enum type.
    Enum,
    /// An input object type.
    Input,
    /// A scalar type.
    Scalar,
}

impl Kind {
    /// Returns the value of `__TypeKind` that introspection answers for the
    /// kind, such as `OBJECT` or `INPUT_OBJECT`.
    ///
    /// The six values are those of a named type, and the two of a wrapper are
    /// no kind of the model.
    ///
    /// [specification]: https://spec.graphql.org/October2021/
    #[must_use]
    pub const fn introspection_word(self) -> &'static str {
        match self {
            Kind::Object => "OBJECT",
            Kind::Interface => "INTERFACE",
            Kind::Union => "UNION",
            Kind::Enum => "ENUM",
            Kind::Input => "INPUT_OBJECT",
            Kind::Scalar => "SCALAR",
        }
    }
}

/// One of the three operation roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RootKind {
    /// The query root.
    Query,
    /// The mutation root.
    Mutation,
    /// The subscription root.
    Subscription,
}

impl RootKind {
    /// Returns the keyword of the operation type of the root, `query`,
    /// `mutation` or `subscription`.
    ///
    /// The three are the words of `OperationType`, and an operation route,
    /// the file of a root page and a sentence of the generator write a root
    /// by them.
    ///
    /// [specification]: https://spec.graphql.org/October2021/
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            RootKind::Query => "query",
            RootKind::Mutation => "mutation",
            RootKind::Subscription => "subscription",
        }
    }
}

/// A type reference with its wrappers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRef {
    /// A named type.
    Named(TypeId),
    /// A list of the inner reference.
    List(Box<TypeRef>),
    /// The inner reference, never null.
    NonNull(Box<TypeRef>),
}

impl TypeRef {
    /// Returns the named type inside every wrapper.
    #[must_use]
    pub fn named(&self) -> TypeId {
        let mut cur = self;
        // The wrappers nest at most `ReadLimits::max_depth` deep, which the
        // builder checks, and each iteration strips one.
        loop {
            match cur {
                TypeRef::Named(id) => return *id,
                TypeRef::List(inner) | TypeRef::NonNull(inner) => cur = inner,
            }
        }
    }
}

/// A deprecation the schema states by directive.
///
/// The tag inside a description is a mark of the transforms, never of the
/// model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deprecation {
    /// The reason the directive gives, if any.
    pub reason: Option<String>,
}

/// A named type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type {
    /// The name.
    pub name: Name,
    /// The kind.
    pub kind: Kind,
    /// The description.
    pub description: Option<String>,
    /// The fields of an object or an interface and the input fields of an
    /// input type, in schema order.
    pub fields: Vec<FieldId>,
    /// The values of an enum, in schema order.
    pub values: Vec<ValueId>,
    /// The members of a union, in schema order.
    pub members: Vec<TypeId>,
    /// The interfaces an object or an interface implements, in schema order.
    pub implements: Vec<TypeId>,
    /// The URL a scalar names as its specification.
    pub specified_by_url: Option<String>,
}

/// A field of an object or an interface, or an input field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The type the field belongs to.
    pub owner: TypeId,
    /// The name.
    pub name: Name,
    /// The description.
    pub description: Option<String>,
    /// The type the field returns or takes.
    pub type_ref: TypeRef,
    /// The arguments, in schema order.
    pub args: Vec<ArgId>,
    /// The default value of an input field as the schema prints it.
    pub default_value: Option<String>,
    /// The deprecation, if the schema states one.
    pub deprecation: Option<Deprecation>,
}

/// An argument of a field or of a directive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arg {
    /// The field the argument belongs to, or [`FieldId::DIRECTIVE`].
    pub owner: FieldId,
    /// The name.
    pub name: Name,
    /// The description.
    pub description: Option<String>,
    /// The type the argument takes.
    pub type_ref: TypeRef,
    /// The default value as the schema prints it.
    pub default_value: Option<String>,
    /// The deprecation, if the schema states one.
    pub deprecation: Option<Deprecation>,
}

/// A value of an enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumValue {
    /// The enum the value belongs to.
    pub owner: TypeId,
    /// The name.
    pub name: Name,
    /// The description.
    pub description: Option<String>,
    /// The deprecation, if the schema states one.
    pub deprecation: Option<Deprecation>,
}

/// A directive definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directive {
    /// The name, without the `@`.
    pub name: Name,
    /// The description.
    pub description: Option<String>,
    /// The locations, in schema order.
    pub locations: Vec<String>,
    /// The arguments, in schema order.
    pub args: Vec<ArgId>,
    /// Whether the directive may repeat at one location.
    pub repeatable: bool,
}

/// The three operation roots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Roots {
    /// The query root.
    pub query: Option<TypeId>,
    /// The mutation root.
    pub mutation: Option<TypeId>,
    /// The subscription root.
    pub subscription: Option<TypeId>,
}

impl Roots {
    /// Returns the root of `kind`, if the schema names one.
    #[must_use]
    pub fn get(&self, kind: RootKind) -> Option<TypeId> {
        match kind {
            RootKind::Query => self.query,
            RootKind::Mutation => self.mutation,
            RootKind::Subscription => self.subscription,
        }
    }

    /// Returns the first kind of root the type `ty` is, in the order query,
    /// mutation, subscription, or `None` for a type that is no root.
    #[must_use]
    pub fn kind_of(&self, ty: TypeId) -> Option<RootKind> {
        [RootKind::Query, RootKind::Mutation, RootKind::Subscription]
            .into_iter()
            .find(|&kind| self.get(kind) == Some(ty))
    }
}

/// The whole schema as arenas, read only after
/// [`Builder::finish`](super::Builder::finish).
///
/// Every element is addressed by its id. The names live in one text buffer in
/// byte order, and the implementer and referrer tables are computed once by
/// the builder, each as one list with a start per type.
#[derive(Clone, Debug)]
pub struct Model {
    pub(super) description: Option<String>,
    pub(super) roots: Roots,
    pub(super) order: Vec<TypeId>,
    pub(super) types: Vec<Type>,
    pub(super) fields: Vec<Field>,
    pub(super) args: Vec<Arg>,
    pub(super) values: Vec<EnumValue>,
    pub(super) directive_order: Vec<DirectiveId>,
    pub(super) directives: Vec<Directive>,
    /// The text of every name, concatenated in byte order.
    pub(super) name_text: String,
    /// The end of each name inside `name_text`, one per [`Name`].
    pub(super) name_ends: Vec<usize>,
    /// Every type sorted by name, for the lookup by name.
    pub(super) by_name: Vec<TypeId>,
    /// The implementers of every interface, in schema order.
    pub(super) implementers: Vec<TypeId>,
    /// The start of each type's run inside `implementers`, one per type and
    /// one more.
    pub(super) implementer_starts: Vec<usize>,
    /// Every field and argument naming a type, grouped by the type.
    pub(super) referrers: Vec<(FieldId, RefKind)>,
    /// The start of each type's run inside `referrers`, one per type and one
    /// more.
    pub(super) referrer_starts: Vec<usize>,
}
