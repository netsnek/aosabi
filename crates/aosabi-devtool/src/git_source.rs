// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool git-source: reads the source of a dependency from a
 * repository the way the command line reads it, and writes the entry the
 * manifests of the generated workspace name it by.
 *
 * No analogue in the reference, whose packages come from npm. The crates
 * under test are the reader git_source of aosabi-cli, which reads the source
 * cargo metadata states for a dependency, and its writer spec, which names
 * the dependency in the manifest of a generated package.
 */

//! The `git-source` primitive of the seam.
//!
//! Its one form is `--requests PATH`, the batch form of [`crate::batch`].
//!
//! | `op` | Request fields | Payload |
//! | --- | --- | --- |
//! | `git_source` | `source` | `url`, `key`, `value` and `spec` |
//!
//! The field `source` is the source of a dependency as cargo metadata states
//! it, `git+<url>` with the query of its reference. The payload holds what
//! [`aosabi_cli::project::git_source`] reads from it, the URL and the key and
//! the value of the reference, each `null` for the default branch, and
//! `spec`, the entry [`aosabi_cli::generate::spec`] writes for the dependency
//! inside the inline table of a generated manifest. A source of another kind
//! answers `null` in all four.
//!
//! The seam names the command line on a Unix host alone, so this primitive
//! exists on such a host alone, and the seam still builds for
//! `wasm32-unknown-unknown`.

use std::{
    path::Path,
    process::ExitCode, //
};

use aosabi_cli::{
    generate,
    project::{
        DepSource,
        git_source, //
    },
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
    dotenv,
    seam::{
        self,
        BatchArgs, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "git-source";

/// The value the primitive reports as `lands_at` under `--smoke`, the one of
/// `dotenv`, the other reader of the command line.
const LANDS_AT: &str = dotenv::LANDS_AT;

/// The operations of the batch form.
const OPERATIONS: &Operations = &[("git_source", op_git_source)];

/// Runs the `git-source` primitive, whose one form is the batch form.
pub fn run(args: &BatchArgs) -> ExitCode {
    seam::run_batch(NAME, LANDS_AT, OPERATIONS, args)
}

/// Answers the operation `git_source`.
fn op_git_source(request: &JsObject) -> Result<JsObject, JsString> {
    let source = batch::require_string(request, "source")?;
    let mut payload = JsObject::with_capacity(4);
    let Some((url, reference)) = git_source(source) else {
        for field in ["url", "key", "value", "spec"] {
            payload.insert(field, JsValue::Null);
        }
        return Ok(payload);
    };
    let (key, value) = reference
        .key_value()
        .map_or((JsValue::Null, JsValue::Null), |(key, value)| {
            (JsValue::from(key), JsValue::from(value))
        });
    let spec = generate::spec(
        &DepSource::Git {
            url: url.clone(),
            reference,
        },
        Path::new("."),
    )
    .map_err(|error| JsString::from(error.message.as_str()))?;
    payload.insert("url", url);
    payload.insert("key", key);
    payload.insert("value", value);
    payload.insert("spec", spec);
    Ok(payload)
}
