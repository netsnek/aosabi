// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `KnownTypeNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/KnownTypeNamesRule.ts, MIT.
 */

//! The `KnownTypeNamesRule` of the validation layer.

use alloc::{
    borrow::ToOwned,
    boxed::Box,
    format,
    string::String,
    vec,
    vec::Vec, //
};

use aosabi_json::{
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        DefinitionNode,
        NameNode,
        NamedTypeNode, //
    },
    predicates::{
        is_type_definition_node,
        is_type_system_definition_node,
        is_type_system_extension_node, //
    },
    visitor::{
        EnterLeave,
        NodeOrList,
        VisitContext,
        VisitResult,
        Visitor, //
    }, //
};
use aosabi_type::{
    DefinitionError,
    NamedType,
    ObjMap,
    did_you_mean,
    introspection_types,
    specified_scalar_types,
    suggestion_list, //
};

use crate::validation_context::{
    Rule,
    SdlOrValidationContext, //
};

/// Known type names
///
/// A GraphQL document is only valid if referenced types (specifically
/// variable definitions and fragment conditions) are defined by the type schema.
///
/// See <https://spec.graphql.org/draft/#sec-Fragment-Spread-Type-Existence>
///
/// The visitor holds the `NamedType` hook, which reports a name that neither
/// the schema nor the document defines, with the names that lie close to it.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownTypeNamesRule.ts, MIT.
pub struct KnownTypeNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: SdlOrValidationContext<'c, 'a>,
    /// The type map of the schema, absent where the reference holds an
    /// empty null prototype object.
    existing_types_map: Option<&'a ObjMap<NamedType>>,
    /// The names of the types the document defines.
    defined_types: IndexMap<&'a str, bool, KeyHasherBuilder>,
    /// The names of the schema followed by the names of the document.
    type_names: Vec<String>,
}

impl<'c, 'a> Rule<'c, 'a> for KnownTypeNamesRule<'c, 'a> {
    type Context = SdlOrValidationContext<'c, 'a>;

    fn new(context: Self::Context) -> Self {
        let schema = context.get_schema();
        let existing_types_map = schema.map(aosabi_type::Schema::get_type_map);

        let mut defined_types = IndexMap::default();
        for def in &context.get_document().definitions {
            if is_type_definition_node(def.into())
                && let Some(name) = type_definition_name(def)
            {
                defined_types.insert(name.value(), true);
            }
        }

        // `Object.keys` lists an index key before every other key and the
        // other keys in the order they were set. A type name starts with a
        // letter or an underscore, so no key is an index and the order is
        // the order of insertion.
        let type_names = existing_types_map
            .into_iter()
            .flat_map(ObjMap::keys)
            .cloned()
            .chain(defined_types.keys().map(|name| (*name).to_owned()))
            .collect();

        Self {
            context,
            existing_types_map,
            defined_types,
            type_names,
        }
    }
}

impl<'a> KnownTypeNamesRule<'_, 'a> {
    /// Checks the named type `node`, the body of the `NamedType` hook at
    /// lines 49-69.
    ///
    /// # Errors
    ///
    /// Returns what [`standard_type_names()`] returns.
    fn check_named_type(
        &self,
        node: &'a NamedTypeNode,
        visit_context: &VisitContext<'a, '_>,
    ) -> Result<(), DefinitionError> {
        let type_name = node.name.value();
        let is_existing = self
            .existing_types_map
            .is_some_and(|existing_types_map| existing_types_map.contains_key(type_name));
        if is_existing || self.defined_types.contains_key(type_name) {
            return Ok(());
        }

        // An index past the end of an array reads as `undefined`, which the
        // `??` of the reference replaces with the parent.
        let definition_node = visit_context
            .ancestors
            .get(2)
            .copied()
            .or(visit_context.parent);
        let is_sdl = definition_node.is_some_and(is_sdl_node);
        let standard_type_names = standard_type_names()?;
        if is_sdl && standard_type_names.iter().any(|name| name == type_name) {
            return Ok(());
        }

        let suggested_types = suggestion_list(
            type_name,
            &if is_sdl {
                [standard_type_names.as_slice(), self.type_names.as_slice()].concat()
            } else {
                self.type_names.clone()
            },
        );
        self.context.report_error(GraphQLError::new(
            format!("Unknown type \"{type_name}\".") + &did_you_mean(None, &suggested_types),
            GraphQLErrorOptions {
                nodes: Some(vec![node.clone().into()]),
                ..GraphQLErrorOptions::default()
            },
        ));
        Ok(())
    }
}

impl<'a> Visitor<'a> for KnownTypeNamesRule<'_, 'a> {
    fn enter_named_type(
        &mut self,
        node: &'a NamedTypeNode,
        context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        match self.check_named_type(node, context) {
            Ok(()) => VisitResult::Continue,
            Err(error) => self.context.throw_error(error),
        }
    }
}

/// Returns the name of a type definition, the `def.name` the reference reads
/// once `isTypeDefinitionNode` narrowed `def`.
///
/// `None` answers every other definition. The rule of the possible type
/// extensions reads the same property after the same predicate and calls
/// this function.
pub(crate) const fn type_definition_name(def: &DefinitionNode) -> Option<&NameNode> {
    match def {
        DefinitionNode::ScalarTypeDefinition(def) => Some(&def.name),
        DefinitionNode::ObjectTypeDefinition(def) => Some(&def.name),
        DefinitionNode::InterfaceTypeDefinition(def) => Some(&def.name),
        DefinitionNode::UnionTypeDefinition(def) => Some(&def.name),
        DefinitionNode::EnumTypeDefinition(def) => Some(&def.name),
        DefinitionNode::InputObjectTypeDefinition(def) => Some(&def.name),
        _ => None,
    }
}

/// Returns the names of the specified scalar types followed by the names of
/// the introspection types.
///
/// The reference computes the constant when the module loads, and the list
/// is computed on the call here, because the introspection types are built
/// on their first use.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownTypeNamesRule.ts, MIT.
///
/// # Errors
///
/// Returns what [`introspection_types()`] returns.
fn standard_type_names() -> Result<Vec<String>, DefinitionError> {
    let scalar_names = specified_scalar_types()
        .iter()
        .map(|scalar_type| scalar_type.name.clone());
    let introspection_names = introspection_types()?
        .iter()
        .map(|introspection_type| introspection_type.name().to_owned());
    Ok(scalar_names.chain(introspection_names).collect())
}

/// Returns whether `value` is a type system definition or extension node.
///
/// An array of nodes has no `kind` property, so the `in` test of the
/// reference answers false for it.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownTypeNamesRule.ts, MIT.
fn is_sdl_node(value: NodeOrList<'_>) -> bool {
    match value {
        NodeOrList::Node(value) => {
            is_type_system_definition_node(value) || is_type_system_extension_node(value)
        }
        NodeOrList::List(_) => false,
    }
}

/// Creates the visitor of `KnownTypeNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownTypeNamesRule.ts, MIT.
#[must_use]
pub fn known_type_names_rule<'c, 'a>(
    context: SdlOrValidationContext<'c, 'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    KnownTypeNamesRule::boxed(context)
}
