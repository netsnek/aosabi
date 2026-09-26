// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueOperationNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueOperationNamesRule.ts, MIT.
 */

//! The `UniqueOperationNamesRule` of the validation layer.

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
        FragmentDefinitionNode,
        NameNode,
        OperationDefinitionNode, //
    },
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};

use crate::validation_context::{
    AstValidationContext,
    Rule, //
};

/// Unique operation names
///
/// A GraphQL document is only valid if all defined operations have unique names.
///
/// See <https://spec.graphql.org/draft/#sec-Operation-Name-Uniqueness>
///
/// The visitor holds the `OperationDefinition` hook, which reports a name
/// an earlier operation carries, and the `FragmentDefinition` hook, which
/// skips the fragment. Neither definition is walked any deeper.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueOperationNamesRule.ts, MIT.
pub struct UniqueOperationNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c AstValidationContext<'a>,
    /// The name node of the first operation of each name, the null
    /// prototype object `knownOperationNames` of the reference.
    known_operation_names: IndexMap<&'a str, &'a NameNode, KeyHasherBuilder>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueOperationNamesRule<'c, 'a> {
    type Context = &'c AstValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            known_operation_names: IndexMap::default(),
        }
    }
}

impl<'a> Visitor<'a> for UniqueOperationNamesRule<'_, 'a> {
    fn enter_operation_definition(
        &mut self,
        node: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let Some(operation_name) = &node.name else {
            return VisitResult::Skip;
        };
        // The reference tests the stored name node for truthiness, and an
        // object is truthy where the `undefined` of a missing property is
        // not.
        if let Some(known_operation_name) = self.known_operation_names.get(operation_name.value()) {
            self.context.report_error(GraphQLError::new(
                format!(
                    "There can be only one operation named \"{}\".",
                    operation_name.value()
                ),
                GraphQLErrorOptions {
                    nodes: Some(vec![
                        (*known_operation_name).clone().into(),
                        operation_name.clone().into(),
                    ]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        } else {
            self.known_operation_names
                .insert(operation_name.value(), operation_name);
        }
        VisitResult::Skip
    }

    fn enter_fragment_definition(
        &mut self,
        _node: &'a FragmentDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        VisitResult::Skip
    }
}

/// Creates the visitor of `UniqueOperationNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueOperationNamesRule.ts, MIT.
#[must_use]
pub fn unique_operation_names_rule<'c, 'a>(
    context: &'c AstValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueOperationNamesRule::boxed(context)
}
