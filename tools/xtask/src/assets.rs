// SPDX-License-Identifier: MIT
/*
 * xtask assets: the build of the wasm module of aoviewer and the writing of
 * the committed products.
 *
 * The module is built with --locked for wasm32-unknown-unknown under the
 * profile wasm with the paths of the repository and of the cargo home remapped,
 * and --check builds into target/xtask/ and exits 1 on the first file that
 * differs.
 */

//! The viewer assets.
//!
//! [`viewer_assets`] builds `aoviewer-wasm` into `target/xtask/`, strips the
//! module through [`wasm_strip::strip`] and renders `assets.lock` through
//! [`lock::render`]. Under `--check` it writes both under `target/xtask/` with
//! the same names and compares the committed files with them, the module first.
//! When one differs, it names every source whose SHA-256 in the committed lock
//! is not the one of the tree, which is what the lock records the sources for.
//! The sources are the files git tracks under [`SOURCES`], so beside the two
//! packages the root manifest with `[profile.wasm]`, `Cargo.lock`,
//! `rust-toolchain.toml` and the cargo configuration of the repository are
//! sources too, and a change of one of them names its file.
//!
//! The compiler and the cargo are asked for their release before the build, and
//! a release that is not the channel of `rust-toolchain.toml` stops the run, so
//! no product comes from another toolchain. The build is told to run that
//! compiler through `--config build.rustc`, which wins over a cargo
//! configuration file, and `built_by` names it by the release, the commit, the
//! host and the LLVM that `rustc -vV` answers, so a compiler of the same
//! release built from other code or for another host writes another lock, and
//! the check names the line that moved. The sources are hashed before and after
//! the build, and a source that changes in between stops the run rather than
//! leave a lock that names bytes the module was not built from. Both products
//! are first written whole to temporary files of their own beside them and
//! synced, and only then does each replace its file by a rename made durable by
//! a sync of its directory, the module first. So a write that fails leaves both
//! files as they were, a run that stops leaves each product whole, the old one
//! or the new one, and only a run that stops between the two renames leaves the
//! new module beside the old lock, which the next check names. A later run
//! removes a temporary file that a stopped run left and nobody touched for
//! [`STALE`].
//!
//! Cargo keeps `RUSTFLAGS` out of `-C metadata`, and out of the unit id as well
//! when one flag remaps, so the source sides may differ from host to host. The
//! flags travel as `CARGO_ENCODED_RUSTFLAGS`, the form of `RUSTFLAGS` that
//! keeps a path with a space one argument and that wins over a `RUSTFLAGS` of
//! the caller. The build also runs without the variables of [`CLEARED`], which
//! set a value of a profile, incremental compilation, the compiler, its
//! wrappers or a setting of the target. A cargo configuration file that the
//! build would read and the lock does not record, one in a directory above the
//! root or in the cargo home, or one of the root that git does not track, stops
//! the run, since its profile, its wrapper, its linker or its `[env]` would
//! move the module with no line of the lock to show it. So the build depends on
//! the sources the lock hashes, the compiler it names and the remap alone.

use std::{
    env,
    ffi::{
        OsStr,
        OsString, //
    },
    fmt,
    fs::{
        self,
        OpenOptions, //
    },
    io::{
        self,
        Write, //
    },
    path::{
        Path,
        PathBuf, //
    },
    process::{
        self,
        Command,
        ExitStatus, //
    },
    time::Duration, //
};

use crate::{
    FileError,
    lock::{
        self,
        Product,
        SourceError, //
    },
    wasm_strip::{
        self,
        StripError, //
    }, //
};

/// The package of the module.
const PACKAGE: &str = "aoviewer-wasm";

/// The target of the module.
const TARGET: &str = "wasm32-unknown-unknown";

/// The profile of the module, `[profile.wasm]` of the root manifest.
const PROFILE: &str = "wasm";

/// The features of the build, the default ones of the package.
const FEATURES: &str = "default";

/// The file cargo writes for the package under the target and the profile.
const ARTIFACT: &str = "aoviewer_wasm.wasm";

/// The name of the product, which is the key of its entry in the lock.
const PRODUCT: &str = "aoviewer.wasm";

/// The directory of the committed products.
const PAGE_DIR: &str = "crates/aoviewer-page";

/// The module under the directory of the products.
const MODULE: &str = "assets/aoviewer.wasm";

