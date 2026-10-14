// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the switch AOSABI_STD, the standard library of a release build
 * from source on the pinned stable compiler.
 *
 * No analogue in the reference.
 */

//! The switch `AOSABI_STD`.
//!
//! The switch reads `prebuilt` where it is unset or empty, the stock form on
//! the prebuilt standard library of the toolchain, or `source`, and the
//! command refuses any other value. A dev build reads no switch, so it takes
//! no `-Z` flag and no `RUSTC_BOOTSTRAP` at any value.
//!
//! At `source` a release build of a card whose [`SourceSet`] the table of the
//! built in cards names does four things.
//!
//! 1. It refuses a toolchain whose sysroot holds no `lib/rustlib/src/rust`,
//!    the component `rust-src`, with [`refusal`], which names the `rustup`
//!    command that adds the component and which the command never runs, as it
//!    checks every pin.
//! 2. It runs cargo with `RUSTC_BOOTSTRAP=1` and names the crates of
//!    `-Zbuild-std` and the features of `-Zbuild-std-features` in
//!    [`BUILD_STD_ENV`] and [`BUILD_STD_FEATURES_ENV`], the variables cargo
//!    reads them from, an empty list of features taking every feature away as
//!    `-Zbuild-std-features=` does.
//! 3. It gives cargo the target directory [`SOURCE_TARGET_DIR`] inside the
//!    target directory of the project, one target directory per value of the
//!    switch, so neither form builds the crates of the other again, while the
//!    generated workspace and the artifacts of its build script stay in
//!    `target/aosabi/`, whose paths the configuration of the project is named
//!    by. The binary then goes where a build at `prebuilt` puts it, through a
//!    copy under a new name that takes the place in one step, since cargo
//!    links the binary there to its artifact under `deps/` and a write through
//!    that link would change the artifact of the other value.
//!
//! The remap of the source of the standard library stands in every release
//! build at both values, after the other prefixes of the build ([`crate::build`]),
//! and the switch reaches the builder stage of the card `docker` through
//! [`crate::container`]. A card whose table names no set builds stock at
//! every value.
//!
//! The wrapper adds [`NO_DEBUG_TEXT`] and [`NO_LOCATION_DETAIL`] to a unit
//! whose command line holds `--target` and the name of a crate of the
//! framework, [`NO_LOCATION_DETAIL`] alone to `core`, `alloc` and `std` built
//! for the target, and passes every other unit as it is. A host unit, a build
//! script and a proc macro carry no `--target`, so they take no flag of the
//! set, and the app's own crate and every crate it names itself keep their
//! Debug text and their locations. A profile entry of `rustflags` would reach
//! the host build of the same crate, whose generated code `aosabi-build`
//! writes through `{:?}`, and would need the bootstrap for every cargo call on
//! the generated workspace, the dev build among them, so no profile carries
//! the flags. The crates of the framework are the packages cargo resolves from
//! a path in the workspace of a path facade, its members and its forks, and
//! for a facade from a registry or a git source the packages named `aosabi` or
//! `aosabi-*` and those of [`crate::project::FRAMEWORK_PACKAGES`],
//! `graphql-over-http` among them.

use std::{
    ffi::OsString,
    fs,
    path::{
        Path,
        PathBuf, //
    },
};

use crate::{
    dotenv::DotEnv,
    env,
    report::CliError, //
};

/// The variable of the switch.
pub const SWITCH_ENV: &str = "AOSABI_STD";

/// The value of the switch that builds the stock form on the prebuilt
/// standard library of the toolchain, also when the switch is unset or empty.
pub const PREBUILT: &str = "prebuilt";

/// The value of the switch that builds the standard library of a release
/// build from source with the set of its card.
pub const SOURCE: &str = "source";

/// The target directory of a release build at `source`, inside the target
/// directory of the project.
pub const SOURCE_TARGET_DIR: &str = "std-source";

/// The variable that lets the pinned stable compiler take the `-Z` flags.
pub const BOOTSTRAP_ENV: &str = "RUSTC_BOOTSTRAP";

/// The variable cargo reads the crates of `-Zbuild-std` from.
pub const BUILD_STD_ENV: &str = "CARGO_UNSTABLE_BUILD_STD";

