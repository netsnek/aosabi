// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the request checks of the function.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/request-validation, MIT.
 */

//! The request checks.
//!
//! [`answer`](crate::answer::answer) runs three of them as straight code:
//!
//! - [`method_refusal`] refuses every method but `GET` and `POST`, `HEAD` among
//!   them, with `GraphQL only supports GET and POST requests.`, status 405,
//!   `Allow: GET, POST` and code `BAD_REQUEST`, after the hooks `onRequestParse`
//!   of the app.
//! - [`check_graphql_query_params`] refuses parameters that are no object, a
//!   parameter that is neither null nor one of `query`, `variables`,
//!   `operationName` and `extensions` nor an extra name, a missing `query`, a
//!   `query` that is no string, and `variables` or `extensions` that are neither
//!   absent, null nor an object, each with its message and `Allow: GET, POST` on
//!   all but the unknown parameter, after the hooks `onParams` of the app. The
//!   parameters are walked in the order `for in` walks a parsed object, the array
//!   indices first.
//! - [`single_operation`] refuses every batch with `Batching is not supported.`,
//!   status 400 and code `BAD_REQUEST`, after the done hooks of the request parse
//!   of the app, which is Yoga's refusal with batching off. The function holds no
//!   batching, the named divergence D58 of `docs/byte-equality.md` section 3, so
//!   the option `batching` that raises the limit has no reading.
//!
//! Two stand in the hook layer, at the places of their plugins in Yoga's
//! list, since they act on the parse and the validation that the hook layer
//! runs:
//!
//! - `MutationCheck` stamps a parse error with the code `GRAPHQL_PARSE_FAILED`,
//!   `http.spec` and status 400, and refuses a document with no operation to run
//!   with `Could not determine what operation to execute.`,
//!   `OPERATION_RESOLUTION_FAILURE` and 400, and a mutation over GET with
//!   `Can only perform a mutation operation from a POST request.`, 405 and
//!   `Allow: POST`. The throw of its done hook ends the parse, which the done
//!   hook answers by replacing the result with the error, because no done hook of
//!   the parse follows it.
//! - `ValidationStamp` gives every error of a validation that failed the code
//!   `GRAPHQL_VALIDATION_FAILED` and the `http` extension with `spec: true` and
//!   status 400, each only where absent.

use aosabi_execute::{
    ErrorObject,
    Value,
    errors::{
        copy_graphql_error,
        create_graphql_error, //
    },
    normalized_executor::get_operation_ast, //
};
use aosabi_hooks::{
    HookPlugin,
    plugin::{
        OnParse,
        OnParseDoneHook,
        OnValidate,
        OnValidateDoneHook, //
    }, //
};
use aosabi_http::Request;
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        DocumentNode,
        OperationTypeNode, //
    },
    js_string::to_string_lossy, //
};

use crate::{
    plugin::{
        GraphQLParams,
        OperationName,
        RequestParserResult, //
    },
    utils::mask_error::truthy, //
};

/// The names of the parameters of an operation, `expectedParameters`.
const EXPECTED_PARAMETERS: [&str; 4] = ["query", "variables", "operationName", "extensions"];

/// Returns the `GraphQLError` of `message` with `extensions` as the thrown value.
fn refusal(message: impl Into<aosabi_json::JsString>, extensions: JsObject) -> Value {
    Value::from(ErrorObject::GraphQL(create_graphql_error(
        message,
        GraphQLErrorOptions {
            extensions: Some(extensions),
            ..GraphQLErrorOptions::default()
        },
    )))
}

/// Returns whether `method` is one GraphQL answers, `isValidMethodForGraphQL(method)`.
#[must_use]
pub fn is_valid_method_for_graphql(method: &str) -> bool {
    method == "GET" || method == "POST"
}

/// Returns the refusal of a method GraphQL does not answer, the throw of `useCheckMethodForGraphQL`.
#[must_use]
pub fn method_refusal() -> Value {
    let mut headers = JsObject::new();
    headers.insert("Allow", JsValue::from("GET, POST"));
    let mut http = JsObject::new();
    http.insert("status", JsValue::from(405_u32));
    http.insert("headers", JsValue::Object(headers));
    let mut extensions = JsObject::new();
    extensions.insert("http", JsValue::Object(http));
    extensions.insert("code", JsValue::from("BAD_REQUEST"));
    refusal("GraphQL only supports GET and POST requests.", extensions)
}

