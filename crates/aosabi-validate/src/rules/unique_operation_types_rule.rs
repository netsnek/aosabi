// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueOperationTypesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueOperationTypesRule.ts, MIT.
 */

//! The `UniqueOperationTypesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        OperationTypeDefinitionNode,
        SchemaDefinitionNode,
        SchemaExtensionNode, //
    },
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    NamedType,
    ObjMap, //
};

use crate::validation_context::{
    Rule,
    SdlValidationContext, //
};

/// Unique operation types
///
/// A GraphQL document is only valid if it has only one type per operation.
///
/// Both maps are keyed by the operation as the reference spells it, which
/// is `query`, `mutation` or `subscription`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueOperationTypesRule.ts, MIT.
pub struct UniqueOperationTypesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c SdlValidationContext<'a>,
    /// The operation types the document has defined so far, the
    /// `definedOperationTypes` of the reference.
    defined_operation_types: ObjMap<&'a OperationTypeDefinitionNode>,
    /// The root types of the schema to extend, the `existingOperationTypes`
    /// of the reference, which is empty without such a schema.
    existing_operation_types: ObjMap<Option<&'a NamedType>>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueOperationTypesRule<'c, 'a> {
    type Context = &'c SdlValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        let schema = context.get_schema();
        let mut existing_operation_types = ObjMap::default();
        if let Some(schema) = schema {
            existing_operation_types.insert(String::from("query"), schema.get_query_type());
            existing_operation_types.insert(String::from("mutation"), schema.get_mutation_type());
            existing_operation_types
                .insert(String::from("subscription"), schema.get_subscription_type());
        }

        Self {
            context,
            defined_operation_types: ObjMap::default(),
            existing_operation_types,
        }
    }
}

impl<'a> UniqueOperationTypesRule<'_, 'a> {
    /// Reports every operation type of `operation_types_nodes` that the
    /// schema or the document defines already.
    ///
    /// The reference takes the node and reads its `operationTypes`, which
    /// the two node structs hold in two shapes, so the caller passes the
    /// list.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/UniqueOperationTypesRule.ts, MIT.
    fn check_operation_types(
        &mut self,
        operation_types_nodes: &'a [OperationTypeDefinitionNode],
    ) -> VisitResult {
        for operation_type in operation_types_nodes {
            let operation = operation_type.operation.as_str();
            let already_defined_operation_type = self.defined_operation_types.get(operation);

            if self
                .existing_operation_types
                .get(operation)
                .is_some_and(Option::is_some)
            {
                self.context.report_error(GraphQLError::new(
                    format!(
                        "Type for {operation} already defined in the schema. It cannot be redefined."
                    ),
                    GraphQLErrorOptions {
                        nodes: Some(vec![AstNode::from(operation_type.clone())]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            } else if let Some(already_defined_operation_type) = already_defined_operation_type {
                self.context.report_error(GraphQLError::new(
                    format!("There can be only one {operation} type in schema."),
                    GraphQLErrorOptions {
                        nodes: Some(vec![
                            AstNode::from((*already_defined_operation_type).clone()),
                            AstNode::from(operation_type.clone()),
                        ]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            } else {
                self.defined_operation_types
                    .insert(String::from(operation), operation_type);
            }
        }

        VisitResult::Skip
    }
}

impl<'a> Visitor<'a> for UniqueOperationTypesRule<'_, 'a> {
    fn enter_schema_definition(
        &mut self,
        node: &'a SchemaDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_operation_types(&node.operation_types)
    }

    fn enter_schema_extension(
        &mut self,
        node: &'a SchemaExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        // A schema extension may hold no list of operation types, see
        // https://github.com/graphql/graphql-js/issues/2203.
        self.check_operation_types(node.operation_types.as_deref().unwrap_or_default())
    }
}

/// Creates the visitor of `UniqueOperationTypesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueOperationTypesRule.ts, MIT.
#[must_use]
pub fn unique_operation_types_rule<'c, 'a>(
    context: &'c SdlValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueOperationTypesRule::boxed(context)
}