/// The variable cargo reads the features of `-Zbuild-std-features` from.
pub const BUILD_STD_FEATURES_ENV: &str = "CARGO_UNSTABLE_BUILD_STD_FEATURES";

/// The variable cargo runs a wrapper of rustc from.
pub const WRAPPER_ENV: &str = "RUSTC_WRAPPER";

/// The file of the wrapper of rustc beside the generated workspace.
pub const WRAPPER_FILE: &str = "rustc-std-source.sh";

/// The flag the wrapper adds to a crate of the framework.
pub const NO_DEBUG_TEXT: &str = "-Zfmt-debug=none";

/// The flag the wrapper adds to a crate of the framework and to `core`,
/// `alloc` and `std`.
pub const NO_LOCATION_DETAIL: &str = "-Zlocation-detail=none";

/// The path of the source of the standard library in a sysroot, where the
/// component `rust-src` puts it.
pub const SOURCE_DIR: [&str; 4] = ["lib", "rustlib", "src", "rust"];

/// The prefix of the path the prebuilt standard library names its source by,
/// followed by the commit of the compiler.
pub const REMAPPED_PREFIX: &str = "/rustc/";

/// The value of the switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Std {
    /// The prebuilt standard library of the toolchain.
    Prebuilt,
    /// The standard library from source with the set of the card.
    Source,
}

/// The set a card takes at `source`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSet {
    /// The crates of `-Zbuild-std`.
    pub crates: &'static str,
    /// Whether the crates of the framework lose their Debug text and their
    /// location detail and the standard library its location detail, the
    /// options per crate.
    pub per_crate: bool,
}

/// The set of the Linux cards, the with the options per crate.
pub const LINUX_SET: SourceSet = SourceSet {
    crates: "std,panic_abort",
    per_crate: true,
};

impl Std {
    /// Returns the value of the switch in the environment of the command and
    /// in `.env`.
    ///
    /// # Errors
    ///
    /// Returns a failure for a value that is neither `prebuilt` nor `source`.
    pub fn read(env_file: &DotEnv) -> Result<Self, CliError> {
        match env::var(env_file, SWITCH_ENV).as_deref() {
            None | Some("" | PREBUILT) => Ok(Self::Prebuilt),
            Some(SOURCE) => Ok(Self::Source),
            Some(other) => Err(CliError::new(format!(
                "{SWITCH_ENV} is {other:?}, and its values are {PREBUILT} and {SOURCE}"
            ))),
        }
    }
}

/// Returns the path of the source of the standard library in `sysroot`.
#[must_use]
pub fn source_dir(sysroot: &Path) -> PathBuf {
    SOURCE_DIR
        .iter()
        .fold(sysroot.to_path_buf(), |dir, part| dir.join(part))
}

/// Returns the refusal of a build at `source` on the toolchain of the release
/// `release`, whose sysroot holds no `rust-src`.
#[must_use]
pub fn refusal(release: &str) -> CliError {
    CliError::new(format!(
        "the switch {SWITCH_ENV} at {SOURCE} builds the standard library of the card from source, and the \
         toolchain {release} holds no rust-src: run `rustup component add rust-src --toolchain {release}` \
         once, which no build runs for you"
    ))
}

/// Returns the pairs the cargo call of a release build at `source` takes for
/// `set`, with the wrapper `wrapper` where the set holds the options per
/// crate.
#[must_use]
pub fn cargo_env(
    env_file: &DotEnv,
    set: SourceSet,
    wrapper: Option<&Path>,
) -> Vec<(&'static str, OsString)> {
    let mut pairs = vec![
        (BOOTSTRAP_ENV, OsString::from("1")),
        (BUILD_STD_ENV, OsString::from(set.crates)),
    ];
    if env::var(env_file, BUILD_STD_FEATURES_ENV).is_none() {
        pairs.push((BUILD_STD_FEATURES_ENV, OsString::new()));
    }
    if let Some(wrapper) = wrapper {
        pairs.push((WRAPPER_ENV, wrapper.as_os_str().to_owned()));
    }
    pairs
}