/// The lock under the directory of the products, beside `assets/`, where
/// `NOTICE` section 3 names it.
const LOCK: &str = "assets.lock";

/// The target directory of every build, which a check also writes its
/// products into.
const BUILD_DIR: &str = "target/xtask";

/// The sources of the module, the directories and files from the root whose
/// every tracked file the lock hashes: the sources and the manifests of the two
/// packages, and the files of the root that give the build its profile, its
/// lints and edition, its locked dependencies, its toolchain and the
/// configuration of cargo, under either name cargo reads.
const SOURCES: [&str; 9] = [
    ".cargo/config",
    ".cargo/config.toml",
    "Cargo.lock",
    "Cargo.toml",
    "crates/aoviewer-core/Cargo.toml",
    "crates/aoviewer-core/src",
    "crates/aoviewer-wasm/Cargo.toml",
    "crates/aoviewer-wasm/src",
    "rust-toolchain.toml",
];

/// The file that pins the toolchain.
const TOOLCHAIN: &str = "rust-toolchain.toml";

/// The target side of the remap of the repository root.
const ROOT_REMAP: &str = "/aosabi/project";

/// The target side of the remap of the cargo home.
const CARGO_HOME_REMAP: &str = "/aosabi/cargo";

/// The separator of the flags of `CARGO_ENCODED_RUSTFLAGS`.
const FLAG_SEPARATOR: char = '\x1f';

/// The directory of a cargo configuration file, in the root and in every
/// directory above it.
const CONFIG_DIR: &str = ".cargo";

/// The names of a cargo configuration file, in a directory `.cargo` and in the
/// cargo home, both of which cargo reads.
const CONFIG_NAMES: [&str; 2] = ["config", "config.toml"];

/// The prefixes of the variables the build runs without, which set a value of a
/// profile, incremental compilation, the compiler, a wrapper of the compiler or
/// a setting of the target such as its linker.
///
/// Incremental compilation overrides the profile. `RUSTFLAGS` and
/// `CARGO_BUILD_RUSTFLAGS` need no entry and stay in the environment, since
/// `CARGO_ENCODED_RUSTFLAGS` wins over both, and `RUSTC` stays, since the
/// release check asks the compiler it names.
const CLEARED: [&str; 7] = [
    "CARGO_PROFILE_",
    "CARGO_INCREMENTAL",
    "CARGO_BUILD_INCREMENTAL",
    "CARGO_BUILD_RUSTC",
    "CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
];

/// The suffix of the temporary file a product is written through, after the
/// name of the product, a dot, the id of the process and a dot.
const TEMPORARY: &str = "tmp";

/// The age after which a temporary file belongs to no running write, which
/// takes milliseconds, so a later run removes it.
const STALE: Duration = Duration::from_mins(1);

/// What a run did.
#[derive(Default)]
pub struct Outcome {
    /// Each file written, by its path from the root and its length.
    pub written: Vec<String>,
    /// The first committed file that differs from the build, under `--check`.
    pub stale: Option<String>,
    /// The sources whose SHA-256 in the committed lock is not the one of the
    /// tree, when a committed file differs.
    pub moved: Vec<String>,
    /// The `built_by` of the committed lock and the one of the build, when a
    /// committed file differs and the two do.
    pub compiler: Option<(String, String)>,
}

