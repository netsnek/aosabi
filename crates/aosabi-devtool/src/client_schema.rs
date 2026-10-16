// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool client-schema: builds the client schema of an introspection
 * result and prints it, and writes the introspection query that pull posts.
 *
 * No analogue in the reference. The judging notebook is
 * tests/notebooks/02_language.ipynb, which sends the same request lines to
 * this primitive and to tests/sidecar/graphql-js.mjs.
 */

//! The `client-schema` primitive of the seam.
//!
//! Its one form is `--requests PATH`, the batch form of [`crate::batch`].
//!
//! | `op` | Request fields | Payload |
//! | --- | --- | --- |
//! | `client_schema` | `introspection`, optional `options` | `sdl`, or `error` |
//! | `introspection_query` | optional `options` | `query` |
//!
//! The two operations carry the names and the request fields of the two
//! operations of the graphql-js oracle, so one request line reaches both
//! sides.
//!
//! The field `introspection` of `client_schema` is the JSON text of the `data`
//! member of an execution result, `{"__schema":...}`, which is read by the
//! reader of `aosabi-json` and handed to [`build_client_schema()`]. Its
//! `options` carries `assumeValid`, which takes effect only when it is `true`,
//! as the constructor of the schema compares it. The payload `sdl` is the text
//! [`print_schema()`] writes for the schema, and `error` the message of what
//! either function throws. A text that is not JSON answers `fatal`, as the
//! oracle answers it.
//!
//! The `options` of `introspection_query` are the six flags of
//! [`IntrospectionOptions`] by their names in the reference. The payload
//! `query` is the text [`get_introspection_query()`] writes, the query `pull`
//! posts when no option is named.

use std::process::ExitCode;

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_type::{
    BuildClientSchemaOptions,
    DefinitionError,
    IntrospectionOptions,
    build_client_schema,
    get_introspection_query,
    print_schema, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    json::reader,
    seam::{
        self,
        BatchArgs, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "client-schema";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M8";

/// The operations of the batch form.
const OPERATIONS: &Operations = &[
    ("client_schema", op_client_schema),
    ("introspection_query", op_introspection_query),
];

/// Runs the `client-schema` primitive, whose one form is the batch form.
pub fn run(args: &BatchArgs) -> ExitCode {
    seam::run_batch(NAME, LANDS_AT, OPERATIONS, args)
}

/// Returns `printSchema(buildClientSchema(introspection, options))`.
///
/// # Errors
///
/// Returns what either function throws.
fn print(
    introspection: &JsValue,
    options: BuildClientSchemaOptions,
) -> Result<String, DefinitionError> {
    print_schema(&build_client_schema(introspection, options)?)
}

/// Answers the operation `client_schema`.
fn op_client_schema(request: &JsObject) -> Result<JsObject, JsString> {
    let text = batch::require_string(request, "introspection")?;
    let introspection = reader::parse(text.as_bytes()).map_err(|error| {
        JsString::from(format!(
            "client_schema: the introspection field is not JSON: {error}"
        ))
    })?;
    let options = BuildClientSchemaOptions {
        assume_valid: matches!(
            batch::options_of(request)?.and_then(|options| options.get("assumeValid")),
            Some(JsValue::Bool(true))
        ),
    };
    let mut payload = JsObject::with_capacity(1);
    match print(&introspection, options) {
        Ok(sdl) => payload.insert("sdl", sdl),
        Err(error) => payload.insert("error", error.to_string()),
    };
    Ok(payload)
}

/// Answers the operation `introspection_query`.
fn op_introspection_query(request: &JsObject) -> Result<JsObject, JsString> {
    let mut options = IntrospectionOptions::default();
    if let Some(named) = batch::options_of(request)? {
        let flags: [(&str, &mut bool); 6] = [
            ("descriptions", &mut options.descriptions),
            ("specifiedByUrl", &mut options.specified_by_url),
            (
                "directiveIsRepeatable",
                &mut options.directive_is_repeatable,
            ),
            ("schemaDescription", &mut options.schema_description),
            (
                "inputValueDeprecation",
                &mut options.input_value_deprecation,
            ),
            ("oneOf", &mut options.one_of),
        ];
        for (name, flag) in flags {
            if let Some(value) = named.get(name) {
                *flag = value.truthy();
            }
        }
    }
    let mut payload = JsObject::with_capacity(1);
    payload.insert("query", get_introspection_query(options));
    Ok(payload)
}
