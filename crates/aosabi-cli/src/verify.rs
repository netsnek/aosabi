// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the command verify, its four checks and its verdict.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/verify.ts and
 * packages/pylon/src/cli/index.ts, Apache-2.0. Changed by the Aosabi authors.
 */

//! The command `verify`.
//!
//! [`verify_app`] runs four checks in order with the output of every child
//! captured and nothing printed until the verdict:
//!
//! 1. `build` runs [`build::run`] in [`Flavour::Verify`] for the `server`
//!    card, the build step included, under `target/aosabi-verify/`, which a
//!    guard removes on every path out of the check. It passes with
//!    `Server binary + schema generated.` and fails with the first error
//!    cargo rendered, the standard error of the failed build script, or the
//!    output of the failed build step, through [`one_line`].
//! 2. `typecheck` runs `cargo check --all-targets` in the project and passes
//!    with `cargo check clean.`, fails with [`last_lines`] of its output and
//!    12, or is `skip` with `No Cargo.toml or cargo, skipped.` when the
//!    project has no manifest or cargo does not start. It is not clippy,
//!    because a lint has no counterpart in `tsc --noEmit`.
//! 3. `migrations` passes when the build passed, with the detail of a
//!    project with no model, `Schema captured; no uncaptured model changes.`.
//! 4. The harvest of the model is not run, because no output of the command line prints
//!    it.
//!
//! The names, the statuses, the marks and the verdict line are fixed texts.
//! The details name Aosabi's own tools, and a comparison reports them side by
//! side and never compares them. The verdict is `fail` on any failed check, else `review` on any warning, else
//! `pass`. [`command`] writes `JSON.stringify({verdict, checks}, null, 2)`
//! and a newline under `--json`, else `  <mark> <name>: <detail>` per check
//! with `✓`, `✗`, `!` or `·`, a blank line and `verdict: <VERDICT>`, and
//! exits 1 on `fail` and 0 otherwise.

use std::{
    fmt::Write as _,
    fs,
    io::{
        self,
        Write, //
    },
    path::PathBuf,
    process::ExitCode,
};

use aosabi_json::{
    JsObject,
    JsValue,
    to_string_with_gap, //
};

use crate::{
    args::{
        BuildArgs,
        Global,
        LockMode,
        VerifyArgs, //
    },
    build,
    cards::{
        self,
        DEFAULT_CARD, //
    },
    cargo::{
        Cargo,
        Output, //
    },
    codes,
    dotenv::{
        self,
        DotEnv, //
    },
    env,
    generate::{
        self,
        Flavour, //
    },
    inspect,
    project::{
        self,
        Project, //
    },
    report::{
        self,
        CliError, //
    },
};

/// The longest detail of a failed build, in UTF-16 code units.
pub const ONE_LINE_LIMIT: usize = 500;

/// The longest detail of a failed typecheck, in UTF-16 code units.
pub const LAST_LINES_LIMIT: usize = 2000;

/// The lines a failed typecheck keeps.
pub const TYPECHECK_LINES: usize = 12;

/// The detail of a build that passed.
const BUILD_PASS: &str = "Server binary + schema generated.";

/// The detail of a typecheck that passed.
const TYPECHECK_PASS: &str = "cargo check clean.";

/// The detail of a typecheck that could not run.
const TYPECHECK_SKIP: &str = "No Cargo.toml or cargo, skipped.";

/// The detail of the migrations of a project with no model.
const MIGRATIONS_PASS: &str = "Schema captured; no uncaptured model changes.";

/// The gap of the JSON form, the one of `JSON.stringify(value, null, 2)`.
const GAP: &str = "  ";

/// The status of one check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    /// The check holds.
    Pass,
    /// The check failed, which fails the verdict.
    Fail,
    /// The check asks for a follow up, which makes the verdict `review`.
    Warn,
    /// The check could not run.
    Skip,
}

impl CheckStatus {
    /// Returns the name the JSON form writes.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Warn => "warn",
            Self::Skip => "skip",
        }
    }

    /// Returns the mark the plain form writes.
    #[must_use]
    pub const fn mark(self) -> &'static str {
        match self {
            Self::Pass => "\u{2713}",
            Self::Fail => "\u{2717}",
            Self::Warn => "!",
            Self::Skip => "\u{b7}",
        }
    }
}

/// One check of [`verify_app`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyCheck {
    /// The name of the check.
    pub name: &'static str,
    /// Its status.
    pub status: CheckStatus,
    /// What it found, in one line or the last lines of a tool.
    pub detail: String,
}