/// Returns `typeof` with `null` and `array` told apart, `extendedTypeof(val)`.
#[must_use]
pub fn extended_typeof(value: Option<&JsValue>) -> &'static str {
    match value {
        None | Some(JsValue::Undefined) => "undefined",
        Some(JsValue::Null) => "null",
        Some(JsValue::Bool(_)) => "boolean",
        Some(JsValue::Number(_)) => "number",
        Some(JsValue::String(_)) => "string",
        Some(JsValue::Array(_)) => "array",
        Some(JsValue::Object(_) | JsValue::Opaque(_)) => "object",
    }
}

/// Returns the refusal `message` of the parameters with the status 400, with
/// `spec` when it holds and `Allow: GET, POST` when `allow` holds.
fn params_refusal(message: String, spec: bool, allow: bool) -> Value {
    let mut http = JsObject::new();
    if spec {
        http.insert("spec", JsValue::Bool(true));
    }
    http.insert("status", JsValue::from(400_u32));
    if allow {
        let mut headers = JsObject::new();
        headers.insert("Allow", JsValue::from("GET, POST"));
        http.insert("headers", JsValue::Object(headers));
    }
    let mut extensions = JsObject::new();
    extensions.insert("http", JsValue::Object(http));
    extensions.insert("code", JsValue::from("BAD_REQUEST"));
    refusal(message, extensions)
}

/// Refuses a parameter the check does not know, `assertInvalidParams(params, extraParamNames)`.
///
/// # Errors
///
/// Returns the refusal of parameters that are no object and of an unknown parameter.
pub fn assert_invalid_params(
    params: &JsValue,
    extra_param_names: Option<&[String]>,
) -> Result<(), Value> {
    let JsValue::Object(object) = params else {
        let message = String::from("Invalid \"params\" in the request body");
        return Err(params_refusal(message, true, false));
    };
    for (key, value) in object.own_keys() {
        if matches!(value, JsValue::Undefined | JsValue::Null) {
            continue;
        }
        let key = to_string_lossy(key);
        if !EXPECTED_PARAMETERS.contains(&key.as_ref()) {
            if extra_param_names.is_some_and(|names| names.iter().any(|name| *name == key)) {
                continue;
            }
            let mut http = JsObject::new();
            http.insert("status", JsValue::from(400_u32));
            let mut extensions = JsObject::new();
            extensions.insert("http", JsValue::Object(http));
            extensions.insert("code", JsValue::from("BAD_REQUEST"));
            return Err(refusal(
                format!("Unexpected parameter \"{key}\" in the request body."),
                extensions,
            ));
        }
    }
    Ok(())
}

/// Checks the parameters of an operation, `checkGraphQLQueryParams(params, extraParamNames)`.
///
/// # Errors
///
/// Returns the refusal of the first check that fails.
pub fn check_graphql_query_params(
    params: &GraphQLParams,
    extra_param_names: Option<&[String]>,
) -> Result<(), Value> {
    let value = &params.value;
    if !matches!(value, JsValue::Object(_)) {
        let given = extended_typeof(Some(value));
        return Err(params_refusal(
            format!("Expected params to be an object but given {given}."),
            false,
            true,
        ));
    }
    assert_invalid_params(value, extra_param_names)?;
    if matches!(
        params.get("query"),
        None | Some(JsValue::Undefined | JsValue::Null)
    ) {
        return Err(params_refusal(
            String::from("Must provide query string."),
            true,
            true,
        ));
    }
    let query_type = extended_typeof(params.get("query"));
    if query_type != "string" {
        return Err(params_refusal(
            format!("Expected \"query\" param to be a string, but given {query_type}."),
            false,
            true,
        ));
    }
    let variables_type = extended_typeof(params.get("variables"));
    if !["object", "null", "undefined"].contains(&variables_type) {
        return Err(params_refusal(
            format!(
                "Expected \"variables\" param to be empty or an object, but given {variables_type}."
            ),
            false,
            true,
        ));
    }
    let extensions_type = extended_typeof(params.get("extensions"));
    if !["object", "null", "undefined"].contains(&extensions_type) {
        return Err(params_refusal(
            format!(
                "Expected \"extensions\" param to be empty or an object, but given {extensions_type}."
            ),
            false,
            true,
        ));
    }
    Ok(())
}

/// Returns the parameters of the one operation of `result`, and refuses a batch, the done hook of `useLimitBatching(0)`.
///
/// # Errors
///
/// Returns the refusal of the batch.
pub fn single_operation(result: RequestParserResult) -> Result<GraphQLParams, Value> {
    match result {
        RequestParserResult::Single(params) => Ok(params),
        RequestParserResult::Batch(_) => {
            let mut http = JsObject::new();
            http.insert("status", JsValue::from(400_u32));
            let mut extensions = JsObject::new();
            extensions.insert("http", JsValue::Object(http));
            extensions.insert("code", JsValue::from("BAD_REQUEST"));
            Err(refusal("Batching is not supported.", extensions))
        }
    }
}

