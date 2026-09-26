// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueDirectiveNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueDirectiveNamesRule.ts, MIT.
 */

//! The `UniqueDirectiveNamesRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        DirectiveDefinitionNode,
        NameNode, //
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
    Schema, //
};

use crate::validation_context::{
    Rule,
    SdlValidationContext, //
};

/// Unique directive names
///
/// A GraphQL document is only valid if all defined directives have unique names.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueDirectiveNamesRule.ts, MIT.
pub struct UniqueDirectiveNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c SdlValidationContext<'a>,
    /// The name node of every directive the document has defined so far,
    /// the `knownDirectiveNames` of the reference.
    known_directive_names: ObjMap<&'a NameNode>,
    /// The schema to extend, when there is one.
    schema: Option<&'a Schema>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueDirectiveNamesRule<'c, 'a> {
    type Context = &'c SdlValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            known_directive_names: ObjMap::default(),
            schema: context.get_schema(),
        }
    }
}

impl<'a> Visitor<'a> for UniqueDirectiveNamesRule<'_, 'a> {
    // The branch that finds the name in the schema returns `undefined` in
    // the reference and the two others return `false`, which is why the
    // answers differ.
    fn enter_directive_definition(
        &mut self,
        node: &'a DirectiveDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let directive_name = node.name.value();

        if self
            .schema
            .is_some_and(|schema| schema.get_directive(directive_name).is_some())
        {
            self.context.report_error(GraphQLError::new(
                format!(
                    "Directive \"@{directive_name}\" already exists in the schema. It cannot be redefined."
                ),
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(node.name.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
            return VisitResult::Continue;
        }

        if let Some(known_directive_name) = self.known_directive_names.get(directive_name) {
            self.context.report_error(GraphQLError::new(
                format!("There can be only one directive named \"@{directive_name}\"."),
                GraphQLErrorOptions {
                    nodes: Some(vec![
                        AstNode::from((*known_directive_name).clone()),
                        AstNode::from(node.name.clone()),
                    ]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        } else {
            self.known_directive_names
                .insert(String::from(directive_name), &node.name);
        }

        VisitResult::Skip
    }
}

/// Creates the visitor of `UniqueDirectiveNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueDirectiveNamesRule.ts, MIT.
#[must_use]
pub fn unique_directive_names_rule<'c, 'a>(
    context: &'c SdlValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueDirectiveNamesRule::boxed(context)
}
