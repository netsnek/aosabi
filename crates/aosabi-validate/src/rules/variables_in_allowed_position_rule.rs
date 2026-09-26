// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `VariablesInAllowedPositionRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/VariablesInAllowedPositionRule.ts, MIT.
 */

//! The `VariablesInAllowedPositionRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::{
        String,
        ToString, //
    },
    vec, //
};

use aosabi_json::JsValue;
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        OperationDefinitionNode,
        ValueNode,
        VariableDefinitionNode, //
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
    ObjMap,
    Schema,
    Type,
    is_non_null_type,
    is_nullable_type,
    is_type_sub_type_of,
    type_from_ast, //
};

use crate::validation_context::{
    Rule,
    ValidationContext,
    VariableUsage, //
};

/// Variables in allowed position
///
/// Variable usages must be compatible with the arguments they are passed to.
///
/// See <https://spec.graphql.org/draft/#sec-All-Variable-Usages-are-Allowed>
///
/// Derived from graphql-js 16.12.0, src/validation/rules/VariablesInAllowedPositionRule.ts, MIT.
pub struct VariablesInAllowedPositionRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
    /// The variable definitions of the operation the walk is in, by
    /// variable name, the `varDefMap` of the reference.
    var_def_map: ObjMap<&'a VariableDefinitionNode>,
}

impl<'c, 'a> Rule<'c, 'a> for VariablesInAllowedPositionRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            var_def_map: ObjMap::default(),
        }
    }
}

impl<'a> VariablesInAllowedPositionRule<'_, 'a> {
    /// Reports every variable usage of `operation` whose position does not
    /// allow the type of the variable, the body of the leave hook of
    /// `OperationDefinition` in the reference.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while the usages are
    /// collected or two types are compared.
    fn check_operation(
        &self,
        operation: &'a OperationDefinitionNode,
    ) -> Result<VisitResult, DefinitionError> {
        let usages = self.context.get_recursive_variable_usages(operation)?;

        for usage in usages.iter() {
            self.check_usage(usage)?;
        }
        Ok(VisitResult::Continue)
    }

    /// Reports `usage` when its position does not allow the type of its
    /// variable, the body of the loop over the usages in the reference.
    ///
    /// A type in a message is its `toString`, which is the `Display` of
    /// [`Type`]. The first message gets it through `inspect`, which calls the
    /// `toJSON` of an object that has one, and the second through its template
    /// literal, which applies `ToString` to each substitution.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while two types are
    /// compared.
    fn check_usage(&self, usage: &VariableUsage<'a>) -> Result<(), DefinitionError> {
        let VariableUsage {
            node,
            r#type,
            default_value,
            parent_type,
        } = usage;
        let var_name = node.name.value();
        let var_def = self.var_def_map.get(var_name);
        let (Some(var_def), Some(r#type)) = (var_def, r#type) else {
            return Ok(());
        };

        // A var type is allowed if it is the same or more strict (e.g. is
        // a subtype of) than the expected type. It can be more strict if
        // the variable type is non-null when the expected type is nullable.
        // If both are list types, the variable item type can be more strict
        // than the expected item type (contravariant).
        let schema = self.context.get_schema();
        let var_type = type_from_ast(schema, &var_def.r#type);
        if let Some(var_type) = &var_type
            && !allowed_variable_usage(
                schema,
                var_type,
                var_def.default_value.as_ref(),
                r#type,
                default_value.as_ref(),
            )?
        {
            let var_type_str = var_type.to_string();
            let type_str = r#type.to_string();
            self.context.report_error(GraphQLError::new(
                format!(
                    "Variable \"${var_name}\" of type \"{var_type_str}\" used in position expecting type \"{type_str}\"."
                ),
                GraphQLErrorOptions {
                    nodes: Some(vec![
                        AstNode::from((*var_def).clone()),
                        AstNode::from((*node).clone()),
                    ]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }

        // `isNullableType` of the reference is false for the `undefined` of
        // a variable type the schema does not know, so an absent type ends
        // the condition.
        if let Some(parent_type) = parent_type
            && let Some(input_object) = parent_type.as_input_object()
            && input_object.is_one_of
            && let Some(var_type) = &var_type
            && is_nullable_type(var_type)
        {
            self.context.report_error(GraphQLError::new(
                format!(
                    "Variable \"${var_name}\" is of type \"{var_type}\" but must be non-nullable to be used for OneOf Input Object \"{parent_type}\"."
                ),
                GraphQLErrorOptions {
                    nodes: Some(vec![
                        AstNode::from((*var_def).clone()),
                        AstNode::from((*node).clone()),
                    ]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        Ok(())
    }
}

impl<'a> Visitor<'a> for VariablesInAllowedPositionRule<'_, 'a> {
    fn enter_operation_definition(
        &mut self,
        _operation: &'a OperationDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.var_def_map = ObjMap::default();
        VisitResult::Continue
    }

    fn leave_operation_definition(
        &mut self,
        operation: &'a OperationDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_operation(operation)
            .unwrap_or_else(|error| self.context.throw_error(error))
    }

    fn enter_variable_definition(
        &mut self,
        node: &'a VariableDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.var_def_map
            .insert(String::from(node.variable.name.value()), node);
        VisitResult::Continue
    }
}

/// Returns true if the variable is allowed in the location it was found,
/// which includes considering if default values exist for either the variable
/// or the location at which it is located.
///
/// `location_default_value` is absent or `undefined` where the location
/// declares no default value, and both are the `undefined` the reference
/// compares with.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/VariablesInAllowedPositionRule.ts, MIT.
///
/// # Errors
///
/// Returns what a thunk of the schema throws while the types are compared.
fn allowed_variable_usage(
    schema: &Schema,
    var_type: &Type,
    var_default_value: Option<&ValueNode>,
    location_type: &Type,
    location_default_value: Option<&JsValue>,
) -> Result<bool, DefinitionError> {
    if let Type::NonNull(non_null_location_type) = location_type
        && !is_non_null_type(var_type)
    {
        let has_non_null_variable_default_value =
            var_default_value.is_some_and(|value| !matches!(value, ValueNode::NullValue(_)));
        let has_location_default_value =
            location_default_value.is_some_and(|value| !value.is_undefined());
        if !has_non_null_variable_default_value && !has_location_default_value {
            return Ok(false);
        }
        let nullable_location_type = non_null_location_type.of_type();
        return is_type_sub_type_of(schema, var_type, nullable_location_type);
    }
    is_type_sub_type_of(schema, var_type, location_type)
}

/// Creates the visitor of `VariablesInAllowedPositionRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/VariablesInAllowedPositionRule.ts, MIT.
#[must_use]
pub fn variables_in_allowed_position_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    VariablesInAllowedPositionRule::boxed(context)
}
