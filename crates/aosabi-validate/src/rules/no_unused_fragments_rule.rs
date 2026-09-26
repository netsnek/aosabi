// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `NoUnusedFragmentsRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/NoUnusedFragmentsRule.ts, MIT.
 */

//! The `NoUnusedFragmentsRule` of the validation layer.

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
        DocumentNode,
        FragmentDefinitionNode,
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

/// No unused fragments
///
/// A GraphQL document is only valid if all fragment definitions are spread
/// within operations, or spread within other fragments spread within operations.
///
/// See <https://spec.graphql.org/draft/#sec-Fragments-Must-Be-Used>
///
/// The visitor holds the `OperationDefinition` hook and the
/// `FragmentDefinition` hook, which collect the definitions and skip their
/// children, and the leave hook of `Document`, which reports every fragment
/// no operation reaches.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/NoUnusedFragmentsRule.ts, MIT.
pub struct NoUnusedFragmentsRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c AstValidationContext<'a>,
    /// The operations of the document in source order.
    operation_defs: Vec<&'a OperationDefinitionNode>,
    /// The fragments of the document in source order.
    fragment_defs: Vec<&'a FragmentDefinitionNode>,
}

impl<'c, 'a> Rule<'c, 'a> for NoUnusedFragmentsRule<'c, 'a> {
    type Context = &'c AstValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            operation_defs: Vec::new(),
            fragment_defs: Vec::new(),
        }
    }
}

impl<'a> Visitor<'a> for NoUnusedFragmentsRule<'_, 'a> {
    fn enter_operation_definition(
        &mut self,
        node: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.operation_defs.push(node);
        VisitResult::Skip
    }

    fn enter_fragment_definition(
        &mut self,
        node: &'a FragmentDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.fragment_defs.push(node);
        VisitResult::Skip
    }

    fn leave_document(
        &mut self,
        _node: &'a DocumentNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let mut fragment_name_used: IndexMap<&'a str, bool, KeyHasherBuilder> = IndexMap::default();
        for operation in &self.operation_defs {
            for fragment in self
                .context
                .get_recursively_referenced_fragments(operation)
                .iter()
            {
                fragment_name_used.insert(fragment.name.value(), true);
            }
        }

        for fragment_def in &self.fragment_defs {
            let frag_name = fragment_def.name.value();
            if fragment_name_used.get(frag_name) != Some(&true) {
                self.context.report_error(GraphQLError::new(
                    format!("Fragment \"{frag_name}\" is never used."),
                    GraphQLErrorOptions {
                        nodes: Some(vec![(*fragment_def).clone().into()]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            }
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `NoUnusedFragmentsRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/NoUnusedFragmentsRule.ts, MIT.
#[must_use]
pub fn no_unused_fragments_rule<'c, 'a>(
    context: &'c AstValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    NoUnusedFragmentsRule::boxed(context)
}
