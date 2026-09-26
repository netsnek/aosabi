// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueArgumentDefinitionNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueArgumentDefinitionNamesRule.ts, MIT.
 */

//! The `UniqueArgumentDefinitionNamesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    vec::Vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        DirectiveDefinitionNode,
        FieldDefinitionNode,
        InputValueDefinitionNode,
        InterfaceTypeDefinitionNode,
        InterfaceTypeExtensionNode,
        NameNode,
        ObjectTypeDefinitionNode,
        ObjectTypeExtensionNode, //
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
        Rule,
        SdlValidationContext, //
    }, //
};

/// Unique argument definition names
///
/// A GraphQL Object or Interface type is only valid if all its fields have uniquely named arguments.
/// A GraphQL Directive is only valid if all its arguments are uniquely named.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueArgumentDefinitionNamesRule.ts, MIT.
pub struct UniqueArgumentDefinitionNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c SdlValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueArgumentDefinitionNamesRule<'c, 'a> {
    type Context = &'c SdlValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> UniqueArgumentDefinitionNamesRule<'_, 'a> {
    /// Checks the arguments of every field of a type definition or
    /// extension.
    ///
    /// The reference takes the node, and the four node structs share no type, so the caller passes
    /// the `name` and the `fields` of the node. A node may hold no list of fields and a field no
    /// list of arguments, see <https://github.com/graphql/graphql-js/issues/2203>.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/UniqueArgumentDefinitionNamesRule.ts, MIT.
    fn check_arg_uniqueness_per_field(
        &self,
        name: &'a NameNode,
        fields: Option<&'a [FieldDefinitionNode]>,
    ) -> VisitResult {
        let type_name = name.value();

        let field_nodes = fields.unwrap_or_default();

        for field_def in field_nodes {
            let field_name = field_def.name.value();

            let argument_nodes = field_def.arguments.as_deref().unwrap_or_default();

            self.check_arg_uniqueness(&format!("{type_name}.{field_name}"), argument_nodes);
        }

        VisitResult::Skip
    }

    /// Reports every argument name that `argument_nodes` defines more than
    /// once below `parent_name`.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/UniqueArgumentDefinitionNamesRule.ts, MIT.
    fn check_arg_uniqueness(
        &self,
        parent_name: &str,
        argument_nodes: &'a [InputValueDefinitionNode],
    ) -> VisitResult {
        let seen_args = group_by(argument_nodes, |arg| arg.name.value());

        for (arg_name, arg_nodes) in &seen_args {
            if arg_nodes.len() > 1 {
                self.context.report_error(GraphQLError::new(
                    format!("Argument \"{parent_name}({arg_name}:)\" can only be defined once."),
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

        VisitResult::Skip
    }
}

impl<'a> Visitor<'a> for UniqueArgumentDefinitionNamesRule<'_, 'a> {
    fn enter_directive_definition(
        &mut self,
        directive_node: &'a DirectiveDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        // A directive definition may hold no list of arguments, see
        // https://github.com/graphql/graphql-js/issues/2203.
        let argument_nodes = directive_node.arguments.as_deref().unwrap_or_default();

        self.check_arg_uniqueness(&format!("@{}", directive_node.name.value()), argument_nodes)
    }

    fn enter_interface_type_definition(
        &mut self,
        node: &'a InterfaceTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_arg_uniqueness_per_field(&node.name, node.fields.as_deref())
    }

    fn enter_interface_type_extension(
        &mut self,
        node: &'a InterfaceTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_arg_uniqueness_per_field(&node.name, node.fields.as_deref())
    }

    fn enter_object_type_definition(
        &mut self,
        node: &'a ObjectTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_arg_uniqueness_per_field(&node.name, node.fields.as_deref())
    }

    fn enter_object_type_extension(
        &mut self,
        node: &'a ObjectTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_arg_uniqueness_per_field(&node.name, node.fields.as_deref())
    }
}

/// Creates the visitor of `UniqueArgumentDefinitionNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueArgumentDefinitionNamesRule.ts, MIT.
#[must_use]
pub fn unique_argument_definition_names_rule<'c, 'a>(
    context: &'c SdlValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueArgumentDefinitionNamesRule::boxed(context)
}
