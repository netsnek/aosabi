// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `KnownFragmentNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/KnownFragmentNamesRule.ts, MIT.
 */

//! The `KnownFragmentNamesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::FragmentSpreadNode,
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};

use crate::validation_context::{
    Rule,
    ValidationContext, //
};

/// Known fragment names
///
/// A GraphQL document is only valid if all `...Fragment` fragment spreads refer
/// to fragments defined in the same document.
///
/// See <https://spec.graphql.org/draft/#sec-Fragment-spread-target-defined>
///
/// The visitor holds the `FragmentSpread` hook, which reports a spread of a
/// fragment the document does not define.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownFragmentNamesRule.ts, MIT.
pub struct KnownFragmentNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for KnownFragmentNamesRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> Visitor<'a> for KnownFragmentNamesRule<'_, 'a> {
    fn enter_fragment_spread(
        &mut self,
        node: &'a FragmentSpreadNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let fragment_name = node.name.value();
        let fragment = self.context.get_fragment(fragment_name);
        if fragment.is_none() {
            self.context.report_error(GraphQLError::new(
                format!("Unknown fragment \"{fragment_name}\"."),
                GraphQLErrorOptions {
                    nodes: Some(vec![node.name.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `KnownFragmentNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownFragmentNamesRule.ts, MIT.
#[must_use]
pub fn known_fragment_names_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    KnownFragmentNamesRule::boxed(context)
}
