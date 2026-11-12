// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the command new, the scaffold of a service.
 *
 * Each file of the directory names the part it translates.
 */

//! The command `new`.
//!
//! In the reference, `create-pylon` prompts for what is missing, names the
//! project after the last component of the target or of the working directory
//! for `.`, refuses a feature the runtime lacks with
//! `Invalid feature selected: <feature>`, confirms a directory that is not
//! empty, writes the files and installs. [`command`] names the project after
//! the last component of the target or of the working directory for `.`, writes
//! the project and runs nothing, no cargo and no install, and prompts for
//! nothing. Its refusals stand in this order, its own first and then those of
//! the reference in the order of the reference, the features first and the
//! directory last, and each comes before the command writes a file:
//!
//! - `AO-C0405` for a command that names no checkout of Aosabi through
//!   `--aosabi-path`. Aosabi is released through its repository alone, so
//!   the scaffold names the facade by path from a checkout.
//! - `AO-C0404` for a feature the runtime lacks. The runtime `native` carries
//!   `auth` and `pages`, and `workers` carries `auth`.
//! - `AO-C0402` for a target that gives no package name cargo takes.
//! - `AO-C0401` for a target that exists and is not empty, unless `--force`.
//!
//! The hints of the last three name the checkout the command was given.
//!
//! The scaffold is a cargo workspace of its own with the one dependency on the
//! facade, its index `src/lib.rs`, the crate root by cargo's default, and
//! `.gitignore`, with the publish workflow on `native`, `wrangler.toml` on
//! `workers`, and under `--features pages` the page `public/index.html`. The
//! features are calls of the chain of the entry, [`templates::lib_rs`]: under
//! `--features auth` on either runtime the chain registers the identity over
//! `zitadel_auth` and the login routes of `zitadel_login`, and the manifest
//! names the facade with its feature `auth-zitadel`, which carries both, and
//! under `--features pages` the chain registers the file server of `public/`
//! on the cards that serve sockets. The manifest names the facade a second
//! time for a Unix target with the features of the cards that serve on Linux,
//! `serve` among them for the file server, so its lock pins the packages of
//! the engine that the check of the derived lock asks for, and a Worker build
//! links no engine. The dependency names the version of the command line, and
//! `--aosabi-path <dir>` names `<dir>/crates/aosabi` by path in its place,
//! written relative to the target, so a scaffold inside a checkout builds
//! against it there and in the context of an image, and the publish workflow
//! checks out the submodules of the repository and installs the command line
//! from `<dir>/crates/aosabi-cli` of the same checkout by the same relative
//! path, which must hold both packages. A runner checks out the repository of
//! the service alone, so the workflow runs where the checkout lies inside the
//! target, as a submodule of the service for example, and where it lies
//! outside, the command says so in a note. The scaffold names no build side, so
//! the generated packages of its build name `aosabi-build` from where the
//! facade comes, and the check of the derived lock passes the build side as
//! path packages beside a path facade and by the pin of the facade beside a
//! facade of a repository. The command prints every file it wrote and
//! `cd <target> && cargo aosabi dev` to standard error, and between them the
//! note [`templates::WORKFLOW_NOTE`] for a checkout outside the target on
//! `native` and under `--features auth` the note [`templates::AUTH_NOTE`]: the
//! issuer is a placeholder, the key comes from `AUTH_KEY` or `key.json`, the
//! ignore file keeps `key.json` out of git and a changed key takes a restart. A
//! checkout inside the target is a workspace of its own, which the workspace
//! table of the scaffold excludes, since cargo takes every path package inside
//! the folder of a workspace as its member.

use std::{
    fmt::Write as _,
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
    process::ExitCode,
};

use crate::{
    args::{
        Feature,
        NewArgs,
        Runtime, //
    },
    codes,
    generate::{
        self,
        LINUX_FEATURES,
        toml_string, //
    },
    report::{
        self,
        CliError, //
    },
};

pub mod templates;

/// The features each runtime carries.
const NATIVE_FEATURES: [Feature; 2] = [Feature::Auth, Feature::Pages];

/// The features the runtime `workers` carries.
const WORKERS_FEATURES: [Feature; 1] = [Feature::Auth];

