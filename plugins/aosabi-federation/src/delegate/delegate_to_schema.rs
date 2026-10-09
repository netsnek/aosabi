// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the delegation of a field to a schema.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.cjs, MIT.
 */

//! The delegation of one field, `delegate_to_schema`.
//!
//! A delegation builds the request of the field it delegates, runs it
//! through its transformer, hands it to the executor of its target and runs
//! the result back through the transformer. The target of the federation
//! is the wrapped schema of a remote, a plain schema, so [`get_executor`]
//! answers the local executor of that schema, and its root fields delegate
//! again to the subschema configuration of the remote, whose executor is the
//! HTTP executor. The two layers stay two, and every error passes the two
//! checks of the result it passes in the reference (the plan, section
//! DELEGATE, rule 6).
//!
//! The executor answers its result or a promise of it on the agent of the
//! request, and the result goes through the transformer in the one
//! continuation of `handleMaybePromise`, so a refusal of a transform settles
//! synchronously and a result that waits on the network settles when it
//! arrives, at the promise jobs of the reference (the plan, section
//! DELEGATE, rule 7).
//!
//! The branch of `delegateRequest` for an executor that answers an async
//! iterable, a subscription or an incremental result, is left out with
//! `setObjectKeyPath` at `:2652-2670`, because the federation delegates
//! no subscription and sends no incremental request. The stitching
//! information a schema of a stitched gateway carries and the batching of a
//! subschema are absent from the federation, so the lookups of
//! `stitchingInfo.subschemaMap` and `getBatchingExecutor` are left out, and
//! `GLOBAL_CONTEXT`, the key of the batching, with them. The option
//! `validateRequest`, which the reference never sets, takes the validator of
//! the caller, since the validation rules live in a crate this one does not
//! link. The fragments of a request are the values of the fragment map of
//! the resolve information in its order, which `getFragmentDefinitions`
//! keeps in a memo per resolve information that has no reading.

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    execute::{
        FieldNodes,
        ResolveInfo, //
    },
    executor_from_schema::{
        SchemaRequest,
        executor_from_schema, //
    },
    handle_maybe_promise::handle_maybe_promise,
    promise::Agent,
    value::{
        Completion,
        ContextValue,
        ErrorObject,
        Value, //
    }, //
};
use aosabi_json::JsValue;
use aosabi_lang::{
    GraphQLError,
    OriginalError,
    ast::{
        DocumentNode,
        OperationTypeNode,
        SelectionNode,
        SelectionSetNode, //
    },
    js_string::join, //
};
use aosabi_type::{
    NamedType,
    Schema,
    Type, //
};

use crate::{
    delegate::{
        create_request::{
            CreateRequestOptions,
            create_request, //
        },
        subschema::{
            Executor,
            SchemaOrSubschemaConfig,
            apply_schema_transforms, //
        },
        transformer::{
            Transformer,
            get_delegating_operation, //
        }, //
    },
    transforms::{
        DelegationContext,
        ExecutionRequest,
        Transform, //
    },
    utils::{
        get_defined_root_type,
        get_operation_ast_from_request, //
    }, //
};

/// A validator of a document against a schema, `graphql.validate`, which answers its errors.
pub type Validator = Rc<dyn Fn(&Schema, &DocumentNode) -> Vec<GraphQLError>>;

/// The options of `delegateToSchema`, `IDelegateToSchemaOptions`.
///
/// A member that is `None` takes the default `delegateToSchema` gives it.
#[derive(Clone)]
pub struct DelegateToSchemaOptions {
    /// The schema or the subschema configuration to delegate to, `schema`.
    pub schema: SchemaOrSubschemaConfig,
    /// The resolve information of the field that delegates, `info`.
    pub info: Rc<ResolveInfo>,
    /// The name of the operation, `operationName`, the client's by default.
    pub operation_name: Option<String>,
    /// The type of the operation, `operation`, the one of the parent type by default.
    pub operation: Option<OperationTypeNode>,
    /// The name of the target's root field, `fieldName`, the delegating field's by default.
    pub field_name: Option<String>,
    /// The type the delegated value completes as, `returnType`.
    pub return_type: Option<Type>,
    /// The arguments of the target's root field, `args`.
    pub args: Option<aosabi_json::JsObject>,
    /// The selection set of the root field, `selectionSet`.
    pub selection_set: Option<SelectionSetNode>,
    /// The field nodes of the delegating field, `fieldNodes`, the info's by default.
    pub field_nodes: Option<FieldNodes>,
    /// The context value, `context`.
    pub context: Option<ContextValue>,
    /// The value the root fields resolve on, `rootValue`.
    pub root_value: Option<Value>,
    /// The transforms of the delegation in their array order, `transforms`.
    pub transforms: Vec<Rc<dyn Transform>>,
    /// The validator to run the request through, `validateRequest`, none by default.
    pub validate_request: Option<Validator>,
    /// Whether the merging of types is skipped, `skipTypeMerging`.
    pub skip_type_merging: bool,
}

