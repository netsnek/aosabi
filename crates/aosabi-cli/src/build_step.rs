// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the build step, the one run of the built binary under
 * AOSABI_BUILD=1.
 *
 * No analogue in the reference. The step is the one of docs/cli-and-targets.md
 * section 3.3.
 */

//! The build step.
//!
//! After the cargo build, `cargo aosabi build` runs the binary once with
//! `AOSABI_BUILD=1`. That run evaluates the configuration, calls the `build`
//! hook of every plugin once and exits, and a configuration that panics aborts
//! the build there with the abort message of the reference, whose one file
//! name reads `aosabi.config`. The binary prints that message itself, so the
//! step passes its streams through and adds no text: under [`Output::Inherit`]
//! its standard output goes to the standard error of the command line, which
//! carries every line that is no result, and its standard error goes there as
//! it comes.

use std::{
    io,
    path::Path,
    process::{
        Command,
        Stdio, //
    },
};

use crate::{
    build,
    cargo::Output,
    dotenv::DotEnv,
    report::CliError, //
};

/// The variable that runs the build step in place of the server.
pub const BUILD_ENV: &str = "AOSABI_BUILD";

/// Runs `binary` once in `cwd` with `AOSABI_BUILD=1` and the pairs of
/// `env`, its streams as `output` says.
///
/// # Errors
///
/// Returns a failure when the binary cannot be started or exits with a
/// status other than 0. Under [`Output::Inherit`] the binary has printed
/// what it has to say, so the failure is [`build::shown`], and under
/// [`Output::Capture`] it carries the two streams as its detail.
pub fn run_build_step(
    binary: &Path,
    cwd: &Path,
    env: &DotEnv,
    output: Output,
) -> Result<(), CliError> {
    let mut command = Command::new(binary);
    command
        .current_dir(cwd)
        .envs(env.pairs.iter().map(|(key, value)| (key, value)))
        .env(BUILD_ENV, "1")
        .stdin(Stdio::null());
    match output {
        Output::Inherit => {
            command
                .stdout(Stdio::from(io::stderr()))
                .stderr(Stdio::inherit());
        }
        Output::Capture => {
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
        }
    }
    let done = command.output().map_err(|error| {
        CliError::new(format!(
            "the build step could not start {}: {error}",
            binary.display()
        ))
    })?;
    if done.status.success() {
        return Ok(());
    }
    match output {
        Output::Inherit => Err(build::shown()),
        Output::Capture => {
            let mut detail = String::from_utf8_lossy(&done.stderr).into_owned();
            detail.push_str(&String::from_utf8_lossy(&done.stdout));
            let status = done
                .status
                .code()
                .map_or_else(|| String::from("a signal"), |code| format!("status {code}"));
            Err(CliError::new(format!("the build step ended with {status}")).with_detail(detail))
        }
    }
}
