// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `KnownDirectivesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/KnownDirectivesRule.ts, MIT.
 */

//! The `KnownDirectivesRule` of the validation layer.

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
        AstNode,
        DefinitionNode,
        DirectiveNode,
        NameNode,
        OperationTypeNode, //
    },
    visitor::{
        EnterLeave,
        NodeOrList,
        NodeRef,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    DefinitionError,
    DirectiveLocation,
    ObjMap,
    specified_directives, //
};

use crate::validation_context::{
    Rule,
    SdlOrValidationContext, //
};

/// Known directives
///
/// A GraphQL document is only valid if all `@directives` are known by the
/// schema and legally positioned.
///
/// See <https://spec.graphql.org/draft/#sec-Directives-Are-Defined>
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownDirectivesRule.ts, MIT.
pub struct KnownDirectivesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: SdlOrValidationContext<'c, 'a>,
    /// The locations of every directive by its name, the `locationsMap` of
    /// the reference. A location is the string the reference stores, since
    /// a directive definition of the document names its locations as text.
    locations_map: ObjMap<Vec<&'a str>>,
}

impl<'c, 'a> Rule<'c, 'a> for KnownDirectivesRule<'c, 'a> {
    type Context = SdlOrValidationContext<'c, 'a>;

    fn new(context: Self::Context) -> Self {
        let mut locations_map: ObjMap<Vec<&'a str>> = ObjMap::default();

        let schema = context.get_schema();
        let defined_directives = match schema {
            Some(schema) => schema.get_directives(),
            None => specified_directives(),
        };
        for directive in defined_directives {
            let locations = directive.locations.iter().map(|location| location.as_str());
            locations_map.insert(directive.name.clone(), locations.collect());
        }

        let ast_definitions = &context.get_document().definitions;
        for def in ast_definitions {
            if let DefinitionNode::DirectiveDefinition(def) = def {
                let locations = def.locations.iter().map(NameNode::value);
                locations_map.insert(String::from(def.name.value()), locations.collect());
            }
        }

        Self {
            context,
            locations_map,
        }
    }
}

