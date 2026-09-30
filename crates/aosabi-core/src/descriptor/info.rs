// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the descriptor of a named type and the records it lists.
 *
 * No analogue on the reference side.
 */

//! The descriptor of a named type and the records it lists.

use super::{
    facts::{
        DecoratorFact,
        GuardFact, //
    },
    link::TypeLink, //
};

/// What a named type is to the inference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TypeKind {
    /// A struct, the twin of a TypeScript class.
    Object,
    /// An enum whose variants carry no data, the twin of a union of string
    /// literals.
    Enum,
    /// An enum whose variants carry no data under `#[aosabi(enum)]`, the twin
    /// of a TypeScript `enum` declaration, which carries a symbol and so the
    /// documentation a union of string literals lacks.
    EnumDeclaration,
    /// An enum whose variants each carry one payload, the twin of a union of
    /// object types.
    Union,
    /// A newtype under `#[aosabi::scalar]`, the twin of a custom scalar.
    Scalar,
}

/// The documentation of a described item, as the checker hands it over.
///
/// The checker answers a symbol with its documentation comment and, apart from
/// it, with its tags, so the descriptor carries the two apart, and the
/// inference composes them as the reference does. The macros read both from the
/// last documentation block of the item by the rule of `docs/authoring.md`
/// section 29.5.
#[derive(Clone, Copy, Debug)]
pub struct Docs {
    /// The comment, the text before the first tag, empty when the item has
    /// none.
    pub comment: &'static str,
    /// The tags in source order.
    pub tags: &'static [DocTag],
}

impl Docs {
    /// The documentation of an item that carries none.
    pub const NONE: Self = Self {
        comment: "",
        tags: &[],
    };
}

/// One tag of a documentation comment, as `symbol.getJsDocTags` lists it.
#[derive(Clone, Copy, Debug)]
pub struct DocTag {
    /// The tag name without the `@`.
    pub name: &'static str,
    /// The tag text, empty for a tag with none.
    pub text: &'static str,
}

/// Where a described item stands in its source and the documentation block it
/// carries there, as written.
///
/// The build writes these facts into `authoring.json`, which check C2 of
/// `tests/notebooks/12_authoring_parity.ipynb` compares with the declarations
/// of the TypeScript twin. The inference reads none of them.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    /// The value of `file!()` at the macro that describes the item.
    pub file: &'static str,
    /// The source line of the name of the item, counted from 1.
    pub line: u32,
    /// The lines of the last documentation block, a `///` line without its
    /// slashes and one space and a `/** */` line without its gutter, with
    /// trailing whitespace and trailing blank lines removed.
    pub docs: &'static [&'static str],
    /// The number of documentation blocks the item carries.
    pub blocks: u32,
}

impl Site {
    /// Returns the site of an item at `line` of `file` with no documentation.
    #[must_use]
    pub const fn bare(file: &'static str, line: u32) -> Self {
        Self {
            file,
            line,
            docs: &[],
            blocks: 0,
        }
    }
}

/// Whether a member is visible outside its type.
///
/// `pub` and every restricted `pub(...)` are [`Visibility::Public`], and a
/// member with no visibility keyword is [`Visibility::Private`], the twin of a
/// TypeScript `private` member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// The member becomes a field of the schema.
    Public,
    /// The member is skipped.
    Private,
}

/// The place in the source where a macro ran.
///
/// The order of the fields is the order of the comparison, so sorting by this
/// key sorts by file, then by line, then by column. The inference sorts the
/// declarations of an application by it, because the order of the
/// declaration list is the order of a scan of the module tree, which is no
/// order a schema may depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceKey {
    /// The value of `file!()` at the macro, or for a scalar marker of this
    /// crate the path of its file inside the crate directory.
    pub file: &'static str,
    /// The value of `line!()` at the macro.
    pub line: u32,
    /// The value of `column!()` at the macro.
    pub column: u32,
}

/// The descriptor of a named type.
///
/// One of the three macros writes it as a promoted constant, and
/// [󰀀](super::TypeExpr::Named) carries the reference. Every list keeps the
/// order of the source, because the order of the inference walk is the order of
/// the schema. A list that does not apply to the [`TypeKind`] is empty.
#[derive(Debug)]
pub struct TypeInfo {
    /// What the type is to the inference.
    pub kind: TypeKind,
    /// The declared identifier with a raw prefix removed, or the name a scalar
    /// states.
    pub name: &'static str,
    /// The value of `CARGO_PKG_NAME` in the crate that declares the type.
    pub krate: &'static str,
    /// The place of the macro that wrote the descriptor.
    pub source: SourceKey,
    /// The documentation of the type.
    pub docs: Docs,
    /// Where the type stands and the documentation block it carries.
    pub site: Site,
    /// The name override, the twin of the `__typename` literal of a TypeScript
    /// class.
    pub typename: Option<&'static str>,
    /// Whether the type stands for an anonymous TypeScript type, which has no
    /// alias symbol to take a name from.
    pub anonymous: bool,
    /// The fields of a struct, in declaration order.
    pub fields: &'static [FieldInfo],
    /// The methods of the resolvers block, in declaration order.
    pub methods: &'static [MethodInfo],
    /// The variants of a [`TypeKind::Enum`] or a [`TypeKind::EnumDeclaration`],
    /// in declaration order.
    pub variants: &'static [VariantInfo],
    /// The payload types of a [`TypeKind::Union`], in variant order.
    pub members: &'static [TypeLink],
    /// One link per generic type parameter, in declaration order.
    pub type_arguments: &'static [TypeLink],
    /// The base types the type names, in attribute order.
    pub heritage: &'static [HeritageInfo],
}

