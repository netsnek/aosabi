// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool case: maps text through the case conversions of aosabi-json
 * that keep the mappings to ASCII alone.
 *
 * No analogue in the reference. The crate under test is aosabi-json, whose
 * module case reduces String.prototype.toLowerCase and toUpperCase to the
 * characters whose mapping holds an ASCII character, and the judging notebook
 * is tests/notebooks/02_language.ipynb, section 9, which maps every Unicode
 * scalar value through node and through this primitive.
 */

//! The `case` primitive of the seam.
//!
//! `--requests PATH` is its one form. A request names the operation `lower`
//! or `upper` and carries `texts`, an array of strings, and the response
//! carries `mapped`, the array of their mappings in the same order, through
//! [`aosabi_json::case::lower_ascii_exact()`] or
//! [`aosabi_json::case::upper_ascii_exact()`]. A text that carries a lone
//! surrogate is refused with a `fatal`, since the functions under test take a
//! Rust string.

use std::{
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    case::{
        lower_ascii_exact,
        upper_ascii_exact, //
    }, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    cli::{
        self,
        Grammar,
        Matches, //
    },
    seam, //
};

/// The name of the primitive.
const NAME: &str = "case";

/// The plan whose notebook judges the primitive.
const LANDS_AT: &str = "SIZE1";

/// The operations of the batch form.
const OPERATIONS: &Operations = &[("lower", op_lower), ("upper", op_upper)];

/// The command line of the `case` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `case` primitive accepts.
#[derive(Debug)]
pub struct CaseArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl CaseArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `case` primitive.
pub fn run(args: &CaseArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|requests| batch::run(OPERATIONS, requests));
    seam::finish(NAME, outcome)
}

/// Answers the operation `lower`.
fn op_lower(request: &JsObject) -> Result<JsObject, JsString> {
    mapped(request, "lower", lower_ascii_exact)
}

/// Answers the operation `upper`.
fn op_upper(request: &JsObject) -> Result<JsObject, JsString> {
    mapped(request, "upper", upper_ascii_exact)
}

/// Returns the payload that carries every text of `request` mapped by `map`.
///
/// # Errors
///
/// Returns the `fatal` of a request of the operation `op` that carries no
/// array `texts`, or a text in it that is no string or carries a lone
/// surrogate.
fn mapped(request: &JsObject, op: &str, map: fn(&str) -> String) -> Result<JsObject, JsString> {
    let Some(JsValue::Array(texts)) = request.get("texts") else {
        return Err(JsString::from(format!(
            "{op}: the request carries no array field texts"
        )));
    };
    let mut out = Vec::with_capacity(texts.len());
    for (index, text) in texts.iter().enumerate() {
        let JsValue::String(text) = text else {
            return Err(JsString::from(format!(
                "{op}: the text at {index} is no string"
            )));
        };
        let Some(text) = text.as_str() else {
            return Err(JsString::from(format!(
                "{op}: the text at {index} carries a lone surrogate, which a Rust string cannot hold"
            )));
        };
        out.push(JsValue::from(map(text)));
    }
    let mut payload = JsObject::with_capacity(1);
    payload.insert("mapped", JsValue::Array(out));
    Ok(payload)
}