impl DelegateToSchemaOptions {
    /// Returns the options of a delegation of the field `info` describes to `schema`, every other member absent.
    #[must_use]
    pub fn new(schema: SchemaOrSubschemaConfig, info: Rc<ResolveInfo>) -> Self {
        Self {
            schema,
            info,
            operation_name: None,
            operation: None,
            field_name: None,
            return_type: None,
            args: None,
            selection_set: None,
            field_nodes: None,
            context: None,
            root_value: None,
            transforms: Vec::new(),
            validate_request: None,
            skip_type_merging: false,
        }
    }
}

/// The options of `delegateRequest`, the options of a delegation with its request and target schema.
#[derive(Clone)]
pub struct DelegateRequestOptions {
    /// The options of the delegation.
    pub options: DelegateToSchemaOptions,
    /// The request, `request`.
    pub request: ExecutionRequest,
    /// The schema the request is prepared against, `targetSchema`.
    pub target_schema: Arc<Schema>,
}

/// Delegates the field `options.info` describes to a schema and answers its value or a promise of it.
#[must_use]
pub fn delegate_to_schema(agent: &Agent, options: DelegateToSchemaOptions) -> Completion {
    let info = Rc::clone(&options.info);
    let root_value =
        options
            .root_value
            .clone()
            .unwrap_or_else(|| match options.schema.root_value() {
                Value::Undefined | Value::Null => info.root_value.clone(),
                root_value => root_value,
            });
    let operation_name = options.operation_name.clone().or_else(|| {
        info.operation
            .name
            .as_ref()
            .map(|name| String::from(name.value()))
    });
    let operation = options.operation.unwrap_or_else(|| {
        get_delegating_operation(
            &NamedType::Object(Arc::clone(&info.parent_type)),
            &info.schema,
        )
    });
    let field_name = options
        .field_name
        .clone()
        .unwrap_or_else(|| info.field_name.clone());
    let field_nodes = options
        .field_nodes
        .clone()
        .unwrap_or_else(|| Rc::clone(&info.field_nodes));
    let target_schema = match &options.schema {
        SchemaOrSubschemaConfig::Subschema(subschema) => subschema.transformed_schema(),
        SchemaOrSubschemaConfig::Schema(schema) => Arc::clone(schema),
        SchemaOrSubschemaConfig::Config(config) => apply_schema_transforms(&config.schema, config),
    };
    let fragments = info.fragments.values().cloned().collect();
    let request = create_request(CreateRequestOptions {
        subgraph_name: options.schema.name(),
        fragments: Some(fragments),
        root_value,
        target_operation_name: operation_name,
        target_operation: operation,
        target_schema: Some(Arc::clone(&target_schema)),
        target_field_name: Some(field_name),
        selection_set: options.selection_set.clone(),
        field_nodes: Some(field_nodes),
        context: options.context.clone(),
        info: Some(Rc::clone(&info)),
        args: options.args.clone(),
    });
    match request {
        Ok(request) => delegate_request(
            agent,
            DelegateRequestOptions {
                options,
                request,
                target_schema,
            },
        ),
        Err(error) => Completion::Throw(error),
    }
}

/// Sends a request a delegation built through its transformer and its executor.
///
/// The transforms of the request run before the executor is called, so a
/// refusal of a transform is a synchronous throw.
#[must_use]
pub fn delegate_request(agent: &Agent, options: DelegateRequestOptions) -> Completion {
    let delegation_context = match get_delegation_context(&options) {
        Ok(delegation_context) => Rc::new(delegation_context),
        Err(error) => return Completion::Throw(error),
    };
    let transformer = Rc::new(Transformer::new(Rc::clone(&delegation_context)));
    let processed_request = match transformer.transform_request(options.request) {
        Ok(processed_request) => processed_request,
        Err(error) => return Completion::Throw(error),
    };
    if let Some(validator) = &options.options.validate_request
        && let Err(error) =
            validate_request(validator, &delegation_context, &processed_request.document)
    {
        return Completion::Throw(error);
    }
    let executor = get_executor(&delegation_context);
    handle_maybe_promise(
        agent,
        move |agent| executor(agent, processed_request),
        Box::new(move |_agent, executor_result| {
            Completion::from(transformer.transform_result(executor_result))
        }),
        None,
    )
}

