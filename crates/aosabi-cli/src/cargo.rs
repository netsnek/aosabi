// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the cargo runs of the command line and what they print.
 *
 * No analogue in the reference. The reference bundles with rolldown inside
 * its own process, and a Rust service is compiled by cargo, which the command
 * line runs as a child.
 */

//! The cargo runs.
//!
//! Every cargo call of the command line goes through [`Cargo`]:
//! `cargo metadata` for the project and for the generated workspace,
//! `cargo locate-project` for the root of a workspace,
//! `cargo generate-lockfile` for a project with no lock, `cargo build` for the
//! server package, the inspect package and the builds of a card for another
//! target, and `cargo check --all-targets` for the typecheck of `verify`.
//!
//! A build runs with `--message-format json-render-diagnostics`, so cargo
//! writes its JSON messages to standard output, which the command line reads
//! for the path of every executable and the package of every build script run,
//! and its rendered diagnostics to standard error beside its own status lines.
//! Under [`Output::Inherit`] that standard error reaches the terminal as it
//! comes, and under [`Output::Capture`] it is kept for the caller, which is
//! how `verify` folds a build into one line. Every JSON text cargo prints is
//! read with the reader of `aosabi-json`.

use std::{
    ffi::{
        OsStr,
        OsString, //
    },
    path::{
        Path,
        PathBuf, //
    },
    process::{
        Command,
        ExitStatus,
        Stdio, //
    }, //
};

use aosabi_build::source::Cfg;
use aosabi_json::{
    JsValue,
    parse, //
};

use crate::{
    args::LockMode,
    dotenv::DotEnv,
    env,
    report::CliError, //
};

/// The extension of the library file of a Rust crate, as rustc writes it.
const RLIB: &str = ".rlib";

/// The variable cargo reads the flags of every compiler call for the target
/// from, one flag apart from the next by the unit separator `0x1f`.
pub const ENCODED_RUSTFLAGS: &str = "CARGO_ENCODED_RUSTFLAGS";

/// The separator of the flags of [`ENCODED_RUSTFLAGS`].
const FLAG_SEPARATOR: char = '\u{1f}';

/// Where the compile runs that builds the binary a card ships, which decides
/// whether the flags of the environment of the command line reach it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compile {
    /// On the host, where cargo passes rustc the flags of
    /// `CARGO_ENCODED_RUSTFLAGS` of the environment, else those of
    /// `RUSTFLAGS`, and the flags of the build after them.
    Host,
    /// In the builder stage of the image of the card `docker`, whose
    /// `RUSTFLAGS` the container file writes with the flags of the build
    /// alone, so no flag of the environment of the command line reaches it.
    /// The build of that card on the host passes the flags of the stage as
    /// well, so its build step checks the parts the image compiles.
    BuilderStage,
}

/// Where the standard error of a run goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// To the standard error of the command line, as it comes.
    Inherit,
    /// Into the run, for the caller to fold or print.
    Capture,
}

/// One artifact cargo reported with a `compiler-artifact` message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    /// The id of the package the target belongs to.
    pub package_id: String,
    /// The name of the target.
    pub target: String,
    /// The kinds of the target, such as `bin` or `lib`.
    pub kinds: Vec<String>,
    /// The files the compilation wrote.
    pub filenames: Vec<PathBuf>,
    /// The executable of a binary target.
    pub executable: Option<PathBuf>,
}

/// What a `cargo build` or `cargo check` answered.
#[derive(Clone, Debug)]
pub struct CargoRun {
    /// The exit status of cargo.
    pub status: ExitStatus,
    /// Its standard output, the JSON messages.
    pub stdout: Vec<u8>,
    /// Its standard error under [`Output::Capture`], else empty.
    pub stderr: Vec<u8>,
    /// The path of every executable cargo reported, in its order.
    pub executables: Vec<PathBuf>,
    /// The count of build scripts cargo reported as run, fresh ones among
    /// them, which is every build script of the build.
    pub build_scripts_run: usize,
    /// The id of the package of every build script cargo reported as run, in
    /// its order.
    pub build_script_packages: Vec<String>,
    /// Every artifact cargo reported, in its order.
    pub artifacts: Vec<Artifact>,
}

