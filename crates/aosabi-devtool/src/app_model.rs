// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool app-model: forms the canonical application model of a raw
 * model and writes it the way aosabi inspect --json prints it.
 *
 * No analogue in the reference. The judging notebook is
 * tests/notebooks/70_cli.ipynb, which hands this primitive the raw model the
 * operation inspect_raw of the IR sidecar answers.
 */

//! The `app-model` primitive of the seam.
//!
//! Its one form is `--requests PATH`, the batch form of [`crate::batch`].
//!
//! | `op` | Request fields | Payload |
//! | --- | --- | --- |
//! | `app_model` | `ir`, optional `authz` and `queues` | `json`, or `error` |
//!
//! The three fields are the members of the raw model `{ir, authz, queues}`
//! that `inspectApp` assembles before it prunes and sorts, each carried as
//! its JSON text, so the bytes of the text reach the reader unchanged, as the
//! field `ir` of [`crate::print_sdl`] does. The field `ir` is the
//! intermediate representation the schema builder answers. The field `authz`
//! is the list of rows `{model, table, app?, tenant?, secure}` and `queues`
//! the list of queues `{name, attempts?, concurrency?, hasSchema}`, each in
//! the order the harvest gives, and an absent field reads as the empty list
//! the inspect runner of Aosabi writes until the harvests land. A member that
//! is absent or `null` where the row names it optional is absent.
//!
//! The payload `json` is the text `aosabi inspect --json` prints for the
//! model, `JSON.stringify(model, null, 2)` and one newline, and `error` is
//! the message of what [`app_model()`] refuses. A field that is not JSON or
//! does not hold its shape answers `fatal`.

use std::process::ExitCode;

use aosabi_ir::{
    SchemaIr,
    app_model::{
        AuthzInfo,
        QueueInfo,
        app_model,
        app_model_to_js, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
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
const NAME: &str = "app-model";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M8";

/// The gap of `aosabi inspect --json`, two spaces.
const GAP: &str = "  ";

/// The operations of the batch form.
const OPERATIONS: &Operations = &[("app_model", op_app_model)];

/// Runs the `app-model` primitive, whose one form is the batch form.
pub fn run(args: &BatchArgs) -> ExitCode {
    seam::run_batch(NAME, LANDS_AT, OPERATIONS, args)
}

/// Answers the operation `app_model`.
fn op_app_model(request: &JsObject) -> Result<JsObject, JsString> {
    let ir = ir_of(request)?;
    let authz = rows_of(request, "authz", authz_at)?;
    let queues = rows_of(request, "queues", queue_at)?;
    let mut payload = JsObject::with_capacity(1);
    match app_model(ir, authz, queues) {
        Ok(model) => {
            // The model is an object, which always has a JSON form.
            let mut text =
                aosabi_json::to_string_with_gap(&app_model_to_js(&model), GAP).unwrap_or_default();
            text.push('\n');
            payload.insert("json", text);
        }
        Err(error) => {
            payload.insert("error", error.to_string());
        }
    }
    Ok(payload)
}

/// Returns the value the JSON text of the field `name` of `request` holds.
///
/// # Errors
///
/// Returns the `fatal` of a field that is no string or is not JSON.
fn json_field(request: &JsObject, name: &str) -> Result<JsValue, String> {
    let text = batch::require_string(request, name)?;
    reader::parse(text.as_bytes())
        .map_err(|error| format!("app_model: the field {name} is not JSON: {error}"))
}

/// Returns the intermediate representation that travels as `ir`.
///
/// # Errors
///
/// Returns the `fatal` of a field that is missing, is no string, is not JSON
/// or is not an intermediate representation.
fn ir_of(request: &JsObject) -> Result<SchemaIr, String> {
    let value = json_field(request, "ir")?;
    aosabi_ir::from_js_value(&value)
        .map_err(|error| format!("app_model: the field ir is not an IR: {error}"))
}

/// Returns the rows the field `name` of `request` lists, each read by `row`,
/// or no row when the request carries no such field.
///
/// # Errors
///
/// Returns the `fatal` of a field that is no JSON text of a list, and of a
/// row `row` refuses, with the index of the row.
fn rows_of<T>(
    request: &JsObject,
    name: &str,
    row: fn(&JsObject) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    if !request.contains_key(name) {
        return Ok(Vec::new());
    }
    let JsValue::Array(items) = json_field(request, name)? else {
        return Err(format!("app_model: the field {name} is not a list"));
    };
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            match item {
                JsValue::Object(object) => row(object),
                _ => Err(String::from("is not an object")),
            }
            .map_err(|reason| format!("app_model: {name}[{index}] {reason}"))
        })
        .collect()
}

/// Reads one row of `authz`, the members the project runner writes.
///
/// # Errors
///
/// Returns the reason a member does not hold its type.
fn authz_at(row: &JsObject) -> Result<AuthzInfo, String> {
    Ok(AuthzInfo {
        model: string_at(row, "model")?,
        table: string_at(row, "table")?,
        app: optional_string_at(row, "app")?,
        tenant: optional_string_at(row, "tenant")?,
        secure: bool_at(row, "secure")?,
    })
}

/// Reads one queue of `queues`, the members `describe` writes.
///
/// # Errors
///
/// Returns the reason a member does not hold its type.
fn queue_at(row: &JsObject) -> Result<QueueInfo, String> {
    Ok(QueueInfo {
        name: string_at(row, "name")?,
        attempts: optional_number_at(row, "attempts")?,
        concurrency: optional_number_at(row, "concurrency")?,
        has_schema: bool_at(row, "hasSchema")?,
    })
}

/// Returns the member `key` of `row`, absent for a missing, undefined or
/// `null` member.
fn present<'a>(row: &'a JsObject, key: &str) -> Option<&'a JsValue> {
    row.get(key)
        .filter(|value| !matches!(value, JsValue::Undefined | JsValue::Null))
}

/// Returns the string member `key` of `row`.
///
/// # Errors
///
/// Returns the reason when the member is missing, is no string or holds a
/// lone surrogate.
fn string_at(row: &JsObject, key: &str) -> Result<String, String> {
    optional_string_at(row, key)?.ok_or_else(|| format!("names no string {key}"))
}

/// Returns the string member `key` of `row`, or `None` when it is absent.
///
/// # Errors
///
/// Returns the reason when the member is no string or holds a lone
/// surrogate.
fn optional_string_at(row: &JsObject, key: &str) -> Result<Option<String>, String> {
    match present(row, key) {
        None => Ok(None),
        Some(JsValue::String(text)) => text
            .as_str()
            .map(|text| Some(text.to_owned()))
            .ok_or_else(|| format!("carries a lone surrogate in {key}")),
        Some(_) => Err(format!("names {key} with no string")),
    }
}

/// Returns the number member `key` of `row`, or `None` when it is absent.
///
/// # Errors
///
/// Returns the reason when the member is no number.
fn optional_number_at(row: &JsObject, key: &str) -> Result<Option<f64>, String> {
    match present(row, key) {
        None => Ok(None),
        Some(JsValue::Number(number)) => Ok(Some(*number)),
        Some(_) => Err(format!("names {key} with no number")),
    }
}

/// Returns the boolean member `key` of `row`.
///
/// # Errors
///
/// Returns the reason when the member is missing or is no boolean.
fn bool_at(row: &JsObject, key: &str) -> Result<bool, String> {
    match row.get(key) {
        Some(JsValue::Bool(value)) => Ok(*value),
        _ => Err(format!("names no boolean {key}")),
    }
}