/// The Rust keywords, strict and reserved, no package of which names a
/// library crate, `gen` among them for the edition 2024 of a scaffold.
const KEYWORDS: [&str; 52] = [
    "Self", "abstract", "as", "async", "await", "become", "box", "break", "const", "continue",
    "crate", "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if",
    "impl", "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub",
    "ref", "return", "self", "static", "struct", "super", "trait", "true", "try", "type", "typeof",
    "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// The directory names cargo writes into a target directory, which no package
/// may take.
const ARTIFACT_NAMES: [&str; 4] = ["deps", "examples", "build", "incremental"];

/// Runs `new` with `args` and answers the exit code.
#[must_use]
pub fn command(args: &NewArgs) -> ExitCode {
    let created = std::env::current_dir()
        .map_err(|error| CliError::new(format!("the working directory is unreadable: {error}")))
        .and_then(|cwd| create_directory(args, &cwd));
    match created {
        Ok(created) => {
            let mut text = String::new();
            for file in &created.files {
                text.push_str(&file.display().to_string());
                text.push('\n');
            }
            let mut notes = String::new();
            if created.checkout_outside {
                notes.push_str(templates::WORKFLOW_NOTE);
            }
            if args.features.contains(&Feature::Auth) {
                notes.push_str(templates::AUTH_NOTE);
            }
            if !notes.is_empty() {
                text.push('\n');
                text.push_str(&notes);
            }
            let _ = write!(text, "\ncd {} && cargo aosabi dev\n", args.target.display());
            let _ = io::stderr().lock().write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Err(error) => {
            report::report_failure(&error, false);
            ExitCode::FAILURE
        }
    }
}

/// What [`create_directory`] wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Created {
    /// Every file it wrote, as the target and its path below it.
    pub files: Vec<PathBuf>,
    /// Whether it wrote a publish workflow that names a checkout of Aosabi
    /// outside the target, which a runner of the workflow does not hold.
    pub checkout_outside: bool,
}

/// Writes the scaffold `args` asks for, its target resolved against `cwd`,
/// and answers every file it wrote and whether its publish workflow names a
/// checkout outside the target.
///
/// # Errors
///
/// Returns `AO-C0405`, `AO-C0404`, `AO-C0402` or `AO-C0401` as the module
/// says, a failure for an `--aosabi-path` that holds no facade or no command
/// line, or whose path the publish workflow cannot carry, and the failure of
/// a write.
pub fn create_directory(args: &NewArgs, cwd: &Path) -> Result<Created, CliError> {
    let Some(checkout) = &args.aosabi_path else {
        return Err(codes::no_checkout(args));
    };
    check_features(args, checkout)?;
    let target = resolve(cwd, &args.target);
    let name = target
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    if let Err(rule) = check_name(&name) {
        return Err(codes::bad_package_name(&name, rule, args, checkout));
    }
    if target.exists() {
        let entries = fs::read_dir(&target).map_err(|error| io_failure(&target, &error))?;
        if !args.force && entries.count() > 0 {
            return Err(codes::target_not_empty(args, checkout));
        }
    }
    let (checkout, inside) = checkout_path(checkout, &target)?;
    let files = files(args, &checkout, inside, &name)?;
    let mut written = Vec::with_capacity(files.len());
    for (path, text) in files {
        let full = target.join(path);
        if let Some(dir) = full.parent() {
            fs::create_dir_all(dir).map_err(|error| io_failure(dir, &error))?;
        }
        fs::write(&full, text).map_err(|error| io_failure(&full, &error))?;
        written.push(args.target.join(path));
    }
    Ok(Created {
        files: written,
        checkout_outside: args.runtime == Runtime::Native && !inside,
    })
}

/// Refuses a feature the runtime of `args` lacks, `AO-C0404`, with the
/// checkout `checkout` in its hint.
fn check_features(args: &NewArgs, checkout: &Path) -> Result<(), CliError> {
    let supported: &[Feature] = match args.runtime {
        Runtime::Native => &NATIVE_FEATURES,
        Runtime::Workers => &WORKERS_FEATURES,
    };
    if let Some(feature) = args
        .features
        .iter()
        .find(|feature| !supported.contains(feature))
    {
        return Err(codes::invalid_feature(
            feature_name(*feature),
            args,
            checkout,
        ));
    }
    Ok(())
}

/// Returns the name of `feature` as `--features` spells it.
#[must_use]
pub const fn feature_name(feature: Feature) -> &'static str {
    match feature {
        Feature::Auth => "auth",
        Feature::Pages => "pages",
    }
}

/// Returns the rule cargo applies that the package name `name` breaks.
///
/// The rules are those cargo applies to a new package, read for ASCII:
/// a name is not empty, starts with a letter or `_`, holds letters, digits,
/// `-` and `_`, is no keyword, is not `test` and takes no directory name of
/// a target directory. A name with a character outside ASCII, which cargo
/// takes with a warning, is refused as well, and so are the names of the
/// facade, which the service depends on, and of its build side, which the
/// generated packages of the service depend on beside the service.
fn check_name(name: &str) -> Result<(), &'static str> {
    let Some(first) = name.chars().next() else {
        return Err("the package name cannot be empty");
    };
    if first.is_ascii_digit() {
        return Err("the name cannot start with a digit");
    }
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err("the first character must be an ASCII letter or `_`");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("the characters must be ASCII letters, digits, `-` or `_`");
    }
    if KEYWORDS.contains(&name) || KEYWORDS.contains(&name.replace('-', "_").as_str()) {
        return Err("the name is a Rust keyword, which no library crate takes");
    }
    if name == "test" {
        return Err("the name `test` conflicts with Rust's built-in test library");
    }
    if ARTIFACT_NAMES.contains(&name) {
        return Err("the name conflicts with a directory cargo writes into the target directory");
    }
    if matches!(name, "aosabi" | "aosabi-build" | "aosabi_build") {
        return Err(
            "the name is the name of the facade or of its build side, which the service and its generated packages depend on",
        );
    }
    Ok(())
}

