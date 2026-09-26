// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the lookup that turns a type node of a document into the type
 * of a schema.
 *
 * Derived from graphql-js 16.12.0, src/utilities/typeFromAST.ts, MIT.
 */

//! The type a type node names in a schema.
//!
//! The reference declares four overloads that narrow the return type by the
//! kind of the node, and one body serves all of them. The port has the one
//! body, and a caller that knows the node kind narrows the [`Type`] with the
//! accessors of the definition module.

use crate::{
    definition::{
        ListType,
        NonNullType,
        Type, //
    },
    schema::Schema, //
};
use aosabi_lang::ast::{
    NullableTypeNode,
    TypeNode, //
};

/// Given a Schema and an AST node describing a type, return a `GraphQLType`
/// definition which applies to that type.
///
/// For example, if provided the parsed AST node for `[User]`, a [`ListType`]
/// is returned, containing the type called `User` found in the schema. If a
/// type called `User` is not found in the schema, then `None` is returned.
/// `schema` is the schema that owns the named types and `type_node` is the
/// node to resolve.
///
/// Derived from graphql-js 16.12.0, src/utilities/typeFromAST.ts, MIT.
#[must_use]
pub fn type_from_ast(schema: &Schema, type_node: &TypeNode) -> Option<Type> {
    match type_node {
        TypeNode::ListType(node) => {
            let inner_type = type_from_ast(schema, &node.r#type)?;
            Some(Type::from(ListType::new(inner_type)))
        }
        TypeNode::NonNullType(node) => {
            let inner_type = nullable_type_from_ast(schema, &node.r#type)?;
            // The inner node is a nullable type node by the grammar of the
            // parser, so the wrapping never sees a non null type and never
            // fails.
            NonNullType::new(inner_type).ok().map(Type::from)
        }
        TypeNode::NamedType(node) => schema.get_type(node.name.value()).map(Type::from),
    }
}

/// Resolves the inner node of a non null type node.
fn nullable_type_from_ast(schema: &Schema, type_node: &NullableTypeNode) -> Option<Type> {
    match type_node {
        NullableTypeNode::ListType(node) => {
            let inner_type = type_from_ast(schema, &node.r#type)?;
            Some(Type::from(ListType::new(inner_type)))
        }
        NullableTypeNode::NamedType(node) => schema.get_type(node.name.value()).map(Type::from),
    }
}
