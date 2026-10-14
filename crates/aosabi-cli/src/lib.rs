// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the command line of Aosabi, behind the two programs cargo-aosabi
 * and aosabi.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * This file holds the module list and the entry of both programs.
 */

//! The command line of Aosabi.
//!
//! One library stands behind two programs, `cargo-aosabi`, which cargo runs
//! for `cargo aosabi`, and `aosabi`, and the `main` of each is [`main`]. The
//! crate runs on the build host and uses the standard library.
//!
//! [`main`] reads the arguments with [`args::parse`], reads `.env` of the
//! working directory with [`dotenv::load`] before any command runs, and runs
//! the command. A
//! command of a later milestone returns its refusal, and [`run`] prints it
//! once through [`report::report_failure`] and turns it into the exit code.
//! Every command returns rather than exits, so every teardown has run. The
//! exit code is 0 on success and 1 on every failure, a usage error, a refusal
//! and a command of a later milestone among them.
//! Standard output carries a result and nothing else, the help under `-h` and
//! the version among them, and every other line goes to standard error.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3.

use std::{
    ffi::OsString,
    io::{
        self,
        Write, //
    },
    process::ExitCode, //
};

use crate::{
    args::{
        Command,
        Global, //
    },
    dotenv::DotEnv, //
};

pub mod args;
pub mod build;
pub mod build_step;
pub mod cards;
pub mod cargo;
pub mod codes;
pub mod container;
pub mod dev;
pub mod dotenv;
pub mod env;
pub mod generate;
pub mod http;
pub mod inspect;
pub mod later;
pub mod lock;
pub mod mcp;
pub mod new;
pub mod project;
pub mod pull;
pub mod report;
pub mod std_source;
pub mod verify;

/// Runs the command line and returns its exit code.
///
/// The arguments are those of the process, the program name first. A usage
/// error prints its text on standard error and answers
/// [`ExitCode::FAILURE`], as every failure does.
#[must_use]
pub fn main() -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    let (global, command) = match args::parse(&argv) {
        Ok(parsed) => parsed,
        Err(usage) => {
            write_out(&mut io::stderr(), &usage.0);
            return ExitCode::FAILURE;
        }
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    let env_file = dotenv::load(&cwd);
    run(&command, &global, &env_file)
}

/// Runs `command` under `global` with the pairs `env_file` of `.env` and
/// returns its exit code.
#[must_use]
pub fn run(command: &Command, global: &Global, env_file: &DotEnv) -> ExitCode {
    let verbose = global.verbose || env::debug(env_file);
    let outcome = match command {
        Command::Help { command } => {
            let text = args::help(command.as_deref()).unwrap_or_default();
            write_out(&mut io::stdout(), &text);
            Ok(())
        }
        Command::Version => {
            write_out(&mut io::stdout(), &format!("{}\n", args::VERSION));
            Ok(())
        }
        Command::Later { command, lands } => Err(later::refusal(command, lands)),
        Command::Build(args) => return build::command(args, global, env_file),
        Command::New(args) => return new::command(args),
        Command::Dev(args) => return dev::command(args, global, env_file),
        Command::Inspect(args) => return inspect::command(args, global, env_file),
        Command::Verify(args) => return verify::command(args, global, env_file),
        Command::Mcp(args) => return mcp::command(args, global),
        Command::Pull(args) => return pull::command(args, global),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report::report_failure(&error, verbose);
            ExitCode::FAILURE
        }
    }
}

/// Writes `text` to `out` whole, dropping a failure of the stream, which has
/// no other stream to report to.
fn write_out(out: &mut impl Write, text: &str) {
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}
