// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rules `KnownArgumentNamesRule` and
 * `KnownArgumentNamesOnDirectivesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/KnownArgumentNamesRule.ts, MIT.
 */

//! The `KnownArgumentNamesRule` and the `KnownArgumentNamesOnDirectivesRule`
//! of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec,
    vec::Vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        ArgumentNode,
        AstNode,
        DefinitionNode,
        DirectiveNode, //
    },
    visitor::{
        EnterLeave,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    ObjMap,
    did_you_mean,
    specified_directives,
    suggestion_list, //
};

use crate::validation_context::{
    Rule,
    SdlOrValidationContext,
    ValidationContext, //
};

/// Known argument names
///
/// A GraphQL field is only valid if all supplied arguments are defined by
/// that field.
///
/// See <https://spec.graphql.org/draft/#sec-Argument-Names>
/// See <https://spec.graphql.org/draft/#sec-Directives-Are-In-Valid-Locations>
///
/// The reference spreads the visitor of `KnownArgumentNamesOnDirectivesRule` into
/// its own, so this rule holds that rule and hands it the `Directive` hook, the one
/// hook it has.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownArgumentNamesRule.ts, MIT.
pub struct KnownArgumentNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
    /// The rule whose visitor the reference spreads into this one.
    on_directives: KnownArgumentNamesOnDirectivesRule<'c, 'a>,
}

impl<'c, 'a> Rule<'c, 'a> for KnownArgumentNamesRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            on_directives: KnownArgumentNamesOnDirectivesRule::new(context.into()),
        }
    }
}

impl<'a> Visitor<'a> for KnownArgumentNamesRule<'_, 'a> {
    fn enter_directive(
        &mut self,
        directive_node: &'a DirectiveNode,
        context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.on_directives.enter_directive(directive_node, context)
    }

    fn enter_argument(
        &mut self,
        arg_node: &'a ArgumentNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let arg_def = self.context.get_argument();
        let field_def = self.context.get_field_def();
        let parent_type = self.context.get_parent_type();

        if arg_def.is_none()
            && let Some(field_def) = field_def
            && let Some(parent_type) = parent_type
        {
            let arg_name = arg_node.name.value();
            let known_args_names: Vec<String> =
                field_def.args.iter().map(|arg| arg.name.clone()).collect();
            let suggestions = suggestion_list(arg_name, &known_args_names);
            self.context.report_error(GraphQLError::new(
                format!(
                    "Unknown argument \"{arg_name}\" on field \"{}.{}\".",
                    parent_type.name(),
                    field_def.name
                ) + &did_you_mean(None, &suggestions),
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(arg_node.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `KnownArgumentNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownArgumentNamesRule.ts, MIT.
#[must_use]
pub fn known_argument_names_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    KnownArgumentNamesRule::boxed(context)
}

/// The visitor of `KnownArgumentNamesOnDirectivesRule`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownArgumentNamesRule.ts, MIT.
pub struct KnownArgumentNamesOnDirectivesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: SdlOrValidationContext<'c, 'a>,
    /// The argument names of every directive by its name, the
    /// `directiveArgs` of the reference.
    directive_args: ObjMap<Vec<String>>,
}

impl<'c, 'a> Rule<'c, 'a> for KnownArgumentNamesOnDirectivesRule<'c, 'a> {
    type Context = SdlOrValidationContext<'c, 'a>;

    fn new(context: Self::Context) -> Self {
        let mut directive_args: ObjMap<Vec<String>> = ObjMap::default();

        let schema = context.get_schema();
        let defined_directives = match schema {
            Some(schema) => schema.get_directives(),
            None => specified_directives(),
        };
        for directive in defined_directives {
            let names = directive.args.iter().map(|arg| arg.name.clone());
            directive_args.insert(directive.name.clone(), names.collect());
        }

        let ast_definitions = &context.get_document().definitions;
        for def in ast_definitions {
            if let DefinitionNode::DirectiveDefinition(def) = def {
                // A directive definition may hold no list of arguments, see
                // https://github.com/graphql/graphql-js/issues/2203.
                let args_nodes = def.arguments.as_deref().unwrap_or_default();

                let names = args_nodes.iter().map(|arg| String::from(arg.name.value()));
                directive_args.insert(String::from(def.name.value()), names.collect());
            }
        }

        Self {
            context,
            directive_args,
        }
    }
}

impl<'a> Visitor<'a> for KnownArgumentNamesOnDirectivesRule<'_, 'a> {
    fn enter_directive(
        &mut self,
        directive_node: &'a DirectiveNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let directive_name = directive_node.name.value();
        let known_args = self.directive_args.get(directive_name);

        if let Some(arguments) = &directive_node.arguments
            && let Some(known_args) = known_args
        {
            for arg_node in arguments {
                let arg_name = arg_node.name.value();
                if !known_args.iter().any(|known_arg| known_arg == arg_name) {
                    let suggestions = suggestion_list(arg_name, known_args);
                    self.context.report_error(GraphQLError::new(
                        format!(
                            "Unknown argument \"{arg_name}\" on directive \"@{directive_name}\"."
                        ) + &did_you_mean(None, &suggestions),
                        GraphQLErrorOptions {
                            nodes: Some(vec![AstNode::from(arg_node.clone())]),
                            ..GraphQLErrorOptions::default()
                        },
                    ));
                }
            }
        }

        VisitResult::Skip
    }
}

/// Creates the visitor of `KnownArgumentNamesOnDirectivesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownArgumentNamesRule.ts, MIT.
#[must_use]
pub fn known_argument_names_on_directives_rule<'c, 'a>(
    context: SdlOrValidationContext<'c, 'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    KnownArgumentNamesOnDirectivesRule::boxed(context)
}
