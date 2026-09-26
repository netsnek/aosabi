// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the rule `SingleFieldSubscriptionsRule` and the field
 * collection of the executor that the rule runs over a subscription.
 *
 * Derived from graphql-js 16.12.0, src/validation/rules/SingleFieldSubscriptionsRule.ts, MIT.
 * Derived from graphql-js 16.12.0, src/execution/collectFields.ts, MIT.
 */

//! The `SingleFieldSubscriptionsRule` of the validation layer.
//!
//! The rule collects the root fields of a subscription the way the executor
//! does, so this module also carries `collectFields` of the reference and
//! the helpers below it. `collectSubfields` of that file has no caller in
//! the validation layer and is left to the executor.

use alloc::{
    boxed::Box,
    format,
    string::String,
    vec::Vec, //
};

use aosabi_json::{
    JsObject,
    JsValue,
    map::{
        IndexMap,
        IndexSet, //
    },
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        AstNode,
        DefinitionNode,
        DirectiveNode,
        FieldNode,
        FragmentDefinitionNode,
        NameNode,
        NamedTypeNode,
        OperationDefinitionNode,
        OperationTypeNode,
        SelectionNode,
        SelectionSetNode,
        TypeNode, //
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
    Schema,
    directives::{
        graphql_include_directive,
        graphql_skip_directive, //
    },
    get_directive_values,
    is_abstract_type,
    type_from_ast, //
};

use crate::validation_context::{
    Rule,
    ValidationContext, //
};

/// The fields [`collect_fields()`] answers, the `Map` of the reference from
/// a response key to the field nodes that share it.
///
/// A `Map` iterates in the order its keys were first set, and the rule blames
/// the fields behind the first key, so the carrier keeps insertion order.
type CollectedFields<'a> = IndexMap<&'a str, Vec<&'a FieldNode>, KeyHasherBuilder>;

/// The names of the fragments a collection spreads already, the `Set` of
/// the reference.
type VisitedFragmentNames<'a> = IndexSet<&'a str, KeyHasherBuilder>;

/// Subscriptions must only include a non-introspection field.
///
/// A GraphQL subscription is valid only if it contains a single root field and
/// that root field is not an introspection field.
///
/// See <https://spec.graphql.org/draft/#sec-Single-root-field>
///
/// The visitor holds the `OperationDefinition` hook, which collects the root
/// fields of a subscription and reports every field beyond the first and every
/// introspection field.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/SingleFieldSubscriptionsRule.ts, MIT.
pub struct SingleFieldSubscriptionsRule<'c, 'a> {
    /// The context the rule reads and reports to.
    context: &'c ValidationContext<'a>,
}

impl<'c, 'a> Rule<'c, 'a> for SingleFieldSubscriptionsRule<'c, 'a> {
    type Context = &'c ValidationContext<'a>;

    fn new(context: Self::Context) -> Self {
        Self { context }
    }
}

