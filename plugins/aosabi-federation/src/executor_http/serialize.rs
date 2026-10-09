// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the body of an outbound request, its four members and
 * its printed query.
 *
 * Derived from @graphql-tools/executor-common 1.0.6, dist/index.cjs, MIT.
 * Derived from envelop 5.5.1, cjs/document-string-map.js, MIT.
 */

//! The serialization of a request, `serialize_execution_request`.
//!
//! The body the HTTP executor sends is `JSON.stringify` of four members,
//! `query`, `variables`, `operationName` and `extensions`, in that order and
//! each left out when absent, with `variables` and `extensions` present only
//! when they have a key and `operationName` only when it is no empty string.
//! The query is `stripIgnoredCharacters(print(document))`, the printed
//! document with every ignored token removed. The body is written through
//! the one writer of `aosabi-json`.
//!
//! `getDocumentString` keeps the printed text of a document in a `WeakMap`
//! keyed by the document, which Yoga also fills with the source of a parsed
//! client document. A document a delegation sends is one the finalization
//! built, which nothing put in that map, so the text is always printed, and
//! the map, which keeps a pure answer per document, has no reading.

use aosabi_execute::value::{
    ErrorObject,
    Value, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    ast::DocumentNode,
    print,
    strip_ignored_characters::strip_ignored_characters, //
};

use crate::transforms::{
    ExecutionRequest,
    Variables, //
};

/// The body of a request before it is written, the object `serializeExecutionRequest` answers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SerializedExecutionRequest {
    /// The printed query, `query`, absent when the query is excluded.
    pub query: Option<String>,
    /// The variables, `variables`, absent when there is none.
    pub variables: Option<JsObject>,
    /// The name of the operation, `operationName`, absent when there is none.
    pub operation_name: Option<String>,
    /// The extensions, `extensions`, absent when there is none.
    pub extensions: Option<JsObject>,
}

impl SerializedExecutionRequest {
    /// Returns the object `JSON.stringify` writes, the four members in their order with the absent ones left out.
    #[must_use]
    pub fn to_js_value(&self) -> JsValue {
        let mut body = JsObject::with_capacity(4);
        if let Some(query) = &self.query {
            body.insert("query", query.as_str());
        }
        if let Some(variables) = &self.variables {
            body.insert("variables", variables.clone());
        }
        if let Some(operation_name) = &self.operation_name {
            body.insert("operationName", operation_name.as_str());
        }
        if let Some(extensions) = &self.extensions {
            body.insert("extensions", extensions.clone());
        }
        JsValue::Object(body)
    }

    /// Returns `JSON.stringify` of the body, the text the HTTP executor sends.
    ///
    /// The writer answers `None` for `undefined` alone, and the body is an
    /// object, so the default of the empty text is never taken.
    #[must_use]
    pub fn to_json(&self) -> String {
        aosabi_json::to_string(&self.to_js_value()).unwrap_or_default()
    }
}

/// A function that prints a document as the query a body carries, `printFn`.
pub type PrintFn = dyn Fn(&DocumentNode) -> Result<String, Value>;

/// Returns the body of `execution_request`, its query printed by `print_fn` unless `exclude_query` holds.
///
/// The printer is [`default_print_fn`] when `print_fn` is `None`.
///
/// # Errors
///
/// Returns what the printer throws.
pub fn serialize_execution_request(
    execution_request: &ExecutionRequest,
    exclude_query: bool,
    print_fn: Option<&PrintFn>,
) -> Result<SerializedExecutionRequest, Value> {
    let query = if exclude_query {
        None
    } else {
        Some(match print_fn {
            Some(print_fn) => print_fn(&execution_request.document)?,
            None => default_print_fn(&execution_request.document)?,
        })
    };
    Ok(SerializedExecutionRequest {
        query,
        variables: execution_request
            .variables
            .as_ref()
            .filter(|variables| !variables.is_empty())
            .map(Variables::to_object),
        operation_name: execution_request
            .operation_name
            .clone()
            .filter(|operation_name| !operation_name.is_empty()),
        extensions: execution_request
            .extensions
            .clone()
            .filter(|extensions| !extensions.is_empty()),
    })
}

/// Returns the query a body carries for `document`, its printed text with every ignored token removed.
///
/// # Errors
///
/// Returns the syntax error of the lexer for a printed text that does not
/// lex, which a printed document never is.
pub fn default_print_fn(document: &DocumentNode) -> Result<String, Value> {
    strip_ignored_characters(print(document))
        .map_err(|error| Value::from(ErrorObject::GraphQL(error)))
}
