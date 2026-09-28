// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the run of one operation through the hook layer.
 *
 * Derived from graphql-yoga 5.18.0, cjs/process-request.js, MIT.
 */

//! `process_request`.
//!
//! [`process_request`] parses, validates, builds the context and executes one
//! operation through the enveloped functions, and answers the validation
//! errors as the result when there are any. A subscription operation runs
//! `subscribe` and any other `execute`, the operation chosen by
//! `getOperationAST` with the operation name of the parameters, and the stream
//! a subscription answers keeps that agent, which the driver of its readable
//! stream runs. The processing of the result, `process-request.js:7-37`, is a
//! step of the function, [`crate::server::HttpServer::process_result`].

use std::rc::Rc;

use aosabi_execute::{
    Agent,
    ErrorObject,
    ExecutionArgs,
    ExecutionResult,
    SharedVariables,
    Value,
    normalized_executor::get_operation_ast,
    value::{
        inspect::inspect,
        to_js_string, //
    }, //
};
use aosabi_hooks::{
    ExecutionOutcome,
    Hooked, //
};
use aosabi_json::JsValue;
use aosabi_lang::ast::OperationTypeNode;

use crate::{
    plugin::{
        GraphQLParams,
        OperationName, //
    },
    server::AgentSetup, //
};

/// Returns the operation name `ExecutionArgs` carries for `params`.
///
/// A name that is no string matches no operation, and the executor names it in
/// its refusal as `String(operationName)` writes it.
fn operation_name_of(params: &GraphQLParams) -> Option<String> {
    match params.operation_name() {
        OperationName::Absent => None,
        OperationName::Name(name) => Some(String::from(name)),
        OperationName::Unmatchable => params.get("operationName").map(|value| {
            aosabi_lang::js_string::to_string_lossy(&to_js_string(&Value::from_js(value)))
                .into_owned()
        }),
    }
}

/// Runs the operation of `params` through `hooked`, `processRequest({ params, enveloped })`.
///
/// The arguments of the execution share the variables of `params`, which the
/// context of the request holds as well, by `Rc`, as the reference hands the
/// executor `params.variables` itself, so no value of them is copied.
///
/// # Errors
///
/// Returns the parse error, the value a hook, the context building or the
/// execution throws, and the error of a missing schema.
pub async fn process_request(
    params: &Rc<GraphQLParams>,
    hooked: &Hooked,
    setup: &AgentSetup,
) -> Result<ExecutionOutcome, Value> {
    // Parse GraphQLParams.
    let query = match params.get("query") {
        Some(JsValue::String(query)) => aosabi_lang::js_string::to_string_lossy(query),
        other => {
            let received = inspect(&other.map_or(Value::Undefined, Value::from_js));
            return Err(Value::from(ErrorObject::error(format!(
                "Body must be a string. Received: {received}."
            ))));
        }
    };
    let document = hooked.parse(&query)?;
    // Validate parsed Document Node.
    let Some(schema) = hooked.schema() else {
        return Err(Value::from(ErrorObject::error(
            "Expected undefined to be a GraphQL schema.",
        )));
    };
    let errors = hooked.validate(&schema, &document)?;
    if !errors.is_empty() {
        return Ok(ExecutionOutcome::Single(ExecutionResult::from_errors(
            errors,
        )));
    }
    // Build the context for the execution.
    let context = hooked.context_factory().await?;
    let operation_name = operation_name_of(params);
    // Get the actual operation.
    let is_subscription = get_operation_ast(&document, operation_name.as_deref())
        .is_some_and(|operation| operation.operation == OperationTypeNode::Subscription);
    let args = ExecutionArgs {
        schema,
        document,
        root_value: Value::Undefined,
        context_value: Rc::new(context),
        variable_values: params
            .variables()
            .is_some()
            .then(|| Rc::clone(params) as SharedVariables),
        operation_name,
        field_resolver: setup.field_resolver.clone(),
        type_resolver: setup.type_resolver.clone(),
        subscribe_field_resolver: setup.subscribe_field_resolver.clone(),
        leaf_serializers: setup.leaf_serializers.clone(),
    };
    let agent = Agent::new(setup.options);
    if let Some(hook) = &setup.setup {
        hook(&agent);
    }
    // Choose the right executor.
    if is_subscription {
        hooked.subscribe(&agent, args).await
    } else {
        hooked.execute(&agent, args).await
    }
}
