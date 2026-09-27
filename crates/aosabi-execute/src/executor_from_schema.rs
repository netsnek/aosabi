// SPDX-License-Identifier: MIT
/*
 * aosabi-execute executor_from_schema: the local executor over a schema a delegation runs.
 *
 * Derived from @graphql-tools/executor 1.5.1, cjs/execution/normalizedExecutor.js, MIT.
 */

//! The executor of a request over a schema, `executor_from_schema`, which
//! runs the request through the normalized executor.
//!
//! A delegation of Federation hands `delegateToSchema` the wrapped schema of
//! a remote, a plain schema with no subschema configuration, so `getExecutor`
//! of `@graphql-tools/delegate` answers `executorFromSchema(targetSchema)`
//! and the outer layer of a delegation executes on the front, over the
//! resolvers of the wrapped schema. The executor answers what
//! [`normalized_executor`] answers, the result or a promise of it, on the
//! agent of the request whose resolver delegates.
//!
//! The reference memoizes the function per schema in a `WeakMap`, so one
//! schema answers one function. The function holds the schema and nothing
//! else, and `getExecutor` calls it once and compares it with nothing, since
//! the server configures no batching, so the memo has no reading here and each
//! call answers a function of its own.
//!
//! The module uses `core` and `alloc` and no item of the standard library,
//! shares the executor through `Rc` and holds no static, so a build of the
//! crate without the standard library changes it in no line.

use alloc::{
    rc::Rc,
    string::String, //
};

use aosabi_json::JsValue;
use aosabi_lang::ast::DocumentNode;

use crate::{
    execute::{
        ExecutionArgs,
        SharedSchema,
        SharedVariables, //
    },
    handle_maybe_promise::MaybePromise,
    normalized_executor::normalized_executor,
    promise::Agent,
    value::{
        ContextValue,
        Value, //
    }, //
};

/// The members of a request the executor of a schema reads.
///
/// The `signal` and the `schemaCoordinateInErrors` of the request have no
/// reading, because [`ExecutionArgs`] leaves both out with the branches that
/// read them.
pub struct SchemaRequest {
    /// The document of the request, which the executor shares from then on.
    pub document: DocumentNode,
    /// The raw variable values, `request.variables`.
    pub variables: Option<JsValue>,
    /// The name of the operation to run, `request.operationName`.
    pub operation_name: Option<String>,
    /// The value the root fields are resolved on, `request.rootValue`.
    pub root_value: Value,
    /// The context value every resolver receives, `request.context`.
    pub context: ContextValue,
}

/// An executor of a schema, which runs a request on the agent it is given
/// and answers the result or a promise of it.
///
/// The `Err` is the value the executor throws before it runs, as
/// [`normalized_executor`] throws it.
pub type SchemaExecutor = Rc<dyn Fn(&Agent, SchemaRequest) -> Result<MaybePromise, Value>>;

/// Returns the executor that runs every request against `schema`.
///
/// The executor passes the request on to [`normalized_executor`] with the
/// default field and type resolvers, which is what `schemaExecutor` hands it,
/// and with no serializer of an executor value, since the scalars of a schema
/// a delegation wraps carry no hook of their own.
#[must_use]
pub fn executor_from_schema(schema: SharedSchema) -> SchemaExecutor {
    Rc::new(move |agent: &Agent, request: SchemaRequest| {
        normalized_executor(
            agent,
            ExecutionArgs {
                schema: schema.clone(),
                document: request.document.into(),
                root_value: request.root_value,
                context_value: request.context,
                variable_values: request
                    .variables
                    .map(|values| -> SharedVariables { Rc::new(values) }),
                operation_name: request.operation_name,
                field_resolver: None,
                type_resolver: None,
                subscribe_field_resolver: None,
                leaf_serializers: None,
            },
        )
    })
}
