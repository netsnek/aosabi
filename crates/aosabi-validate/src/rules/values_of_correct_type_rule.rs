// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `ValuesOfCorrectTypeRule`.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/ValuesOfCorrectTypeRule.ts, MIT.
 */

//! The `ValuesOfCorrectTypeRule` of the validation layer.

use alloc::{
    boxed::Box,
    format,
    string::{
        String,
        ToString, //
    },
    vec,
    vec::Vec, //
};

use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    JsStringBuilder,
    OriginalError,
    ast::{
        AstNode,
        BooleanValueNode,
        EnumValueNode,
        FloatValueNode,
        IntValueNode,
        ListValueNode,
        NullValueNode,
        ObjectFieldNode,
        ObjectValueNode,
        OperationDefinitionNode,
        StringValueNode,
        ValueNode,
        VariableDefinitionNode, //
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
    DefinitionError,
    InputObjectType,
    NamedType,
    ObjMap,
    did_you_mean,
    get_named_type,
    get_nullable_type,
    is_list_type,
    is_non_null_type,
    is_required_input_field,
    suggestion_list, //
};

use crate::{
    key_map::key_map,
    validation_context::{
        Rule,
        ValidationContext, //
    }, //
};

/// Value literals of correct type
///
/// A GraphQL document is only valid if all value literals are of the type
/// expected at their position.
///
/// See <https://spec.graphql.org/draft/#sec-Values-of-Correct-Type>
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ValuesOfCorrectTypeRule.ts, MIT.
///
/// A type in a message is its `toString`, which is the `Display` of
/// [`aosabi_type::Type`], because `inspect` calls the `toJSON` of an object that
/// has one and every type answers its string form there.
///
/// A hook receives the node struct of its kind, and `parseLiteral` and the
/// printer of a value take the [`ValueNode`] that wraps it, so a hook wraps a
/// copy of its node before it calls `is_valid_value_node`.
pub struct ValuesOfCorrectTypeRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
    /// The variable definitions of the operation the walk is in, by variable
    /// name, the `variableDefinitions` of the reference, which writes the map and
    /// never reads it.
    variable_definitions: ObjMap<&'a VariableDefinitionNode>,
}

impl<'c, 'a> Rule<'c, 'a> for ValuesOfCorrectTypeRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self {
            context,
            variable_definitions: ObjMap::default(),
        }
    }
}