impl<'a> KnownDirectivesRule<'_, 'a> {
    /// Reports `node` when its directive is unknown or stands where it may
    /// not, the body of the `Directive` hook of the reference.
    ///
    /// # Errors
    ///
    /// Returns what [`get_directive_location_for_ast_path()`] throws.
    fn check_directive(
        &self,
        node: &'a DirectiveNode,
        ancestors: &[NodeOrList<'a>],
    ) -> Result<VisitResult, DefinitionError> {
        let name = node.name.value();
        let locations = self.locations_map.get(name);

        let Some(locations) = locations else {
            self.context.report_error(GraphQLError::new(
                format!("Unknown directive \"@{name}\"."),
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(node.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
            return Ok(VisitResult::Continue);
        };

        let candidate_location = get_directive_location_for_ast_path(ancestors)?;
        if !locations.contains(&candidate_location.as_str()) {
            self.context.report_error(GraphQLError::new(
                format!("Directive \"@{name}\" may not be used on {candidate_location}."),
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(node.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        Ok(VisitResult::Continue)
    }
}

impl<'a> Visitor<'a> for KnownDirectivesRule<'_, 'a> {
    fn enter_directive(
        &mut self,
        node: &'a DirectiveNode,
        context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_directive(node, context.ancestors)
            .unwrap_or_else(|error| self.context.throw_error(error))
    }
}

/// Returns the directive location of the node a directive is applied to,
/// which is the last of `ancestors`.
///
/// The reference declares that it may return `undefined`, yet every arm of
/// its switch returns a location or throws, so the caller of this function
/// reads a location without the check for `undefined`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownDirectivesRule.ts, MIT.
///
/// An input value definition is an input field only below an input object
/// type definition. Below an input object type extension the reference
/// answers `ARGUMENT_DEFINITION`, and so does this function.
///
/// # Errors
///
/// Returns the plain `Error` of `invariant` when the last of `ancestors`,
/// or the third last below an input value definition, is no node, and
/// when the node is of a kind that holds no directives.
fn get_directive_location_for_ast_path(
    ancestors: &[NodeOrList<'_>],
) -> Result<DirectiveLocation, DefinitionError> {
    let applied_to = ancestor_node(ancestors, 1)?;

    match applied_to {
        NodeRef::OperationDefinition(applied_to) => {
            Ok(get_directive_location_for_operation(applied_to.operation))
        }
        NodeRef::Field(_) => Ok(DirectiveLocation::Field),
        NodeRef::FragmentSpread(_) => Ok(DirectiveLocation::FragmentSpread),
        NodeRef::InlineFragment(_) => Ok(DirectiveLocation::InlineFragment),
        NodeRef::FragmentDefinition(_) => Ok(DirectiveLocation::FragmentDefinition),
        NodeRef::VariableDefinition(_) => Ok(DirectiveLocation::VariableDefinition),
        NodeRef::SchemaDefinition(_) | NodeRef::SchemaExtension(_) => Ok(DirectiveLocation::Schema),
        NodeRef::ScalarTypeDefinition(_) | NodeRef::ScalarTypeExtension(_) => {
            Ok(DirectiveLocation::Scalar)
        }
        NodeRef::ObjectTypeDefinition(_) | NodeRef::ObjectTypeExtension(_) => {
            Ok(DirectiveLocation::Object)
        }
        NodeRef::FieldDefinition(_) => Ok(DirectiveLocation::FieldDefinition),
        NodeRef::InterfaceTypeDefinition(_) | NodeRef::InterfaceTypeExtension(_) => {
            Ok(DirectiveLocation::Interface)
        }
        NodeRef::UnionTypeDefinition(_) | NodeRef::UnionTypeExtension(_) => {
            Ok(DirectiveLocation::Union)
        }
        NodeRef::EnumTypeDefinition(_) | NodeRef::EnumTypeExtension(_) => {
            Ok(DirectiveLocation::Enum)
        }
        NodeRef::EnumValueDefinition(_) => Ok(DirectiveLocation::EnumValue),
        NodeRef::InputObjectTypeDefinition(_) | NodeRef::InputObjectTypeExtension(_) => {
            Ok(DirectiveLocation::InputObject)
        }
        NodeRef::InputValueDefinition(_) => {
            let parent_node = ancestor_node(ancestors, 3)?;
            Ok(
                if matches!(parent_node, NodeRef::InputObjectTypeDefinition(_)) {
                    DirectiveLocation::InputFieldDefinition
                } else {
                    DirectiveLocation::ArgumentDefinition
                },
            )
        }
        // Not reachable, all possible types have been considered.
        _ => Err(DefinitionError::plain(format!(
            "Unexpected kind: \"{}\"",
            applied_to.kind()
        ))),
    }
}

/// Returns the node that stands `from_end` places before the end of
/// `ancestors`.
///
/// An array among the ancestors has no `kind`, so it fails the invariant,
/// and so does a place before the start of the list.
///
/// # Errors
///
/// Returns the plain `Error` of `invariant(false)`, whose default message
/// this is.
fn ancestor_node<'a>(
    ancestors: &[NodeOrList<'a>],
    from_end: usize,
) -> Result<NodeRef<'a>, DefinitionError> {
    let index = ancestors.len().checked_sub(from_end);
    match index.and_then(|index| ancestors.get(index)) {
        Some(NodeOrList::Node(node)) => Ok(*node),
        _ => Err(DefinitionError::plain("Unexpected invariant triggered.")),
    }
}

/// Returns the directive location of an operation of kind `operation`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownDirectivesRule.ts, MIT.
const fn get_directive_location_for_operation(operation: OperationTypeNode) -> DirectiveLocation {
    match operation {
        OperationTypeNode::Query => DirectiveLocation::Query,
        OperationTypeNode::Mutation => DirectiveLocation::Mutation,
        OperationTypeNode::Subscription => DirectiveLocation::Subscription,
    }
}

/// Creates the visitor of `KnownDirectivesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownDirectivesRule.ts, MIT.
#[must_use]
pub fn known_directives_rule<'c, 'a>(
    context: SdlOrValidationContext<'c, 'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    KnownDirectivesRule::boxed(context)
}