/// What `rustc -vV` names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RustcFacts {
    /// The target triple of the host.
    pub host: String,
    /// The release, such as `1.95.0`.
    pub release: String,
    /// The commit the compiler was built from, empty when rustc names none,
    /// whose path `/rustc/<commit>` the prebuilt standard library names its
    /// source by.
    pub commit: String,
}

/// The cargo program the command line runs and the pairs every run adds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cargo {
    /// The program, `CARGO` or `cargo`.
    pub program: OsString,
    /// The pairs of `.env` the environment does not set.
    pub env: Vec<(OsString, OsString)>,
}

/// One `cargo build`.
#[derive(Clone, Debug)]
pub struct BuildRun<'a> {
    /// The working directory of cargo.
    pub cwd: &'a Path,
    /// The `--manifest-path`, `None` for the manifest of `cwd`.
    pub manifest: Option<&'a Path>,
    /// The `--target-dir`.
    pub target_dir: &'a Path,
    /// The `-p`, `None` for the package of the manifest.
    pub package: Option<&'a str>,
    /// The `--target`, `None` for the host.
    pub triple: Option<&'a str>,
    /// `--no-default-features`.
    pub no_default_features: bool,
    /// Every `--features` value, in order.
    pub features: &'a [&'a str],
    /// `--lib`, the library target alone.
    pub lib: bool,
    /// `--release`.
    pub release: bool,
    /// The lock flags.
    pub lock: LockMode,
    /// Where the standard error goes.
    pub output: Output,
    /// The pairs this run adds to the environment of cargo.
    pub env: &'a [(&'a str, &'a OsStr)],
}

impl Cargo {
    /// Returns the cargo of `CARGO`, else `cargo` from the path, with the
    /// pairs of `env_file`.
    #[must_use]
    pub fn from_env(env_file: &DotEnv) -> Self {
        let program = env::var(env_file, "CARGO")
            .filter(|program| !program.is_empty())
            .map_or_else(|| OsString::from("cargo"), OsString::from);
        Self {
            program,
            env: env_file.pairs.clone(),
        }
    }

    /// Returns the JSON of `cargo metadata --format-version 1` for the
    /// manifest `manifest`, or the one of `cwd`, under the lock flags of
    /// `lock`.
    ///
    /// # Errors
    ///
    /// Returns the failure of cargo, its first error line with its whole
    /// standard error as the detail, or the failure to read its answer.
    pub fn metadata(
        &self,
        cwd: &Path,
        manifest: Option<&Path>,
        lock: LockMode,
    ) -> Result<JsValue, CliError> {
        let mut command = self.command(cwd);
        command.args(["metadata", "--format-version", "1"]);
        add_manifest(&mut command, manifest);
        command.args(lock_flags(lock));
        let output = run_captured(&mut command, "cargo metadata")?;
        parse(&output.stdout)
            .map_err(|error| CliError::new(format!("cargo metadata answered no JSON: {error}")))
    }

    /// Returns the root directory of the workspace that `manifest`, or the
    /// manifest of `cwd`, belongs to, as `cargo locate-project --workspace`
    /// names it.
    ///
    /// # Errors
    ///
    /// Returns the failure of cargo, as for [`Cargo::metadata`].
    pub fn locate_workspace(
        &self,
        cwd: &Path,
        manifest: Option<&Path>,
    ) -> Result<PathBuf, CliError> {
        let mut command = self.command(cwd);
        command.args(["locate-project", "--workspace", "--message-format", "plain"]);
        add_manifest(&mut command, manifest);
        let output = run_captured(&mut command, "cargo locate-project")?;
        let text = String::from_utf8_lossy(&output.stdout);
        let root_manifest = PathBuf::from(text.trim_end_matches(['\n', '\r']));
        Ok(root_manifest
            .parent()
            .map_or_else(PathBuf::new, Path::to_path_buf))
    }

    /// Writes the lock of the manifest `manifest` with `cargo
    /// generate-lockfile`, with nothing fetched when `offline`.
    ///
    /// # Errors
    ///
    /// Returns the failure of cargo, as for [`Cargo::metadata`].
    pub fn generate_lockfile(
        &self,
        cwd: &Path,
        manifest: Option<&Path>,
        offline: bool,
    ) -> Result<(), CliError> {
        let mut command = self.command(cwd);
        command.arg("generate-lockfile");
        add_manifest(&mut command, manifest);
        if offline {
            command.arg("--offline");
        }
        run_captured(&mut command, "cargo generate-lockfile").map(|_| ())
    }

