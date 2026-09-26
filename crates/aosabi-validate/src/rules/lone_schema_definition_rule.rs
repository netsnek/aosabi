// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `LoneSchemaDefinitionRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/LoneSchemaDefinitionRule.ts, MIT.
 */

//! The `LoneSchemaDefinitionRule` of the validation layer.

use alloc::{
    boxed::Box,
    vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        SchemaDefinitionNode, //
    },
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};

use crate::validation_context::{
    Rule,
    SdlValidationContext, //
};

/// Lone Schema definition
///
/// A GraphQL document is only valid if it contains only one schema definition.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/LoneSchemaDefinitionRule.ts, MIT.
pub struct LoneSchemaDefinitionRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c SdlValidationContext<'a>,
    /// Whether the schema to extend defines a schema already, the
    /// `alreadyDefined` of the reference.
    already_defined: bool,
    /// The number of schema definitions the walk has entered, the
    /// `schemaDefinitionsCount` of the reference.
    schema_definitions_count: usize,
}

impl<'c, 'a> Rule<'c, 'a> for LoneSchemaDefinitionRule<'c, 'a> {
    type Context = &'c SdlValidationContext<'a>;

    /// Creates the rule over `context`.
    ///
    /// The reference keeps the first of the schema definition node and the
    /// three root types that is neither `null` nor `undefined` and reads it
    /// as a condition only. Each of the four is an object where it is
    /// present, and an object is true as a condition, so the chain of `??`
    /// is whether any of the four is present.
    fn new(context: Self::Context) -> Self {
        let old_schema = context.get_schema();
        let already_defined = old_schema.is_some_and(|old_schema| {
            old_schema.ast_node.is_some()
                || old_schema.get_query_type().is_some()
                || old_schema.get_mutation_type().is_some()
                || old_schema.get_subscription_type().is_some()
        });

        Self {
            context,
            already_defined,
            schema_definitions_count: 0,
        }
    }
}

impl<'a> Visitor<'a> for LoneSchemaDefinitionRule<'_, 'a> {
    fn enter_schema_definition(
        &mut self,
        node: &'a SchemaDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        if self.already_defined {
            self.context.report_error(GraphQLError::new(
                "Cannot define a new schema within a schema extension.",
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(node.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
            return VisitResult::Continue;
        }

        if self.schema_definitions_count > 0 {
            self.context.report_error(GraphQLError::new(
                "Must provide only one schema definition.",
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(node.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        self.schema_definitions_count += 1;
        VisitResult::Continue
    }
}

/// Creates the visitor of `LoneSchemaDefinitionRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/LoneSchemaDefinitionRule.ts, MIT.
#[must_use]
pub fn lone_schema_definition_rule<'c, 'a>(
    context: &'c SdlValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    LoneSchemaDefinitionRule::boxed(context)
}
