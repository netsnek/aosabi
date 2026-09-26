// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `LoneAnonymousOperationRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/LoneAnonymousOperationRule.ts, MIT.
 */

//! The `LoneAnonymousOperationRule` of the validation layer.

use alloc::{
    boxed::Box,
    vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    Kind,
    ast::{
        DocumentNode,
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

/// Lone anonymous operation
///
/// A GraphQL document is only valid if when it contains an anonymous operation
/// (the query short-hand) that it contains only that one operation definition.
///
/// See <https://spec.graphql.org/draft/#sec-Lone-Anonymous-Operation>
///
/// The visitor holds the `Document` hook, which counts the operations, and the
/// `OperationDefinition` hook, which reports an anonymous operation that is
/// not alone.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/LoneAnonymousOperationRule.ts, MIT.
pub struct LoneAnonymousOperationRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c AstValidationContext<'a>,
    /// The count of operation definitions of the document.
    operation_count: usize,
}

impl<'c, 'a> Rule<'c, 'a> for LoneAnonymousOperationRule<'c, 'a> {
    type Context = &'c AstValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            operation_count: 0,
        }
    }
}

impl<'a> Visitor<'a> for LoneAnonymousOperationRule<'_, 'a> {
    fn enter_document(
        &mut self,
        node: &'a DocumentNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.operation_count = node
            .definitions
            .iter()
            .filter(|definition| definition.kind() == Kind::OperationDefinition)
            .count();
        VisitResult::Continue
    }

    fn enter_operation_definition(
        &mut self,
        node: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        if node.name.is_none() && self.operation_count > 1 {
            self.context.report_error(GraphQLError::new(
                "This anonymous operation must be the only defined operation.",
                GraphQLErrorOptions {
                    nodes: Some(vec![node.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `LoneAnonymousOperationRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/LoneAnonymousOperationRule.ts, MIT.
#[must_use]
pub fn lone_anonymous_operation_rule<'c, 'a>(
    context: &'c AstValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    LoneAnonymousOperationRule::boxed(context)
}
