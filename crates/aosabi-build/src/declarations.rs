// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the build script of an application crate, which writes the
 * declaration list of every target of the crate.
 *
 * This module scans the module tree of the crate before rustc compiles it.
 */

//! The declaration list of an application crate.
//!
//! The heritage sweep of the inference visits every class of the program, also
//! one no root reaches. A walk from the roots cannot find such a type, so the
//! build script of the application crate scans the module tree of the crate and
//! writes the list of its declarations, one per `#[derive(Aosabi)]` and
//! `#[aosabi::scalar]`, before rustc compiles the crate. The whole build script
//! is one call:
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     aosabi_build::declarations_main()
//! }
//! ```
//!
//! The list of a target is the file `OUT_DIR/aosabi_declarations/<target>.rs`,
//! where `<target>` is `lib-<crate>` for the library and for an example of a
//! library crate type, `bin-<name>` for a binary and a binary example and
//! `test-<crate>` for an integration test and a benchmark. It is an expression
//! of the type `&'static [Declaration]` whose elements name the constants
//! `crate::<module>::__AOSABI_DECLARATION_<NAME>` the macros emit, each under
//! the `#[cfg]` of its item and of every module above it. The compiler takes a
//! `cfg` on an element of an array expression and removes the element where the
//! predicate fails. The derive includes the list of the target it expands in,
//! so the roots of the crate carry the list, and [󰀉](crate::emit()) reads it
//! through them. The list is a constant on every target, safe code with no
//! linker section, and its order is the order of the scan, which the inference
//! replaces by the order of the source positions.
//!
//! The module `scan` walks the module tree of one target, and the module
//! `manifest` finds the targets of the package.

pub(crate) mod manifest;
mod scan;

use std::{
    env,
    fs,
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode, //
};

use self::{
    manifest::Target,
    scan::Entry, //
};
use crate::{
    error::{
        BuildError,
        ScanRefusal, //
    },
    update_file_if_changed::update_file_if_changed, //
};

/// The directory of the lists inside `OUT_DIR`, which the derive reads as well.
pub const DECLARATIONS_DIR: &str = "aosabi_declarations";

/// Says what a run of the build script of an application crate wrote and
/// read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// Every list, with the number of its declarations.
    pub lists: Vec<(PathBuf, usize)>,
    /// Every file and directory whose change asks for a new list.
    pub watched: Vec<PathBuf>,
}

