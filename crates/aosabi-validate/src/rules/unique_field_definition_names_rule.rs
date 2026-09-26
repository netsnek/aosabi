// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueFieldDefinitionNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueFieldDefinitionNamesRule.ts, MIT.
 */

//! The `UniqueFieldDefinitionNamesRule` of the validation layer.

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
        InputObjectTypeDefinitionNode,
        InputObjectTypeExtensionNode,
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
use aosabi_type::{
    DefinitionError,
    NamedType,
    ObjMap,
    Schema, //
};

use crate::validation_context::{
    Rule,
    SdlValidationContext, //
};

/// Unique field definition names
///
/// A GraphQL complex type is only valid if all its fields are uniquely named.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueFieldDefinitionNamesRule.ts, MIT.
pub struct UniqueFieldDefinitionNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c SdlValidationContext<'a>,
    /// The type map of the schema to extend, the `existingTypeMap` of the
    /// reference. The reference reads an empty object without such a
    /// schema, in which every lookup finds nothing, as it does here.
    existing_type_map: Option<&'a ObjMap<NamedType>>,
    /// The name node of every field the document has defined so far, by
    /// type name and field name, the `knownFieldNames` of the reference.
    known_field_names: ObjMap<ObjMap<&'a NameNode>>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueFieldDefinitionNamesRule<'c, 'a> {
    type Context = &'c SdlValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        let schema = context.get_schema();
        Self {
            context,
            existing_type_map: schema.map(Schema::get_type_map),
            known_field_names: ObjMap::default(),
        }
    }
}

impl<'a> UniqueFieldDefinitionNamesRule<'_, 'a> {
    /// Reports every field of a type definition or extension that the
    /// schema or the document defines already.
    ///
    /// The reference takes the node and reads its `name` and the `name` of each
    /// of its `fields`, and the six node structs share no type, so the caller
    /// passes the `name` of the node and the names of its fields as
    /// `field_nodes`. A node without a list of fields passes none, see <https://github.com/graphql/graphql-js/issues/2203>.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/UniqueFieldDefinitionNamesRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the field thunk of the existing type throws.
    fn check_field_uniqueness(
        &mut self,
        name: &'a NameNode,
        field_nodes: impl Iterator<Item = &'a NameNode>,
    ) -> Result<VisitResult, DefinitionError> {
        let type_name = name.value();

        let field_names = self
            .known_field_names
            .entry(String::from(type_name))
            .or_default();

        for field_def_name in field_nodes {
            let field_name = field_def_name.value();

            let existing_type = self
                .existing_type_map
                .and_then(|existing_type_map| existing_type_map.get(type_name));
            if has_field(existing_type, field_name)? {
                self.context.report_error(GraphQLError::new(
                    format!(
                        "Field \"{type_name}.{field_name}\" already exists in the schema. It cannot also be defined in this type extension."
                    ),
                    GraphQLErrorOptions {
                        nodes: Some(vec![AstNode::from(field_def_name.clone())]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            } else if let Some(known_field_name) = field_names.get(field_name) {
                self.context.report_error(GraphQLError::new(
                    format!("Field \"{type_name}.{field_name}\" can only be defined once."),
                    GraphQLErrorOptions {
                        nodes: Some(vec![
                            AstNode::from((*known_field_name).clone()),
                            AstNode::from(field_def_name.clone()),
                        ]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            } else {
                field_names.insert(String::from(field_name), field_def_name);
            }
        }

        Ok(VisitResult::Skip)
    }
}

impl<'a> Visitor<'a> for UniqueFieldDefinitionNamesRule<'_, 'a> {
    fn enter_input_object_type_definition(
        &mut self,
        node: &'a InputObjectTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let field_nodes = node.fields.iter().flatten().map(|field| &field.name);
        self.check_field_uniqueness(&node.name, field_nodes)
            .unwrap_or_else(|error| self.context.throw_error(error))
    }

    fn enter_input_object_type_extension(
        &mut self,
        node: &'a InputObjectTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let field_nodes = node.fields.iter().flatten().map(|field| &field.name);
        self.check_field_uniqueness(&node.name, field_nodes)
            .unwrap_or_else(|error| self.context.throw_error(error))
    }

    fn enter_interface_type_definition(
        &mut self,
        node: &'a InterfaceTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let field_nodes = node.fields.iter().flatten().map(|field| &field.name);
        self.check_field_uniqueness(&node.name, field_nodes)
            .unwrap_or_else(|error| self.context.throw_error(error))
    }

    fn enter_interface_type_extension(
        &mut self,
        node: &'a InterfaceTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let field_nodes = node.fields.iter().flatten().map(|field| &field.name);
        self.check_field_uniqueness(&node.name, field_nodes)
            .unwrap_or_else(|error| self.context.throw_error(error))
    }

    fn enter_object_type_definition(
        &mut self,
        node: &'a ObjectTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let field_nodes = node.fields.iter().flatten().map(|field| &field.name);
        self.check_field_uniqueness(&node.name, field_nodes)
            .unwrap_or_else(|error| self.context.throw_error(error))
    }

    fn enter_object_type_extension(
        &mut self,
        node: &'a ObjectTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let field_nodes = node.fields.iter().flatten().map(|field| &field.name);
        self.check_field_uniqueness(&node.name, field_nodes)
            .unwrap_or_else(|error| self.context.throw_error(error))
    }
}

/// Returns whether `ty` is an object, interface or input object type that
/// holds a field named `field_name`.
///
/// The reference passes the `undefined` of a type name the type map lacks,
/// which no type predicate accepts, and `ty` is absent for it here.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueFieldDefinitionNamesRule.ts, MIT.
///
/// # Errors
///
/// Returns what the field thunk of `ty` throws.
fn has_field(ty: Option<&NamedType>, field_name: &str) -> Result<bool, DefinitionError> {
    match ty {
        Some(NamedType::Object(ty)) => Ok(ty.get_fields()?.contains_key(field_name)),
        Some(NamedType::Interface(ty)) => Ok(ty.get_fields()?.contains_key(field_name)),
        Some(NamedType::InputObject(ty)) => Ok(ty.get_fields()?.contains_key(field_name)),
        _ => Ok(false),
    }
}

/// Creates the visitor of `UniqueFieldDefinitionNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueFieldDefinitionNamesRule.ts, MIT.
#[must_use]
pub fn unique_field_definition_names_rule<'c, 'a>(
    context: &'c SdlValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueFieldDefinitionNamesRule::boxed(context)
}
