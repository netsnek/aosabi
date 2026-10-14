// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the command dev, the development loop.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/index.ts and
 * packages/pylon/src/cli/dev/dev-server.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * Each file of the directory names the part it translates.
 */

//! The command `dev`.
//!
//! `cargo aosabi dev` is a supervisor over child processes in both roles,
//! `web` and `worker`, as `docs/cli-and-targets.md` section 4 states:
//! [`command`] reads the project with the refusals of `build`, takes the
//! `server` card, computes the port with [`env::port`], snapshots the sources
//! with [`watch::Watcher`] and hands them to [`supervisor::Supervisor`], which
//! builds, runs the child and rebuilds on every burst of changes until the
//! process ends. A refusal of the project prints its report and exits 1.
//!
//! Every rebuild of a plain crate runs the steps of `build`, so it reads the
//! crate from its source again and refreshes the generated copy of the crate
//! under the target directory, its links, its crate root and the module of its
//! glue, before cargo compiles it, and an edit that adds a field, a type or a
//! module reaches the copy the child serves.
//!
//! | File | Holds |
//! | --- | --- |
//! | `watch.rs` | `Kind`, `classify`, `Watcher` |
//! | `supervisor.rs` | `Supervisor`, its readiness probe, its lines and its box |

pub mod supervisor;
pub mod watch;

use std::process::ExitCode;

use crate::{
    args::{
        DevArgs,
        Global, //
    },
    cards::{
        self,
        DEFAULT_CARD, //
    },
    cargo::Cargo,
    codes,
    dotenv::DotEnv,
    env,
    project,
    report::{
        self,
        CliError, //
    }, //
};

/// Runs `dev` with `args` under `global` and the pairs `env_file` of `.env`.
///
/// The command returns only on a refusal of the project or when its root can
/// no longer be read, each with [`ExitCode::FAILURE`], and otherwise runs
/// until a signal ends it with its child.
#[must_use]
pub fn command(args: &DevArgs, global: &Global, env_file: &DotEnv) -> ExitCode {
    let verbose = global.verbose || env::debug(env_file);
    match setup(args, global, env_file, verbose) {
        Ok((setup, mut watcher)) => supervisor::Supervisor::new(setup).run(&mut watcher),
        Err(error) => {
            report::report_failure(&error, verbose);
            ExitCode::FAILURE
        }
    }
}

/// Reads the project, the card, the port and the first snapshot of the
/// sources.
fn setup(
    args: &DevArgs,
    global: &Global,
    env_file: &DotEnv,
    verbose: bool,
) -> Result<(supervisor::Setup, watch::Watcher), CliError> {
    let cwd = std::env::current_dir()
        .map_err(|error| CliError::new(format!("the working directory is unreadable: {error}")))?;
    let cargo = Cargo::from_env(env_file);
    let project = project::load(&cwd, global.manifest_path.as_deref(), args.lock, &cargo)?;
    let card = cards::by_name(DEFAULT_CARD).ok_or_else(|| codes::no_builtin_card(DEFAULT_CARD))?;
    let watcher = watch::Watcher::new(&project.root, args.worker).map_err(|error| {
        CliError::new(format!(
            "the project root {} is unreadable: {error}",
            project.root.display()
        ))
    })?;
    let setup = supervisor::Setup {
        cwd,
        manifest_path: global.manifest_path.clone(),
        lock: args.lock,
        cargo,
        project,
        card,
        port: env::port(env_file),
        worker: args.worker,
        verbose,
    };
    Ok((setup, watcher))
}