    /// Runs `cargo build` as `run` describes it and reads its JSON messages.
    ///
    /// The answer carries the exit status, and a build that fails is a run
    /// with a failed status and no error, so the caller decides what it
    /// prints.
    ///
    /// # Errors
    ///
    /// Returns a failure when cargo cannot be started.
    pub fn build(&self, run: &BuildRun<'_>) -> Result<CargoRun, CliError> {
        let mut command = self.command(run.cwd);
        command.args(["build", "--message-format", "json-render-diagnostics"]);
        add_manifest(&mut command, run.manifest);
        command.arg("--target-dir").arg(run.target_dir);
        if let Some(package) = run.package {
            command.args(["-p", package]);
        }
        if let Some(triple) = run.triple {
            command.args(["--target", triple]);
        }
        if run.lib {
            command.arg("--lib");
        }
        if run.no_default_features {
            command.arg("--no-default-features");
        }
        if !run.features.is_empty() {
            command.arg("--features").arg(run.features.join(","));
        }
        if run.release {
            command.arg("--release");
        }
        command.args(lock_flags(run.lock));
        command.envs(run.env.iter().map(|(key, value)| (*key, *value)));
        run_messages(&mut command, run.output, "cargo build")
    }

    /// Runs `cargo check --all-targets` in `cwd` and reads its JSON messages,
    /// as [`Cargo::build`] does.
    ///
    /// # Errors
    ///
    /// Returns a failure when cargo cannot be started.
    pub fn check_all_targets(&self, cwd: &Path, output: Output) -> Result<CargoRun, CliError> {
        let mut command = self.command(cwd);
        command.args([
            "check",
            "--all-targets",
            "--message-format",
            "json-render-diagnostics",
        ]);
        run_messages(&mut command, output, "cargo check")
    }

    /// Returns the rustc that belongs to this cargo: `RUSTC`, else the
    /// `rustc` beside the program of `CARGO` when it names a file, else
    /// `rustc` from the path.
    #[must_use]
    pub fn rustc(&self) -> OsString {
        if let Some(rustc) = self.var("RUSTC").filter(|rustc| !rustc.is_empty()) {
            return OsString::from(rustc);
        }
        let sibling = Path::new(&self.program)
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(|dir| dir.join("rustc"));
        match sibling {
            Some(path) if path.is_file() => path.into_os_string(),
            _ => OsString::from("rustc"),
        }
    }

    /// Returns the target triple of the host, as `rustc -vV` names it in
    /// `cwd`, or `None` when rustc does not answer.
    #[must_use]
    pub fn host_triple(&self, cwd: &Path) -> Option<String> {
        let output = self.rustc_command(cwd).arg("-vV").output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines()
            .find_map(|line| line.strip_prefix("host: "))
            .map(str::to_owned)
    }

