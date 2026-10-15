// SPDX-License-Identifier: MIT
/*
 * xtask: the task runner of the repository, run as cargo xtask.
 *
 * The one sub command, viewer-assets with its flag --check, builds the wasm
 * module of aoviewer, strips it and writes it with assets.lock, and it is the
 * only writer of those files.
 */

//! The task runner of the repository.
//!
//! The command line is parsed by hand, because it is one sub command and one
//! flag. The exit code follows the exit status of POSIX `cmp`: 0 when the
//! products are written or match the build, 1 when `--check` finds a file that
//! differs, and 2 for a usage error or a failure, so a call that never got to
//! compare is never read as a stale product.
//!
//! [`FileError`] is the one error of a file of every module, so the build and
//! the lock report a path that cannot be read or written in one form.

mod assets;
mod lock;
mod wasm_strip;

use std::{
    env,
    ffi::OsString,
    fmt,
    io,
    process::ExitCode, //
};

use crate::assets::Outcome;

/// A file or a directory that cannot be read or written.
#[derive(Debug)]
pub struct FileError {
    /// The path from the root.
    path: String,
    /// The error of the operating system.
    error: io::Error,
}

impl FileError {
    /// Returns the error `error` of the file at `path`, a path from the root.
    #[must_use]
    pub fn new(path: &str, error: io::Error) -> Self {
        Self {
            path: String::from(path),
            error,
        }
    }
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.error)
    }
}

/// The usage line, printed for every call that is not one of the two forms.
const USAGE: &str = "xtask: usage: cargo xtask viewer-assets [--check]";

/// The exit code of a check that finds a file that differs.
const EXIT_DIFFERS: u8 = 1;

/// The exit code of a usage error and of a failure.
const EXIT_TROUBLE: u8 = 2;

/// Parses the arguments after the program name.
///
/// The result is `Some(false)` for `viewer-assets`, `Some(true)` for
/// `viewer-assets --check` and `None` for every other call.
fn parse(args: &[OsString]) -> Option<bool> {
    match args {
        [command] if command == "viewer-assets" => Some(false),
        [command, flag] if command == "viewer-assets" && flag == "--check" => Some(true),
        _ => None,
    }
}

/// Runs `viewer-assets`, or prints the usage line and exits 2.
fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let Some(check) = parse(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(EXIT_TROUBLE);
    };
    match assets::viewer_assets(check) {
        Ok(Outcome {
            written,
            stale,
            moved,
            compiler,
        }) => {
            let verb = if check { "built" } else { "wrote" };
            for line in written {
                println!("xtask: {verb} {line}");
            }
            let Some(line) = stale else {
                return ExitCode::SUCCESS;
            };
            eprintln!("xtask: {line}");
            for path in moved {
                eprintln!("xtask: {path} moved since the committed assets.lock");
            }
            if let Some((committed, built)) = compiler {
                eprintln!(
                    "xtask: the committed assets.lock names {committed}, the build ran {built}"
                );
            }
            ExitCode::from(EXIT_DIFFERS)
        }
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::from(EXIT_TROUBLE)
        }
    }
}
