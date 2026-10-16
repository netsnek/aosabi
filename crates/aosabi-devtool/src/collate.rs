// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool collate: collates two type names the way the reference
 * collates them.
 *
 * No analogue in the reference. That call is real collation rather than byte
 * order, so the primitive is judged over name pairs against node, and the
 * judging notebook is tests/notebooks/15_sdl_printer.ipynb.
 */

//! The `collate` primitive of the seam.
//!
//! `--a NAME` and `--b NAME` are the two names, the reference string first.
//! The payload is `-1`, `0` or `1` with no trailing newline, which is the sign
//! `a.localeCompare(b)` has under the root collation, and the exit code is 0.
//! A name that leaves the alphabet `[_0-9A-Za-z]` is refused by the crate
//! under test, which writes a JSON array that holds the one refusal as
//! `message` and exits 1.
//!
//! `--requests PATH` is the batch form, which answers the four operations of
//! the schema layer listed in [`crate::print_sdl`]. Its `order` is the same
//! sign.

use std::{
    cmp::Ordering,
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_ir::CollateError;
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};

use crate::{
    batch,
    cli::{
        self,
        Dash,
        Grammar,
        Matches,
        Opt,
        Refusal, //
    },
    print_sdl,
    seam::{
        self,
        Answer,
        Failure, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "collate";

/// The command line of the `collate` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        Opt::value(
            "a",
            "NAME",
            "The name on the left of the comparison, which is the reference string",
        )
        .dash(Dash::Any),
        Opt::value("b", "NAME", "The name on the right of the comparison").dash(Dash::Any),
        cli::REQUESTS,
    ],
    positional: None,
    conflicts: &[("requests", &["a", "b"])],
};

/// What the `collate` primitive accepts.
#[derive(Debug)]
pub struct CollateArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The name on the left of the comparison.
    a: Option<String>,
    /// The name on the right of the comparison.
    b: Option<String>,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl CollateArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when a name is not UTF-8.
    pub fn read(matches: &Matches) -> Result<Self, Refusal> {
        Ok(Self {
            smoke: matches.flag("smoke"),
            a: matches.text("a")?,
            b: matches.text("b")?,
            requests: matches.path("requests"),
        })
    }
}

/// Runs the `collate` primitive.
pub fn run(args: &CollateArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, print_sdl::LANDS_AT);
    }
    if let Some(requests) = &args.requests {
        return seam::finish(NAME, batch::run(print_sdl::OPERATIONS, requests));
    }
    let answer = match (&args.a, &args.b) {
        (Some(a), Some(b)) => Ok(match sign(a, b) {
            Ok(order) => Answer::text(order.to_string()),
            Err(error) => Answer::refused(vec![error_json(&error)]),
        }),
        (None, _) => Err(Failure::Usage(String::from("the call names no --a"))),
        (_, None) => Err(Failure::Usage(String::from("the call names no --b"))),
    };
    seam::finish(NAME, answer)
}

/// Returns the sign of `a.localeCompare(b)` under the root collation.
///
/// # Errors
///
/// Returns the refusal of [`aosabi_ir::collate()`] for a name that leaves the
/// alphabet.
fn sign(a: &str, b: &str) -> Result<i32, CollateError> {
    Ok(match aosabi_ir::collate(a, b)? {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    })
}

/// Returns the entry of an error array that carries the refusal `error`.
///
/// The reference has no such error, because `localeCompare` orders every
/// string. The entry therefore has the shape of a plain `Error`, which is
/// `message` alone.
pub fn error_json(error: &CollateError) -> JsValue {
    seam::message_json(error.to_string())
}

/// Answers the operation `collate`.
pub fn op_collate(request: &JsObject) -> Result<JsObject, JsString> {
    let a = batch::require_string(request, "a")?;
    let b = batch::require_string(request, "b")?;
    let mut payload = JsObject::with_capacity(1);
    match sign(a, b) {
        Ok(order) => payload.insert("order", order),
        Err(error) => payload.insert("errors", vec![error_json(&error)]),
    };
    Ok(payload)
}