/// Returns `target` resolved against `cwd`, its existing part with its links
/// resolved and every `.` and `..` of the rest taken lexically.
fn resolve(cwd: &Path, target: &Path) -> PathBuf {
    let mut full = PathBuf::new();
    for part in cwd.join(target).components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                full.pop();
            }
            part => full.push(part),
        }
    }
    let existing = full
        .ancestors()
        .find(|dir| dir.exists())
        .map(Path::to_path_buf);
    match existing.and_then(|dir| fs::canonicalize(&dir).ok().map(|real| (dir, real))) {
        Some((dir, real)) => real.join(full.strip_prefix(&dir).unwrap_or(Path::new(""))),
        None => full,
    }
}

/// Returns every file of the scaffold `args` asks for, the service named
/// `name` from the checkout `checkout`, written relative to the target, which
/// lies inside the target where `inside` holds, as its path below the target
/// and its text, in the order the scaffold writes them.
fn files(
    args: &NewArgs,
    checkout: &Path,
    inside: bool,
    name: &str,
) -> Result<Vec<(&'static str, String)>, CliError> {
    let pages = args.features.contains(&Feature::Pages);
    let auth = args.features.contains(&Feature::Auth);
    let aosabi = Spec::path(&generate::utf8(&checkout.join("crates").join("aosabi"))?);
    let facade = if auth {
        format!(
            "{{ {}, features = [{}] }}",
            aosabi.inner,
            toml_string(templates::AUTH_FEATURE)
        )
    } else {
        aosabi.value.clone()
    };
    let features: Vec<String> = LINUX_FEATURES
        .iter()
        .map(|feature| toml_string(feature))
        .collect();
    let unix = format!(
        "{{ {}, features = [{}] }}",
        aosabi.inner,
        features.join(", ")
    );
    let image = name.to_ascii_lowercase();
    // A checkout below the target leaves the workspace of the scaffold, and
    // a checkout that is the target holds no package of its own to leave.
    let excluded = if inside && checkout != Path::new(".") {
        Some(generate::utf8(checkout)?)
    } else {
        None
    };
    let mut files: Vec<(&'static str, String)> = vec![
        (
            "Cargo.toml",
            templates::cargo_toml(name, &facade, &unix, excluded.as_deref()),
        ),
        ("src/lib.rs", templates::lib_rs(auth, pages).to_owned()),
    ];
    match args.runtime {
        Runtime::Native => {
            let cli = generate::utf8(&checkout.join("crates").join("aosabi-cli"))?;
            // A reader of YAML 1.1 takes the line and the paragraph separator
            // as line breaks, as it takes a line feed, so either ends the
            // literal block of the step.
            if cli
                .chars()
                .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
                || cli.contains("${{")
            {
                return Err(CliError::new(format!(
                    "the path {cli} of the command line holds a control character, a line or paragraph separator or the marker ${{{{ of an expression, which the publish workflow cannot carry"
                )));
            }
            files.push((".gitignore", templates::GITIGNORE.to_owned()));
            files.push((
                ".github/workflows/publish.yml",
                templates::publish_yml(&image, &cli),
            ));
        }
        Runtime::Workers => {
            files.push((".gitignore", templates::GITIGNORE_WORKERS.to_owned()));
            files.push(("wrangler.toml", templates::wrangler_toml(name)));
        }
    }
    if pages {
        files.push(("public/index.html", templates::public_index_html(name)));
    }
    Ok(files)
}

/// The dependency of a scaffold on the facade, as the value of its key and as
/// the pairs inside an inline table.
struct Spec {
    /// The value of the key, an inline table.
    value: String,
    /// The pairs that name the source inside an inline table.
    inner: String,
}

impl Spec {
    /// Returns the dependency on the path `path`.
    fn path(path: &str) -> Self {
        let inner = format!("path = {}", toml_string(path));
        Self {
            value: format!("{{ {inner} }}"),
            inner,
        }
    }
}

/// Returns the checkout `checkout` of `--aosabi-path` written relative to the
/// target `target`, the path the scaffold names the facade and the command
/// line by, and whether the checkout lies inside the target.
fn checkout_path(checkout: &Path, target: &Path) -> Result<(PathBuf, bool), CliError> {
    let crates = checkout.join("crates");
    let missing = ["aosabi", "aosabi-cli"]
        .into_iter()
        .find(|package| !crates.join(package).join("Cargo.toml").is_file());
    let real = fs::canonicalize(checkout)
        .ok()
        .filter(|_| missing.is_none())
        .ok_or_else(|| {
            CliError::new(format!(
                "the directory {} holds no crates/{}/Cargo.toml, which --aosabi-path names",
                checkout.display(),
                missing.unwrap_or("aosabi")
            ))
        })?;
    Ok((
        generate::relative_path(target, &real),
        real.starts_with(target),
    ))
}

/// Returns the failure of a read or a write of `path`.
fn io_failure(path: &Path, error: &io::Error) -> CliError {
    CliError::new(format!("{}: {error}", path.display()))
}