    /// Returns what `rustc -vV` names in `cwd`, or `None` when rustc does not
    /// answer.
    #[must_use]
    pub fn rustc_facts(&self, cwd: &Path) -> Option<RustcFacts> {
        let output = self.rustc_command(cwd).arg("-vV").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let line = |key: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(key))
                .map_or_else(String::new, |value| value.trim().to_owned())
        };
        Some(RustcFacts {
            host: line("host:"),
            release: line("release:"),
            commit: line("commit-hash:"),
        })
    }

    /// Returns whether the toolchain of `cwd` carries the standard library of
    /// the target `triple`, a `libcore` in the directory `rustc --print
    /// target-libdir` names.
    #[must_use]
    pub fn has_target(&self, cwd: &Path, triple: &str) -> bool {
        let Ok(output) = self
            .rustc_command(cwd)
            .args(["--print", "target-libdir", "--target", triple])
            .output()
        else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let Ok(entries) = std::fs::read_dir(text.trim()) else {
            return false;
        };
        entries.filter_map(Result::ok).any(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("libcore-") && name.ends_with(RLIB))
        })
    }

    /// Returns the cargo home of this cargo, `CARGO_HOME`, else `.cargo` in
    /// `HOME`, the directory cargo unpacks the sources of a registry into.
    #[must_use]
    pub fn home(&self) -> Option<PathBuf> {
        if let Some(home) = self.var("CARGO_HOME").filter(|home| !home.is_empty()) {
            return Some(PathBuf::from(home));
        }
        self.var("HOME")
            .filter(|home| !home.is_empty())
            .map(|home| Path::new(&home).join(".cargo"))
    }

    /// Returns the value of [`ENCODED_RUSTFLAGS`] of a build of the compile
    /// `compile` that passes the flags `flags` of the build, those
    /// [`Cargo::compile_flags`] answers.
    ///
    /// Cargo reads `CARGO_ENCODED_RUSTFLAGS` before `RUSTFLAGS`, and either
    /// before the `rustflags` of its configuration, so a build that sets it
    /// passes rustc no flag of `RUSTFLAGS` and none of the configuration, and
    /// on the host the flags of the environment stand first, those of
    /// `CARGO_ENCODED_RUSTFLAGS` when it is set and else those of `RUSTFLAGS`
    /// split at white space.
    #[must_use]
    pub fn encoded_rustflags(&self, compile: Compile, flags: &[String]) -> OsString {
        OsString::from(
            self.compile_flags(compile, flags)
                .join(&FLAG_SEPARATOR.to_string()),
        )
    }

    /// Returns the flags the compile `compile` passes rustc after the flags
    /// of the profile: on the host the flags the environment of this cargo
    /// names and then `flags`, and in the builder stage of an image `flags`
    /// alone, which no flag of the environment reaches.
    #[must_use]
    pub fn compile_flags(&self, compile: Compile, flags: &[String]) -> Vec<String> {
        let mut all = match compile {
            Compile::Host => self.environment_flags(),
            Compile::BuilderStage => Vec::new(),
        };
        all.extend(flags.iter().cloned());
        all
    }

    /// Returns the active set of a build of the compile `compile` for the
    /// target `triple`, the host for `None`, which the rustc of this cargo
    /// prints in `cwd` with the flags `profile` cargo passes for the profile
    /// of the build and then the flags [`Cargo::compile_flags`] answers for
    /// the flags `flags` of the build, with the target information over those
    /// flags alone, as [`Cfg::from_rustc`] asks it, and with the profile
    /// [`Cargo::configured_profile`] finds. So the set of a card is the set
    /// rustc prints with the flags of the compile that builds the binary the
    /// card ships, the flags of the environment among them on the host and
    /// none of them for the card `docker`, whose builder stage takes none.
    ///
    /// The reader of the source path strips a plain crate over that set, so
    /// the glue and the schema of a card are those of the crate rustc compiles
    /// for it.
    ///
    /// # Errors
    ///
    /// Returns the failure of rustc with its first line of standard error.
    pub fn cfg(
        &self,
        cwd: &Path,
        triple: Option<&str>,
        compile: Compile,
        profile: &[String],
        flags: &[String],
    ) -> Result<Cfg, CliError> {
        let all = self.compile_flags(compile, flags);
        let mut cfg = Cfg::from_rustc(&|| self.rustc_command(cwd), triple, profile, &all)
            .map_err(CliError::new)?;
        if compile == Compile::BuilderStage {
            cfg = cfg.with_own_flags(String::from(
                "the builder stage of the image of the card `docker`",
            ));
        }
        Ok(match self.configured_profile(cwd) {
            Some(origin) => cfg.with_configured_profile(origin),
            None => cfg,
        })
    }

    /// Returns where the configuration of this cargo names a profile for a
    /// build in `cwd`, a variable or a file, or `None` where the build takes
    /// the profiles of its workspace alone.
    ///
    /// Cargo merges into the profiles of a workspace the variables
    /// `CARGO_PROFILE_<name>_<key>` of its environment and the tables and the
    /// keys of `profile` of the configuration files it reads for a build in
    /// `cwd`, `.cargo/config.toml` or `.cargo/config` in that directory and in
    /// each directory above it and `config.toml` or `config` in its cargo
    /// home. The command line reads neither, so a variable of the key
    /// `OPT_LEVEL`, `DEBUG_ASSERTIONS` or `PANIC` and a file that names
    /// `profile` at the start of a header or of a key count, the first in that
    /// order named, which refuses beyond need at most.
    #[must_use]
    pub fn configured_profile(&self, cwd: &Path) -> Option<String> {
        let mut names: Vec<String> = std::env::vars_os()
            .map(|(name, _)| name)
            .chain(self.env.iter().map(|(name, _)| name.clone()))
            .filter_map(|name| name.into_string().ok())
            .filter(|name| {
                name.starts_with("CARGO_PROFILE_")
                    && ["_OPT_LEVEL", "_DEBUG_ASSERTIONS", "_PANIC"]
                        .iter()
                        .any(|key| name.ends_with(key))
            })
            .collect();
        names.sort();
        if let Some(name) = names.first() {
            return Some(format!("the variable `{name}`"));
        }
        let start = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
        let mut files: Vec<PathBuf> = Vec::new();
        for dir in start.ancestors() {
            files.push(dir.join(".cargo").join("config"));
            files.push(dir.join(".cargo").join("config.toml"));
        }
        if let Some(home) = self.home() {
            files.push(home.join("config"));
            files.push(home.join("config.toml"));
        }
        files
            .into_iter()
            .find(|file| std::fs::read_to_string(file).is_ok_and(|text| names_profile(&text)))
            .map(|file| format!("the file `{}`", file.display()))
    }

    /// Returns the flags the environment of this cargo names for rustc, those
    /// of `CARGO_ENCODED_RUSTFLAGS` when it is set and else those of
    /// `RUSTFLAGS` split at white space.
    fn environment_flags(&self) -> Vec<String> {
        match self.var(ENCODED_RUSTFLAGS) {
            Some(encoded) => encoded
                .split(FLAG_SEPARATOR)
                .filter(|flag| !flag.is_empty())
                .map(str::to_owned)
                .collect(),
            None => self
                .var("RUSTFLAGS")
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
        }
    }

    /// Returns the variable `name` as a child of this cargo sees it.
    fn var(&self, name: &str) -> Option<String> {
        env::var(
            &DotEnv {
                pairs: self.env.clone(),
            },
            name,
        )
    }

    /// Returns a command of this cargo in `cwd` with the pairs of `.env` and
    /// no standard input.
    fn command(&self, cwd: &Path) -> Command {
        let mut command = Command::new(&self.program);
        command
            .current_dir(cwd)
            .envs(self.env.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::null());
        command
    }

    /// Returns a command of the rustc of this cargo in `cwd`.
    fn rustc_command(&self, cwd: &Path) -> Command {
        let mut command = Command::new(self.rustc());
        command
            .current_dir(cwd)
            .envs(self.env.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::null());
        command
    }
}

