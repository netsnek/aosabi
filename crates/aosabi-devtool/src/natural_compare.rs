// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool natural-compare: orders two strings the way naturalCompare
 * of graphql-js orders them.
 *
 * No analogue in the reference.
 */

//! The `natural-compare` primitive of the seam.
//!
//! The two strings are the two positional arguments, and a string that opens
//! with `-` goes behind a `--`. The payload is `-1`, `0` or `1` with no
//! trailing newline, and the exit code is 0.
//!
//! The payload is the sign of the comparison and not the raw number of the
//! reference. The reference returns the difference of the two lengths when no
//! character and no run of digits decides first, and only the sign of that
//! number reaches a sort, so the port of aosabi-type answers an `Ordering`
//! and the sign is all the seam can write.
//!
//! `--requests PATH` is the batch form, which answers the four operations of
//! the type layer listed in [`crate::print_schema`]. Its `order` is the same
//! sign.

use std::{
    cmp::Ordering,
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsString, //
};

use crate::{
    batch,
    cli::{
        self,
        Grammar,
        Matches,
        Positional,
        Refusal, //
    },
    print_schema,
    seam::{
        self,
        Answer,
        Failure, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "natural-compare";

/// The command line of the `natural-compare` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: Some(Positional {
        value: "STRING",
        most: 2,
        help: "The two strings to compare, the reference string first",
    }),
    conflicts: &[("requests", &["STRING"])],
};

/// What the `natural-compare` primitive accepts.
#[derive(Debug)]
pub struct NaturalCompareArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
    /// The two strings to compare, the reference string first.
    strings: Vec<String>,
}

impl NaturalCompareArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when a string is not UTF-8.
    pub fn read(matches: &Matches) -> Result<Self, Refusal> {
        Ok(Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
            strings: matches.texts()?,
        })
    }
}

/// Runs the `natural-compare` primitive.
pub fn run(args: &NaturalCompareArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke(NAME);
    }
    if let Some(requests) = &args.requests {
        return seam::finish(NAME, batch::run(print_schema::OPERATIONS, requests));
    }
    let answer = match args.strings.as_slice() {
        [a, b] => Ok(Answer::text(sign(a, b).to_string())),
        _ => Err(Failure::Usage(String::from(
            "the call names no two strings",
        ))),
    };
    seam::finish(NAME, answer)
}

/// Returns the sign of `naturalCompare(a, b)`.
fn sign(a: &str, b: &str) -> i32 {
    match aosabi_type::natural_compare(a, b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// Answers the operation `natural_compare`.
pub fn op_natural_compare(request: &JsObject) -> Result<JsObject, JsString> {
    let a = batch::require_string(request, "a")?;
    let b = batch::require_string(request, "b")?;
    let mut payload = JsObject::with_capacity(1);
    payload.insert("order", sign(a, b));
    Ok(payload)
}
