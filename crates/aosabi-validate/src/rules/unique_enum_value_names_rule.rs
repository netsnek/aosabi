// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueEnumValueNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueEnumValueNamesRule.ts, MIT.
 */

//! The `UniqueEnumValueNamesRule` of the validation layer.

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
        EnumTypeDefinitionNode,
        EnumTypeExtensionNode,
        EnumValueDefinitionNode,
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
    DefinitionError,
    NamedType,
    ObjMap,
    Schema, //
};

use crate::validation_context::{
    Rule,
    SdlValidationContext, //
};

/// Unique enum value names
///
/// A GraphQL enum type is only valid if all its values are uniquely named.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueEnumValueNamesRule.ts, MIT.
pub struct UniqueEnumValueNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c SdlValidationContext<'a>,
    /// The type map of the schema to extend, the `existingTypeMap` of the
    /// reference. The reference reads an empty object without such a
    /// schema, in which every lookup finds nothing, as it does here.
    existing_type_map: Option<&'a ObjMap<NamedType>>,
    /// The name node of every value the document has defined so far, by
    /// type name and value name, the `knownValueNames` of the reference.
    known_value_names: ObjMap<ObjMap<&'a NameNode>>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueEnumValueNamesRule<'c, 'a> {
    type Context = &'c SdlValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        let schema = context.get_schema();
        Self {
            context,
            existing_type_map: schema.map(Schema::get_type_map),
            known_value_names: ObjMap::default(),
        }
    }
}

impl<'a> UniqueEnumValueNamesRule<'_, 'a> {
    /// Reports every value of an enum definition or extension that the
    /// schema or the document defines already.
    ///
    /// The reference takes the node, and the two node structs share no
    /// type, so the caller passes the `name` and the `values` of the
    /// node.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/UniqueEnumValueNamesRule.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what the value thunk of the existing enum type throws.
    fn check_value_uniqueness(
        &mut self,
        name: &'a NameNode,
        values: Option<&'a [EnumValueDefinitionNode]>,
    ) -> Result<VisitResult, DefinitionError> {
        let type_name = name.value();

        // An enum may hold no list of values, see
        // https://github.com/graphql/graphql-js/issues/2203.
        let value_nodes = values.unwrap_or_default();
        let value_names = self
            .known_value_names
            .entry(String::from(type_name))
            .or_default();

        for value_def in value_nodes {
            let value_name = value_def.name.value();

            let existing_type = self
                .existing_type_map
                .and_then(|existing_type_map| existing_type_map.get(type_name));
            let existing_enum = existing_type.and_then(NamedType::as_enum);
            if let Some(existing_enum) = existing_enum
                && existing_enum.get_value(value_name)?.is_some()
            {
                self.context.report_error(GraphQLError::new(
                    format!(
                        "Enum value \"{type_name}.{value_name}\" already exists in the schema. It cannot also be defined in this type extension."
                    ),
                    GraphQLErrorOptions {
                        nodes: Some(vec![AstNode::from(value_def.name.clone())]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            } else if let Some(known_value_name) = value_names.get(value_name) {
                self.context.report_error(GraphQLError::new(
                    format!("Enum value \"{type_name}.{value_name}\" can only be defined once."),
                    GraphQLErrorOptions {
                        nodes: Some(vec![
                            AstNode::from((*known_value_name).clone()),
                            AstNode::from(value_def.name.clone()),
                        ]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            } else {
                value_names.insert(String::from(value_name), &value_def.name);
            }
        }

        Ok(VisitResult::Skip)
    }
}

impl<'a> Visitor<'a> for UniqueEnumValueNamesRule<'_, 'a> {
    fn enter_enum_type_definition(
        &mut self,
        node: &'a EnumTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_value_uniqueness(&node.name, node.values.as_deref())
            .unwrap_or_else(|error| self.context.throw_error(error))
    }

    fn enter_enum_type_extension(
        &mut self,
        node: &'a EnumTypeExtensionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_value_uniqueness(&node.name, node.values.as_deref())
            .unwrap_or_else(|error| self.context.throw_error(error))
    }
}

/// Creates the visitor of `UniqueEnumValueNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueEnumValueNamesRule.ts, MIT.
#[must_use]
pub fn unique_enum_value_names_rule<'c, 'a>(
    context: &'c SdlValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueEnumValueNamesRule::boxed(context)
}
