// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueDirectivesPerLocationRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueDirectivesPerLocationRule.ts, MIT.
 */

//! The `UniqueDirectivesPerLocationRule` of the validation layer.

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
        DefinitionNode,
        DirectiveNode,
        NameNode, //
    },
    visitor::{
        EnterLeave,
        NodeRef,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    ObjMap,
    specified_directives, //
};

use crate::validation_context::{
    Rule,
    SdlOrValidationContext, //
};

/// Unique directive names per location
///
/// A GraphQL document is only valid if all non-repeatable directives at
/// a given location are uniquely named.
///
/// See <https://spec.graphql.org/draft/#sec-Directives-Are-Unique-Per-Location>
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueDirectivesPerLocationRule.ts, MIT.
pub struct UniqueDirectivesPerLocationRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: SdlOrValidationContext<'c, 'a>,
    /// Whether a directive may stand once only, by its name, the
    /// `uniqueDirectiveMap` of the reference.
    unique_directive_map: ObjMap<bool>,
    /// The directives seen on the schema definition and its extensions, the
    /// `schemaDirectives` of the reference.
    schema_directives: ObjMap<&'a DirectiveNode>,
    /// The directives seen on a type definition and its extensions, by type
    /// name, the `typeDirectivesMap` of the reference.
    type_directives_map: ObjMap<ObjMap<&'a DirectiveNode>>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueDirectivesPerLocationRule<'c, 'a> {
    type Context = SdlOrValidationContext<'c, 'a>;

    fn new(context: Self::Context) -> Self {
        let mut unique_directive_map: ObjMap<bool> = ObjMap::default();

        let schema = context.get_schema();
        let defined_directives = match schema {
            Some(schema) => schema.get_directives(),
            None => specified_directives(),
        };
        for directive in defined_directives {
            unique_directive_map.insert(directive.name.clone(), !directive.is_repeatable);
        }

        let ast_definitions = &context.get_document().definitions;
        for def in ast_definitions {
            if let DefinitionNode::DirectiveDefinition(def) = def {
                unique_directive_map.insert(String::from(def.name.value()), !def.repeatable);
            }
        }

        Self {
            context,
            unique_directive_map,
            schema_directives: ObjMap::default(),
            type_directives_map: ObjMap::default(),
        }
    }
}

impl<'a> Visitor<'a> for UniqueDirectivesPerLocationRule<'_, 'a> {
    // Many different AST nodes may contain directives. Rather than listing
    // them all, just listen for entering any node, and check to see if it
    // defines any directives.
    fn enter(&mut self, node: NodeRef<'a>, _: &VisitContext<'a, '_>) -> VisitResult {
        let Some(directives) = directives_of(node) else {
            return VisitResult::Continue;
        };

        let mut location_directives = ObjMap::default();
        let seen_directives = if matches!(
            node,
            NodeRef::SchemaDefinition(_) | NodeRef::SchemaExtension(_)
        ) {
            &mut self.schema_directives
        } else if let Some(name) = type_definition_or_extension_name(node) {
            let type_name = name.value();
            self.type_directives_map
                .entry(String::from(type_name))
                .or_default()
        } else {
            &mut location_directives
        };

        for directive in directives {
            let directive_name = directive.name.value();

            if self
                .unique_directive_map
                .get(directive_name)
                .is_some_and(|unique| *unique)
            {
                if let Some(seen_directive) = seen_directives.get(directive_name) {
                    self.context.report_error(GraphQLError::new(
                        format!(
                            "The directive \"@{directive_name}\" can only be used once at this location."
                        ),
                        GraphQLErrorOptions {
                            nodes: Some(vec![
                                AstNode::from((*seen_directive).clone()),
                                AstNode::from(directive.clone()),
                            ]),
                            ..GraphQLErrorOptions::default()
                        },
                    ));
                } else {
                    seen_directives.insert(String::from(directive_name), directive);
                }
            }
        }
        VisitResult::Continue
    }
}

/// Returns the directives of `node`, when its kind holds directives and
/// the node carries the list.
///
/// The kinds are the ones whose node declares `directives`.
fn directives_of(node: NodeRef<'_>) -> Option<&[DirectiveNode]> {
    match node {
        NodeRef::OperationDefinition(node) => node.directives.as_deref(),
        NodeRef::VariableDefinition(node) => node.directives.as_deref(),
        NodeRef::Field(node) => node.directives.as_deref(),
        NodeRef::FragmentSpread(node) => node.directives.as_deref(),
        NodeRef::InlineFragment(node) => node.directives.as_deref(),
        NodeRef::FragmentDefinition(node) => node.directives.as_deref(),
        NodeRef::SchemaDefinition(node) => node.directives.as_deref(),
        NodeRef::ScalarTypeDefinition(node) => node.directives.as_deref(),
        NodeRef::ObjectTypeDefinition(node) => node.directives.as_deref(),
        NodeRef::FieldDefinition(node) => node.directives.as_deref(),
        NodeRef::InputValueDefinition(node) => node.directives.as_deref(),
        NodeRef::InterfaceTypeDefinition(node) => node.directives.as_deref(),
        NodeRef::UnionTypeDefinition(node) => node.directives.as_deref(),
        NodeRef::EnumTypeDefinition(node) => node.directives.as_deref(),
        NodeRef::EnumValueDefinition(node) => node.directives.as_deref(),
        NodeRef::InputObjectTypeDefinition(node) => node.directives.as_deref(),
        NodeRef::SchemaExtension(node) => node.directives.as_deref(),
        NodeRef::ScalarTypeExtension(node) => node.directives.as_deref(),
        NodeRef::ObjectTypeExtension(node) => node.directives.as_deref(),
        NodeRef::InterfaceTypeExtension(node) => node.directives.as_deref(),
        NodeRef::UnionTypeExtension(node) => node.directives.as_deref(),
        NodeRef::EnumTypeExtension(node) => node.directives.as_deref(),
        NodeRef::InputObjectTypeExtension(node) => node.directives.as_deref(),
        _ => None,
    }
}

/// Returns the name of `node` when it is a type definition or a type
/// extension.
///
/// The kinds are the ones the two predicates accept.
const fn type_definition_or_extension_name(node: NodeRef<'_>) -> Option<&NameNode> {
    match node {
        NodeRef::ScalarTypeDefinition(node) => Some(&node.name),
        NodeRef::ObjectTypeDefinition(node) => Some(&node.name),
        NodeRef::InterfaceTypeDefinition(node) => Some(&node.name),
        NodeRef::UnionTypeDefinition(node) => Some(&node.name),
        NodeRef::EnumTypeDefinition(node) => Some(&node.name),
        NodeRef::InputObjectTypeDefinition(node) => Some(&node.name),
        NodeRef::ScalarTypeExtension(node) => Some(&node.name),
        NodeRef::ObjectTypeExtension(node) => Some(&node.name),
        NodeRef::InterfaceTypeExtension(node) => Some(&node.name),
        NodeRef::UnionTypeExtension(node) => Some(&node.name),
        NodeRef::EnumTypeExtension(node) => Some(&node.name),
        NodeRef::InputObjectTypeExtension(node) => Some(&node.name),
        _ => None,
    }
}

/// Creates the visitor of `UniqueDirectivesPerLocationRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueDirectivesPerLocationRule.ts, MIT.
#[must_use]
pub fn unique_directives_per_location_rule<'c, 'a>(
    context: SdlOrValidationContext<'c, 'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueDirectivesPerLocationRule::boxed(context)
}
