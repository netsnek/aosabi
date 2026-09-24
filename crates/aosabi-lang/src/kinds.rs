// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the kind of a node and the kind of a token, with the string
 * each one carries in graphql-js.
 *
 * Derived from graphql-js 16.12.0, src/language/kinds.ts, MIT.
 * Derived from graphql-js 16.12.0, src/language/tokenKind.ts, MIT.
 */

//! The two kind enums of the language layer.
//!
//! graphql-js declares both as string enums, and the string is observable: a
//! syntax error names the token kind it found, so `Unexpected Name "x"` and
//! `Expected "{", found <EOF>` carry [`TokenKind`] values verbatim. Each
//! Rust enum keeps the member names of the reference with Rust casing
//! applied, and [`Kind::as_str`] and [`TokenKind::as_str`] return the string
//! the reference member is assigned, copied from the vendored source. The
//! deprecated `KindEnum` and `TokenKindEnum` type aliases describe the
//! TypeScript enum object itself and have no Rust reading.

use core::fmt;

/// The set of allowed kind values for AST nodes.
///
/// The member order is the order of the reference.
///
/// Derived from graphql-js 16.12.0, src/language/kinds.ts, MIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// `Name`.
    Name,
    /// `Document`.
    Document,
    /// `OperationDefinition`.
    OperationDefinition,
    /// `VariableDefinition`.
    VariableDefinition,
    /// `SelectionSet`.
    SelectionSet,
    /// `Field`.
    Field,
    /// `Argument`.
    Argument,
    /// `FragmentSpread`.
    FragmentSpread,
    /// `InlineFragment`.
    InlineFragment,
    /// `FragmentDefinition`.
    FragmentDefinition,
    /// `Variable`.
    Variable,
    /// `IntValue`.
    Int,
    /// `FloatValue`.
    Float,
    /// `StringValue`.
    String,
    /// `BooleanValue`.
    Boolean,
    /// `NullValue`.
    Null,
    /// `EnumValue`.
    Enum,
    /// `ListValue`.
    List,
    /// `ObjectValue`.
    Object,
    /// `ObjectField`.
    ObjectField,
    /// `Directive`.
    Directive,
    /// `NamedType`.
    NamedType,
    /// `ListType`.
    ListType,
    /// `NonNullType`.
    NonNullType,
    /// `SchemaDefinition`.
    SchemaDefinition,
    /// `OperationTypeDefinition`.
    OperationTypeDefinition,
    /// `ScalarTypeDefinition`.
    ScalarTypeDefinition,
    /// `ObjectTypeDefinition`.
    ObjectTypeDefinition,
    /// `FieldDefinition`.
    FieldDefinition,
    /// `InputValueDefinition`.
    InputValueDefinition,
    /// `InterfaceTypeDefinition`.
    InterfaceTypeDefinition,
    /// `UnionTypeDefinition`.
    UnionTypeDefinition,
    /// `EnumTypeDefinition`.
    EnumTypeDefinition,
    /// `EnumValueDefinition`.
    EnumValueDefinition,
    /// `InputObjectTypeDefinition`.
    InputObjectTypeDefinition,
    /// `DirectiveDefinition`.
    DirectiveDefinition,
    /// `SchemaExtension`.
    SchemaExtension,
    /// `ScalarTypeExtension`.
    ScalarTypeExtension,
    /// `ObjectTypeExtension`.
    ObjectTypeExtension,
    /// `InterfaceTypeExtension`.
    InterfaceTypeExtension,
    /// `UnionTypeExtension`.
    UnionTypeExtension,
    /// `EnumTypeExtension`.
    EnumTypeExtension,
    /// `InputObjectTypeExtension`.
    InputObjectTypeExtension,
    /// `TypeCoordinate`.
    TypeCoordinate,
    /// `MemberCoordinate`.
    MemberCoordinate,
    /// `ArgumentCoordinate`.
    ArgumentCoordinate,
    /// `DirectiveCoordinate`.
    DirectiveCoordinate,
    /// `DirectiveArgumentCoordinate`.
    DirectiveArgumentCoordinate,
}

