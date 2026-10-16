// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool pubsub: runs scripts of the pub sub and the repeater, each
 * on one fresh agent with virtual timers.
 *
 * No analogue in the reference. The crates under test are the Value level pub
 * sub of aosabi-core and the repeater of aosabi-execute, the scripts are those
 * of the sidecar tests/sidecar/pubsub.mjs, so one file drives both sides, and
 * the judging notebook is tests/notebooks/40_pubsub.ipynb.
 */

//! The `pubsub` primitive of the seam.
//!
//! `--requests PATH` answers a file of request lines, one response line each,
//! in the batch form of [`crate::batch`], with the two operations of the
//! sidecar `tests/sidecar/pubsub.mjs`.
//!
//! - `versions` answers the versions of the two packages the crates under
//!   test port, `subscription` for `@graphql-yoga/subscription` and
//!   `repeater` for `@repeaterjs/repeater`.
//! - `script` runs the list `steps` on a fresh agent with virtual timers and
//!   answers `{"observations": [...], "thrown": ...}`, which [`script`]
//!   describes.
//!
//! The steps are those of the sidecar's protocol, and one step is the seam's
//! alone: `{"publish_on_thread": "p", "key": "k", "args": [...]}` publishes
//! as the step `publish` does from a second operating system thread of this
//! process and waits for that thread to end, so a subscriber on the agent of
//! the script is woken through the foreign waker of its pump.

mod script;

use std::{
    path::PathBuf,
    process::ExitCode, //
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
    cli::{
        self,
        Grammar,
        Matches, //
    },
    seam::{
        self,
        Failure, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "pubsub";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "M6";

/// The version of `@graphql-yoga/subscription` the pub sub of aosabi-core ports.
const SUBSCRIPTION_VERSION: &str = "5.0.5";

/// The version of `@repeaterjs/repeater` the repeater of aosabi-execute ports.
const REPEATER_VERSION: &str = "3.0.6";

/// The operations of the protocol.
const OPERATIONS: &Operations = &[("versions", op_versions), ("script", op_script)];

/// The command line of the `pubsub` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        cli::REQUESTS, //
    ],
    positional: None,
    conflicts: &[],
};

/// What the `pubsub` primitive accepts.
#[derive(Debug)]
pub struct PubsubArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines.
    requests: Option<PathBuf>,
}

impl PubsubArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// Runs the `pubsub` primitive.
pub fn run(args: &PubsubArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = match &args.requests {
        Some(requests) => batch::run(OPERATIONS, requests),
        None => Err(Failure::Usage(String::from("the call names no --requests"))),
    };
    seam::finish(NAME, outcome)
}

/// Answers the operation `versions`, the versions of the ported packages.
// The signature is the one every operation of the batch form has.
#[expect(clippy::unnecessary_wraps)]
fn op_versions(_request: &JsObject) -> Result<JsObject, JsString> {
    let mut payload = JsObject::new();
    payload.insert("subscription", SUBSCRIPTION_VERSION);
    payload.insert("repeater", REPEATER_VERSION);
    Ok(payload)
}

/// Answers the operation `script`, which runs one script of steps.
fn op_script(request: &JsObject) -> Result<JsObject, JsString> {
    script::run(request).map_err(JsString::from)
}
