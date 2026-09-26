// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueVariableNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueVariableNamesRule.ts, MIT.
 */

//! The `UniqueVariableNamesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        OperationDefinitionNode, //
    },
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};

use crate::{
    group_by::group_by,
    validation_context::{
        AstValidationContext,
        Rule, //
    }, //
};

/// Unique variable names
///
/// A GraphQL operation is only valid if all its variables are uniquely named.
///
/// The visitor holds the `OperationDefinition` hook, which reports every
/// variable name that more than one definition of the operation carries.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueVariableNamesRule.ts, MIT.
pub struct UniqueVariableNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c AstValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueVariableNamesRule<'c, 'a> {
    type Context = &'c AstValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> Visitor<'a> for UniqueVariableNamesRule<'_, 'a> {
    fn enter_operation_definition(
        &mut self,
        operation_node: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        // See: https://github.com/graphql/graphql-js/issues/2203
        let variable_definitions = operation_node
            .variable_definitions
            .as_deref()
            .unwrap_or_default();

        let seen_variable_definitions =
            group_by(variable_definitions, |node| node.variable.name.value());

        for (variable_name, variable_nodes) in &seen_variable_definitions {
            if variable_nodes.len() > 1 {
                self.context.report_error(GraphQLError::new(
                    format!("There can be only one variable named \"${variable_name}\"."),
                    GraphQLErrorOptions {
                        nodes: Some(
                            variable_nodes
                                .iter()
                                .map(|node| AstNode::from(node.variable.name.clone()))
                                .collect(),
                        ),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            }
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `UniqueVariableNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueVariableNamesRule.ts, MIT.
#[must_use]
pub fn unique_variable_names_rule<'c, 'a>(
    context: &'c AstValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueVariableNamesRule::boxed(context)
}
