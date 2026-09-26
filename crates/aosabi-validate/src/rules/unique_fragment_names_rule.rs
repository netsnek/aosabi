// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueFragmentNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueFragmentNamesRule.ts, MIT.
 */

//! The `UniqueFragmentNamesRule` of the validation layer.

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

/// Unique fragment names
///
/// A GraphQL document is only valid if all defined fragments have unique names.
///
/// See <https://spec.graphql.org/draft/#sec-Fragment-Name-Uniqueness>
///
/// The visitor holds the `OperationDefinition` hook, which skips the
/// operation, and the `FragmentDefinition` hook, which reports a name an
/// earlier fragment carries. Neither definition is walked any deeper.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueFragmentNamesRule.ts, MIT.
pub struct UniqueFragmentNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c AstValidationContext<'a>,
    /// The name node of the first fragment of each name, the null prototype
    /// object `knownFragmentNames` of the reference.
    known_fragment_names: IndexMap<&'a str, &'a NameNode, KeyHasherBuilder>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueFragmentNamesRule<'c, 'a> {
    type Context = &'c AstValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            known_fragment_names: IndexMap::default(),
        }
    }
}

impl<'a> Visitor<'a> for UniqueFragmentNamesRule<'_, 'a> {
    fn enter_operation_definition(
        &mut self,
        _node: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        VisitResult::Skip
    }

    fn enter_fragment_definition(
        &mut self,
        node: &'a FragmentDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let fragment_name = node.name.value();
        // The reference tests the stored name node for truthiness, and an
        // object is truthy where the `undefined` of a missing property is
        // not.
        if let Some(known_fragment_name) = self.known_fragment_names.get(fragment_name) {
            self.context.report_error(GraphQLError::new(
                format!("There can be only one fragment named \"{fragment_name}\"."),
                GraphQLErrorOptions {
                    nodes: Some(vec![
                        (*known_fragment_name).clone().into(),
                        node.name.clone().into(),
                    ]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        } else {
            self.known_fragment_names.insert(fragment_name, &node.name);
        }
        VisitResult::Skip
    }
}

/// Creates the visitor of `UniqueFragmentNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueFragmentNamesRule.ts, MIT.
#[must_use]
pub fn unique_fragment_names_rule<'c, 'a>(
    context: &'c AstValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueFragmentNamesRule::boxed(context)
}