/// Refuses a document that runs no operation or a mutation over GET, `assertMutationViaGet(method, document, operationName)`.
///
/// # Errors
///
/// Returns the refusal.
pub fn assert_mutation_via_get(
    method: &str,
    document: Option<&DocumentNode>,
    operation_name: OperationName<'_>,
) -> Result<(), Value> {
    let operation = document.and_then(|document| match operation_name {
        OperationName::Absent => get_operation_ast(document, None),
        OperationName::Name(name) => get_operation_ast(document, Some(name)),
        OperationName::Unmatchable => None,
    });
    let Some(operation) = operation else {
        let mut http = JsObject::new();
        http.insert("status", JsValue::from(400_u32));
        let mut extensions = JsObject::new();
        extensions.insert("code", JsValue::from("OPERATION_RESOLUTION_FAILURE"));
        extensions.insert("http", JsValue::Object(http));
        return Err(refusal(
            "Could not determine what operation to execute.",
            extensions,
        ));
    };
    if operation.operation == OperationTypeNode::Mutation && method == "GET" {
        let mut headers = JsObject::new();
        headers.insert("Allow", JsValue::from("POST"));
        let mut http = JsObject::new();
        http.insert("status", JsValue::from(405_u32));
        http.insert("headers", JsValue::Object(headers));
        let mut extensions = JsObject::new();
        extensions.insert("http", JsValue::Object(http));
        extensions.insert("code", JsValue::from("BAD_REQUEST"));
        return Err(refusal(
            "Can only perform a mutation operation from a POST request.",
            extensions,
        ));
    }
    Ok(())
}

/// Stamps `error` with the code `code` and the `http` extension of status 400, `||=` on each member.
///
/// The check of a mutation over GET stamps a parse error the same way with its
/// own code, except that it fills `spec` with `||=` where the stamp of a
/// validation error keeps a `spec` that is not null.
pub fn stamp_http_error(error: &mut GraphQLError, code: &str, keep_false_spec: bool) {
    let extensions = &mut error.extensions;
    if !extensions.get("code").is_some_and(truthy) {
        extensions.insert("code", JsValue::from(code));
    }
    if !extensions.get("http").is_some_and(truthy) {
        extensions.insert("http", JsValue::Object(JsObject::new()));
    }
    if let Some(JsValue::Object(http)) = extensions.get_mut("http") {
        let keep = match http.get("spec") {
            Some(JsValue::Undefined | JsValue::Null) | None => false,
            Some(spec) => keep_false_spec || truthy(spec),
        };
        if !keep {
            http.insert("spec", JsValue::Bool(true));
        }
        if !http.get("status").is_some_and(truthy) {
            http.insert("status", JsValue::from(400_u32));
        }
    }
}

/// The stamp of a parse error and the refusal of a mutation over GET, the hooks of `usePreventMutationViaGET()`.
pub(crate) struct MutationCheck;

impl HookPlugin for MutationCheck {
    fn on_parse(&self, _p: &mut OnParse<'_>) -> Result<Option<OnParseDoneHook>, Value> {
        Ok(Some(Box::new(|done| {
            // Run only if this is a request over HTTP.
            let Some(request) = done.context.get_as::<Request>("request") else {
                return;
            };
            let params = done.context.get_as::<GraphQLParams>("params");
            let stamped = match &*done.result {
                Err(Value::Error(error)) => match error.as_ref() {
                    ErrorObject::GraphQL(error) => {
                        let mut error = copy_graphql_error(error);
                        stamp_http_error(&mut error, "GRAPHQL_PARSE_FAILED", false);
                        Some(Err(Value::from(ErrorObject::GraphQL(error))))
                    }
                    _ => None,
                },
                Err(_) => None,
                Ok(document) => {
                    let operation_name = params
                        .as_deref()
                        .map_or(OperationName::Absent, GraphQLParams::operation_name);
                    assert_mutation_via_get(&request.method, Some(document), operation_name)
                        .err()
                        .map(Err)
                }
            };
            if let Some(result) = stamped {
                done.replace_parse_result(result);
            }
        })))
    }
}

/// The stamp of a validation error, the hook of `useHTTPValidationError()`.
pub(crate) struct ValidationStamp;

impl HookPlugin for ValidationStamp {
    fn on_validate(&self, _p: &mut OnValidate<'_>) -> Option<OnValidateDoneHook> {
        Some(Box::new(|done| {
            if !done.valid {
                for error in done.result.iter_mut() {
                    stamp_http_error(error, "GRAPHQL_VALIDATION_FAILED", true);
                }
            }
        }))
    }
}
