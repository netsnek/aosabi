// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the wrapping schema of a remote and the proxying
 * resolvers that delegate its root fields.
 *
 * Derived from @graphql-tools/wrap 11.1.9, dist/index.cjs, MIT.
 */

//! The wrapping of a remote schema, `wrap_schema` and the proxying resolvers.
//!
//! [`wrap_schema`] makes the wrapping schema of the subschema configuration
//! `{schema, executor}` over that client schema: every root field resolves
//! through a proxying resolver that delegates to the configuration, so the
//! inner delegation sends through the executor, and every object field
//! resolves through `default_merged_resolver`. The outer delegation of the
//! reference's federation runs over that schema with the local executor, so
//! the two layers stay two (rule 6).
//!
//! A resolver of the type layer is a value the executor shares between
//! threads, and a subschema configuration holds the executor of the one
//! runtime thread behind an `Rc`. The proxying resolver therefore carries
//! the id of its configuration, which a thread local table maps to a weak
//! handle of it, and reads the configuration back at each call. The table
//! and the memo of [`wrap_schema`], the `WeakMap` of `memoize1`, keep their
//! entries while the configuration lives and drop them at the next
//! registration after it is gone, the next call of [`SubschemaHandle::of`],
//! which [`wrap_schema`] and [`generate_proxying_resolvers`] make, while
//! [`SubschemaHandle::get`], which a proxying resolver calls at each call,
//! prunes nothing. A wrapping schema whose configuration is gone answers the
//! plain `Error` of [`GONE`] from a root field, which no delegation meets
//! while the federation that made the configuration keeps it.
//!
//! The configuration of a subschema carries no `createProxyingResolver`,
//! since the federation sets none, so the proxying resolver is always
//! [`default_create_proxying_resolver`]. The branch of `schemaFromExecutor`
//! for an executor that answers an async iterable is left out, because the
//! HTTP executor of a federation answers one result or a promise of it.

use std::{
    cell::{
        Cell,
        RefCell, //
    },
    rc::{
        Rc,
        Weak, //
    },
    sync::Arc, //
};

