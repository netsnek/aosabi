// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `UniqueTypeNamesRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/UniqueTypeNamesRule.ts, MIT.
 */

//! The `UniqueTypeNamesRule` of the validation layer.

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
        InputObjectTypeDefinitionNode,
        InterfaceTypeDefinitionNode,
        NameNode,
        ObjectTypeDefinitionNode,
        ScalarTypeDefinitionNode,
        UnionTypeDefinitionNode, //
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

/// Unique type names
///
/// A GraphQL document is only valid if all defined types have unique names.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueTypeNamesRule.ts, MIT.
pub struct UniqueTypeNamesRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c SdlValidationContext<'a>,
    /// The name node of every type the document has defined so far, the
    /// `knownTypeNames` of the reference.
    known_type_names: ObjMap<&'a NameNode>,
    /// The schema to extend, when there is one.
    schema: Option<&'a Schema>,
}

impl<'c, 'a> Rule<'c, 'a> for UniqueTypeNamesRule<'c, 'a> {
    type Context = &'c SdlValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            known_type_names: ObjMap::default(),
            schema: context.get_schema(),
        }
    }
}

impl<'a> UniqueTypeNamesRule<'_, 'a> {
    /// Reports a type definition whose name the schema or the document
    /// defines already.
    ///
    /// The reference takes the node and reads its `name` alone, and the six
    /// node structs share no type, so the caller passes `name`. The branch
    /// that finds the name in the schema returns `undefined` and the two
    /// others return `false`, which is why the answers differ.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/rules/UniqueTypeNamesRule.ts, MIT.
    fn check_type_name(&mut self, name: &'a NameNode) -> VisitResult {
        let type_name = name.value();

        if self
            .schema
            .is_some_and(|schema| schema.get_type(type_name).is_some())
        {
            self.context.report_error(GraphQLError::new(
                format!(
                    "Type \"{type_name}\" already exists in the schema. It cannot also be defined in this type definition."
                ),
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(name.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
            return VisitResult::Continue;
        }

        if let Some(known_type_name) = self.known_type_names.get(type_name) {
            self.context.report_error(GraphQLError::new(
                format!("There can be only one type named \"{type_name}\"."),
                GraphQLErrorOptions {
                    nodes: Some(vec![
                        AstNode::from((*known_type_name).clone()),
                        AstNode::from(name.clone()),
                    ]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        } else {
            self.known_type_names.insert(String::from(type_name), name);
        }

        VisitResult::Skip
    }
}

impl<'a> Visitor<'a> for UniqueTypeNamesRule<'_, 'a> {
    fn enter_scalar_type_definition(
        &mut self,
        node: &'a ScalarTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_type_name(&node.name)
    }

    fn enter_object_type_definition(
        &mut self,
        node: &'a ObjectTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_type_name(&node.name)
    }

    fn enter_interface_type_definition(
        &mut self,
        node: &'a InterfaceTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_type_name(&node.name)
    }

    fn enter_union_type_definition(
        &mut self,
        node: &'a UnionTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_type_name(&node.name)
    }

    fn enter_enum_type_definition(
        &mut self,
        node: &'a EnumTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_type_name(&node.name)
    }

    fn enter_input_object_type_definition(
        &mut self,
        node: &'a InputObjectTypeDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_type_name(&node.name)
    }
}

/// Creates the visitor of `UniqueTypeNamesRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/UniqueTypeNamesRule.ts, MIT.
#[must_use]
pub fn unique_type_names_rule<'c, 'a>(
    context: &'c SdlValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    UniqueTypeNamesRule::boxed(context)
}