/// A field of a struct.
#[derive(Debug)]
pub struct FieldInfo {
    /// The identifier with a raw prefix removed.
    pub name: &'static str,
    /// The documentation of the field.
    pub docs: Docs,
    /// Where the field stands and the documentation block it carries.
    pub site: Site,
    /// Whether the field is visible outside the struct.
    pub visibility: Visibility,
    /// The type of the field.
    pub r#type: TypeLink,
    /// Whether the field stands for an object spread, `{...value}`, whose
    /// members the inference presents in place of the field.
    pub spread: bool,
}

/// A method of a resolvers block, the function of an inline entry of
/// `graphql!` or a free function under `#[aosabi::resolver]`.
///
/// A method whose receiver is `&self` is a member of its type, and a function
/// with no receiver is the twin of a static member or of a module function,
/// which is never a field of its type and which an entry of `graphql!`
/// references.
#[derive(Debug)]
pub struct MethodInfo {
    /// The identifier with a raw prefix removed.
    pub name: &'static str,
    /// The documentation of the method.
    pub docs: Docs,
    /// Where the method stands and the documentation block it carries.
    pub site: Site,
    /// Whether the method is visible outside the type.
    pub visibility: Visibility,
    /// Whether the method is an `async fn`.
    pub is_async: bool,
    /// The parameters after the receiver, in declaration order.
    pub params: &'static [ParamInfo],
    /// The return type, which is a [`Promise`](super::Promise) for an
    /// `async fn` and `()` where the method states none.
    pub returns: TypeLink,
    /// Whether the function has no receiver, a static member or a free
    /// function.
    pub receiverless: bool,
    /// The decorators of the function in source order, the outermost first.
    pub decorators: &'static [DecoratorFact],
    /// The guards of the leading run of the body in source order, empty for
    /// a body that opens with no guard.
    pub guards: &'static [GuardFact],
}

/// A parameter of a method.
#[derive(Debug)]
pub struct ParamInfo {
    /// The identifier with a raw prefix removed.
    pub name: &'static str,
    /// The type of the parameter.
    pub r#type: TypeLink,
    /// Whether the parameter carries a default, the twin of a TypeScript
    /// parameter with an initializer.
    pub optional: bool,
    /// The documentation of the parameter, its own comment composed with the
    /// `@param` tags of the method that name it.
    pub docs: Docs,
    /// Where the parameter stands and the documentation block it carries.
    pub site: Site,
}

/// A variant of an enum whose variants carry no data.
#[derive(Debug)]
pub struct VariantInfo {
    /// The identifier with a raw prefix removed.
    pub name: &'static str,
    /// The string the variant stands for, which is the name unless the variant
    /// states a value.
    pub value: &'static str,
    /// The documentation of the variant.
    pub docs: Docs,
    /// Where the variant stands and the documentation block it carries.
    pub site: Site,
}

/// How a type names a base type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeritageKind {
    /// The twin of a TypeScript `extends` clause, stated on the type.
    Extends,
    /// The twin of a TypeScript `implements` clause.
    Implements,
    /// The twin of a TypeScript `extends` clause, stated as a field under
    /// `#[aosabi(extends)]` that holds the base.
    ///
    /// The field is no member of the type. The inference presents the members
    /// of the base after the own members, nearest base first, and a member the
    /// type redeclares keeps its own position, which is how the checker lists
    /// the members of a derived class. The relation is an `extends` clause to
    /// the heritage sweep.
    Embedded {
        /// The name of the field that holds the base.
        field: &'static str,
    },
}

impl HeritageKind {
    /// Returns whether the relation is an `extends` clause of the twin, stated
    /// on the type or as an embedded base.
    #[must_use]
    pub const fn is_extends(self) -> bool {
        matches!(self, Self::Extends | Self::Embedded { .. })
    }
}

/// One base type a type names.
#[derive(Debug)]
pub struct HeritageInfo {
    /// The clause the base type stands in.
    pub kind: HeritageKind,
    /// The base type.
    pub base: TypeLink,
}
