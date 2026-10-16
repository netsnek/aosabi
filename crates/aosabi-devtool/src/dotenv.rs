// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool dotenv: parses the text of a .env file the way the command
 * line reads it.
 *
 * No analogue in the reference. The judging notebook is
 * tests/notebooks/70_scaffold.ipynb, which sends the same request lines to
 * this primitive and to the operation dotenv_parse of the command line
 * sidecar.
 */

//! The `dotenv` primitive of the seam.
//!
//! Its one form is `--requests PATH`, the batch form of [`crate::batch`].
//!
//! | `op` | Request fields | Payload |
//! | --- | --- | --- |
//! | `dotenv_parse` | `text` | `pairs` |
//!
//! The operation carries the name, the request field and the payload of the
//! operation `dotenv_parse` of the command line oracle, so one request
//! line reaches both sides and the two answers compare as bytes. The payload
//! `pairs` holds `[key, value]` in the order of the keys of the object
//! `dotenv.parse(text)` returns, the first place of a key with the value of
//! its last line, which is the order [`aosabi_cli::dotenv::parse`] answers.
//!
//! The seam names the command line on a Unix host alone, the hosts of the
//! runtime the plan gives the command line, so this primitive exists on such
//! a host alone, and the seam still builds for `wasm32-unknown-unknown`.

use std::process::ExitCode;

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
    seam::{
        self,
        BatchArgs, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "dotenv";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M8";

/// The operations of the batch form.
const OPERATIONS: &Operations = &[("dotenv_parse", op_dotenv_parse)];

/// Runs the `dotenv` primitive, whose one form is the batch form.
pub fn run(args: &BatchArgs) -> ExitCode {
    seam::run_batch(NAME, LANDS_AT, OPERATIONS, args)
}

/// Answers the operation `dotenv_parse`.
fn op_dotenv_parse(request: &JsObject) -> Result<JsObject, JsString> {
    let text = batch::require_string(request, "text")?;
    let pairs: Vec<JsValue> = aosabi_cli::dotenv::parse(text)
        .into_iter()
        .map(|(key, value)| JsValue::Array(vec![JsValue::from(key), JsValue::from(value)]))
        .collect();
    let mut payload = JsObject::with_capacity(1);
    payload.insert("pairs", pairs);
    Ok(payload)
}