/// Why a run stopped before it wrote or compared every product.
#[derive(Debug)]
pub enum Error {
    /// A file or a directory cannot be read or written.
    Io(FileError),
    /// A path is not UTF-8, holds the separator of the encoded flags, or holds
    /// a quote, a backslash or a control character that the value of
    /// `--config` would have to escape.
    Path(PathBuf),
    /// The sources cannot be hashed.
    Sources(SourceError),
    /// Sources changed, appeared or went away while the module was built,
    /// never an empty list.
    Moved(Vec<String>),
    /// The lock has no JSON form, which the writer answers for no object.
    Json,
    /// A program cannot be started.
    Spawn {
        /// The program.
        program: OsString,
        /// The error of the operating system.
        error: io::Error,
    },
    /// The build of the module failed.
    Build(ExitStatus),
    /// The compiler or the cargo, by its name, lacks a line of `-vV` that the
    /// xtask reads.
    Toolchain(&'static str),
    /// `rust-toolchain.toml` names no channel.
    Channel,
    /// The release of the compiler or of the cargo is not the channel of the
    /// pin.
    Release {
        /// The name of the program, `rustc` or `cargo`.
        tool: &'static str,
        /// The channel of `rust-toolchain.toml`.
        pinned: String,
        /// The release of the program.
        found: String,
    },
    /// Neither `CARGO_HOME` nor a home directory is known.
    CargoHome,
    /// The manifest of the xtask stands less than two levels below a root.
    Root,
    /// A cargo configuration file the build would read is no source of the
    /// lock.
    Configuration(PathBuf),
    /// The module breaks the section list or imports.
    Strip(StripError),
}

impl Error {
    /// Returns [`Error::Io`] for `path`.
    fn io(path: &str, error: io::Error) -> Self {
        Self::Io(FileError::new(path, error))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Path(path) => {
                write!(
                    f,
                    "{} is not UTF-8 or holds a byte cargo cannot take",
                    path.display()
                )
            }
            Self::Sources(error) => write!(f, "{error}"),
            Self::Moved(paths) => write!(
                f,
                "{} changed, appeared or went away during the build, run again",
                paths.join(", ")
            ),
            Self::Json => f.write_str("assets.lock has no JSON form"),
            Self::Spawn { program, error } => {
                write!(f, "cannot run {}: {error}", program.display())
            }
            Self::Build(status) => write!(f, "the build of {PACKAGE} failed, {status}"),
            Self::Toolchain(tool) => write!(f, "{tool} -vV lacks a line the xtask reads"),
            Self::Channel => f.write_str("rust-toolchain.toml names no channel"),
            Self::Release {
                tool,
                pinned,
                found,
            } => write!(f, "{tool} {found} is not the pinned {tool} {pinned}"),
            Self::CargoHome => f.write_str("neither CARGO_HOME nor a home directory is known"),
            Self::Root => f.write_str("tools/xtask stands less than two levels below a root"),
            Self::Configuration(path) => write!(
                f,
                "{} is a cargo configuration that assets.lock does not record",
                path.display()
            ),
            Self::Strip(error) => write!(f, "the module of {PACKAGE} is refused, {error}"),
        }
    }
}

/// Builds the module and writes it with `assets.lock`, or under `check`
/// compares both with the committed files.
///
/// # Errors
///
/// Returns an [`Error`] when the compiler or the cargo is not the pinned one,
/// a cargo configuration file is no source, the build fails, a source moves
/// during it, the module is refused, a source cannot be hashed or a file
/// cannot be read or written.
pub fn viewer_assets(check: bool) -> Result<Outcome, Error> {
    let root = root()?;
    let pinned = channel(&root)?;
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let built_by = built_by(&root, &rustc, &pinned)?;
    version(&root, &cargo, "cargo", &pinned)?;
    let cargo_home = cargo_home(&root)?;
    let sources = lock::sources(&root, &SOURCES).map_err(Error::Sources)?;
    refuse_configuration(&root, &cargo_home, &sources)?;
    let raw = build(&root, &cargo, &rustc, &cargo_home)?;
    let after = lock::sources(&root, &SOURCES).map_err(Error::Sources)?;
    let changed = lock::changed(&sources, &after);
    if !changed.is_empty() {
        return Err(Error::Moved(changed));
    }
    let module = wasm_strip::strip(&raw).map_err(Error::Strip)?;
    let lock = lock::render(&[Product {
        name: PRODUCT,
        bytes: &module,
        built_by: &built_by,
        features: FEATURES,
        profile: PROFILE,
        sources: &sources,
    }])
    .ok_or(Error::Json)?;
    let products = [(MODULE, module.as_slice()), (LOCK, lock.as_slice())];
    let out_dir = if check { BUILD_DIR } else { PAGE_DIR };
    let mut outcome = Outcome {
        written: write(&root, out_dir, &products)?,
        ..Outcome::default()
    };
    if check {
        compare(&root, &products, &sources, &built_by, &mut outcome)?;
    }
    Ok(outcome)
}

/// Returns the repository root, two levels above the manifest of the xtask.
///
/// The manifest directory is the one `cargo run` names in `CARGO_MANIFEST_DIR`
/// when it starts the xtask, so the root follows the checkout as it stands, and
/// the one of the build when the binary runs alone. The root is the path with
/// every link resolved, which is the directory cargo works in and names to the
/// compiler, so the remap and the directories above it are the ones of the
/// build.
fn root() -> Result<PathBuf, Error> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), PathBuf::from);
    let root = manifest_dir.ancestors().nth(2).ok_or(Error::Root)?;
    fs::canonicalize(root).map_err(|error| Error::io(&root.display().to_string(), error))
}