impl<'a> ValuesOfCorrectTypeRule<'_, 'a> {
    /// Checks an object literal against the input object type of its
    /// position, the body of the `ObjectValue` hook of the reference.
    ///
    /// The reference walks the fields of the type with `Object.values`, which
    /// follows the order the fields were defined in, since a field name is never
    /// an array index.
    ///
    /// # Errors
    ///
    /// Returns what the field thunk of the input object type throws.
    fn check_object_value(
        &self,
        node: &'a ObjectValueNode,
    ) -> Result<VisitResult, DefinitionError> {
        let input_type = self.context.get_input_type();
        let named_type = input_type.as_ref().map(get_named_type);
        let Some(r#type) = named_type.as_ref().and_then(NamedType::as_input_object) else {
            is_valid_value_node(self.context, &ValueNode::from(node.clone()));
            return Ok(VisitResult::Skip); // Don't traverse further.
        };
        // Ensure every required field exists.
        let field_node_map = key_map(&node.fields, |field| String::from(field.name.value()));
        for field_def in r#type.get_fields()?.values() {
            let field_node = field_node_map.get(&field_def.name);
            if field_node.is_none() && is_required_input_field(field_def) {
                let type_str = field_def.r#type.to_string();
                self.context.report_error(GraphQLError::new(
                    format!(
                        "Field \"{}.{}\" of required type \"{type_str}\" was not provided.",
                        r#type.name, field_def.name
                    ),
                    GraphQLErrorOptions {
                        nodes: Some(vec![AstNode::from(node.clone())]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            }
        }

        if r#type.is_one_of {
            validate_one_of_input_object(self.context, node, r#type, &field_node_map);
        }
        Ok(VisitResult::Continue)
    }

    /// Reports an object field that the input object type of its position
    /// does not define, the body of the `ObjectField` hook of the reference.
    ///
    /// # Errors
    ///
    /// Returns what the field thunk of the input object type throws.
    fn check_object_field(
        &self,
        node: &'a ObjectFieldNode,
    ) -> Result<VisitResult, DefinitionError> {
        let parent_input_type = self.context.get_parent_input_type();
        let parent_type = parent_input_type.as_ref().map(get_named_type);
        let field_type = self.context.get_input_type();
        if field_type.is_none()
            && let Some(parent_type) = parent_type.as_ref().and_then(NamedType::as_input_object)
        {
            let field_names: Vec<String> = parent_type.get_fields()?.keys().cloned().collect();
            let suggestions = suggestion_list(node.name.value(), &field_names);
            self.context.report_error(GraphQLError::new(
                format!(
                    "Field \"{}\" is not defined by type \"{}\".",
                    node.name.value(),
                    parent_type.name
                ) + &did_you_mean(None, &suggestions),
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(node.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        Ok(VisitResult::Continue)
    }
}

impl<'a> Visitor<'a> for ValuesOfCorrectTypeRule<'_, 'a> {
    fn enter_operation_definition(
        &mut self,
        _node: &'a OperationDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.variable_definitions = ObjMap::default();
        VisitResult::Continue
    }

    fn enter_variable_definition(
        &mut self,
        definition: &'a VariableDefinitionNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.variable_definitions
            .insert(String::from(definition.variable.name.value()), definition);
        VisitResult::Continue
    }

    fn enter_list_value(
        &mut self,
        node: &'a ListValueNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        // Note: TypeInfo will traverse into a list's item type, so look to the
        // parent input type to check if it is a list.
        let parent_input_type = self.context.get_parent_input_type();
        let r#type = parent_input_type.as_ref().map(get_nullable_type);
        if !r#type.is_some_and(is_list_type) {
            is_valid_value_node(self.context, &ValueNode::from(node.clone()));
            return VisitResult::Skip; // Don't traverse further.
        }
        VisitResult::Continue
    }

    fn enter_object_value(
        &mut self,
        node: &'a ObjectValueNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_object_value(node)
            .unwrap_or_else(|error| self.context.throw_error(error))
    }

    fn enter_object_field(
        &mut self,
        node: &'a ObjectFieldNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        self.check_object_field(node)
            .unwrap_or_else(|error| self.context.throw_error(error))
    }

    fn enter_null_value(
        &mut self,
        node: &'a NullValueNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let r#type = self.context.get_input_type();
        if let Some(r#type) = r#type
            && is_non_null_type(&r#type)
        {
            self.context.report_error(GraphQLError::new(
                format!("Expected value of type \"{type}\", found {}.", print(node)),
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(node.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
        VisitResult::Continue
    }

    fn enter_enum_value(
        &mut self,
        node: &'a EnumValueNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        is_valid_value_node(self.context, &ValueNode::from(node.clone()));
        VisitResult::Continue
    }

    fn enter_int_value(&mut self, node: &'a IntValueNode, _: &VisitContext<'a, '_>) -> VisitResult {
        is_valid_value_node(self.context, &ValueNode::from(node.clone()));
        VisitResult::Continue
    }

    fn enter_float_value(
        &mut self,
        node: &'a FloatValueNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        is_valid_value_node(self.context, &ValueNode::from(node.clone()));
        VisitResult::Continue
    }

    fn enter_string_value(
        &mut self,
        node: &'a StringValueNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        is_valid_value_node(self.context, &ValueNode::from(node.clone()));
        VisitResult::Continue
    }

    fn enter_boolean_value(
        &mut self,
        node: &'a BooleanValueNode,
        _: &VisitContext<'a, '_>,
    ) -> VisitResult {
        is_valid_value_node(self.context, &ValueNode::from(node.clone()));
        VisitResult::Continue
    }
}

/// Any value literal may be a valid representation of a Scalar, depending on
/// that scalar type.
///
/// Only a scalar and an enum carry `parseLiteral`, so the arm of the match that
/// is neither is the `!isLeafType(type)` branch of the reference. What the `try`
/// of the reference guards is `parseLiteral`, whose outcome is the `Result` here.
/// A thrown `GraphQLError` is reported as it is, and any other error is wrapped
/// with the expected type ahead of its message.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ValuesOfCorrectTypeRule.ts, MIT.
fn is_valid_value_node(context: &ValidationContext<'_>, node: &ValueNode) {
    // Report any error at the full type expected by the location.
    let Some(location_type) = context.get_input_type() else {
        return;
    };

    let r#type = get_named_type(&location_type);

    // Scalars and Enums determine if a literal value is valid via parseLiteral(),
    // which may throw or return an invalid value to indicate failure.
    let parse_result = match &r#type {
        NamedType::Scalar(r#type) => r#type.parse_literal(node, None /* variables */),
        NamedType::Enum(r#type) => r#type.parse_literal(node, None /* variables */),
        _ => {
            let type_str = location_type.to_string();
            context.report_error(GraphQLError::new(
                format!(
                    "Expected value of type \"{type_str}\", found {}.",
                    print(node)
                ),
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(node.clone())]),
                    ..GraphQLErrorOptions::default()
                },
            ));
            return;
        }
    };

    match parse_result {
        Ok(parse_result) => {
            if parse_result.is_undefined() {
                let type_str = location_type.to_string();
                context.report_error(GraphQLError::new(
                    format!(
                        "Expected value of type \"{type_str}\", found {}.",
                        print(node)
                    ),
                    GraphQLErrorOptions {
                        nodes: Some(vec![AstNode::from(node.clone())]),
                        ..GraphQLErrorOptions::default()
                    },
                ));
            }
        }
        Err(DefinitionError::GraphQL(error)) => context.report_error(*error),
        Err(error) => {
            let type_str = location_type.to_string();
            let mut message = JsStringBuilder::new();
            message
                .push_str(&format!(
                    "Expected value of type \"{type_str}\", found {}; ",
                    print(node)
                ))
                .push(error.message());
            context.report_error(GraphQLError::new(
                message.finish(),
                GraphQLErrorOptions {
                    nodes: Some(vec![AstNode::from(node.clone())]),
                    original_error: Some(OriginalError::from(error)),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
    }
}

/// Reports a literal of a `OneOf` input object that does not hold exactly
/// one field whose value is not `null`.
///
/// `field_node_map` holds the fields of `node` by name, and its one key is read
/// through a slice pattern, which is the check of `keys.length` and the read of
/// `keys[0]` in one step.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ValuesOfCorrectTypeRule.ts, MIT.
fn validate_one_of_input_object(
    context: &ValidationContext<'_>,
    node: &ObjectValueNode,
    r#type: &InputObjectType,
    field_node_map: &ObjMap<&ObjectFieldNode>,
) {
    let keys: Vec<&String> = field_node_map.keys().collect();

    let [key] = keys.as_slice() else {
        context.report_error(GraphQLError::new(
            format!(
                "OneOf Input Object \"{}\" must specify exactly one key.",
                r#type.name
            ),
            GraphQLErrorOptions {
                nodes: Some(vec![AstNode::from(node.clone())]),
                ..GraphQLErrorOptions::default()
            },
        ));
        return;
    };

    let value = field_node_map.get(*key).map(|field| &field.value);
    let is_null_literal = value.is_none_or(|value| matches!(value, ValueNode::NullValue(_)));

    if is_null_literal {
        context.report_error(GraphQLError::new(
            format!("Field \"{}.{key}\" must be non-null.", r#type.name),
            GraphQLErrorOptions {
                nodes: Some(vec![AstNode::from(node.clone())]),
                ..GraphQLErrorOptions::default()
            },
        ));
    }
}

/// Creates the visitor of `ValuesOfCorrectTypeRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/ValuesOfCorrectTypeRule.ts, MIT.
#[must_use]
pub fn values_of_correct_type_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    ValuesOfCorrectTypeRule::boxed(context)
}