/// The verdict of [`verify_app`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Every check passed or was skipped.
    Pass,
    /// No check failed and one asks for a follow up.
    Review,
    /// A check failed.
    Fail,
}

impl Verdict {
    /// Returns the name the JSON form writes, the plain form in upper case.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Review => "review",
            Self::Fail => "fail",
        }
    }
}

/// The verdict and the checks it folds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyResult {
    /// The verdict.
    pub verdict: Verdict,
    /// The checks in the order they ran.
    pub checks: Vec<VerifyCheck>,
}

/// Runs `verify` with `args` under `global` and the pairs `env_file` of
/// `.env`, and answers the exit code, 1 on a verdict of `fail`.
#[must_use]
pub fn command(args: &VerifyArgs, global: &Global, env_file: &DotEnv) -> ExitCode {
    let verbose = global.verbose || env::debug(env_file);
    let result = match load(args, global, env_file) {
        Ok((project, cargo)) => verify_app(&project, args.lock, &cargo),
        Err(error) => {
            report::report_failure(&error, verbose);
            return ExitCode::FAILURE;
        }
    };
    let text = if args.json {
        json(&result)
    } else {
        plain(&result)
    };
    let mut out = io::stdout().lock();
    if out
        .write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .is_err()
    {
        return ExitCode::FAILURE;
    }
    if result.verdict == Verdict::Fail {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Runs the four checks on `project` under the lock flags of `lock` and
/// answers the verdict.
#[must_use]
pub fn verify_app(project: &Project, lock: LockMode, cargo: &Cargo) -> VerifyResult {
    let mut checks = Vec::with_capacity(3);
    let built = build_check(project, lock, cargo);
    let passed = built.status == CheckStatus::Pass;
    checks.push(built);
    checks.push(typecheck(project, cargo));
    if passed {
        checks.push(VerifyCheck {
            name: "migrations",
            status: CheckStatus::Pass,
            detail: String::from(MIGRATIONS_PASS),
        });
    }
    let verdict = if checks.iter().any(|check| check.status == CheckStatus::Fail) {
        Verdict::Fail
    } else if checks.iter().any(|check| check.status == CheckStatus::Warn) {
        Verdict::Review
    } else {
        Verdict::Pass
    };
    VerifyResult { verdict, checks }
}

/// Returns `text` with every run of the white space of ECMA-262 folded to one
/// space, trimmed, and cut at [`ONE_LINE_LIMIT`] UTF-16 code units, which is
/// `s.replace(/\s+/g, ' ').trim().slice(0, 500)`.
///
/// A cut that falls inside a surrogate pair drops the whole character, where
/// the reference keeps the lone high surrogate.
#[must_use]
pub fn one_line(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if dotenv::is_space(c) {
            space = true;
        } else {
            if space && !folded.is_empty() {
                folded.push(' ');
            }
            space = false;
            folded.push(c);
        }
    }
    cut_utf16(&folded, ONE_LINE_LIMIT)
}

/// Returns the last `n` lines of `text` after trimming it, joined by a line
/// break and cut at [`LAST_LINES_LIMIT`] UTF-16 code units, which is
/// `s.trim().split('\n').slice(-n).join('\n').slice(0, 2000)`. An `n` of 0
/// keeps every line, as `slice(-0)` does.
#[must_use]
pub fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.trim_matches(dotenv::is_space).split('\n').collect();
    let start = if n == 0 {
        0
    } else {
        lines.len().saturating_sub(n)
    };
    cut_utf16(&lines[start..].join("\n"), LAST_LINES_LIMIT)
}

/// Returns the longest prefix of `text` of at most `limit` UTF-16 code
/// units that ends on a character boundary.
fn cut_utf16(text: &str, limit: usize) -> String {
    let mut units = 0;
    for (at, c) in text.char_indices() {
        units += c.len_utf16();
        if units > limit {
            return text[..at].to_owned();
        }
    }
    text.to_owned()
}

/// Reads the project of `verify`, refusing an entry that is not its library
/// root.
fn load(
    args: &VerifyArgs,
    global: &Global,
    env_file: &DotEnv,
) -> Result<(Project, Cargo), CliError> {
    let cwd = std::env::current_dir()
        .map_err(|error| CliError::new(format!("the working directory is unreadable: {error}")))?;
    let cargo = Cargo::from_env(env_file);
    let project = project::load(&cwd, global.manifest_path.as_deref(), args.lock, &cargo)?;
    if let Some(entry) = &args.entry {
        inspect::check_entry(&cwd, entry, &project)?;
    }
    Ok((project, cargo))
}