impl Kind {
    /// Returns the string the reference assigns to this kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Document => "Document",
            Self::OperationDefinition => "OperationDefinition",
            Self::VariableDefinition => "VariableDefinition",
            Self::SelectionSet => "SelectionSet",
            Self::Field => "Field",
            Self::Argument => "Argument",
            Self::FragmentSpread => "FragmentSpread",
            Self::InlineFragment => "InlineFragment",
            Self::FragmentDefinition => "FragmentDefinition",
            Self::Variable => "Variable",
            Self::Int => "IntValue",
            Self::Float => "FloatValue",
            Self::String => "StringValue",
            Self::Boolean => "BooleanValue",
            Self::Null => "NullValue",
            Self::Enum => "EnumValue",
            Self::List => "ListValue",
            Self::Object => "ObjectValue",
            Self::ObjectField => "ObjectField",
            Self::Directive => "Directive",
            Self::NamedType => "NamedType",
            Self::ListType => "ListType",
            Self::NonNullType => "NonNullType",
            Self::SchemaDefinition => "SchemaDefinition",
            Self::OperationTypeDefinition => "OperationTypeDefinition",
            Self::ScalarTypeDefinition => "ScalarTypeDefinition",
            Self::ObjectTypeDefinition => "ObjectTypeDefinition",
            Self::FieldDefinition => "FieldDefinition",
            Self::InputValueDefinition => "InputValueDefinition",
            Self::InterfaceTypeDefinition => "InterfaceTypeDefinition",
            Self::UnionTypeDefinition => "UnionTypeDefinition",
            Self::EnumTypeDefinition => "EnumTypeDefinition",
            Self::EnumValueDefinition => "EnumValueDefinition",
            Self::InputObjectTypeDefinition => "InputObjectTypeDefinition",
            Self::DirectiveDefinition => "DirectiveDefinition",
            Self::SchemaExtension => "SchemaExtension",
            Self::ScalarTypeExtension => "ScalarTypeExtension",
            Self::ObjectTypeExtension => "ObjectTypeExtension",
            Self::InterfaceTypeExtension => "InterfaceTypeExtension",
            Self::UnionTypeExtension => "UnionTypeExtension",
            Self::EnumTypeExtension => "EnumTypeExtension",
            Self::InputObjectTypeExtension => "InputObjectTypeExtension",
            Self::TypeCoordinate => "TypeCoordinate",
            Self::MemberCoordinate => "MemberCoordinate",
            Self::ArgumentCoordinate => "ArgumentCoordinate",
            Self::DirectiveCoordinate => "DirectiveCoordinate",
            Self::DirectiveArgumentCoordinate => "DirectiveArgumentCoordinate",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An enum describing the different kinds of tokens that the lexer emits.
///
/// The member order is the order of the reference.
///
/// Derived from graphql-js 16.12.0, src/language/tokenKind.ts, MIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// `<SOF>`, the start of the file.
    Sof,
    /// `<EOF>`, the end of the file.
    Eof,
    /// `!`.
    Bang,
    /// `$`.
    Dollar,
    /// `&`.
    Amp,
    /// `(`.
    ParenL,
    /// `)`.
    ParenR,
    /// `.`.
    Dot,
    /// `...`.
    Spread,
    /// `:`.
    Colon,
    /// `=`.
    Equals,
    /// `@`.
    At,
    /// `[`.
    BracketL,
    /// `]`.
    BracketR,
    /// `{`.
    BraceL,
    /// `|`.
    Pipe,
    /// `}`.
    BraceR,
    /// `Name`.
    Name,
    /// `Int`.
    Int,
    /// `Float`.
    Float,
    /// `String`.
    String,
    /// `BlockString`.
    BlockString,
    /// `Comment`.
    Comment,
}

impl TokenKind {
    /// Returns the string the reference assigns to this token kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sof => "<SOF>",
            Self::Eof => "<EOF>",
            Self::Bang => "!",
            Self::Dollar => "$",
            Self::Amp => "&",
            Self::ParenL => "(",
            Self::ParenR => ")",
            Self::Dot => ".",
            Self::Spread => "...",
            Self::Colon => ":",
            Self::Equals => "=",
            Self::At => "@",
            Self::BracketL => "[",
            Self::BracketR => "]",
            Self::BraceL => "{",
            Self::Pipe => "|",
            Self::BraceR => "}",
            Self::Name => "Name",
            Self::Int => "Int",
            Self::Float => "Float",
            Self::String => "String",
            Self::BlockString => "BlockString",
            Self::Comment => "Comment",
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
