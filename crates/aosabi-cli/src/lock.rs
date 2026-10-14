// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the reader of Cargo.lock and the seed and the check of the lock
 * of the generated workspace.
 *
 * No analogue in the reference. The reference resolves npm packages once for
 * the project and bundles from node_modules, and the generated workspace of
 * cargo aosabi resolves its own lock, which this file derives from the lock
 * of the project.
 */

//! The derived lock.
//!
//! The generated workspace under `target/aosabi/` is a workspace of its own,
//! so its lock is `target/aosabi/Cargo.lock`, a file derived from the lock of
//! the project and never committed. [`seed`] writes it from the lock of the
//! project, `cargo metadata` on the generated manifest resolves it, which
//! adds the two generated packages and keeps every pin it finds, and
//! [`check`] compares the result with the lock of the project. Every cargo
//! call on the generated workspace after the check runs with `--locked`, so
//! the build compiles exactly the pins the check passed.
//!
//! A package with no source is a path package and passes. A package with a
//! source passes when the lock of the project pins it at the same name,
//! version, source and checksum, and every other package refuses with
//! `AO-C0301`, a pin of the project among them that the resolve would move.
//! The server package names the facade with the features of its card, and the
//! engine those features bring reaches registry packages that cargo writes
//! into the lock of the project only while a feature the project names
//! enables them, since it leaves an optional dependency of a dependency out
//! of a lock otherwise. So a service names the features of the cards it
//! builds in its own table of the facade for a Unix target, as the scaffold
//! of `new` does, and its lock pins the packages of the engine.
//!
//! A plain crate names no build side, so the generated packages of its
//! workspace name `aosabi-build` from where the facade comes, and the derived
//! lock holds `aosabi-build` and the two crates only it reaches,
//! `aosabi-infer` and `aosabi-ir`, which the lock of the project does not
//! pin. Beside a path facade they are path packages and pass. Every other
//! package the lock of the project does not pin still refuses.
//!
//! The reader reads the subset cargo writes, the `[[package]]` tables with
//! their `name`, `version`, `source`, `checksum` and `dependencies`, and
//! skips every other line.

use std::{
    fs,
    path::Path, //
};

use aosabi_build::update_file_if_changed;

use crate::{
    codes,
    project::FACADE,
    report::CliError, //
};

/// The packages of the build side that the generated packages of a plain
/// crate bring into the derived lock, `aosabi-build` and the two crates only
/// it reaches.
const BUILD_SIDE: [&str; 3] = ["aosabi-build", "aosabi-infer", "aosabi-ir"];

/// One `[[package]]` table of a lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockEntry {
    /// The package name.
    pub name: String,
    /// The package version.
    pub version: String,
    /// The source, `None` for a path package.
    pub source: Option<String>,
    /// The checksum of a registry package.
    pub checksum: Option<String>,
    /// The dependencies, each `name` or `name version`.
    pub dependencies: Vec<String>,
}

/// What [`check`] found in a derived lock that passes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockReport {
    /// The count of packages of the derived lock.
    pub packages: usize,
}