impl<'a> SingleFieldSubscriptionsRule<'_, 'a> {
    /// Checks the operation `node`, the body of the `OperationDefinition`
    /// hook at lines 28-80.
    ///
    /// # Errors
    ///
    /// Returns what [`collect_fields()`] returns, which the reference
    /// throws out of the hook.
    fn check_operation(&self, node: &'a OperationDefinitionNode) -> Result<(), DefinitionError> {
        if node.operation != OperationTypeNode::Subscription {
            return Ok(());
        }
        let schema = self.context.get_schema();
        let Some(subscription_type) = schema.get_subscription_type() else {
            return Ok(());
        };
        let operation_name = node.name.as_ref().map(NameNode::value);
        let variable_values = JsObject::new();
        let document = self.context.get_document();
        let mut fragments: ObjMap<&'a FragmentDefinitionNode> = ObjMap::default();
        for definition in &document.definitions {
            if let DefinitionNode::FragmentDefinition(definition) = definition {
                fragments.insert(String::from(definition.name.value()), definition);
            }
        }
        let fields = collect_fields(
            schema,
            &fragments,
            &variable_values,
            subscription_type,
            &node.selection_set,
        )?;
        self.report_extra_fields(operation_name, &fields);
        self.report_introspection_fields(operation_name, &fields);
        Ok(())
    }

    /// Reports the fields behind every response key but the first, lines
    /// 51-63 of the reference.
    fn report_extra_fields(&self, operation_name: Option<&str>, fields: &CollectedFields<'a>) {
        if fields.len() <= 1 {
            return;
        }
        let extra_field_selections: Vec<AstNode> = fields
            .values()
            .skip(1)
            .flatten()
            .map(|field| AstNode::from((*field).clone()))
            .collect();
        let message = match operation_name {
            Some(operation_name) => {
                format!("Subscription \"{operation_name}\" must select only one top level field.")
            }
            None => String::from("Anonymous Subscription must select only one top level field."),
        };
        self.context.report_error(GraphQLError::new(
            message,
            GraphQLErrorOptions {
                nodes: Some(extra_field_selections),
                ..GraphQLErrorOptions::default()
            },
        ));
    }

    /// Reports the fields of every response key whose first field is an
    /// introspection field, lines 64-77 of the reference.
    fn report_introspection_fields(
        &self,
        operation_name: Option<&str>,
        fields: &CollectedFields<'a>,
    ) {
        for field_nodes in fields.values() {
            let Some(field) = field_nodes.first() else {
                continue;
            };
            let field_name = field.name.value();
            if !field_name.starts_with("__") {
                continue;
            }
            let message = match operation_name {
                Some(operation_name) => format!(
                    "Subscription \"{operation_name}\" must not select an introspection top level field."
                ),
                None => String::from(
                    "Anonymous Subscription must not select an introspection top level field.",
                ),
            };
            self.context.report_error(GraphQLError::new(
                message,
                GraphQLErrorOptions {
                    nodes: Some(
                        field_nodes
                            .iter()
                            .map(|field| AstNode::from((*field).clone()))
                            .collect(),
                    ),
                    ..GraphQLErrorOptions::default()
                },
            ));
        }
    }
}

impl<'a> Visitor<'a> for SingleFieldSubscriptionsRule<'_, 'a> {
    fn enter_operation_definition(
        &mut self,
        node: &'a OperationDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        match self.check_operation(node) {
            Ok(()) => VisitResult::Continue,
            Err(error) => self.context.throw_error(error),
        }
    }
}

/// Given a selectionSet, collects all of the fields and returns them.
///
/// `CollectFields` requires the "runtime type" of an object. For a field that
/// returns an Interface or Union type, the "runtime type" will be the actual
/// object type returned by that field.
///
/// `fragments` maps a fragment name to its definition, `variable_values` holds
/// the coerced variables, `runtime_type` is the object type the selection
/// applies to and `selection_set` is the selection to collect.
///
/// Derived from graphql-js 16.12.0, src/execution/collectFields.ts, MIT.
///
/// # Errors
///
/// Returns what [`get_directive_values()`] returns for a `@skip` or an
/// `@include` whose argument does not coerce, and what a thunk of the schema
/// returns.
fn collect_fields<'a>(
    schema: &Schema,
    fragments: &ObjMap<&'a FragmentDefinitionNode>,
    variable_values: &JsObject,
    runtime_type: &NamedType,
    selection_set: &'a SelectionSetNode,
) -> Result<CollectedFields<'a>, DefinitionError> {
    let mut collection = FieldCollection {
        schema,
        fragments,
        variable_values,
        runtime_type,
        fields: CollectedFields::default(),
        visited_fragment_names: VisitedFragmentNames::default(),
    };
    collection.collect_fields_impl(selection_set)?;
    Ok(collection.fields)
}

/// The arguments `collectFieldsImpl` of the reference passes down its
/// recursion unchanged, beside the two collections it fills.
struct FieldCollection<'s, 'a> {
    /// The schema that resolves a type condition.
    schema: &'s Schema,
    /// The fragment definitions of the document by name.
    fragments: &'s ObjMap<&'a FragmentDefinitionNode>,
    /// The coerced variable values.
    variable_values: &'s JsObject,
    /// The object type the selection applies to.
    runtime_type: &'s NamedType,
    /// The collected fields by response key.
    fields: CollectedFields<'a>,
    /// The names of the fragments spread so far.
    visited_fragment_names: VisitedFragmentNames<'a>,
}

