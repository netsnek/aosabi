// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the command pull.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/pull/index.ts
 * and packages/pylon/src/cli/index.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * Each file of the directory names the part it translates.
 */

//! The command `pull`.
//!
//! `aosabi pull <url> [-n <name>] [-o <dir>]` prints
//! `Pulling <name> schema from <url>...`, writes the registry, prints
//! `Remote types generated successfully at <path>`, and on any failure prints
//! `Failed to pull <name> remote schema:` with the error and exits 1.
//! [`command`] runs [`fetch_schema`] and [`emit_registry`], creates the directory, writes `<dir>/<name>.rs` and
//! `<dir>/mod.rs` through `update_file_if_changed`, `mod.rs` gaining the line
//! `pub mod <name>;` once when it is absent, so the author adds one
//! `mod generated;`, and a second pull of the same remote writes no file.
//! Every line goes to standard error, and a failure prints its head and the
//! report of the failure on one line, its code first, with its hint below.
//!
//! The name becomes a module, so a name that is no Rust identifier refuses
//! before the remote is asked, and a keyword is written raw in `mod.rs`.
//!
//! | File | Holds |
//! | --- | --- |
//! | `remote.rs` | `fetch_schema`, the post of the introspection query, and `schema_of_answer`, the client schema of the answer |
//! | `generate.rs` | `emit_registry`, the Rust registry |

pub mod generate;
pub mod remote;

use std::{
    fs,
    path::Path,
    process::ExitCode, //
};

use aosabi_build::update_file_if_changed;

pub use self::{
    generate::emit_registry,
    remote::{
        fetch_schema,
        schema_of_answer, //
    }, //
};
use crate::{
    args::{
        Global,
        PullArgs, //
    },
    report::{
        self,
        CliError, //
    },
};

/// The file of the module list beside the registry.
pub const MOD_FILE: &str = "mod.rs";

/// The keywords a module name is written raw for, and the names that cannot
/// be raw, which `mod.rs` cannot name.
const NOT_A_MODULE: [&str; 5] = ["self", "super", "crate", "Self", "_"];

/// Runs `pull` with `args` under `global` and answers the exit code.
#[must_use]
pub fn command(args: &PullArgs, global: &Global) -> ExitCode {
    eprintln!("Pulling {} schema from {}...", args.name, args.url);
    match pull(args) {
        Ok(path) => {
            eprintln!("Remote types generated successfully at {path}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            let text = report::render(&error, global.verbose);
            let text = text.strip_prefix("error: ").unwrap_or(&text);
            eprint!("Failed to pull {} remote schema: {text}", args.name);
            ExitCode::FAILURE
        }
    }
}

/// Pulls the registry of `args` and answers the path it wrote, as the
/// reference prints it.
fn pull(args: &PullArgs) -> Result<String, CliError> {
    let module = module_name(&args.name)?;
    let schema = fetch_schema(&args.url)?;
    let text = emit_registry(&schema)?;
    fs::create_dir_all(&args.output).map_err(|error| io_failure(&args.output, &error))?;
    let file = args.output.join(format!("{}.rs", args.name));
    update_file_if_changed(&file, text.as_bytes()).map_err(|error| io_failure(&file, &error))?;
    let list = args.output.join(MOD_FILE);
    let line = format!("pub mod {module};");
    let current = match fs::read_to_string(&list) {
        Ok(current) => current,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(io_failure(&list, &error)),
    };
    if !current.lines().any(|known| known.trim() == line) {
        let mut next = current;
        if !next.is_empty() && !next.ends_with('\n') {
            next.push('\n');
        }
        next.push_str(&line);
        next.push('\n');
        update_file_if_changed(&list, next.as_bytes())
            .map_err(|error| io_failure(&list, &error))?;
    }
    Ok(format!("{}/{}.rs", args.output.display(), args.name))
}

/// Returns the module name `mod.rs` writes for the registry `name`, raw for
/// a keyword.
///
/// # Errors
///
/// Returns a failure for a name that is no Rust identifier, which no module
/// can carry.
pub fn module_name(name: &str) -> Result<String, CliError> {
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !NOT_A_MODULE.contains(&name);
    if !valid {
        return Err(CliError::new(format!(
            "the name {name} is no Rust identifier, and the registry is the module `pub mod <name>;` of mod.rs"
        ))
        .with_hint("aosabi pull <url> -n <name> with a name of letters, digits and _"));
    }
    let raw = generate::is_keyword(name);
    Ok(if raw {
        format!("r#{name}")
    } else {
        name.to_owned()
    })
}

/// Returns the failure of a read or a write of `path`.
fn io_failure(path: &Path, error: &std::io::Error) -> CliError {
    CliError::new(format!("{}: {error}", path.display()))
}