/// Cargo runs a wrapper as `<wrapper> <rustc> <arguments>`, and the script
/// finds the unit's `--crate-name` and whether its command line holds
/// `--target` in one pass over the arguments, so it needs no program beside
/// the shell.
#[must_use]
pub fn wrapper(crates: &[String], chain: Option<&str>) -> String {
    let runner = chain.map_or_else(
        || String::from("\"$rustc\""),
        |chain| format!("{} \"$rustc\"", shell_quote(chain)),
    );
    let names = crates.join(" ");
    format!(
        "#!/bin/sh\n\
         # The wrapper of rustc of a release build at {SWITCH_ENV}={SOURCE}, written by cargo aosabi.\n\
         rustc=\"$1\"\n\
         shift\n\
         target=no\n\
         name=\n\
         previous=\n\
         for argument in \"$@\"; do\n    \
         case \"$argument\" in --target|--target=*) target=yes ;; esac\n    \
         if [ \"$previous\" = \"--crate-name\" ]; then name=\"$argument\"; fi\n    \
         previous=\"$argument\"\n\
         done\n\
         if [ \"$target\" = yes ]; then\n    \
         case \" {names} \" in *\" $name \"*) exec {runner} \"$@\" {NO_DEBUG_TEXT} {NO_LOCATION_DETAIL} ;; esac\n    \
         case \"$name\" in core|alloc|std) exec {runner} \"$@\" {NO_LOCATION_DETAIL} ;; esac\n\
         fi\n\
         exec {runner} \"$@\"\n"
    )
}

/// Writes `text` to the program `path` with the mode `0o755`, through a file
/// under a new name that takes the place in one step, so a build that runs
/// the program while another writes it never reads half a script.
///
/// # Errors
///
/// Returns the failure of a write, of the mode or of the rename.
pub fn write_program(path: &Path, text: &str) -> Result<(), CliError> {
    let failure =
        |what: &Path, error: &std::io::Error| CliError::new(format!("{}: {error}", what.display()));
    if fs::read(path).is_ok_and(|held| held == text.as_bytes()) && is_program(path) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|error| failure(dir, &error))?;
    }
    let mut staged = path.as_os_str().to_owned();
    staged.push(format!(".{}", std::process::id()));
    let staged = PathBuf::from(staged);
    fs::write(&staged, text).map_err(|error| failure(&staged, &error))?;
    set_program_mode(&staged).map_err(|error| failure(&staged, &error))?;
    fs::rename(&staged, path).map_err(|error| failure(path, &error))
}

/// Copies the file `from` to `to` through a file under a new name that takes
/// the place of `to` in one step, so the file `to` named before keeps its
/// bytes, and writes nothing where `to` holds the bytes of `from` already, as
/// every file of a build that changes nothing.
///
/// # Errors
///
/// Returns the failure of a read, of the copy or of the rename.
// A memory map of both files would compare them with no copy.
pub fn place(from: &Path, to: &Path) -> Result<(), CliError> {
    let failure =
        |what: &Path, error: &std::io::Error| CliError::new(format!("{}: {error}", what.display()));
    let bytes = fs::read(from).map_err(|error| failure(from, &error))?;
    if fs::read(to).is_ok_and(|held| held == bytes) {
        return Ok(());
    }
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir).map_err(|error| failure(dir, &error))?;
    }
    let mut staged = to.as_os_str().to_owned();
    staged.push(format!(".{SOURCE}"));
    let staged = PathBuf::from(staged);
    fs::copy(from, &staged).map_err(|error| failure(from, &error))?;
    fs::rename(&staged, to).map_err(|error| failure(to, &error))
}

/// Returns `text` quoted for the shell, in single quotes with each single
/// quote of it closed, escaped and opened again.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Gives the file `path` the mode `0o755`.
#[cfg(unix)]
fn set_program_mode(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

/// Leaves the file `path` as it is, since a host that is not Unix knows no
/// mode bits.
#[cfg(not(unix))]
fn set_program_mode(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Returns whether the file `path` carries the mode bit of execution.
#[cfg(unix)]
fn is_program(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

/// Returns whether the file `path` exists, since a host that is not Unix
/// knows no mode bits.
#[cfg(not(unix))]
fn is_program(path: &Path) -> bool {
    path.is_file()
}
