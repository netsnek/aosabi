// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the kind predicates over a node.
 *
 * Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
 */

//! The kind predicates of the language layer.
//!
//! Each predicate answers whether a node belongs to one of the unions of
//! [`crate::ast`] by reading its kind, the way the reference narrows an
//! `ASTNode` to a union type. The narrowing itself is the conversion the
//! union enums carry, so a predicate answers a `bool` and nothing more.
//! Every predicate takes a [`NodeRef`], the borrowed reading of `ASTNode`,
//! so a node reached by the walk or held in a union passes through `into()`.

use crate::{
    kinds::Kind,
    visitor::NodeRef, //
};

/// Returns whether the node is a definition of a document.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_definition_node(node: NodeRef<'_>) -> bool {
    is_executable_definition_node(node)
        || is_type_system_definition_node(node)
        || is_type_system_extension_node(node)
}

/// Returns whether the node is an operation or a fragment definition.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_executable_definition_node(node: NodeRef<'_>) -> bool {
    matches!(
        node.kind(),
        Kind::OperationDefinition | Kind::FragmentDefinition
    )
}

/// Returns whether the node is a field, a fragment spread or an inline
/// fragment.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_selection_node(node: NodeRef<'_>) -> bool {
    matches!(
        node.kind(),
        Kind::Field | Kind::FragmentSpread | Kind::InlineFragment
    )
}

/// Returns whether the node is a value, a variable included.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_value_node(node: NodeRef<'_>) -> bool {
    matches!(
        node.kind(),
        Kind::Variable
            | Kind::Int
            | Kind::Float
            | Kind::String
            | Kind::Boolean
            | Kind::Null
            | Kind::Enum
            | Kind::List
            | Kind::Object
    )
}

/// Returns whether the node is a value the reference reads as constant.
///
/// A list or an object is tested with `some`, which answers `false` on an
/// empty array and `true` as soon as one item passes, so an empty list is
/// not constant here and a list with one constant beside a variable is. The
/// port keeps that reading.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_const_value_node(node: NodeRef<'_>) -> bool {
    is_value_node(node)
        && match node {
            NodeRef::ListValue(list) => list
                .values
                .iter()
                .any(|value| is_const_value_node(value.into())),
            NodeRef::ObjectValue(object) => object
                .fields
                .iter()
                .any(|field| is_const_value_node((&field.value).into())),
            _ => node.kind() != Kind::Variable,
        }
}

/// Returns whether the node is a type reference.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_type_node(node: NodeRef<'_>) -> bool {
    matches!(
        node.kind(),
        Kind::NamedType | Kind::ListType | Kind::NonNullType
    )
}

/// Returns whether the node is a type system definition.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_type_system_definition_node(node: NodeRef<'_>) -> bool {
    node.kind() == Kind::SchemaDefinition
        || is_type_definition_node(node)
        || node.kind() == Kind::DirectiveDefinition
}

/// Returns whether the node is a type definition.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_type_definition_node(node: NodeRef<'_>) -> bool {
    matches!(
        node.kind(),
        Kind::ScalarTypeDefinition
            | Kind::ObjectTypeDefinition
            | Kind::InterfaceTypeDefinition
            | Kind::UnionTypeDefinition
            | Kind::EnumTypeDefinition
            | Kind::InputObjectTypeDefinition
    )
}

/// Returns whether the node is a type system extension.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_type_system_extension_node(node: NodeRef<'_>) -> bool {
    node.kind() == Kind::SchemaExtension || is_type_extension_node(node)
}

/// Returns whether the node is a type extension.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_type_extension_node(node: NodeRef<'_>) -> bool {
    matches!(
        node.kind(),
        Kind::ScalarTypeExtension
            | Kind::ObjectTypeExtension
            | Kind::InterfaceTypeExtension
            | Kind::UnionTypeExtension
            | Kind::EnumTypeExtension
            | Kind::InputObjectTypeExtension
    )
}

/// Returns whether the node is a schema coordinate.
///
/// Derived from graphql-js 16.12.0, src/language/predicates.ts, MIT.
#[must_use]
pub fn is_schema_coordinate_node(node: NodeRef<'_>) -> bool {
    matches!(
        node.kind(),
        Kind::TypeCoordinate
            | Kind::MemberCoordinate
            | Kind::ArgumentCoordinate
            | Kind::DirectiveCoordinate
            | Kind::DirectiveArgumentCoordinate
    )
}
