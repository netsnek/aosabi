// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `NoUnusedVariablesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/NoUnusedVariablesRule.ts, MIT.
 */

//! The `NoUnusedVariablesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    vec,
    vec::Vec, //
};

use aosabi_json::{
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        OperationDefinitionNode,
        VariableDefinitionNode, //
    },
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::DefinitionError;

use crate::validation_context::{
    Rule,
    ValidationContext, //
};

/// No unused variables
///
/// A GraphQL operation is only valid if all variables defined by an operation
/// are used, either directly or within a spread fragment.
///
/// See <https://spec.graphql.org/draft/#sec-All-Variables-Used>
///
/// The visitor holds both hooks of `OperationDefinition`, where enter forgets
/// the definitions of the operation before and leave reports every definition
/// no usage names, and the `VariableDefinition` hook, which collects a
/// definition.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/NoUnusedVariablesRule.ts, MIT.
pub struct NoUnusedVariablesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
    /// The variable definitions of the current operation in source order.
    variable_defs: Vec<&'a VariableDefinitionNode>,
}

impl<'c, 'a> Rule<'c, 'a> for NoUnusedVariablesRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            variable_defs: Vec::new(),
        }
    }
}

impl<'a> NoUnusedVariablesRule<'_, 'a> {
    /// Checks the definitions of `operation`, the body of the leave hook of
    /// `OperationDefinition` at lines 24-45.
    ///
    /// # Errors
    ///
    /// Returns what [`ValidationContext::get_recursive_variable_usages()`]
    /// returns.
    fn check_definitions(
        &self,
        operation: &'a OperationDefinitionNode,
    ) -> Result<(), DefinitionError> {
        let mut variable_name_used: IndexMap<&'a str, bool, KeyHasherBuilder> = IndexMap::default();
        let usages = self.context.get_recursive_variable_usages(operation)?;

        for usage in usages.iter() {
            variable_name_used.insert(usage.node.name.value(), true);
        }

        for variable_def in &self.variable_defs {
            let variable_name = variable_def.variable.name.value();
            if variable_name_used.get(variable_name) == Some(&true) {
                continue;
            }
            let message = match &operation.name {
                Some(operation_name) => format!(
                    "Variable \"${variable_name}\" is never used in operation \"{}\".",
                    operation_name.value()
                ),
                None => format!("Variable \"${variable_name}\" is never used."),
            };
            self.context.report_error(GraphQLError::new(
                message,
                GraphQLErrorOptions {
                    nodes: Some(vec![(*variable_def).clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        Ok(())
    }
}

impl<'a> Visitor<'a> for NoUnusedVariablesRule<'_, 'a> {
    fn enter_operation_definition(
        &mut self,
        _node: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.variable_defs = Vec::new();
        VisitResult::Continue
    }

    fn leave_operation_definition(
        &mut self,
        operation: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        match self.check_definitions(operation) {
            Ok(()) => VisitResult::Continue,
            Err(error) => self.context.throw_error(error),
        }
    }

    fn enter_variable_definition(
        &mut self,
        def: &'a VariableDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.variable_defs.push(def);
        VisitResult::Continue
    }
}

/// Creates the visitor of `NoUnusedVariablesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/NoUnusedVariablesRule.ts, MIT.
#[must_use]
pub fn no_unused_variables_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    NoUnusedVariablesRule::boxed(context)
}