/// Returns what a delegation knows while its transforms run.
///
/// A subschema configuration puts its own transforms before the
/// delegation's, and its schema is the target, and a plain schema is the
/// target itself. The field name is the delegation's, or the name of the
/// first root selection of the request. The return type is the delegation's
/// or the one of its resolve information, which every delegation carries, so
/// the fallback to [`get_delegation_return_type`] is never reached.
///
/// # Errors
///
/// Returns the plain error `Cannot infer operation <name>` of a request
/// whose document names no operation to run and the `TypeError` of a first
/// root selection that has no name.
pub fn get_delegation_context(
    options: &DelegateRequestOptions,
) -> Result<DelegationContext, Value> {
    let operation_definition = get_operation_ast_from_request(&options.request)?;
    let target_field_name = match &options.options.field_name {
        Some(field_name) => field_name.clone(),
        None => first_selection_name(&operation_definition.selection_set)?,
    };
    let operation = operation_definition.operation;
    let delegation = &options.options;
    let return_type = delegation
        .return_type
        .clone()
        .unwrap_or_else(|| delegation.info.return_type.clone());
    let subschema = delegation.schema.clone();
    let subschema_config = subschema.config();
    let (transforms, args) = match &subschema_config {
        Some(config) => (
            config
                .transforms
                .iter()
                .chain(&delegation.transforms)
                .cloned()
                .collect(),
            delegation.args.clone(),
        ),
        None => (delegation.transforms.clone(), None),
    };
    Ok(DelegationContext {
        target_schema: subschema.schema(),
        subschema,
        subschema_config,
        operation,
        field_name: target_field_name,
        args,
        context: options.request.context.clone(),
        info: Some(Rc::clone(&delegation.info)),
        return_type,
        transforms,
        transformed_schema: Arc::clone(&options.target_schema),
        skip_type_merging: delegation.skip_type_merging,
    })
}

/// Returns the type of the target's root field `field_name`.
///
/// # Errors
///
/// Returns the refusal of [`get_defined_root_type`] and the plain error
/// `Unable to find field '<field>' in type '<Type>'.` for a field the root
/// type lacks.
pub fn get_delegation_return_type(
    target_schema: &Schema,
    operation: OperationTypeNode,
    field_name: &str,
) -> Result<Type, Value> {
    let root_type = get_defined_root_type(target_schema, operation, None)?;
    let field = match &root_type {
        NamedType::Object(object) => object
            .get_fields()
            .map_err(aosabi_execute::errors::thrown_definition_error)?
            .get(field_name)
            .cloned(),
        _ => None,
    };
    field.map(|field| field.r#type).ok_or_else(|| {
        Value::from(ErrorObject::error(format!(
            "Unable to find field '{field_name}' in type '{}'.",
            root_type.name()
        )))
    })
}

/// Validates a document against the target schema and throws its first error or all of them.
///
/// Several errors throw one `AggregateError` whose message joins theirs with
/// a comma, a space and a line feed, and one error throws its original error
/// or itself.
///
/// # Errors
///
/// Returns the errors of the validation as described.
pub fn validate_request(
    validator: &Validator,
    delegation_context: &DelegationContext,
    document: &DocumentNode,
) -> Result<(), Value> {
    let mut errors = validator(&delegation_context.target_schema, document);
    if errors.len() > 1 {
        let message = join(errors.iter().map(|error| &error.message), ", \n");
        let errors = errors
            .into_iter()
            .map(|error| Value::from(ErrorObject::GraphQL(error)))
            .collect();
        return Err(Value::from(ErrorObject::Aggregate { message, errors }));
    }
    match errors.pop() {
        Some(error) => Err(match error.original_error {
            Some(OriginalError::GraphQL(original)) => Value::from(ErrorObject::GraphQL(*original)),
            _ => Value::from(ErrorObject::GraphQL(error)),
        }),
        None => Ok(()),
    }
}

/// Returns the executor of the delegation's target, its configured one or the local executor of its schema.
#[must_use]
pub fn get_executor(delegation_context: &DelegationContext) -> Executor {
    if let Some(executor) = delegation_context
        .subschema_config
        .as_ref()
        .and_then(|config| config.executor.clone())
    {
        return executor;
    }
    executor_of_schema(Arc::clone(&delegation_context.target_schema))
}

/// Returns the executor of a schema as a delegation calls it, over `executorFromSchema`.
///
/// The executor takes the document the request holds, which the request a
/// delegation hands it holds alone, as `normalizedExecutor` takes
/// `request.document` itself, and copies only a document another request
/// still holds.
fn executor_of_schema(schema: Arc<Schema>) -> Executor {
    let run = executor_from_schema(schema);
    Rc::new(move |agent: &Agent, request: ExecutionRequest| {
        let schema_request = SchemaRequest {
            document: Rc::try_unwrap(request.document)
                .unwrap_or_else(|shared| shared.as_ref().clone()),
            variables: request
                .variables
                .map(|variables| JsValue::Object(variables.into_object())),
            operation_name: request.operation_name,
            root_value: request.root_value,
            context: request.context.unwrap_or_else(|| Rc::new(())),
        };
        Completion::from(run(agent, schema_request))
    })
}

/// Returns the name of the first root selection, `selections[0].name.value`.
///
/// An inline fragment has no name, and an empty selection set has no first
/// selection, so each throws the `TypeError` of the property V8 cannot read.
fn first_selection_name(selection_set: &SelectionSetNode) -> Result<String, Value> {
    match selection_set.selections.first() {
        Some(SelectionNode::Field(field)) => Ok(String::from(field.name.value())),
        Some(SelectionNode::FragmentSpread(spread)) => Ok(String::from(spread.name.value())),
        Some(SelectionNode::InlineFragment(_)) => Err(Value::from(ErrorObject::type_error(
            "Cannot read properties of undefined (reading 'value')",
        ))),
        None => Err(Value::from(ErrorObject::type_error(
            "Cannot read properties of undefined (reading 'name')",
        ))),
    }
}