/// Returns whether the text `text` of a configuration file of cargo names
/// `profile` at the start of a line, as the first key of a header such as
/// `[profile.release]` or of a key such as `profile.release.opt-level = 1`.
fn names_profile(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        let key = line
            .strip_prefix('[')
            .map_or(line, |header| header.trim_start_matches('[').trim_start());
        key.strip_prefix("profile")
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '=', ']', ' ', '\t']))
    })
}

/// Returns the cargo flags of `lock`.
#[must_use]
pub fn lock_flags(lock: LockMode) -> &'static [&'static str] {
    match lock {
        LockMode::Normal => &[],
        LockMode::Locked => &["--locked"],
        LockMode::Frozen => &["--frozen"],
        LockMode::Offline => &["--offline"],
    }
}

/// Returns whether `lock` forbids the network, `--frozen` or `--offline`.
#[must_use]
pub fn is_offline(lock: LockMode) -> bool {
    matches!(lock, LockMode::Frozen | LockMode::Offline)
}

/// Returns the member `key` of the JSON object `value`.
#[must_use]
pub fn field<'a>(value: &'a JsValue, key: &str) -> Option<&'a JsValue> {
    match value {
        JsValue::Object(object) => object.get(key),
        _ => None,
    }
}

/// Returns the member `key` of the JSON object `value` when it is a string.
#[must_use]
pub fn text<'a>(value: &'a JsValue, key: &str) -> Option<&'a str> {
    match field(value, key) {
        Some(JsValue::String(text)) => text.as_str(),
        _ => None,
    }
}

/// Returns the elements of the member `key` of the JSON object `value`, none
/// when it is no array.
#[must_use]
pub fn items<'a>(value: &'a JsValue, key: &str) -> &'a [JsValue] {
    match field(value, key) {
        Some(JsValue::Array(items)) => items,
        _ => &[],
    }
}

