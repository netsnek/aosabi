// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the transformer that runs the transforms of a
 * delegation in their reversed order.
 *
 * The class Transformer.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.cjs, MIT.
 */

//! The transformer of a delegation, `Transformer`.
//!
//! The constructor reverses the transforms of the delegation, so the request
//! runs through them from the last of the array to the first, between the
//! preparation and the finalization of the document, and the result runs
//! through them from the first to the last, before the result is checked.
//! The gateway hands the four transforms `InjectNeeds`, `InlineArgs`,
//! `ForceArgs` and `Patch` in that array order, so the request path is
//! `Patch`, `ForceArgs`, `InlineArgs`, `InjectNeeds`, and no transform is
//! reordered, which quirk `Q10-5` pins.
//!
//! The transformation context, an empty object per transform, has no reader
//! in the four transforms and no member here.

use std::{
    cell::Cell,
    rc::Rc, //
};

use aosabi_execute::value::{
    ErrorObject,
    ObjectLike,
    PlainObject,
    Value, //
};
use aosabi_json::JsValue;
use aosabi_lang::ast::OperationTypeNode;
use aosabi_type::{
    NamedType,
    Schema, //
};

use crate::{
    delegate::{
        check_result::check_result_and_handle_errors,
        finalize::finalize_gateway_request,
        prepare::prepare_gateway_document, //
    },
    transforms::{
        DelegationContext,
        ExecutionRequest,
        Transform, //
    }, //
};

/// The transforms of one delegation in their reversed order, with the delegation they belong to.
pub struct Transformer {
    /// The transforms, reversed, `transformations`.
    transformations: Vec<Rc<dyn Transform>>,
    /// The delegation, `delegationContext`.
    delegation_context: Rc<DelegationContext>,
    /// Whether the finalization aliased two fields apart, `hasOverlappingAliases`.
    has_overlapping_aliases: Cell<bool>,
}

impl Transformer {
    /// Returns the transformer of a delegation, with its transforms reversed.
    #[must_use]
    pub fn new(context: Rc<DelegationContext>) -> Self {
        let delegation_transforms = context.transforms.iter().rev().cloned().collect();
        Self {
            transformations: delegation_transforms,
            delegation_context: context,
            has_overlapping_aliases: Cell::new(false),
        }
    }

    /// Returns the request prepared, run through every transform and finalized.
    ///
    /// # Errors
    ///
    /// Returns what the preparation, a transform and the finalization throw.
    pub fn transform_request(
        &self,
        original_request: ExecutionRequest,
    ) -> Result<ExecutionRequest, Value> {
        let context = &self.delegation_context;
        let document = prepare_gateway_document(
            &original_request.document,
            original_request.fragments.as_deref().unwrap_or_default(),
            &context.transformed_schema,
            &context.return_type,
            context.info.as_ref().map(|info| &info.schema),
        )?;
        let mut request = ExecutionRequest {
            document: Rc::new(document),
            fragments: None,
            ..original_request
        };
        for transformation in &self.transformations {
            request = transformation
                .transform_request(request, context)
                .map_err(|thrown| thrown.0)?;
        }
        finalize_gateway_request(request, context, &mut || {
            self.has_overlapping_aliases.set(true);
        })
    }

    /// Returns the result run through every transform and checked.
    ///
    /// A result whose request the finalization aliased has the aliases taken
    /// out first.
    ///
    /// # Errors
    ///
    /// Returns what a transform and [`check_result_and_handle_errors`] throw.
    pub fn transform_result(&self, original_result: Value) -> Result<Value, Value> {
        let context = &self.delegation_context;
        let mut result = original_result;
        for transformation in self.transformations.iter().rev() {
            result = transformation
                .transform_result(result, context)
                .map_err(|thrown| thrown.0)?;
        }
        if self.has_overlapping_aliases.get() {
            result = remove_overlapping_aliases(&result);
        }
        check_result_and_handle_errors(&result, context)
    }

    /// Returns the delegation the transformer belongs to.
    #[must_use]
    pub fn delegation_context(&self) -> &Rc<DelegationContext> {
        &self.delegation_context
    }
}

/// Returns a copy of a result with the prefixes `_nullable_` and `_nonNullable_` taken off its keys.
///
/// A list is mapped, and an object, an error among them, becomes a plain
/// object of its enumerable properties, which for a `GraphQLError` are
/// `message`, `path`, `locations` and `extensions` in the order its
/// constructor makes them, and for any other error none. A function and a
/// primitive stay as they are.
#[must_use]
pub fn remove_overlapping_aliases(result: &Value) -> Value {
    match result {
        Value::List(items) => Value::from(
            items
                .iter()
                .map(remove_overlapping_aliases)
                .collect::<Vec<_>>(),
        ),
        Value::Object(object) => {
            let new_result = PlainObject::new();
            for key in object.own_keys() {
                let value = remove_overlapping_aliases(&Rc::clone(object).get(&key));
                let once = key.strip_prefix("_nullable_").unwrap_or(&key);
                let new_key = once.strip_prefix("_nonNullable_").unwrap_or(once);
                new_result.set(new_key, value);
            }
            Value::from(new_result)
        }
        Value::Error(error) => {
            let new_result = PlainObject::new();
            if let ErrorObject::GraphQL(error) = error.as_ref() {
                let formatted = error.to_json();
                new_result.set("message", Value::String(formatted.message.clone()));
                let path = error.path.as_ref().map_or(Value::Undefined, |path| {
                    Value::from(
                        path.iter()
                            .map(|segment| Value::from_js(&JsValue::from(segment)))
                            .collect::<Vec<_>>(),
                    )
                });
                new_result.set("path", path);
                let locations = error
                    .locations
                    .as_ref()
                    .map_or(Value::Undefined, |locations| {
                        Value::from(
                            locations
                                .iter()
                                .map(|location| {
                                    remove_overlapping_aliases(&Value::from_js(&JsValue::from(
                                        *location,
                                    )))
                                })
                                .collect::<Vec<_>>(),
                        )
                    });
                new_result.set("locations", locations);
                new_result.set(
                    "extensions",
                    remove_overlapping_aliases(&Value::from_js(&JsValue::Object(
                        error.extensions.clone(),
                    ))),
                );
            }
            Value::from(new_result)
        }
        Value::Date(_) | Value::Promise(_) | Value::Host(_) => Value::from(PlainObject::new()),
        other => other.clone(),
    }
}

/// Returns the operation a field of `parent_type` delegates as, by the root type it is.
#[must_use]
pub fn get_delegating_operation(parent_type: &NamedType, schema: &Schema) -> OperationTypeNode {
    if schema.get_mutation_type() == Some(parent_type) {
        return OperationTypeNode::Mutation;
    }
    if schema.get_subscription_type() == Some(parent_type) {
        return OperationTypeNode::Subscription;
    }
    OperationTypeNode::Query
}