/// Runs the build script of an application crate.
///
/// Cargo sets `CARGO_MANIFEST_DIR`, `CARGO_PKG_NAME` and `OUT_DIR` for a build
/// script. The run writes the declaration list of every target of the package
/// into `OUT_DIR`, and it tells cargo to run again when the manifest, a file
/// the scan read or one of the directories `src`, `examples`, `tests` and
/// `benches` that exists at the run changes. A directory made after the run is
/// not watched, so its first target has no list until the manifest changes, and
/// the derive refuses that target and says so.
///
/// On an error the message goes to stderr and the answer is
/// [`ExitCode::FAILURE`], which fails the build of the crate.
///
/// [the cargo book on build scripts]: https://doc.rust-lang.org/cargo/reference/build-scripts.html
#[must_use]
pub fn declarations_main() -> ExitCode {
    let declared = build_script_env()
        .and_then(|(dir, package, out_dir)| write_declarations(&dir, &package, &out_dir));
    match declared {
        Ok(declared) => {
            for path in &declared.watched {
                println!("cargo::rerun-if-changed={}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Returns the package directory, the package name and `OUT_DIR` of a build
/// script.
fn build_script_env() -> Result<(PathBuf, String, PathBuf), BuildError> {
    let read = |name: &str| {
        env::var(name)
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| BuildError::NoBuildScript(String::from(name)))
    };
    let dir = PathBuf::from(read("CARGO_MANIFEST_DIR")?);
    let package = read("CARGO_PKG_NAME")?;
    let out_dir = PathBuf::from(read("OUT_DIR")?);
    Ok((dir, package, out_dir))
}

/// Writes the declaration list of every target of the package `package` in
/// `dir` into `out_dir`.
///
/// A list is written only when its bytes change, and a file of an earlier run
/// that names no target any more is removed, so a target whose declarations
/// are gone never reads a stale list.
///
/// # Errors
///
/// Returns [`BuildError::Io`] when a file cannot be read, written or removed,
/// and [`BuildError::Scan`] when the scan of a target refuses its tree or when
/// two targets that declare types share the name of a list.
pub fn write_declarations(
    dir: &Path,
    package: &str,
    out_dir: &Path,
) -> Result<Declared, BuildError> {
    let manifest_path = dir.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|error| BuildError::Io(manifest_path.clone(), error))?;
    let lists_dir = out_dir.join(DECLARATIONS_DIR);
    fs::create_dir_all(&lists_dir).map_err(|error| BuildError::Io(lists_dir.clone(), error))?;

    let mut watched = vec![manifest_path];
    watched.extend(manifest::discovery_dirs(dir));
    let mut scanned = Vec::new();
    for target in manifest::targets(dir, package, &manifest) {
        let scan = scan::scan(&target.root)?;
        watched.extend(scan.files);
        scanned.push((target, scan.entries));
    }

    let mut lists: Vec<(PathBuf, usize)> = Vec::new();
    for (target, _) in &scanned {
        let path = lists_dir.join(target.list_name());
        if lists.iter().any(|(written, _)| *written == path) {
            continue;
        }
        let (target, entries) = owner(&scanned, target)?;
        write_list(&path, &list_text(target, dir, entries))?;
        lists.push((path, entries.len()));
    }
    remove_stale(&lists_dir, &lists)?;
    Ok(Declared { lists, watched })
}

/// Returns the target whose list the list name of `target` holds, with its
/// entries.
///
/// Two targets can share the name of a list: a binary and a binary example of
/// one name get the same `CARGO_BIN_NAME`, and the library and an example of
/// a library crate type of one crate name the same `CARGO_CRATE_NAME`, so
/// the derive cannot tell their lists apart. The list holds the declarations
/// of the one target of that name that declares types, and two such targets
/// are refused.
///
/// # Errors
///
/// Returns [`BuildError::Scan`] when two targets of the name declare types.
fn owner<'a>(
    scanned: &'a [(Target, Vec<Entry>)],
    target: &'a Target,
) -> Result<(&'a Target, &'a [Entry]), BuildError> {
    let name = target.list_name();
    let mut declaring = scanned
        .iter()
        .filter(|(other, entries)| other.list_name() == name && !entries.is_empty());
    let Some((first, entries)) = declaring.next() else {
        return Ok((target, &[]));
    };
    if let Some((second, _)) = declaring.next() {
        return Err(BuildError::Scan {
            path: second.root.clone(),
            refusal: ScanRefusal::SharedList {
                list: name,
                other: first.root.clone(),
            },
        });
    }
    Ok((first, entries))
}

/// Returns the text of the list of `target`, whose package is in `dir`.
fn list_text(target: &Target, dir: &Path, entries: &[Entry]) -> String {
    let root = target.root.strip_prefix(dir).unwrap_or(&target.root);
    let mut lines = vec![
        format!(
            "// The declaration list of {}, which aosabi_build::declarations_main writes.",
            root.display()
        ),
        String::from("&["),
    ];
    for entry in entries {
        lines.extend(entry.cfgs.iter().map(|cfg| format!("    #[cfg({cfg})]")));
        lines.push(format!("    {},", entry.path));
    }
    lines.push(String::from("]\n"));
    lines.join("\n")
}

/// Writes one list and names its path in the error.
fn write_list(path: &Path, text: &str) -> Result<(), BuildError> {
    update_file_if_changed(path, text.as_bytes())
        .map(|_| ())
        .map_err(|error| BuildError::Io(path.to_path_buf(), error))
}

/// Removes every file of `lists_dir` that is no list of `lists`.
fn remove_stale(lists_dir: &Path, lists: &[(PathBuf, usize)]) -> Result<(), BuildError> {
    let entries =
        fs::read_dir(lists_dir).map_err(|error| BuildError::Io(lists_dir.to_path_buf(), error))?;
    for entry in entries {
        let path = entry
            .map_err(|error| BuildError::Io(lists_dir.to_path_buf(), error))?
            .path();
        if lists.iter().any(|(written, _)| *written == path) {
            continue;
        }
        fs::remove_file(&path).map_err(|error| BuildError::Io(path.clone(), error))?;
    }
    Ok(())
}