use aosabi_execute::{
    errors::{
        copy_graphql_error,
        create_graphql_error,
        thrown_definition_error, //
    },
    execute::{
        FieldResolve,
        FieldResolverFn,
        ResolveInfo, //
    },
    handle_maybe_promise::handle_maybe_promise,
    promise::Agent,
    value::{
        Completion,
        ContextValue,
        ErrorObject,
        HostValue,
        ObjectLike,
        PlainObject,
        Value,
        inspect::cross_inspect, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    GraphQLErrorOptions,
    ast::OperationTypeNode,
    parser::{
        ParseOptions,
        parse, //
    }, //
};
use aosabi_type::{
    BuildClientSchemaOptions,
    DefinitionError,
    FieldBinding,
    NamedType,
    ObjMap,
    ResolverHandle,
    Schema,
    build_client_schema,
    get_introspection_query::{
        IntrospectionOptions,
        get_introspection_query, //
    },
    wrapping::create_wrapping_schema, //
};

use crate::{
    delegate::{
        delegate_to_schema::{
            DelegateToSchemaOptions,
            delegate_to_schema, //
        },
        external::annotation_of,
        merged_resolver::default_merged_resolver,
        resolve_external::resolve_external_value,
        subschema::{
            Executor,
            SchemaOrSubschemaConfig,
            SubschemaConfig,
            apply_schema_transforms, //
        }, //
    },
    transforms::ExecutionRequest,
    utils::{
        create_graphql_error_from_object,
        get_response_key_from_info,
        get_root_type_map,
        is_truthy,
        member, //
    }, //
};

/// The message a root field of a wrapping schema throws once its subschema configuration is gone.
pub const GONE: &str = "the subschema configuration of this wrapping schema is gone";

/// The handle of a subschema configuration a resolver carries across threads, the id of a thread local entry.
// An `unsafe` `Send` handle of the configuration would skip the lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubschemaHandle(u64);

thread_local! {
    /// The configurations a proxying resolver reaches, by id, each with the wrapping schema made of it once made.
    static SUBSCHEMAS: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    /// The id the next configuration takes.
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

/// One configuration of the table, its id, a weak handle and its memoized wrapping schema.
struct Entry {
    /// The id the resolvers of the configuration carry.
    id: u64,
    /// The configuration, alive while its owner keeps it.
    config: Weak<SubschemaConfig>,
    /// The wrapping schema of [`wrap_schema`], once made.
    schema: Option<Arc<Schema>>,
}

impl SubschemaHandle {
    /// Returns the handle of `config`, the one it already has or a new one.
    ///
    /// The entries of configurations that are gone leave the table here.
    #[must_use]
    pub fn of(config: &Rc<SubschemaConfig>) -> Self {
        SUBSCHEMAS.with(|table| {
            let mut table = table.borrow_mut();
            table.retain(|entry| entry.config.strong_count() > 0);
            if let Some(entry) = table
                .iter()
                .find(|entry| std::ptr::eq(entry.config.as_ptr(), Rc::as_ptr(config)))
            {
                return Self(entry.id);
            }
            let id = NEXT_ID.with(|next| {
                let id = next.get();
                next.set(id + 1);
                id
            });
            table.push(Entry {
                id,
                config: Rc::downgrade(config),
                schema: None,
            });
            Self(id)
        })
    }

    /// Returns the configuration of the handle, `None` once it is gone or on another thread.
    #[must_use]
    pub fn get(self) -> Option<Rc<SubschemaConfig>> {
        SUBSCHEMAS
            .try_with(|table| {
                table
                    .borrow()
                    .iter()
                    .find(|entry| entry.id == self.0)
                    .and_then(|entry| entry.config.upgrade())
            })
            .ok()
            .flatten()
    }
}

/// Returns the bindings of every root field of the configuration's schema, each to its proxying resolver.
///
/// The root types are walked in the order query, mutation, subscription and
/// their fields in the order of the field map. A field of the subscription
/// root subscribes through the proxying resolver and resolves to the payload
/// itself, the function `identical` at `:41-43`.
///
/// # Errors
///
/// Returns what the field thunk of a root type throws.
pub fn generate_proxying_resolvers(
    subschema_config: &Rc<SubschemaConfig>,
) -> Result<ObjMap<ObjMap<FieldBinding>>, DefinitionError> {
    let handle = SubschemaHandle::of(subschema_config);
    let target_schema = &subschema_config.schema;
    let mut resolvers: ObjMap<ObjMap<FieldBinding>> = ObjMap::default();
    for (operation, root_type) in get_root_type_map(target_schema) {
        let NamedType::Object(root_object) = &root_type else {
            continue;
        };
        let mut fields: ObjMap<FieldBinding> = ObjMap::default();
        for field_name in root_object.get_fields()?.keys() {
            let proxying_resolver = default_create_proxying_resolver(handle, operation);
            let final_resolver =
                create_possibly_nested_proxying_resolver(handle, proxying_resolver);
            let binding = if operation == OperationTypeNode::Subscription {
                FieldBinding {
                    subscribe: Some(ResolverHandle::new(FieldResolve(final_resolver))),
                    resolve: Some(ResolverHandle::new(FieldResolve(Arc::new(identical)))),
                }
            } else {
                FieldBinding {
                    resolve: Some(ResolverHandle::new(FieldResolve(final_resolver))),
                    subscribe: None,
                }
            };
            fields.insert(field_name.clone(), binding);
        }
        resolvers.insert(String::from(root_type.name()), fields);
    }
    Ok(resolvers)
}

/// Answers the payload of a subscription event as the field's value, `identical`.
fn identical(
    _agent: &Agent,
    value: &Value,
    _args: &Rc<PlainObject>,
    _context: &ContextValue,
    _info: &Rc<ResolveInfo>,
) -> Completion {
    Completion::Normal(value.clone())
}

/// Returns the resolver that reads a parent the same remote answered before it delegates.
///
/// A parent that is an external object of the same configuration and holds
/// the response key answers that member through [`resolve_external_value`],
/// and every other call runs `proxying_resolver`.
#[must_use]
pub fn create_possibly_nested_proxying_resolver(
    handle: SubschemaHandle,
    proxying_resolver: FieldResolverFn,
) -> FieldResolverFn {
    Arc::new(move |agent, parent, args, context, info| {
        if !matches!(parent, Value::Undefined | Value::Null) {
            let response_key = get_response_key_from_info(info);
            if let Some(annotation) = annotation_of(parent) {
                let subschema = annotation.subschema_of(response_key);
                let member = match parent {
                    Value::Object(object) => Rc::clone(object).get(response_key),
                    _ => Value::Undefined,
                };
                if let Some(config) = handle.get()
                    && subschema.same(&SchemaOrSubschemaConfig::Config(config))
                    && !matches!(member, Value::Undefined)
                {
                    return Completion::from(resolve_external_value(
                        member,
                        Rc::clone(&annotation.unpathed_errors),
                        &subschema,
                        Some(context),
                        Some(info),
                        None,
                        false,
                    ));
                }
            }
        }
        proxying_resolver(agent, parent, args, context, info)
    })
}

/// Returns the resolver that delegates a root field to the configuration `handle` names.
///
/// The root value is the parent, which the delegation reads only when it is
/// not `undefined`, as the default of a destructuring reads it, and the
/// field name is the one of the resolve information.
#[must_use]
pub fn default_create_proxying_resolver(
    handle: SubschemaHandle,
    operation: OperationTypeNode,
) -> FieldResolverFn {
    Arc::new(move |agent, root_value, args, context, info| {
        let Some(config) = handle.get() else {
            return Completion::Throw(Value::from(ErrorObject::error(GONE)));
        };
        let mut options =
            DelegateToSchemaOptions::new(SchemaOrSubschemaConfig::Config(config), Rc::clone(info));
        options.operation = Some(operation);
        options.root_value = (!matches!(root_value, Value::Undefined)).then(|| root_value.clone());
        options.args = Some(args_object(args));
        options.context = Some(Rc::clone(context));
        delegate_to_schema(agent, options)
    })
}

/// Returns the wrapping schema of `subschema_config`, made once per configuration.
///
/// The schema transforms of the configuration run over the wrapping schema,
/// which [`apply_schema_transforms`] answers as it is.
///
/// # Errors
///
/// Returns what [`generate_proxying_resolvers`] and
/// [`create_wrapping_schema`] throw.
pub fn wrap_schema(subschema_config: &Rc<SubschemaConfig>) -> Result<Arc<Schema>, DefinitionError> {
    let handle = SubschemaHandle::of(subschema_config);
    if let Some(schema) = memoized(handle) {
        return Ok(schema);
    }
    let proxying_resolvers = generate_proxying_resolvers(subschema_config)?;
    let merged_resolver: FieldResolverFn = Arc::new(default_merged_resolver);
    let schema = Arc::new(create_wrapping_schema(
        &subschema_config.schema,
        &proxying_resolvers,
        &ResolverHandle::new(FieldResolve(merged_resolver)),
    )?);
    let transformed = apply_schema_transforms(&schema, subschema_config);
    SUBSCHEMAS.with(|table| {
        if let Some(entry) = table
            .borrow_mut()
            .iter_mut()
            .find(|entry| entry.id == handle.0)
        {
            entry.schema = Some(Arc::clone(&transformed));
        }
    });
    Ok(transformed)
}

/// Returns the wrapping schema the memo keeps for `handle`.
fn memoized(handle: SubschemaHandle) -> Option<Arc<Schema>> {
    SUBSCHEMAS.with(|table| {
        table
            .borrow()
            .iter()
            .find(|entry| entry.id == handle.0)
            .and_then(|entry| entry.schema.clone())
    })
}

/// Returns the client schema of an introspection result, or throws what the result carries instead.
///
/// A result whose `data.__schema` is truthy builds the schema from `data`. A
/// result with a truthy `errors` throws its one error rebuilt as a
/// `GraphQLError`, or an `AggregateError`
/// `Could not obtain introspection result` of all of them, and any other
/// result throws the `GraphQLError` that inspects it.
///
/// # Errors
///
/// Returns the thrown value as described, and what `build_client_schema`
/// throws as the plain `Error` or the `GraphQLError` it is.
pub fn get_schema_from_introspection(introspection_result: &Value) -> Result<Schema, Value> {
    let data = member(introspection_result, "data");
    if is_truthy(&member(&data, "__schema")) {
        return build_client_schema(&data.to_js(), BuildClientSchemaOptions::default())
            .map_err(thrown_definition_error);
    }
    let errors = member(introspection_result, "errors");
    if is_truthy(&errors) {
        let Value::List(errors) = errors else {
            return Err(Value::from(ErrorObject::type_error(
                "introspectionResult.errors.map is not a function",
            )));
        };
        let mut graphql_errors: Vec<Value> = errors.iter().map(rebuilt_error).collect();
        if graphql_errors.len() == 1
            && let Some(error) = graphql_errors.pop()
        {
            return Err(error);
        }
        return Err(Value::from(ErrorObject::Aggregate {
            message: JsString::from("Could not obtain introspection result"),
            errors: graphql_errors,
        }));
    }
    Err(Value::from(ErrorObject::GraphQL(create_graphql_error(
        format!(
            "Could not obtain introspection result, received the following as response; \n {}",
            cross_inspect(introspection_result)
        ),
        GraphQLErrorOptions::default(),
    ))))
}

/// Sends the introspection query through `executor` and answers the client schema of its answer, or a promise of it.
///
/// The request carries `context` and no other member, and the federation's
/// introspection passes no context, so its headers function answers for
/// none. The schema answers as a host value.
#[must_use]
pub fn schema_from_executor(
    agent: &Agent,
    executor: &Executor,
    context: Option<ContextValue>,
) -> Completion {
    let parsed_introspection_query = match parse(
        get_introspection_query(IntrospectionOptions::default()),
        ParseOptions::default(),
    ) {
        Ok(document) => document,
        Err(error) => return Completion::Throw(Value::from(ErrorObject::GraphQL(error))),
    };
    let mut request = ExecutionRequest::new(Rc::new(parsed_introspection_query));
    request.context = context;
    let executor = Rc::clone(executor);
    handle_maybe_promise(
        agent,
        move |agent| executor(agent, request),
        Box::new(
            |_agent, introspection| match get_schema_from_introspection(&introspection) {
                Ok(schema) => Completion::Normal(Value::Host(HostValue::Schema(Arc::new(schema)))),
                Err(error) => Completion::Throw(error),
            },
        ),
        None,
    )
}

/// Returns `createGraphQLError(error.message, error)`, a remote error of the result rebuilt.
///
/// A `GraphQLError` keeps its nodes, its source, its positions, its path,
/// its original error and its extensions, which is what the constructor
/// reads of the error it is handed. Any other value is read member by
/// member as an object, through [`create_graphql_error_from_object`].
fn rebuilt_error(error: &Value) -> Value {
    if let Value::Error(object) = error
        && let ErrorObject::GraphQL(graphql) = object.as_ref()
    {
        return Value::from(ErrorObject::GraphQL(create_graphql_error(
            graphql.message.clone(),
            GraphQLErrorOptions {
                nodes: graphql.nodes.clone(),
                source: graphql.source.clone(),
                positions: graphql.positions.clone(),
                path: graphql.path.clone(),
                original_error: copy_graphql_error(graphql).original_error,
                extensions: Some(graphql.extensions.clone()),
            },
        )));
    }
    create_graphql_error_from_object(error)
}

/// Returns the arguments of a field as the object a delegation passes on.
fn args_object(args: &Rc<PlainObject>) -> JsObject {
    match Value::Object(Rc::clone(args) as Rc<dyn ObjectLike>).to_js() {
        JsValue::Object(object) => object,
        _ => JsObject::new(),
    }
}