/// Returns the compiler `rustc` as `built_by` names it, when its release is
/// `pinned`: `rustc`, its release, its commit, its host and its LLVM, the
/// lines `release`, `commit-hash`, `host` and `LLVM version` of `rustc -vV`.
///
/// The compiler `rustc`, which is `RUSTC` or `rustc`, is asked in `root`, and
/// the build runs the same one. A toolchain chosen by `cargo +<toolchain>` or
/// by `RUSTUP_TOOLCHAIN` reaches both alike, so its release is refused here.
fn built_by(root: &Path, rustc: &OsStr, pinned: &str) -> Result<String, Error> {
    let text = version(root, rustc, "rustc", pinned)?;
    let line = |key| field(&text, key).ok_or(Error::Toolchain("rustc"));
    Ok(format!(
        "rustc {pinned}, commit {}, host {}, LLVM {}",
        line("commit-hash")?,
        line("host")?,
        line("LLVM version")?
    ))
}

/// Returns the output of `program -vV` run in `root`, when the release it
/// names is `pinned`, and `tool` names the program in an error.
///
/// The cargo of the build is `CARGO`, the cargo that runs the xtask as
/// `cargo run` names it, or `cargo`, and its release is asked as well, since
/// cargo chooses the flags and the unit hashes the compiler is given.
fn version(
    root: &Path,
    program: &OsStr,
    tool: &'static str,
    pinned: &str,
) -> Result<String, Error> {
    let output = Command::new(program)
        .arg("-vV")
        .current_dir(root)
        .output()
        .map_err(|error| Error::Spawn {
            program: program.to_owned(),
            error,
        })?;
    if !output.status.success() {
        return Err(Error::Toolchain(tool));
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let found = field(&text, "release").ok_or(Error::Toolchain(tool))?;
    if found != pinned {
        return Err(Error::Release {
            tool,
            pinned: String::from(pinned),
            found: String::from(found),
        });
    }
    Ok(text)
}

/// Returns the value of the line `<key>: <value>` of `text`.
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(": "))
}

/// Returns the channel of `rust-toolchain.toml`, the string of its line
/// `channel = ...` in double or single quotes.
fn channel(root: &Path) -> Result<String, Error> {
    let text =
        fs::read_to_string(root.join(TOOLCHAIN)).map_err(|error| Error::io(TOOLCHAIN, error))?;
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("channel"))
        .filter_map(|rest| rest.trim_start().strip_prefix('='))
        .find_map(|value| quoted(value.trim_start()))
        .map(String::from)
        .ok_or(Error::Channel)
}

/// Returns the text between the quote that opens `value` and the next one of
/// its kind.
fn quoted(value: &str) -> Option<&str> {
    let quote = value.chars().next().filter(|&c| c == '"' || c == '\'')?;
    let (text, _) = value.get(1..)?.split_once(quote)?;
    Some(text)
}

/// Refuses a cargo configuration file that a build in `root` reads and that is
/// not among `sources`.
///
/// Cargo reads `config` and `config.toml` under `.cargo` in the directory it
/// starts in and in every directory above it, and the two in the cargo home
/// `cargo_home`. Of these only a file of the root that git tracks is a source,
/// and any other entry at such a path stops the run.
fn refuse_configuration(
    root: &Path,
    cargo_home: &Path,
    sources: &[(String, String)],
) -> Result<(), Error> {
    let recorded = |path: &Path| {
        path.strip_prefix(root)
            .ok()
            .and_then(Path::to_str)
            .is_some_and(|path| {
                sources
                    .binary_search_by(|(source, _)| source.as_str().cmp(path))
                    .is_ok()
            })
    };
    let dirs = root
        .ancestors()
        .map(|dir| dir.join(CONFIG_DIR))
        .chain([cargo_home.to_path_buf()]);
    for dir in dirs {
        for name in CONFIG_NAMES {
            let path = dir.join(name);
            if fs::symlink_metadata(&path).is_ok() && !recorded(&path) {
                return Err(Error::Configuration(path));
            }
        }
    }
    Ok(())
}

