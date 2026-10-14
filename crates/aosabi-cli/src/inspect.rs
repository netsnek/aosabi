// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the command inspect.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The command `inspect`.
//!
//! `aosabi inspect` loads the project in a child process, builds the schema with
//! the schema builder, validates nothing, and prints the canonical model:
//! `--sdl` prints its SDL, else `--ddl` its DDL, else the model goes out as
//! `JSON.stringify(model, null, 2)`, each followed by one newline.
//!
//! The runner validates nothing, so a service whose schema the build refuses is
//! still inspected.
//!
//! [`command`] prints the model on standard output and nothing else, with the
//! precedence above, and the JSON form with the gap of two spaces
//! through [`to_string_with_gap`], so the bytes of `aosabi inspect --json`
//! equal `target/aosabi/manifest.json` of a build of the same sources. `-e` and
//! `-m` are accepted for their spelling and refused with `AO-C0104` for any
//! value that is not the crate root of the package, the path of its `[lib]`
//! table or cargo's default `src/lib.rs`, because the entry of an Aosabi
//! service is its crate root. A value is read against the working directory.
//!
//! A plain crate, whose roots no fragment of `graphql!` holds, needs no runner:
//! [`inspect_app`] reads its source through `aosabi_build::introspect_source`,
//! the inference of its build, and forms the model of the same IR with nothing
//! compiled.

use std::{
    fs,
    io::{
        self,
        Write, //
    },
    path::{
        Component,
        Path,
        PathBuf, //
    },
    process::{
        Command,
        ExitCode,
        Stdio, //
    },
};

use aosabi_ir::app_model::{
    AppModel,
    app_model,
    app_model_to_ddl,
    app_model_to_js,
    app_model_to_sdl, //
};
use aosabi_json::{
    JsValue,
    to_string_with_gap, //
};

use crate::{
    args::{
        Global,
        InspectArgs,
        LockMode, //
    },
    build,
    cards::{
        self,
        Builds,
        DEFAULT_CARD, //
    },
    cargo::{
        self,
        BuildRun,
        Cargo,
        Compile,
        Output, //
    },
    codes,
    dotenv::DotEnv,
    env,
    generate::Flavour,
    project::{
        self,
        Project, //
    },
    report::{
        self,
        CliError, //
    },
};

/// The file the inspect runner writes, inside the directory of the inspect
/// package.
pub const MODEL_FILE: &str = "model.json";

/// The gap of the JSON form, the one of `JSON.stringify(model, null, 2)`.
const GAP: &str = "  ";

