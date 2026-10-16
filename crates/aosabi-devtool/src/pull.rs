// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool pull: writes the Rust registry aosabi pull writes for the
 * answer of a remote to the introspection query.
 *
 * No analogue in the reference. The judging notebook is
 * tests/notebooks/70_pull.ipynb, which sends the same request lines to this
 * primitive and to the operation pull_types of the command line sidecar
 * and compares the two registries through the parsers of the harness.
 */

//! The `pull` primitive of the seam.
//!
//! Its one form is `--requests PATH`, the batch form of [`crate::batch`].
//!
//! | `op` | Request fields | Payload |
//! | --- | --- | --- |
//! | `pull` | `introspection`, optional `name` | `registry`, or `error` |
//!
//! The field `introspection` is the JSON text of the whole answer a server
//! gives to the introspection query, `{"data":{"__schema":...}}`, the field
//! the operation `pull_types` of the command line oracle hands the
//! reference through a stub of `fetch` with the status 200, so one text
//! reaches both sides with no server. The primitive reads it as an answer
//! with the status 200 through [`schema_of_answer`], the reading of the
//! command, and writes the registry with [`emit_registry`]. The field `name`
//! is the name `-n` gives the command, which the file does not carry, so a
//! name is read only for the check the command makes before it asks the
//! remote.
//!
//! The payload `registry` is the text of `<name>.rs`, and `error` the text
//! the command prints behind its head `Failed to pull <name> remote schema:`
//! without `--verbose`, its last newline taken off, for a name the check
//! refuses, an answer the reading refuses or a schema the registry cannot
//! write.
//!
//! The seam names the command line on a Unix host alone, the hosts of the
//! runtime the plan gives the command line, so this primitive exists on such
//! a host alone, and the seam still builds for `wasm32-unknown-unknown`.

use std::process::ExitCode;

use aosabi_cli::{
    pull::{
        emit_registry,
        module_name,
        schema_of_answer, //
    },
    report::{
        self,
        CliError, //
    },
};
use aosabi_json::{
    JsObject,
    JsString, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    seam::{
        self,
        BatchArgs, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "pull";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M8";

/// The status of the answer the stub of the oracle gives.
const STATUS: u16 = 200;

/// The operations of the batch form.
const OPERATIONS: &Operations = &[("pull", op_pull)];

/// Runs the `pull` primitive, whose one form is the batch form.
pub fn run(args: &BatchArgs) -> ExitCode {
    seam::run_batch(NAME, LANDS_AT, OPERATIONS, args)
}

/// Returns the registry of the answer `introspection` under the name `name`.
///
/// # Errors
///
/// Returns what the check of the name, the reading of the answer or the
/// registry refuses, in that order, the order of the command.
fn registry(introspection: &str, name: Option<&str>) -> Result<String, CliError> {
    if let Some(name) = name {
        module_name(name)?;
    }
    let schema = schema_of_answer(introspection.as_bytes(), STATUS)?;
    emit_registry(&schema)
}

/// Answers the operation `pull`.
fn op_pull(request: &JsObject) -> Result<JsObject, JsString> {
    let introspection = batch::require_string(request, "introspection")?;
    let name = match request.get("name") {
        None => None,
        Some(_) => Some(batch::require_string(request, "name")?),
    };
    let mut payload = JsObject::with_capacity(1);
    match registry(introspection, name) {
        Ok(text) => {
            payload.insert("registry", text);
        }
        Err(error) => {
            let text = report::render(&error, false);
            let text = text.strip_prefix("error: ").unwrap_or(&text);
            payload.insert("error", text.strip_suffix('\n').unwrap_or(text));
        }
    }
    Ok(payload)
}
