// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the plugin that logs the errors of an execution.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/plugins/use-graphql-error-logger.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `use_graphql_error_logger`.
//!
//! The plugin logs every error of a result when the execution is done,
//! through `getLogger().withTag('graphql')`, so the record is correlated with
//! the request. An original `GraphQLError`, a client error that the response
//! shows, is logged at `debug` as `graphql client error` with
//! `{message, path, code}`, and any other error at `error` as `graphql error`
//! with `{err, path, code}`, where `err` is the original error, or the error
//! itself, normalized, `path` is joined by `.` and `code` is
//! `extensions.code`. A field that is `undefined` is left out of a JSON line
//! and written as `undefined` in a pretty one, as the reference writes it.

use std::rc::Rc;

use aosabi_execute::{
    errors::ThrownError,
    value::Value, //
};
use aosabi_hooks::{
    plugin::OnExecuteHooks,
    plugins::use_masked_errors::is_original,
    utils::handle_stream_or_single_execution_result, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    OriginalError,
    PathSegment, //
};

use crate::{
    logger::{
        get_logger,
        normalize_error, //
    },
    plugin::{
        Plugin,
        PluginObject,
        object::__wrap, //
    },
};

/// Returns the plugin that logs the errors of an execution, `useGraphqlErrorLogger()`.
///
/// A plugin with no member of its own and one hook, `onExecute`, whose done
/// hook logs, so its members are `onExecute` alone.
#[must_use]
pub fn use_graphql_error_logger() -> Rc<dyn Plugin> {
    Rc::new(PluginObject {
        on_execute: __wrap::on_execute(|_p| {
            Ok(Some(OnExecuteHooks {
                on_execute_done: Some(Box::new(|payload| {
                    handle_stream_or_single_execution_result(payload, |result| {
                        log_errors(&result.errors);
                    })
                })),
            }))
        }),
        ..PluginObject::default()
    })
}

/// Logs each of `errors`, the body of the done hook.
fn log_errors(errors: &[GraphQLError]) {
    if errors.is_empty() {
        return;
    }
    log_each(errors);
}

/// Logs each of `errors`, which hold one at least.
// Out of line, so a result with no error compiles none of the lines.
#[inline(never)]
fn log_each(errors: &[GraphQLError]) {
    let log = get_logger().with_tag("graphql");
    for err in errors {
        let path = err.path.as_ref().map_or(JsValue::Undefined, |path| {
            JsValue::from(
                path.iter()
                    .map(|segment| match segment {
                        PathSegment::Key(key) => key.clone(),
                        PathSegment::Index(index) => index.to_string(),
                    })
                    .collect::<Vec<String>>()
                    .join("."),
            )
        });
        let code = err
            .extensions
            .get("code")
            .cloned()
            .unwrap_or(JsValue::Undefined);
        let mut fields = JsObject::new();
        if is_original(err) {
            fields.insert("message", JsValue::String(err.message.clone()));
            fields.insert("path", path);
            fields.insert("code", code);
            log.debug("graphql client error", fields);
        } else {
            fields.insert("err", original_of(err));
            fields.insert("path", path);
            fields.insert("code", code);
            log.error("graphql error", fields);
        }
    }
}

/// Returns `err.originalError ?? err` normalized as a log record carries it.
fn original_of(err: &GraphQLError) -> JsValue {
    let error = match &err.original_error {
        Some(OriginalError::GraphQL(original)) => aosabi_execute::value::ErrorObject::GraphQL(
            aosabi_execute::errors::copy_graphql_error(original),
        ),
        Some(OriginalError::Other(original)) => {
            let (name, message) = original.downcast_ref::<ThrownError>().map_or_else(
                || {
                    (
                        aosabi_json::JsString::from("Error"),
                        aosabi_json::JsString::from(original.to_string()),
                    )
                },
                |thrown| (thrown.name.clone(), thrown.message.clone()),
            );
            aosabi_execute::value::ErrorObject::Plain {
                name: aosabi_execute::value::ErrorName::Other(name),
                message,
            }
        }
        None => aosabi_execute::value::ErrorObject::GraphQL(
            aosabi_execute::errors::copy_graphql_error(err),
        ),
    };
    normalize_error(&Value::from(error))
}
