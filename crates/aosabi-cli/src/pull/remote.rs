// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the fetch of the introspection of a remote schema.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/pull/index.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The remote schema.
//!
//! [`fetch_schema`] posts `JSON.stringify({query: getIntrospectionQuery()})`
//! as `application/json` through the `http://` only client of
//! [`http`], which follows no redirect and bounds the connection at 5 seconds,
//! the answer at 30 seconds, its head at 64 KiB and its body at 16 MiB, and
//! refuses `https://` with `AO-C0501`. It reads the body with the reader of
//! `aosabi-json`, refuses an answer that is no JSON, an answer that carries
//! `errors` and an answer without `data`, as the plan asks, and builds the
//! client schema with [`build_client_schema()`], whose type map keeps the order
//! of `__schema.types`, which the registry walks. The scheme `https`, the
//! redirects of 307 and 308, `errors` beside a `data`, the bounds and the
//! codings are the divergences that `NOT_PORTED` of `aosabi-compat` names.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_type::{
    BuildClientSchemaOptions,
    IntrospectionOptions,
    Schema,
    build_client_schema,
    get_introspection_query, //
};

use crate::{
    cargo,
    http,
    report::CliError, //
};

/// The content type of the introspection post.
const CONTENT_TYPE: &str = "application/json";

/// Posts the introspection query to `url` and answers the client schema of
/// the answer.
///
/// # Errors
///
/// Returns `AO-C0501` for a URL that is no `http://` URL, the failure of the
/// client for a connection or an answer that breaks HTTP/1.1 or a bound, and
/// a failure for an answer that is no JSON, carries `errors`, carries no
/// `data`, or holds no introspection `build_client_schema` reads.
pub fn fetch_schema(url: &str) -> Result<Schema, CliError> {
    let mut request = JsObject::with_capacity(1);
    request.insert(
        "query",
        get_introspection_query(IntrospectionOptions::default()),
    );
    let body = aosabi_json::to_string(&JsValue::Object(request)).unwrap_or_default();
    let response = http::post(url, CONTENT_TYPE, body.as_bytes())?;
    schema_of_answer(&response.body, response.status)
}

/// Answers the client schema of `body`, the answer with the status `status`
/// to the introspection query.
///
/// The devtool primitive `pull` reads the answer the operation `pull_types`
/// of the oracle hands the reference through this function, so the two
/// sides read one text with no server.
///
/// # Errors
///
/// Returns a failure for an answer that is no JSON, carries `errors`, carries
/// no `data`, or holds no introspection `build_client_schema` reads.
pub fn schema_of_answer(body: &[u8], status: u16) -> Result<Schema, CliError> {
    let answer = aosabi_json::parse(body).map_err(|error| {
        CliError::new(format!(
            "the answer with status {status} is no JSON: {error}"
        ))
    })?;
    if let Some(errors) = cargo::field(&answer, "errors").filter(|errors| !errors.is_undefined()) {
        return Err(CliError::new(format!(
            "the answer carries errors: {}",
            messages(errors)
        )));
    }
    let data = cargo::field(&answer, "data")
        .ok_or_else(|| CliError::new(format!("the answer with status {status} carries no data")))?;
    build_client_schema(data, BuildClientSchemaOptions::default())
        .map_err(|error| CliError::new(error.to_string()))
}

/// Returns the messages of the GraphQL errors `errors` joined by `, `, or the
/// errors as JSON when they carry none.
fn messages(errors: &JsValue) -> String {
    let texts: Vec<&str> = match errors {
        JsValue::Array(items) => items
            .iter()
            .filter_map(|error| cargo::text(error, "message"))
            .collect(),
        _ => Vec::new(),
    };
    if texts.is_empty() {
        aosabi_json::to_string(errors).unwrap_or_default()
    } else {
        texts.join(", ")
    }
}