/// Builds the module into [`BUILD_DIR`] with the cargo `cargo`, the compiler
/// `rustc` and the cargo home `cargo_home` and returns the bytes cargo wrote.
fn build(root: &Path, cargo: &OsStr, rustc: &OsStr, cargo_home: &Path) -> Result<Vec<u8>, Error> {
    let flags = remap_flags(root, cargo_home)?;
    let compiler = rustc
        .to_str()
        .filter(|text| !text.contains(|c: char| c == '"' || c == '\\' || c.is_control()))
        .ok_or_else(|| Error::Path(PathBuf::from(rustc)))?;
    let mut command = Command::new(cargo);
    command
        .args(["build", "--locked", "-p", PACKAGE, "--target", TARGET])
        .args(["--profile", PROFILE, "--target-dir", BUILD_DIR])
        .arg("--config")
        .arg(format!("build.rustc=\"{compiler}\""))
        .current_dir(root)
        .env("CARGO_ENCODED_RUSTFLAGS", flags);
    for (name, _) in env::vars_os() {
        if cleared(&name) {
            command.env_remove(&name);
        }
    }
    let status = command.status().map_err(|error| Error::Spawn {
        program: cargo.to_owned(),
        error,
    })?;
    if !status.success() {
        return Err(Error::Build(status));
    }
    let artifact = format!("{BUILD_DIR}/{TARGET}/{PROFILE}/{ARTIFACT}");
    fs::read(root.join(&artifact)).map_err(|error| Error::io(&artifact, error))
}

/// Returns whether the build runs without the variable `name`.
fn cleared(name: &OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| CLEARED.iter().any(|prefix| name.starts_with(prefix)))
}

/// Returns the remap flags of the repository root and of the cargo home,
/// joined for `CARGO_ENCODED_RUSTFLAGS`.
///
/// The last remap that matches a path wins, so of two prefixes the longer one
/// goes last, which keeps a repository inside the cargo home under its own
/// target side.
fn remap_flags(root: &Path, cargo_home: &Path) -> Result<String, Error> {
    let mut remaps = [
        (cargo_home.to_path_buf(), CARGO_HOME_REMAP),
        (root.to_path_buf(), ROOT_REMAP),
    ];
    remaps.sort_by_key(|(from, _)| from.as_os_str().len());
    let mut flags = Vec::with_capacity(remaps.len());
    for (from, to) in &remaps {
        let text = from
            .to_str()
            .filter(|text| !text.contains(FLAG_SEPARATOR))
            .ok_or_else(|| Error::Path(from.clone()))?;
        flags.push(format!("--remap-path-prefix={text}={to}"));
    }
    Ok(flags.join(&FLAG_SEPARATOR.to_string()))
}

/// Returns the cargo home as a cargo started in `root` finds it, which is a
/// `CARGO_HOME` that is not empty, taken from `root` when it is relative, or
/// `.cargo` under the home directory, `HOME` or else the entry of the user in
/// the password database.
fn cargo_home(root: &Path) -> Result<PathBuf, Error> {
    match env::var_os("CARGO_HOME").filter(|home| !home.is_empty()) {
        Some(home) => Ok(root.join(home)),
        None => Ok(env::home_dir().ok_or(Error::CargoHome)?.join(".cargo")),
    }
}

/// A product written to its temporary file and not yet renamed.
struct Staged {
    /// The path of the product from the root.
    file: String,
    /// The path of the product.
    path: PathBuf,
    /// The path of its temporary file.
    temporary: PathBuf,
    /// The length of its bytes.
    length: usize,
}

/// Writes each of `products` under `dir`, a path from `root`, and returns
/// what it wrote.
///
/// Every product is first written whole to a temporary file of its own by
/// [`stage`], and only when all of them stand does each replace its file by
/// [`commit`], in the order of `products`. A write or a rename that fails
/// removes the temporary files that are left, so a failure while the products
/// are written leaves every file as it was.
fn write(root: &Path, dir: &str, products: &[(&str, &[u8])]) -> Result<Vec<String>, Error> {
    let mut staged = Vec::with_capacity(products.len());
    for &(name, bytes) in products {
        let file = format!("{dir}/{name}");
        let path = root.join(&file);
        match stage(&path, bytes) {
            Ok(temporary) => staged.push(Staged {
                file,
                path,
                temporary,
                length: bytes.len(),
            }),
            Err(error) => {
                discard(&staged);
                return Err(Error::io(&file, error));
            }
        }
    }
    let mut written = Vec::with_capacity(staged.len());
    for (at, product) in staged.iter().enumerate() {
        if let Err(error) = commit(&product.temporary, &product.path) {
            discard(&staged[at..]);
            return Err(Error::io(&product.file, error));
        }
        written.push(format!("{}, {} bytes", product.file, product.length));
    }
    Ok(written)
}