impl<'a> FieldCollection<'_, 'a> {
    /// Collects the fields of `selection_set` into the collection.
    ///
    /// Derived from graphql-js 16.12.0, src/execution/collectFields.ts, MIT.
    fn collect_fields_impl(
        &mut self,
        selection_set: &'a SelectionSetNode,
    ) -> Result<(), DefinitionError> {
        for selection in selection_set.selections.iter() {
            match selection {
                SelectionNode::Field(selection) => {
                    if !should_include_node(self.variable_values, selection.directives.as_deref())?
                    {
                        continue;
                    }
                    let name = get_field_entry_key(selection);
                    self.fields.entry(name).or_default().push(selection);
                }
                SelectionNode::InlineFragment(selection) => {
                    if !should_include_node(self.variable_values, selection.directives.as_deref())?
                        || !does_fragment_condition_match(
                            self.schema,
                            selection.type_condition.as_ref(),
                            self.runtime_type,
                        )?
                    {
                        continue;
                    }
                    self.collect_fields_impl(&selection.selection_set)?;
                }
                SelectionNode::FragmentSpread(selection) => {
                    let frag_name = selection.name.value();
                    if self.visited_fragment_names.contains(frag_name)
                        || !should_include_node(
                            self.variable_values,
                            selection.directives.as_deref(),
                        )?
                    {
                        continue;
                    }
                    self.visited_fragment_names.insert(frag_name);
                    let Some(fragment) = self.fragments.get(frag_name).copied() else {
                        continue;
                    };
                    if !does_fragment_condition_match(
                        self.schema,
                        Some(&fragment.type_condition),
                        self.runtime_type,
                    )? {
                        continue;
                    }
                    self.collect_fields_impl(&fragment.selection_set)?;
                }
            }
        }
        Ok(())
    }
}

/// Determines if a field should be included based on the `@include` and `@skip`
/// directives, where `@skip` has higher precedence than `@include`.
///
/// The reference tests `skip?.if === true` and `include?.if === false`, which
/// holds for the boolean alone and for no other value.
///
/// Derived from graphql-js 16.12.0, src/execution/collectFields.ts, MIT.
///
/// # Errors
///
/// Returns what [`get_directive_values()`] returns.
fn should_include_node(
    variable_values: &JsObject,
    directives: Option<&[DirectiveNode]>,
) -> Result<bool, DefinitionError> {
    let skip = get_directive_values(&graphql_skip_directive(), directives, Some(variable_values))?;
    if matches!(
        skip.as_ref().and_then(|skip| skip.get("if")),
        Some(JsValue::Bool(true))
    ) {
        return Ok(false);
    }

    let include = get_directive_values(
        &graphql_include_directive(),
        directives,
        Some(variable_values),
    )?;
    if matches!(
        include.as_ref().and_then(|include| include.get("if")),
        Some(JsValue::Bool(false))
    ) {
        return Ok(false);
    }
    Ok(true)
}

/// Determines if a fragment is applicable to the given type.
///
/// The reference compares the two types with `===`, which is identity, and so
/// is the equality of a [`NamedType`].
///
/// Derived from graphql-js 16.12.0, src/execution/collectFields.ts, MIT.
///
/// # Errors
///
/// Returns what [`Schema::is_sub_type()`] returns.
fn does_fragment_condition_match(
    schema: &Schema,
    type_condition_node: Option<&NamedTypeNode>,
    ty: &NamedType,
) -> Result<bool, DefinitionError> {
    let Some(type_condition_node) = type_condition_node else {
        return Ok(true);
    };
    let conditional_type = type_from_ast(schema, &TypeNode::NamedType(type_condition_node.clone()));
    let Some(conditional_type) = conditional_type else {
        return Ok(false);
    };
    let Some(conditional_named_type) = conditional_type.as_named() else {
        return Ok(false);
    };
    if conditional_named_type == *ty {
        return Ok(true);
    }
    if is_abstract_type(&conditional_type) {
        return schema.is_sub_type(&conditional_named_type, ty);
    }
    Ok(false)
}

/// Implements the logic to compute the key of a given field's entry
///
/// Derived from graphql-js 16.12.0, src/execution/collectFields.ts, MIT.
fn get_field_entry_key(node: &FieldNode) -> &str {
    node.alias
        .as_ref()
        .map_or(node.name.value(), |alias| alias.value())
}

/// Creates the visitor of `SingleFieldSubscriptionsRule` over `context`.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/SingleFieldSubscriptionsRule.ts, MIT.
#[must_use]
pub fn single_field_subscriptions_rule<'c, 'a>(
    context: &'c ValidationContext<'a>,
) -> Box<dyn EnterLeave<'a> + 'c> {
    SingleFieldSubscriptionsRule::boxed(context)
}
