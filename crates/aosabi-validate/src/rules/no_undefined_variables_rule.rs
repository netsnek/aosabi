// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `NoUndefinedVariablesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/NoUndefinedVariablesRule.ts, MIT.
 */

//! The `NoUndefinedVariablesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    vec, //
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

/// No undefined variables
///
/// A GraphQL operation is only valid if all variables encountered, both directly
/// and via fragment spreads, are defined by that operation.
///
/// See <https://spec.graphql.org/draft/#sec-All-Variable-Uses-Defined>
///
/// The visitor holds both hooks of `OperationDefinition`, where enter forgets
/// the variables of the operation before and leave reports every usage of a
/// variable the operation does not define, and the `VariableDefinition` hook,
/// which records a defined variable.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/NoUndefinedVariablesRule.ts, MIT.
pub struct NoUndefinedVariablesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
    /// The names of the variables the current operation defines, the null
    /// prototype object `variableNameDefined` of the reference.
    variable_name_defined: IndexMap<&'a str, bool, KeyHasherBuilder>,
}

impl<'c, 'a> Rule<'c, 'a> for NoUndefinedVariablesRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            variable_name_defined: IndexMap::default(),
        }
    }
}

impl<'a> NoUndefinedVariablesRule<'_, 'a> {
    /// Checks the usages below `operation`, the body of the leave hook of
    /// `OperationDefinition` at lines 25-41.
    ///
    /// # Errors
    ///
    /// Returns what [`ValidationContext::get_recursive_variable_usages()`]
    /// returns.
    fn check_usages(&self, operation: &'a OperationDefinitionNode) -> Result<(), DefinitionError> {
        let usages = self.context.get_recursive_variable_usages(operation)?;

        for usage in usages.iter() {
            let node = usage.node;
            let var_name = node.name.value();
            if self.variable_name_defined.get(var_name) == Some(&true) {
                continue;
            }
            let message = match &operation.name {
                Some(operation_name) => format!(
                    "Variable \"${var_name}\" is not defined by operation \"{}\".",
                    operation_name.value()
                ),
                None => format!("Variable \"${var_name}\" is not defined."),
            };
            self.context.report_error(GraphQLError::new(
                message,
                GraphQLErrorOptions {
                    nodes: Some(vec![node.clone().into(), operation.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        Ok(())
    }
}

impl<'a> Visitor<'a> for NoUndefinedVariablesRule<'_, 'a> {
    fn enter_operation_definition(
        &mut self,
        _node: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.variable_name_defined = IndexMap::default();
        VisitResult::Continue
    }

    fn leave_operation_definition(
        &mut self,
        operation: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        match self.check_usages(operation) {
            Ok(()) => VisitResult::Continue,
            Err(error) => self.context.throw_error(error),
        }
    }

    fn enter_variable_definition(
        &mut self,
        node: &'a VariableDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.variable_name_defined
            .insert(node.variable.name.value(), true);
        VisitResult::Continue
    }
}

/// Creates the visitor of `NoUndefinedVariablesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/NoUndefinedVariablesRule.ts, MIT.
#[must_use]
pub fn no_undefined_variables_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    NoUndefinedVariablesRule::boxed(context)
}