/// Removes the workspace of the verify flavour when it drops, so every path
/// out of the build check leaves no `target/aosabi-verify/` behind.
struct VerifyDir(PathBuf);

impl Drop for VerifyDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs check 1, the build in the verify flavour.
fn build_check(project: &Project, lock: LockMode, cargo: &Cargo) -> VerifyCheck {
    let _guard = VerifyDir(generate::root(project, Flavour::Verify));
    let outcome = cards::by_name(DEFAULT_CARD)
        .ok_or_else(|| codes::no_builtin_card(DEFAULT_CARD))
        .and_then(|card| {
            let args = BuildArgs {
                device: None,
                include: Vec::new(),
                release: false,
                lock,
            };
            build::run(
                project,
                card,
                &args,
                Flavour::Verify,
                Output::Capture,
                cargo,
            )
        });
    match outcome {
        Ok(_) => VerifyCheck {
            name: "build",
            status: CheckStatus::Pass,
            detail: String::from(BUILD_PASS),
        },
        Err(error) => VerifyCheck {
            name: "build",
            status: CheckStatus::Fail,
            detail: one_line(&failure_text(&error)),
        },
    }
}

/// Returns the text of a failed build: the standard error of a failed build
/// script, else the first error cargo rendered, else the output of the build
/// step or the message of the failure.
fn failure_text(error: &CliError) -> String {
    let Some(detail) = &error.detail else {
        return match error.code {
            Some(code) => format!("{code} {}", error.message),
            None => error.message.clone(),
        };
    };
    if let Some(stderr) = build_script_stderr(detail) {
        return stderr;
    }
    if let Some(rendered) = first_error(detail) {
        return rendered;
    }
    detail.clone()
}

/// Returns the standard error of the build script cargo reports as failed in
/// `stderr`, the lines under `--- stderr` with their indent of two spaces
/// taken off, when it holds any.
fn build_script_stderr(stderr: &str) -> Option<String> {
    let after = stderr
        .split_once("failed to run custom build command")?
        .1
        .split_once("--- stderr\n")?
        .1;
    let text: Vec<&str> = after
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("  "))
        .map(|line| line.strip_prefix("  ").unwrap_or(line))
        .collect();
    let text = text.join("\n");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Returns the first error block cargo rendered in `stderr`, from its line
/// `error` up to the next empty line.
fn first_error(stderr: &str) -> Option<String> {
    let mut lines = stderr.lines().skip_while(|line| !line.starts_with("error"));
    let first = lines.next()?;
    let mut block = vec![first];
    block.extend(lines.take_while(|line| !line.trim().is_empty()));
    Some(block.join("\n"))
}

/// Runs check 2, `cargo check --all-targets` in the project.
fn typecheck(project: &Project, cargo: &Cargo) -> VerifyCheck {
    let skip = VerifyCheck {
        name: "typecheck",
        status: CheckStatus::Skip,
        detail: String::from(TYPECHECK_SKIP),
    };
    if !project.manifest.is_file() {
        return skip;
    }
    match cargo.check_all_targets(&project.root, Output::Capture) {
        Ok(run) if run.status.success() => VerifyCheck {
            name: "typecheck",
            status: CheckStatus::Pass,
            detail: String::from(TYPECHECK_PASS),
        },
        Ok(run) => VerifyCheck {
            name: "typecheck",
            status: CheckStatus::Fail,
            detail: last_lines(&String::from_utf8_lossy(&run.stderr), TYPECHECK_LINES),
        },
        Err(_) => skip,
    }
}

/// Returns the JSON form of `result` with a newline.
fn json(result: &VerifyResult) -> String {
    let checks: Vec<JsValue> = result
        .checks
        .iter()
        .map(|check| {
            let mut entry = JsObject::with_capacity(3);
            entry.insert("name", check.name);
            entry.insert("status", check.status.name());
            entry.insert("detail", check.detail.as_str());
            JsValue::Object(entry)
        })
        .collect();
    let mut object = JsObject::with_capacity(2);
    object.insert("verdict", result.verdict.name());
    object.insert("checks", checks);
    let mut text = to_string_with_gap(&JsValue::Object(object), GAP).unwrap_or_default();
    text.push('\n');
    text
}

/// Returns the plain form of `result`.
fn plain(result: &VerifyResult) -> String {
    let mut text = String::new();
    for check in &result.checks {
        let _ = writeln!(
            text,
            "  {} {}: {}",
            check.status.mark(),
            check.name,
            check.detail
        );
    }
    let _ = write!(
        text,
        "\nverdict: {}\n",
        result.verdict.name().to_ascii_uppercase()
    );
    text
}
