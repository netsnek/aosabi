// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueArgumentNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueArgumentNamesRule.ts, MIT.
 */

//! The `UniqueArgumentNamesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    vec::Vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        ArgumentNode,
        AstNode,
        DirectiveNode,
        FieldNode, //
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

/// Unique argument names
///
/// A GraphQL field or directive is only valid if all supplied arguments are
/// uniquely named.
///
/// See <https://spec.graphql.org/draft/#sec-Argument-Names>
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueArgumentNamesRule.ts, MIT.
pub struct UniqueArgumentNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c AstValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueArgumentNamesRule<'c, 'a> {
    type Context = &'c AstValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> UniqueArgumentNamesRule<'_, 'a> {
    /// Reports every argument name that a field or a directive supplies
    /// more than once.
    ///
    /// The reference takes the parent node and reads its `arguments`, and
    /// the two node structs share no type, so the caller passes the list. A
    /// node may hold none, see <https://github.com/graphql/graphql-js/issues/2203>.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/UniqueArgumentNamesRule.ts, MIT.
    fn check_arg_uniqueness(&self, arguments: Option<&'a [ArgumentNode]>) -> VisitResult {
        let argument_nodes = arguments.unwrap_or_default();

        let seen_args = group_by(argument_nodes, |arg| arg.name.value());

        for (arg_name, arg_nodes) in &seen_args {
            if arg_nodes.len() > 1 {
                self.context.report_error(GraphQLError::new(
                    format!("There can be only one argument named \"{arg_name}\"."),
                    GraphQLErrorOptions {
                        nodes: Some(
                            arg_nodes
                                .iter()
                                .map(|node| AstNode::from(node.name.clone()))
                                .collect::<Vec<_>>(),
                        ),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            }
        }

        VisitResult::Continue
    }
}

impl<'a> Visitor<'a> for UniqueArgumentNamesRule<'_, 'a> {
    fn enter_field(&mut self, node: &'a FieldNode, _: &VisitContext<'a, '_>) -> VisitResult {
        self.check_arg_uniqueness(node.arguments.as_deref())
    }

    fn enter_directive(
        &mut self,
        node: &'a DirectiveNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_arg_uniqueness(node.arguments.as_deref())
    }
}

/// Creates the visitor of `UniqueArgumentNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueArgumentNamesRule.ts, MIT.
#[must_use]
pub fn unique_argument_names_rule<'c, 'a>(
    context: &'c AstValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueArgumentNamesRule::boxed(context)
}