/// Runs `inspect` with `args` under `global` and the pairs `env_file` of
/// `.env`, and answers the exit code.
#[must_use]
pub fn command(args: &InspectArgs, global: &Global, env_file: &DotEnv) -> ExitCode {
    let verbose = global.verbose || env::debug(env_file);
    match inspect(args, global, env_file) {
        Ok(text) => {
            let mut out = io::stdout().lock();
            if out
                .write_all(text.as_bytes())
                .and_then(|()| out.flush())
                .is_err()
            {
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            report::report_failure(&error, verbose);
            ExitCode::FAILURE
        }
    }
}

/// Returns the canonical model of `project`, the model `aosabi inspect`
/// prints, under the lock flags of `lock`.
///
/// # Errors
///
/// Returns the refusals of the derived lock, a failed build of the inspect
/// package with the output of cargo as its detail, a failed run of the
/// runner with its standard error as its detail, a model file that does not
/// read, and the refusal of [`app_model`] for a name outside the alphabet of
/// its collation or for two or more queues.
pub fn inspect_app(project: &Project, lock: LockMode, cargo: &Cargo) -> Result<AppModel, CliError> {
    if project.plain {
        return inspect_source(project, cargo);
    }
    let card = cards::by_name(DEFAULT_CARD).ok_or_else(|| codes::no_builtin_card(DEFAULT_CARD))?;
    let Builds::ServerPackage(package) = card.builds else {
        return Err(codes::no_builtin_card(DEFAULT_CARD));
    };
    let prepared = build::prepare(project, &package, Flavour::Build, lock, cargo, None)?;
    let generated = &prepared.generated;
    let compiled = cargo.build(&BuildRun {
        cwd: &project.root,
        manifest: Some(&generated.inspect_manifest),
        target_dir: &project.target_dir,
        package: None,
        triple: None,
        no_default_features: false,
        features: &[],
        lib: false,
        release: false,
        lock: generated_lock_mode(lock),
        output: Output::Capture,
        env: &[],
    })?;
    if !compiled.status.success() {
        return Err(cargo::failure(
            &format!("the cargo build of {}", generated.inspect_bin),
            &compiled.stderr,
        ));
    }
    let runner = compiled
        .artifacts
        .iter()
        .filter(|artifact| artifact.target == generated.inspect_bin)
        .find_map(|artifact| artifact.executable.clone())
        .unwrap_or_else(|| {
            project
                .target_dir
                .join("debug")
                .join(&generated.inspect_bin)
        });
    let model_path = generated
        .inspect_manifest
        .parent()
        .unwrap_or(&generated.root)
        .join(MODEL_FILE);
    run_runner(&runner, &model_path, project, cargo)?;
    read_model(&model_path)
}

/// Returns the canonical model of the plain crate of `project` from its
/// source as rustc compiles it for the host in the dev profile, with no
/// runner and no compile, which is the model the build of the crate for the
/// card `server` writes as `manifest.json`.
///
/// # Errors
///
/// Returns the failure of rustc that prints the active set of the host,
/// `AO-C0105` with every refusal of the source path, and the refusal of
/// [`app_model`].
fn inspect_source(project: &Project, cargo: &Cargo) -> Result<AppModel, CliError> {
    let cfg = cargo.cfg(&project.root, None, Compile::Host, &[], &[])?;
    let data = aosabi_build::introspect_source(&project.root, &cfg)
        .map_err(|error| build::source_failure(project, error))?;
    let failure = |what: String| CliError::new(format!("{}: {what}", project.root.display()));
    model_of(&data, failure)
}

/// Runs the steps of `inspect` and answers the text to print.
fn inspect(args: &InspectArgs, global: &Global, env_file: &DotEnv) -> Result<String, CliError> {
    let cwd = std::env::current_dir()
        .map_err(|error| CliError::new(format!("the working directory is unreadable: {error}")))?;
    let cargo = Cargo::from_env(env_file);
    let project = project::load(&cwd, global.manifest_path.as_deref(), args.lock, &cargo)?;
    if let Some(entry) = &args.entry {
        check_entry(&cwd, entry, &project)?;
    }
    let model = inspect_app(&project, args.lock, &cargo)?;
    let mut text = if args.sdl {
        app_model_to_sdl(&model).map_err(|error| CliError::new(error.to_string()))?
    } else if args.ddl {
        app_model_to_ddl(&model)
    } else {
        to_string_with_gap(&app_model_to_js(&model), GAP).unwrap_or_default()
    };
    text.push('\n');
    Ok(text)
}

/// Refuses the entry `entry`, read against `cwd`, with `AO-C0104` when it is
/// not the crate root of `project`, the path of its `[lib]` table or cargo's
/// default `src/lib.rs`.
///
/// # Errors
///
/// Returns `AO-C0104` for an entry that names another file.
pub fn check_entry(cwd: &Path, entry: &Path, project: &Project) -> Result<(), CliError> {
    let named = normalize(&cwd.join(entry));
    let lib = normalize(&project.lib_path);
    let same = named == lib
        || fs::canonicalize(&named)
            .ok()
            .zip(fs::canonicalize(&lib).ok())
            .is_some_and(|(a, b)| a == b);
    if same {
        return Ok(());
    }
    let shown = project
        .lib_path
        .strip_prefix(&project.root)
        .unwrap_or(&project.lib_path);
    Err(codes::entry_not_library(entry, shown))
}

/// Returns `path` with every `.` dropped and every `..` taken back, without
/// reading the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            part => out.push(part),
        }
    }
    out
}

/// Returns the lock mode of the cargo calls on the generated workspace after
/// its check, `--locked`, and `--frozen` where `lock` forbids the network, as
/// `build` runs them.
fn generated_lock_mode(lock: LockMode) -> LockMode {
    if cargo::is_offline(lock) {
        LockMode::Frozen
    } else {
        LockMode::Locked
    }
}

/// Runs the inspect runner `runner` with `model_path` in the root of
/// `project` with the pairs of `.env`, its output captured.
fn run_runner(
    runner: &Path,
    model_path: &Path,
    project: &Project,
    cargo: &Cargo,
) -> Result<(), CliError> {
    let done = Command::new(runner)
        .arg(model_path)
        .current_dir(&project.root)
        .envs(cargo.env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            CliError::new(format!(
                "the inspect runner {} could not start: {error}",
                runner.display()
            ))
        })?;
    if done.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&done.stderr);
    let message = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map_or_else(
            || format!("the inspect runner {} failed", runner.display()),
            str::to_owned,
        );
    Err(CliError::new(message).with_detail(stderr.trim_end().to_owned()))
}

/// Reads the model file the runner wrote at `path` into the canonical
/// model.
fn read_model(path: &Path) -> Result<AppModel, CliError> {
    let failure = |what: String| CliError::new(format!("{}: {what}", path.display()));
    let bytes = fs::read(path).map_err(|error| failure(error.to_string()))?;
    let value = aosabi_json::parse(&bytes).map_err(|error| failure(error.to_string()))?;
    model_of(&value, failure)
}

/// Returns the canonical model of the introspection `value` of an app,
/// `{"ir", "authz", "queues"}`, whose faults `failure` words.
fn model_of(value: &JsValue, failure: impl Fn(String) -> CliError) -> Result<AppModel, CliError> {
    for harvest in ["authz", "queues"] {
        match cargo::field(value, harvest) {
            Some(JsValue::Array(items)) if items.is_empty() => {}
            _ => {
                return Err(failure(format!(
                    "`{harvest}` is no empty list, and this build of aosabi reads no harvest of the ORM or of the queues"
                )));
            }
        }
    }
    let ir = cargo::field(value, "ir").ok_or_else(|| failure(String::from("no `ir` member")))?;
    let ir = aosabi_ir::from_js_value(ir).map_err(|error| failure(error.to_string()))?;
    app_model(ir, Vec::new(), Vec::new()).map_err(|error| failure(error.to_string()))
}
