// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rules `ProvidedRequiredArgumentsRule` and
 * `ProvidedRequiredArgumentsOnDirectivesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/ProvidedRequiredArgumentsRule.ts, MIT.
 */

//! The `ProvidedRequiredArgumentsRule` and the
//! `ProvidedRequiredArgumentsOnDirectivesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::{
        String,
        ToString, //
    },
    vec, //
};

use aosabi_json::{
    map::IndexSet,
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        DefinitionNode,
        DirectiveNode,
        FieldNode,
        InputValueDefinitionNode,
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
    Argument,
    ObjMap,
    is_required_argument,
    specified_directives, //
};

use crate::{
    key_map::key_map,
    validation_context::{
        Rule,
        SdlOrValidationContext,
        ValidationContext, //
    }, //
};

/// A `Set` of names, which a rule asks for membership alone.
type NameSet<'a> = IndexSet<&'a str, KeyHasherBuilder>;

/// Provided required arguments
///
/// A field or directive is only valid if all required (non-null without a
/// default value) field arguments have been provided.
///
/// The reference spreads the visitor of
/// `ProvidedRequiredArgumentsOnDirectivesRule` into its own, so this rule holds
/// that rule and hands it the leave hook of `Directive`, the one hook it has.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ProvidedRequiredArgumentsRule.ts, MIT.
pub struct ProvidedRequiredArgumentsRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
    /// The rule whose visitor the reference spreads into this one.
    on_directives: ProvidedRequiredArgumentsOnDirectivesRule<'c, 'a>,
}

impl<'c, 'a> Rule<'c, 'a> for ProvidedRequiredArgumentsRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            on_directives: ProvidedRequiredArgumentsOnDirectivesRule::new(context.into()),
        }
    }
}

impl<'a> Visitor<'a> for ProvidedRequiredArgumentsRule<'_, 'a> {
    fn leave_directive(
        &mut self,
        directive_node: &'a DirectiveNode,
        context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.on_directives.leave_directive(directive_node, context)
    }

