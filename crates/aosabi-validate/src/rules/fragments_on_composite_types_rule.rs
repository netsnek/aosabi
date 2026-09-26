// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `FragmentsOnCompositeTypesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/FragmentsOnCompositeTypesRule.ts, MIT.
 */

//! The `FragmentsOnCompositeTypesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        FragmentDefinitionNode,
        InlineFragmentNode,
        NamedTypeNode,
        TypeNode, //
    },
    print,
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    is_composite_type,
    type_from_ast, //
};

use crate::validation_context::{
    Rule,
    ValidationContext, //
};

/// Fragments on composite type
///
/// Fragments use a type condition to determine if they apply, since fragments
/// can only be spread into a composite type (object, interface, or union), the
/// type condition must also be a composite type.
///
/// See <https://spec.graphql.org/draft/#sec-Fragments-On-Composite-Types>
///
/// The visitor holds the `InlineFragment` hook and the `FragmentDefinition`
/// hook, and each reports a type condition that names a type of the schema
/// which is not composite.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/FragmentsOnCompositeTypesRule.ts, MIT.
pub struct FragmentsOnCompositeTypesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for FragmentsOnCompositeTypesRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl FragmentsOnCompositeTypesRule<'_, '_> {
    /// Returns whether `type_condition` names a type of the schema that is
    /// not composite, the test both hooks of the reference share.
    ///
    /// An unknown type answers false, because reporting it is the work of
    /// `KnownTypeNamesRule`.
    fn is_non_composite(&self, type_condition: &NamedTypeNode) -> bool {
        let ty = type_from_ast(
            self.context.get_schema(),
            &TypeNode::NamedType(type_condition.clone()),
        );
        ty.is_some_and(|ty| !is_composite_type(&ty))
    }
}

impl<'a> Visitor<'a> for FragmentsOnCompositeTypesRule<'_, 'a> {
    fn enter_inline_fragment(
        &mut self,
        node: &'a InlineFragmentNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let Some(type_condition) = &node.type_condition else {
            return VisitResult::Continue;
        };
        if self.is_non_composite(type_condition) {
            let type_str = print(type_condition);
            self.context.report_error(GraphQLError::new(
                format!("Fragment cannot condition on non composite type \"{type_str}\"."),
                GraphQLErrorOptions {
                    nodes: Some(vec![type_condition.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        VisitResult::Continue
    }

    fn enter_fragment_definition(
        &mut self,
        node: &'a FragmentDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        if self.is_non_composite(&node.type_condition) {
            let type_str = print(&node.type_condition);
            self.context.report_error(GraphQLError::new(
                format!(
                    "Fragment \"{}\" cannot condition on non composite type \"{type_str}\".",
                    node.name.value()
                ),
                GraphQLErrorOptions {
                    nodes: Some(vec![node.type_condition.clone().into()]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `FragmentsOnCompositeTypesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/FragmentsOnCompositeTypesRule.ts, MIT.
#[must_use]
pub fn fragments_on_composite_types_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    FragmentsOnCompositeTypesRule::boxed(context)
}