/// Returns every `[[package]]` table of the lock at `path`, in its order.
///
/// # Errors
///
/// Returns a failure when the file cannot be read or holds a package with
/// no name or no version.
pub fn read(path: &Path) -> Result<Vec<LockEntry>, CliError> {
    let text = read_text(path)?;
    let mut entries = Vec::new();
    let mut current: Option<PartialEntry> = None;
    let mut in_dependencies = false;
    for line in text.lines() {
        if line.starts_with('[') {
            if let Some(partial) = current.take() {
                entries.push(finish(path, partial)?);
            }
            in_dependencies = false;
            if line.trim_end() == "[[package]]" {
                current = Some(PartialEntry::default());
            }
            continue;
        }
        let Some(partial) = current.as_mut() else {
            continue;
        };
        let trimmed = line.trim();
        if in_dependencies {
            if trimmed.starts_with(']') {
                in_dependencies = false;
            } else if let Some(value) = basic_string(trimmed) {
                partial.dependencies.push(value);
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "name" => partial.name = basic_string(value),
            "version" => partial.version = basic_string(value),
            "source" => partial.source = basic_string(value),
            "checksum" => partial.checksum = basic_string(value),
            "dependencies" => {
                let rest = value.trim_start_matches('[').trim();
                in_dependencies = !rest.ends_with(']');
                partial.dependencies.extend(
                    rest.trim_end_matches(']')
                        .split(',')
                        .filter_map(|item| basic_string(item.trim())),
                );
            }
            _ => {}
        }
    }
    if let Some(partial) = current.take() {
        entries.push(finish(path, partial)?);
    }
    Ok(entries)
}

/// Writes the lock at `generated_lock` from the lock of the project at
/// `project_lock`, unless the file holds the same bytes.
///
/// # Errors
///
/// Returns a failure when the lock cannot be read or the file cannot be
/// written.
pub fn seed(project_lock: &Path, generated_lock: &Path) -> Result<(), CliError> {
    let text = read_text(project_lock)?;
    if let Some(dir) = generated_lock.parent() {
        fs::create_dir_all(dir).map_err(|error| io_failure(dir, &error))?;
    }
    update_file_if_changed(generated_lock, text.as_bytes())
        .map(|_| ())
        .map_err(|error| io_failure(generated_lock, &error))
}

/// Checks the derived lock at `generated_lock` against the lock of the
/// project at `project_lock`.
///
/// # Errors
///
/// Returns `AO-C0301` with every package with a source that the lock of the
/// project does not pin, but a package of the build side from the source and
/// at the version of the facade it pins, and a failure when a lock cannot be
/// read.
pub fn check(project_lock: &Path, generated_lock: &Path) -> Result<LockReport, CliError> {
    let pinned = read(project_lock)?;
    let generated = read(generated_lock)?;
    let is_pinned = |entry: &LockEntry| pinned.iter().any(|pin| same_pin(pin, entry));
    let facade = generated
        .iter()
        .find(|entry| entry.name == FACADE && entry.source.is_some() && is_pinned(entry));
    let unpinned: Vec<String> = generated
        .iter()
        .filter(|entry| {
            entry.source.is_some()
                && !is_pinned(entry)
                && !facade.is_some_and(|facade| beside_facade(facade, entry))
        })
        .map(|entry| format!("{} {}", entry.name, entry.version))
        .collect();
    if !unpinned.is_empty() {
        return Err(codes::unpinned_packages(&unpinned));
    }
    Ok(LockReport {
        packages: generated.len(),
    })
}

/// Returns whether two entries pin one package, the same name, version,
/// source and checksum.
fn same_pin(a: &LockEntry, b: &LockEntry) -> bool {
    a.name == b.name && a.version == b.version && a.source == b.source && a.checksum == b.checksum
}

/// Returns whether `entry` is a package of the build side from the source of
/// the facade `facade` at its version, which the pin of the facade stands for.
fn beside_facade(facade: &LockEntry, entry: &LockEntry) -> bool {
    BUILD_SIDE.contains(&entry.name.as_str())
        && entry.source == facade.source
        && entry.version == facade.version
}

/// Returns the text of the lock at `path`.
fn read_text(path: &Path) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(|error| io_failure(path, &error))
}

/// Returns the failure of a read or a write of the lock at `path`.
fn io_failure(path: &Path, error: &std::io::Error) -> CliError {
    CliError::new(format!("{}: {error}", path.display()))
}

/// The fields of a table as they are read.
#[derive(Default)]
struct PartialEntry {
    name: Option<String>,
    version: Option<String>,
    source: Option<String>,
    checksum: Option<String>,
    dependencies: Vec<String>,
}

/// Returns the entry of the fields `partial` of a table of the lock at
/// `path`.
fn finish(path: &Path, partial: PartialEntry) -> Result<LockEntry, CliError> {
    let (Some(name), Some(version)) = (partial.name, partial.version) else {
        return Err(CliError::new(format!(
            "the lock {} holds a [[package]] table with no name or no version",
            path.display()
        )));
    };
    Ok(LockEntry {
        name,
        version,
        source: partial.source,
        checksum: partial.checksum,
        dependencies: partial.dependencies,
    })
}

/// Returns the content of the TOML basic string that opens `value`, its
/// escapes `\"`, `\\`, `\n` and `\t` read, or `None` for a value that is no
/// basic string.
fn basic_string(value: &str) -> Option<String> {
    let mut chars = value.strip_prefix('"')?.chars();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}