    // Validate on leave to allow for deeper errors to appear first.
    fn leave_field(&mut self, field_node: &'a FieldNode, _: &VisitContext<'a, '_>) -> VisitResult {
        let Some(field_def) = self.context.get_field_def() else {
            return VisitResult::Skip;
        };

        // A field may hold no list of arguments, see
        // https://github.com/graphql/graphql-js/issues/2203. The `Set` of
        // `undefined` is empty.
        let provided_args: NameSet<'_> = field_node
            .arguments
            .iter()
            .flatten()
            .map(|arg| arg.name.value())
            .collect();
        for arg_def in &field_def.args {
            if !provided_args.contains(arg_def.name.as_str()) && is_required_argument(arg_def) {
                let arg_type_str = arg_def.r#type.to_string();
                self.context.report_error(GraphQLError::new(
                    format!(
                        "Field \"{}\" argument \"{}\" of type \"{arg_type_str}\" is required, but it was not provided.",
                        field_def.name, arg_def.name
                    ),
                    GraphQLErrorOptions {
                        nodes: Some(vec![AstNode::from(field_node.clone())]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            }
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `ProvidedRequiredArgumentsRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ProvidedRequiredArgumentsRule.ts, MIT.
#[must_use]
pub fn provided_required_arguments_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    ProvidedRequiredArgumentsRule::boxed(context)
}

/// A required argument of a directive, the
/// `GraphQLArgument | InputValueDefinitionNode` of the reference.
#[derive(Clone, Copy)]
enum RequiredArgument<'a> {
    /// An argument of a directive of the schema.
    Argument(&'a Argument),
    /// An argument of a directive definition of the document.
    InputValueDefinition(&'a InputValueDefinitionNode),
}

impl RequiredArgument<'_> {
    /// Returns the type of the argument as the message prints it.
    ///
    /// `inspect` of a type is its `toString`, because `inspect` calls the
    /// `toJSON` of an object that has one and every type answers its string form
    /// there.
    fn type_str(self) -> String {
        match self {
            Self::Argument(arg_def) => arg_def.r#type.to_string(),
            Self::InputValueDefinition(arg_def) => print(&arg_def.r#type),
        }
    }
}

/// The visitor of `ProvidedRequiredArgumentsOnDirectivesRule`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ProvidedRequiredArgumentsRule.ts, MIT.
pub struct ProvidedRequiredArgumentsOnDirectivesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: SdlOrValidationContext<'c, 'a>,
    /// The required arguments of every directive, by directive name and
    /// argument name, the `requiredArgsMap` of the reference.
    required_args_map: ObjMap<ObjMap<RequiredArgument<'a>>>,
}

impl<'c, 'a> Rule<'c, 'a> for ProvidedRequiredArgumentsOnDirectivesRule<'c, 'a> {
    type Context = SdlOrValidationContext<'c, 'a>;

    fn new(context: Self::Context) -> Self {
        let mut required_args_map: ObjMap<ObjMap<RequiredArgument<'a>>> = ObjMap::default();

        let schema = context.get_schema();
        let defined_directives = match schema {
            Some(schema) => schema.get_directives(),
            None => specified_directives(),
        };
        for directive in defined_directives {
            let required_args = directive
                .args
                .iter()
                .filter(|arg| is_required_argument(arg));
            required_args_map.insert(
                directive.name.clone(),
                key_map(required_args, |arg| arg.name.clone())
                    .into_iter()
                    .map(|(arg_name, arg)| (arg_name, RequiredArgument::Argument(arg)))
                    .collect(),
            );
        }

        let ast_definitions = &context.get_document().definitions;
        for def in ast_definitions {
            if let DefinitionNode::DirectiveDefinition(def) = def {
                // A directive definition may hold no list of arguments, see
                // https://github.com/graphql/graphql-js/issues/2203.
                let arg_nodes = def.arguments.as_deref().unwrap_or_default();

                let required_args = arg_nodes
                    .iter()
                    .filter(|arg| is_required_argument_node(arg));
                required_args_map.insert(
                    String::from(def.name.value()),
                    key_map(required_args, |arg| String::from(arg.name.value()))
                        .into_iter()
                        .map(|(arg_name, arg)| {
                            (arg_name, RequiredArgument::InputValueDefinition(arg))
                        })
                        .collect(),
                );
            }
        }

        Self {
            context,
            required_args_map,
        }
    }
}

impl<'a> Visitor<'a> for ProvidedRequiredArgumentsOnDirectivesRule<'_, 'a> {
    // Validate on leave to allow for deeper errors to appear first.
    fn leave_directive(
        &mut self,
        directive_node: &'a DirectiveNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let directive_name = directive_node.name.value();
        let required_args = self.required_args_map.get(directive_name);
        if let Some(required_args) = required_args {
            // A directive may hold no list of arguments, see
            // https://github.com/graphql/graphql-js/issues/2203.
            let arg_nodes = directive_node.arguments.as_deref().unwrap_or_default();
            let arg_node_map: NameSet<'_> = arg_nodes.iter().map(|arg| arg.name.value()).collect();
            for (arg_name, arg_def) in required_args {
                if !arg_node_map.contains(arg_name.as_str()) {
                    let arg_type = arg_def.type_str();
                    self.context.report_error(GraphQLError::new(
                        format!(
                            "Directive \"@{directive_name}\" argument \"{arg_name}\" of type \"{arg_type}\" is required, but it was not provided."
                        ),
                        GraphQLErrorOptions {
                            nodes: Some(vec![AstNode::from(directive_node.clone())]),
                            ..GraphQLErrorOptions::default()
                        },
                    ));
                }
            }
        }
        VisitResult::Continue
    }
}

/// Creates the visitor of `ProvidedRequiredArgumentsOnDirectivesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ProvidedRequiredArgumentsRule.ts, MIT.
#[must_use]
pub fn provided_required_arguments_on_directives_rule<'c, 'a>(
    context: SdlOrValidationContext<'c, 'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    ProvidedRequiredArgumentsOnDirectivesRule::boxed(context)
}

/// Returns whether the argument definition `arg` is required, which is a
/// non-null type without a default value.
///
/// The reference compares the default value with `== null`, which holds for
/// `undefined` and `null` alone. A default value of the literal `null` is a node
/// and so is neither, which leaves the absent default value.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ProvidedRequiredArgumentsRule.ts, MIT.
const fn is_required_argument_node(arg: &InputValueDefinitionNode) -> bool {
    matches!(arg.r#type, TypeNode::NonNullType(_)) && arg.default_value.is_none()
}