/// Returns the name of the package of the id `id` and whether its source is a
/// path, as cargo writes an id in its JSON messages,
/// `<source>#<name>@<version>`, or `<source>#<version>` where the name is the
/// last segment of the path of the source. The older form
/// `<name> <version> (<source>)` reads as well.
#[must_use]
pub fn package_of(id: &str) -> (&str, bool) {
    let Some((source, fragment)) = id.rsplit_once('#') else {
        let name = id.split(' ').next().unwrap_or_default();
        return (name, id.contains("(path+"));
    };
    let name = fragment.split_once('@').map_or_else(
        || {
            let url = source.split('?').next().unwrap_or_default();
            url.rsplit('/').next().unwrap_or_default()
        },
        |(name, _)| name,
    );
    (name, source.starts_with("path+"))
}

/// Returns the failure of the cargo call `what`, whose standard error is
/// `stderr`: the first error line cargo printed as the message, or that the
/// call failed, and the whole standard error as the detail.
#[must_use]
pub fn failure(what: &str, stderr: &[u8]) -> CliError {
    let text = String::from_utf8_lossy(stderr);
    let message = text
        .lines()
        .find_map(|line| line.strip_prefix("error: "))
        .map_or_else(|| format!("{what} failed"), str::to_owned);
    CliError::new(message).with_detail(text.trim_end().to_owned())
}

/// Adds `--manifest-path` when `manifest` names one.
fn add_manifest(command: &mut Command, manifest: Option<&Path>) {
    if let Some(manifest) = manifest {
        command.arg("--manifest-path").arg(manifest);
    }
}

/// Runs `command` with both streams captured and answers its output, or the
/// failure of the call `what`.
fn run_captured(command: &mut Command, what: &str) -> Result<std::process::Output, CliError> {
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| CliError::new(format!("{what} could not start: {error}")))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(failure(what, &output.stderr))
    }
}

/// Runs `command` with its standard output captured and its standard error
/// as `output` says, and reads the JSON messages of its standard output.
fn run_messages(command: &mut Command, output: Output, what: &str) -> Result<CargoRun, CliError> {
    command.stdout(Stdio::piped());
    command.stderr(match output {
        Output::Inherit => Stdio::inherit(),
        Output::Capture => Stdio::piped(),
    });
    let done = command
        .output()
        .map_err(|error| CliError::new(format!("{what} could not start: {error}")))?;
    let mut run = CargoRun {
        status: done.status,
        stdout: Vec::new(),
        stderr: done.stderr,
        executables: Vec::new(),
        build_scripts_run: 0,
        build_script_packages: Vec::new(),
        artifacts: Vec::new(),
    };
    for line in done.stdout.split(|byte| *byte == b'\n') {
        let Ok(message) = parse(line) else {
            continue;
        };
        match text(&message, "reason") {
            Some("compiler-artifact") => {
                let artifact = artifact_of(&message);
                if let Some(executable) = &artifact.executable {
                    run.executables.push(executable.clone());
                }
                run.artifacts.push(artifact);
            }
            Some("build-script-executed") => {
                run.build_scripts_run += 1;
                run.build_script_packages
                    .push(text(&message, "package_id").unwrap_or_default().to_owned());
            }
            _ => {}
        }
    }
    run.stdout = done.stdout;
    Ok(run)
}

/// Returns the artifact a `compiler-artifact` message names.
fn artifact_of(message: &JsValue) -> Artifact {
    let target = field(message, "target");
    let strings = |value: &JsValue, key: &str| -> Vec<String> {
        items(value, key)
            .iter()
            .filter_map(|item| match item {
                JsValue::String(text) => text.as_str().map(str::to_owned),
                _ => None,
            })
            .collect()
    };
    Artifact {
        package_id: text(message, "package_id").unwrap_or_default().to_owned(),
        target: target
            .and_then(|target| text(target, "name"))
            .unwrap_or_default()
            .to_owned(),
        kinds: target
            .map(|target| strings(target, "kind"))
            .unwrap_or_default(),
        filenames: strings(message, "filenames")
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        executable: text(message, "executable").map(PathBuf::from),
    }
}
