// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the failure report every command prints.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The failure report.
//!
//! Most failures of a command line are expected, and a stack through the
//! user's own paths reads like a crash and buries the one line that matters.
//! A command of Aosabi therefore returns a [`CliError`], and
//! [`report_failure`] prints it once, at the top, so every teardown of the
//! command has run before the process sets its exit code.
//!
//! The report reads `error: ` and the message, the code first where the
//! failure is a refusal of the table in [`crate::codes`], then the command
//! that would change the outcome on its own line where the failure carries
//! one, and then either the detail whole, which is the captured output or
//! the cause chain, or the line [`VERBOSE_LINE`] that says where the detail
//! is. The detail is printed whole under `-v`, `--verbose` and
//! `AOSABI_DEBUG=1`, which the caller folds into the one flag `verbose`.
//! The line names no stack trace, because what a failure of the command line
//! hides is the output of a child and not a stack.

use std::io::{
    self,
    Write, //
};

/// The line that follows a report whose detail is hidden.
pub const VERBOSE_LINE: &str = "Run with --verbose for the full output.";

/// A failure of a command, as the command returns it to `main`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliError {
    /// The refusal code of [`crate::codes`], `None` for a failure that is no
    /// refusal, such as a failed build.
    pub code: Option<&'static str>,
    /// The one line that says what failed.
    pub message: String,
    /// The captured output or the cause chain, printed under `--verbose`.
    pub detail: Option<String>,
    /// The exact command that would change the outcome.
    pub hint: Option<String>,
}

impl CliError {
    /// Returns a failure with `message` and no code, detail or hint.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
            detail: None,
            hint: None,
        }
    }

    /// Returns the refusal `code` with `message`.
    #[must_use]
    pub fn coded(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            ..Self::new(message)
        }
    }

    /// Returns the failure with `hint`, the command that would change the
    /// outcome.
    #[must_use]
    pub fn with_hint(self, hint: impl Into<String>) -> Self {
        Self {
            hint: Some(hint.into()),
            ..self
        }
    }

    /// Returns the failure with `detail`, the output a report under
    /// `--verbose` prints whole.
    #[must_use]
    pub fn with_detail(self, detail: impl Into<String>) -> Self {
        Self {
            detail: Some(detail.into()),
            ..self
        }
    }
}

/// Prints `error` on standard error, with its detail whole when `verbose`.
///
/// A write that fails is dropped, because standard error is the stream every
/// failure goes to and a failure to reach it has no other stream to go to.
pub fn report_failure(error: &CliError, verbose: bool) {
    let text = render(error, verbose);
    let _ = io::stderr().lock().write_all(text.as_bytes());
}

/// Returns the text [`report_failure`] prints for `error`, each line ended
/// by a newline.
///
/// A message that is empty after trimming has nothing to say, so the report
/// falls back to the detail whole.
#[must_use]
pub fn render(error: &CliError, verbose: bool) -> String {
    let message = error.message.trim();
    let mut out = match (error.code, message.is_empty()) {
        (Some(code), true) => format!("error: {code}\n"),
        (Some(code), false) => format!("error: {code} {message}\n"),
        (None, _) => format!("error: {message}\n"),
    };
    if let Some(hint) = &error.hint {
        push_line(&mut out, hint);
    }
    if let Some(detail) = &error.detail {
        if verbose || message.is_empty() {
            push_line(&mut out, detail);
        } else {
            push_line(&mut out, VERBOSE_LINE);
        }
    }
    out
}

/// Appends `text` to `out` and ends it with one newline.
fn push_line(out: &mut String, text: &str) {
    out.push_str(text);
    if !text.ends_with('\n') {
        out.push('\n');
    }
}
