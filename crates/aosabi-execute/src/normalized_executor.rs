// SPDX-License-Identifier: MIT
/*
 * aosabi-execute normalized_executor: the entry Yoga calls for every operation.
 *
 * Derived from @graphql-tools/executor 1.5.1, cjs/execution/normalizedExecutor.js, MIT.
 * Derived from graphql-js 16.12.0, src/utilities/getOperationAST.ts, MIT.
 */

//! The entry of the executor as Yoga installs it.
//!
//! Yoga installs `normalizedExecutor` as its `execute` and as its `subscribe`, so
//! this function and not [`execute`] is what a request enters. The three operation
//! resolution errors of [`crate::execute::build_execution_context`] are out of its
//! reach, because it throws before `execute` runs when the document holds no
//! operation to run.
//!
//! The module names `core` and `alloc` and no item of `std`.

use alloc::boxed::Box;

use aosabi_lang::ast::{
    DefinitionNode,
    DocumentNode,
    NameNode,
    OperationDefinitionNode,
    OperationTypeNode, //
};

use crate::{
    execute::{
        ExecutionArgs,
        execute, //
    },
    handle_maybe_promise::{
        MaybePromise,
        handle_maybe_promise, //
    },
    promise::Agent,
    subscribe::subscribe,
    value::{
        Completion,
        ErrorObject,
        Value, //
    }, //
};

/// Returns an operation AST given a document AST and optionally an operation
/// name.
///
/// If a name is not provided, an operation is only returned if only one is
/// provided in the document. With a name the first operation of that name is
/// returned, where [`crate::execute::build_execution_context`] runs the last.
///
/// Derived from graphql-js 16.12.0, src/utilities/getOperationAST.ts, MIT.
#[must_use]
pub fn get_operation_ast<'a>(
    document_ast: &'a DocumentNode,
    operation_name: Option<&str>,
) -> Option<&'a OperationDefinitionNode> {
    let mut operation = None;
    for definition in &document_ast.definitions {
        let DefinitionNode::OperationDefinition(definition) = definition else {
            continue;
        };
        let Some(operation_name) = operation_name else {
            // If no operation name was provided, only return an Operation if there
            // is one defined in the document. Upon encountering the second, return
            // null.
            if operation.is_some() {
                return None;
            }
            operation = Some(definition);
            continue;
        };
        let name = definition.name.as_ref().map(NameNode::value);
        if name == Some(operation_name) {
            return Some(definition);
        }
    }
    operation
}

/// Returns the type of the operation `getOperationAST` finds in `document`.
///
/// This is `getOperationAST(document, operationName)?.operation`, the one
/// reading every caller of the entry needs before it knows whether the
/// answer is a single result or a stream. `None` is the document that holds
/// no operation to run under that name.
#[must_use]
pub fn operation_type(
    document: &DocumentNode,
    operation_name: Option<&str>,
) -> Option<OperationTypeNode> {
    get_operation_ast(document, operation_name).map(|operation| operation.operation)
}

/// Runs the operation `args` names and returns the result object or a promise of it.
///
/// A subscription operation runs through [`subscribe`], whose answer is the error
/// result, the async iterable of results, or a promise of either. The success
/// handler of the reference flattens an incremental result, which is left out with
/// `@defer` and `@stream`, so it is the identity here. The `handleMaybePromise`
/// stays, because a promised result passes through one more reaction on its way
/// out.
///
/// # Errors
///
/// Returns the plain error `Must provide an operation.` when the document
/// holds no operation to run, and what [`execute`] and [`subscribe`] throw.
pub fn normalized_executor(agent: &Agent, args: ExecutionArgs) -> Result<MaybePromise, Value> {
    let operation_ast = get_operation_ast(&args.document, args.operation_name.as_deref());
    let Some(operation_ast) = operation_ast else {
        return Err(Value::from(ErrorObject::error(
            "Must provide an operation.",
        )));
    };
    if operation_ast.operation == OperationTypeNode::Subscription {
        return subscribe(agent, args);
    }
    handle_maybe_promise(
        agent,
        |agent| execute(agent, args).into(),
        Box::new(|_, result| Completion::Normal(result)),
        None,
    )
    .into_result()
}
