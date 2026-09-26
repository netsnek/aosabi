// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `VariablesAreInputTypesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/VariablesAreInputTypesRule.ts, MIT.
 */

//! The `VariablesAreInputTypesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::VariableDefinitionNode,
    print,
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    is_input_type,
    type_from_ast, //
};

use crate::validation_context::{
    Rule,
    ValidationContext, //
};

/// Variables are input types
///
/// A GraphQL operation is only valid if all the variables it defines are of
/// input types (scalar, enum, or input object).
///
/// See <https://spec.graphql.org/draft/#sec-Variables-Are-Input-Types>
///
/// The visitor holds the `VariableDefinition` hook, which reports a variable
/// whose type is a type of the schema that is not an input type.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/VariablesAreInputTypesRule.ts, MIT.
pub struct VariablesAreInputTypesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for VariablesAreInputTypesRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> Visitor<'a> for VariablesAreInputTypesRule<'_, 'a> {
    fn enter_variable_definition(
        &mut self,
        node: &'a VariableDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let ty = type_from_ast(self.context.get_schema(), &node.r#type);

        if ty.is_some_and(|ty| !is_input_type(&ty)) {
            let variable_name = node.variable.name.value();
            let type_name = print(&node.r#type);

            self.context.report_error(GraphQLError::new(
                format!("Variable \"${variable_name}\" cannot be non-input type \"{type_name}\"."),
                GraphQLErrorOptions {
                    nodes: Some(vec![node.r#type.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `VariablesAreInputTypesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/VariablesAreInputTypesRule.ts, MIT.
#[must_use]
pub fn variables_are_input_types_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    VariablesAreInputTypesRule::boxed(context)
}
