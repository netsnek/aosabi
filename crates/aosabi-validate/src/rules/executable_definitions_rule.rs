// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `ExecutableDefinitionsRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/ExecutableDefinitionsRule.ts, MIT.
 */

//! The `ExecutableDefinitionsRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    Kind,
    ast::{
        DefinitionNode,
        DocumentNode,
        NameNode, //
    },
    predicates::is_executable_definition_node,
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

/// Executable definitions
///
/// A GraphQL document is only valid for execution if all definitions are either
/// operation or fragment definitions.
///
/// See <https://spec.graphql.org/draft/#sec-Executable-Definitions>
///
/// The visitor holds the `Document` hook, which reports every definition that
/// is not executable and skips the children of the document.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ExecutableDefinitionsRule.ts, MIT.
pub struct ExecutableDefinitionsRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c AstValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for ExecutableDefinitionsRule<'c, 'a> {
    type Context = &'c AstValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> Visitor<'a> for ExecutableDefinitionsRule<'_, 'a> {
    fn enter_document(
        &mut self,
        node: &'a DocumentNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        for definition in &node.definitions {
            if !is_executable_definition_node(definition.into()) {
                let def_name = def_name(definition);
                self.context.report_error(GraphQLError::new(
                    format!("The {def_name} definition is not executable."),
                    GraphQLErrorOptions {
                        nodes: Some(vec![definition.clone().into()]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            }
        }
        VisitResult::Skip
    }
}

/// Returns the name an error gives `definition`, the `defName` of the
/// reference at lines 24-28.
///
/// A schema definition and a schema extension carry no name and read as
/// `schema`, and every other definition reads as its name in double quotes.
fn def_name(definition: &DefinitionNode) -> String {
    if matches!(
        definition.kind(),
        Kind::SchemaDefinition | Kind::SchemaExtension
    ) {
        return String::from("schema");
    }
    // Every definition that is neither executable nor a schema definition or
    // extension carries a name, so the empty text stands for a case the caller
    // never passes.
    let name = definition_name(definition).map_or("", |name| name.value());
    format!("\"{name}\"")
}

/// Returns the `name` property of `definition`, which a schema definition,
/// a schema extension and an anonymous operation do not carry.
const fn definition_name(definition: &DefinitionNode) -> Option<&NameNode> {
    match definition {
        DefinitionNode::OperationDefinition(node) => node.name.as_ref(),
        DefinitionNode::FragmentDefinition(node) => Some(&node.name),
        DefinitionNode::SchemaDefinition(_) | DefinitionNode::SchemaExtension(_) => None,
        DefinitionNode::ScalarTypeDefinition(node) => Some(&node.name),
        DefinitionNode::ObjectTypeDefinition(node) => Some(&node.name),
        DefinitionNode::InterfaceTypeDefinition(node) => Some(&node.name),
        DefinitionNode::UnionTypeDefinition(node) => Some(&node.name),
        DefinitionNode::EnumTypeDefinition(node) => Some(&node.name),
        DefinitionNode::InputObjectTypeDefinition(node) => Some(&node.name),
        DefinitionNode::DirectiveDefinition(node) => Some(&node.name),
        DefinitionNode::ScalarTypeExtension(node) => Some(&node.name),
        DefinitionNode::ObjectTypeExtension(node) => Some(&node.name),
        DefinitionNode::InterfaceTypeExtension(node) => Some(&node.name),
        DefinitionNode::UnionTypeExtension(node) => Some(&node.name),
        DefinitionNode::EnumTypeExtension(node) => Some(&node.name),
        DefinitionNode::InputObjectTypeExtension(node) => Some(&node.name),
    }
}

/// Creates the visitor of `ExecutableDefinitionsRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ExecutableDefinitionsRule.ts, MIT.
#[must_use]
pub fn executable_definitions_rule<'c, 'a>(
    context: &'c AstValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    ExecutableDefinitionsRule::boxed(context)
}
