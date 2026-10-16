// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool stub: the interface line every primitive answers under
 * --smoke.
 *
 * No analogue in the reference. The environment notebook smokes every sub
 * command of the seam with one cell, so every primitive answers one shape,
 * which names the sub command, whether it is implemented and the milestone it
 * lands at.
 */

//! The interface line of the seam.
//!
//! A primitive answers one line under `--smoke`,
//! `{"subcommand":...,"implemented":...,"lands_at":...}`, through
//! [`interface_line`], and exits 0, so the notebook reads one shape for every
//! sub command. A primitive that answers its operations carries
//! `"implemented":true`, which every primitive of the seam carries. A sub
//! command that answers `--smoke` alone carries `"implemented":false` and the
//! milestone it lands at, and the cell of the seam in
//! `tests/notebooks/00_environment.ipynb` holds such a sub command to exit 64
//! on any other call.

use aosabi_json::{
    JsObject,
    JsValue, //
};

/// Returns the one line a primitive answers under `--smoke`.
///
/// The line is an object with the keys `subcommand`, `implemented` and
/// `lands_at` in that order, written by the crate under test rather than by
/// hand, so the seam carries no JSON of its own.
pub fn interface_line(name: &str, implemented: bool, lands_at: &str) -> String {
    let mut report = JsObject::new();
    report.insert("subcommand", name);
    report.insert("implemented", implemented);
    report.insert("lands_at", lands_at);
    // An object always has a JSON form, so the writer's `None` arm, which is
    // the undefined top level value, is unreachable here.
    aosabi_json::to_string(&JsValue::Object(report)).unwrap_or_default()
}