/// Removes the temporary file of each of `staged`.
fn discard(staged: &[Staged]) {
    for product in staged {
        // The error that stopped the write is the one the run reports, so the
        // removal of what it left is best effort.
        fs::remove_file(&product.temporary).ok();
    }
}

/// Removes the temporary files of `path` in `parent` that a run which stopped
/// left behind, untouched for [`STALE`], so the file of a write in progress
/// stays.
///
/// A file that is gone when it is read or removed was removed by another run
/// at the same time, which is the end this function wants as well.
fn remove_leftovers(parent: &Path, path: &Path) -> io::Result<()> {
    let Some(product) = path.file_name().and_then(OsStr::to_str) else {
        return Ok(());
    };
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if !is_leftover(&entry.file_name(), product) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map(|modified| modified.elapsed().is_ok_and(|age| age > STALE));
        let removed = match stale {
            Ok(true) => fs::remove_file(entry.path()),
            Ok(false) => Ok(()),
            Err(error) => Err(error),
        };
        match removed {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    Ok(())
}

/// Returns whether `name` is a temporary file of `product`, the product's
/// name, a dot, a process id in decimal digits, a dot and [`TEMPORARY`].
fn is_leftover(name: &OsStr, product: &str) -> bool {
    name.to_str()
        .and_then(|name| name.strip_prefix(product)?.strip_prefix('.'))
        .and_then(|rest| rest.strip_suffix(TEMPORARY)?.strip_suffix('.'))
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

/// Creates the directory of `path`, removes the temporary files a stopped run
/// left there for its product, and writes `bytes` to a temporary file beside
/// `path`, synced, whose path it returns.
///
/// The temporary file carries the id of the process, so two runs never share
/// one, and it is created anew, so a link at its name makes the write fail and
/// is never followed. A write that fails removes it, as `replace` of
/// `crates/aoviewer-docs/src/write.rs` does.
fn stage(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        remove_leftovers(parent, path)?;
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".{}.{TEMPORARY}", process::id()));
    let temporary = PathBuf::from(temporary);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if written.is_err() {
        // The error of the write is the one the run reports, so the removal
        // of what it left is best effort.
        fs::remove_file(&temporary).ok();
    }
    written.map(|()| temporary)
}

/// Renames `temporary` onto `path` and syncs the directory, so `path` holds
/// its old bytes or the new ones and never a part, and the rename is durable
/// before the next one starts.
fn commit(temporary: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temporary, path)?;
    match path.parent() {
        Some(parent) => fs::File::open(parent)?.sync_all(),
        None => Ok(()),
    }
}

/// Sets in `outcome` the line of the first committed product that is missing
/// or differs from the one the check wrote, which names both paths, and then
/// the sources the committed lock names with another SHA-256 than `sources`
/// or alone, and its `built_by` beside `built_by` when the two differ.
///
/// Both committed files are read once. A committed lock that is missing or
/// that [`lock::committed`] cannot read names no source and no compiler,
/// since the line names that file already.
fn compare(
    root: &Path,
    products: &[(&str, &[u8])],
    sources: &[(String, String)],
    built_by: &str,
    outcome: &mut Outcome,
) -> Result<(), Error> {
    let mut committed_lock = None;
    for &(name, bytes) in products {
        let committed = format!("{PAGE_DIR}/{name}");
        let built = format!("{BUILD_DIR}/{name}");
        let old = match fs::read(root.join(&committed)) {
            Ok(old) => Some(old),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(Error::io(&committed, error)),
        };
        if outcome.stale.is_none() {
            outcome.stale = match &old {
                Some(old) if old == bytes => None,
                Some(_) => Some(format!("{committed} differs from {built}")),
                None => Some(format!("{committed} is missing, the build wrote {built}")),
            };
        }
        if name == LOCK {
            committed_lock = old;
        }
    }
    if outcome.stale.is_none() {
        return Ok(());
    }
    if let Some(committed) = committed_lock.and_then(|bytes| lock::committed(&bytes, PRODUCT)) {
        outcome.moved = lock::changed(&committed.sources, sources);
        if committed.built_by != built_by {
            outcome.compiler = Some((committed.built_by, String::from(built_by)));
        }
    }
    Ok(())
}
