// SPDX-License-Identifier: MIT
/*
 * aosabi-cli: the service the command line works on, read from the JSON of
 * cargo metadata.
 *
 * A Rust service is a cargo package whose facts cargo itself reports.
 */

//! The project.
//!
//! [`load`] reads the service from `cargo metadata --format-version 1` and
//! never parses a manifest, so every fact is the one cargo resolved: the
//! package, its library target, its version and edition, the target directory,
//! the root of its workspace, the source of the facade `aosabi` and the source
//! of its own build dependency `aosabi-build`. A service is one crate with one
//! facade, so three refusals stand in front of every command that builds:
//!
//! - `AO-C0101` for a package that names no normal dependency `aosabi`, and for
//!   a package of the derive path that names no build dependency
//!   `aosabi-build`, which every application of that path names for the one
//!   line `build.rs` of its declaration list. A facade named under
//!   `[build-dependencies]` alone is no normal dependency.
//! - `AO-C0102` for a package of the derive path whose library target does
//!   not stand at `src/index.rs`.
//! - `AO-C0103` for a package that names `aosabi` from two sources or at two
//!   requirements, or whose workspace lists members beside it. The facade
//!   named in `[dependencies]` and again in `[target.'cfg(unix)'.dependencies]`
//!   with features, from one source at one requirement, is one dependency and
//!   passes, which is how a service that serves files names `serve` for its
//!   Unix builds alone.
//!
//! A project takes the crates of Aosabi by path from a checkout or from a
//! repository, since Aosabi is released through its repository alone. So
//! `AO-C0108` stands before the three for every crate whose name
//! [`is_framework_crate`] answers for, from a registry, at three points, each
//! before any cargo call that compiles:
//!
//! - A package of the workspace that names one in its manifest, read with
//!   `cargo metadata --no-deps` before the lock of the project is written or
//!   read and before cargo resolves a dependency.
//! - A pin of the lock of the project, read once the lock exists and before
//!   cargo resolves the graph, which holds what a package outside the
//!   workspace, a patch or a dependency of a dependency brought into an
//!   earlier resolve, and for a project with no lock what
//!   `cargo generate-lockfile` resolved from the index of the registry alone.
//!   Under the default lock mode the full `cargo metadata` runs with
//!   `--locked` first, and where it fails, as for a lock that lacks a pin,
//!   `cargo update --workspace` writes the pins the lock lacks from the index
//!   alone, the check reads the lock again, and the full `cargo metadata`
//!   runs with `--locked` once more. Under `--offline` the full
//!   `cargo metadata` writes the pins the lock lacks from the crates cargo
//!   keeps, for every feature, so the check reads the lock again after it.
//! - A package of the graph the full `cargo metadata` resolved, read before
//!   the generated workspace is derived. Every such package stands in the
//!   lock the second check read last, so the third check is a second wall
//!   behind it.
//!
//! `cargo metadata`, `cargo generate-lockfile` and `cargo update` compile
//! nothing and run no build script, so no code of such a crate has run when
//! the loader refuses it. Only the full `cargo metadata` downloads crates,
//! and wherever it may, it runs with `--locked` or `--frozen` over a lock the
//! second check read, so the refusal comes before cargo downloads the crate,
//! after a fetch of the index of the registry at most. The second check
//! refuses the pin on every later run until `cargo update --workspace` drops
//! it.
//!
//! A build does not rest on the loader alone, since it may run with the
//! project of an earlier load, as the development loop builds after a change
//! it did not see. Every build reads the lock it compiles against with the
//! second check, `refuse_registry_lock`, right before it: the lock of the
//! project before it derives a lock of its own from it or builds the library
//! of the service over it, the derived lock after cargo resolved it and
//! before it is compared with the lock of the project, and the lock of the
//! workspace of a path facade, which the build writes first where it is
//! missing and the lock mode allows it, before it builds the crates of a
//! kernel there. Every cargo call that compiles runs with `--locked`, or
//! `--frozen` offline, over the lock so read, so a crate of Aosabi from a
//! registry is refused with `AO-C0108` before cargo compiles it, in every
//! lock mode and whatever the last load read.
//!
//! The derive path and `aosabi.config.rs` are retired for application code, so
//! two refusals stand beside them for a package whose facade names no hidden
//! feature `mirror`, the feature that keeps those forms for the twins that wait
//! for the TypeScript judge alone. Both hold against a facade that declares the
//! feature, so the command line still builds a service over an older facade,
//! whose one path is the derive path:
//!
//! - `AO-C0106` for a package of the derive path, whose entry the generated
//!   workspace would name as the function `default`, the lookup of the
//!   entry that the native layer replaces with the entry the builder finds.
//! - `AO-C0107` for a package with `aosabi.config.rs`, whose configuration
//!   the chain of the entry carries in the native layer, served by
//!   `aosabi::serve_app`.
//!
//! A plain crate, whose roots no fragment of `graphql!` holds, takes neither of
//! the first two refusals: its schema comes from its source, read from its
//! crate root wherever `[lib]` puts it, cargo's default `src/lib.rs` among the
//! places, and it needs no build script and no build side, since nothing of it
//! is compiled for the schema. So `AO-C0101` asks a plain crate for the facade
//! alone, and its hint names no build side. The reader of `aosabi-build`
//! decides. A crate whose source it cannot read is a crate of the derive path
//! when it names the build side, which cargo then reports on, and otherwise the
//! loader refuses it with `AO-C0105` and the error of the reader at its file
//! and line, a file that does not parse as Rust among them.
//!
//! The crate root of a project is the source path of its library target as
//! cargo resolves it, [`Project::lib_path`], the file every command reads the
//! service from, which `-e` and `-m` may only name again, `AO-C0104`. The
//! scaffold of `aosabi new` is a plain crate whose manifest names no `[lib]`
//! table, so its crate root is cargo's default `src/lib.rs`, and the derive
//! path alone asks for `src/index.rs`, the layout its twins keep until each
//! moves to the native index.
//!
//! A plain crate also carries what the generated copy of its package takes from
//! its manifest, as cargo resolved it: the crate root relative to the package,
//! the crate types of the library, every normal dependency with its source, its
//! requirement, its features and the target it is named for, and the features
//! of the package.
//!
//! The lock of the project is the lock of its workspace. A project with no lock
//! gets one from `cargo generate-lockfile` with the note
//! `note: wrote Cargo.lock`, unless `--locked` or `--frozen` is given, which
//! refuses with `AO-C0302`, so a pipeline run never writes the committed lock.

use std::path::{
    Path,
    PathBuf, //
};

use aosabi_json::JsValue;

use crate::{
    args::LockMode,
    cargo::{
        self,
        Cargo,
        Compile, //
    },
    codes,
    lock,
    report::CliError, //
};

/// The name of the facade, which a service names under `[dependencies]`.
pub const FACADE: &str = "aosabi";

/// The prefixes of the names of the crates of the project beside the facade,
/// those of the framework, of the transfer protocol `graphtp` and of the
/// viewer.
pub const FRAMEWORK_PREFIXES: &[&str] = &["aosabi-", "graphtp-", "aoviewer-"];

/// The names of the crates of the project that stand outside the prefixes of
/// [`FRAMEWORK_PREFIXES`]: the HTTP layer of the copied Yoga, named after its
/// role, the GraphQL layer over the transfer protocol, the protocol `graphtp`
/// itself, `homa` and the name of the viewer.
pub const FRAMEWORK_PACKAGES: &[&str] = &[
    "graphql-over-http",
    "graphql-over-graphtp",
    "graphtp",
    "homa",
    "aoviewer",
];

/// Returns `name` as a registry compares it with the names it holds: in
/// lower case, with every `_` read as `-`, so `Aosabi_Build` and
/// `aosabi-build` are one name, and in the spelling of the crates of the
/// project, whose names and folders hold no capital and no `_`.
#[must_use]
pub fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
}

/// Returns whether `name` names a crate of the project: the facade, a name
/// with a prefix of [`FRAMEWORK_PREFIXES`] or a name of
/// [`FRAMEWORK_PACKAGES`], each in any spelling [`canonical_name`] reads as
/// one.
///
/// Aosabi is released through its repository alone, so the loader refuses
/// each from a registry with `AO-C0108`, and a facade from a git source
/// counts each among the crates of the framework. A registry reads the
/// spellings of a name that differ by letter case or by `-` and `_` as one
/// name, so a spelling that differs from the name of the project names the
/// same release.
#[must_use]
pub fn is_framework_crate(name: &str) -> bool {
    let name = canonical_name(name);
    name == FACADE
        || FRAMEWORK_PACKAGES.contains(&name.as_str())
        || framework_prefix(&name).is_some()
}

/// Returns the prefix of [`FRAMEWORK_PREFIXES`] by which
/// [`is_framework_crate`] answers for `name`, `None` for the facade, for a
/// name of [`FRAMEWORK_PACKAGES`] and for every name of no prefix.
#[must_use]
pub fn framework_prefix(name: &str) -> Option<&'static str> {
    let name = canonical_name(name);
    if name == FACADE || FRAMEWORK_PACKAGES.contains(&name.as_str()) {
        return None;
    }
    FRAMEWORK_PREFIXES
        .iter()
        .copied()
        .find(|prefix| name.starts_with(prefix))
}

/// The name of the build side, which a service names under
/// `[build-dependencies]`.
pub const BUILD_SIDE: &str = "aosabi-build";

/// The configuration file of a service, beside its `Cargo.toml`.
pub const CONFIG_FILE: &str = "aosabi.config.rs";

/// The hidden feature of the facade that keeps the forms of the derive path.
pub const MIRROR: &str = "mirror";

/// The entry of a service of the derive path, the source path of its library
/// target.
pub const ENTRY: &str = "src/index.rs";

/// The name of the lock beside the root manifest of a workspace.
pub const LOCK_FILE: &str = "Cargo.lock";

/// The lock a pin check reads, which a refusal `AO-C0108` names together
/// with what mends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckedLock<'a> {
    /// The lock of the project, beside the root manifest of its workspace.
    Project,
    /// The lock a build derives from the lock of the project, for the
    /// generated workspace or for the generated copy of a plain crate.
    Derived,
    /// The lock of the workspace of a path facade, at its path, which a card
    /// builds the crates of its kernel over.
    Checkout(&'a Path),
}

/// Where the facade or the build side of the service comes from, a checkout
/// by path or a repository, since the loader refuses a registry, `AO-C0108`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepSource {
    /// A path dependency, with the absolute directory of its package.
    Path(PathBuf),
    /// A git dependency, with its repository and the reference its manifest
    /// names it by.
    Git {
        /// The URL of the repository.
        url: String,
        /// The reference.
        reference: GitReference,
    },
}

/// The reference a manifest names a git dependency by, which cargo keeps in
/// the query of the source it locks, `?tag=v0.1.0#<commit>` for
/// `tag = "v0.1.0"`.
///
/// The check of the derived lock compares the sources of two locks as text, so
/// the generated workspace names a git dependency by the reference of the
/// manifest of the project, and the lock it derives carries the source of the
/// lock of the project as the same text. A workspace that named the commit by
/// `rev` would derive the source `?rev=<commit>#<commit>`, which the lock of a
/// pin by tag, by branch, by a short revision or by the default branch holds
/// for no package. Each kind passes `--frozen`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitReference {
    /// No reference, the default branch of the repository.
    DefaultBranch,
    /// A branch, the key `branch`.
    Branch(String),
    /// A tag, the key `tag`.
    Tag(String),
    /// A revision, the key `rev`, as the manifest writes it.
    Rev(String),
}

impl GitReference {
    /// Returns the key and the value a manifest names the reference by,
    /// `None` for the default branch, which it names by no key.
    #[must_use]
    pub fn key_value(&self) -> Option<(&'static str, &str)> {
        match self {
            Self::DefaultBranch => None,
            Self::Branch(value) => Some(("branch", value)),
            Self::Tag(value) => Some(("tag", value)),
            Self::Rev(value) => Some(("rev", value)),
        }
    }
}

/// Returns the URL and the reference of the git source `source` of a
/// dependency, as cargo metadata states it, `git+<url>` with the query of its
/// reference and no commit, or `None` for a source of another kind.
///
/// The query is one pair, the key and the value as the manifest writes them.
/// Cargo writes the value without percent encoding here and with it in a lock
/// of version 4, `tag=v0.0.0+local` here for the tag `v0.0.0+local` and
/// `tag=v0.0.0%2Blocal` in the lock, so the value is taken whole and decoded by
/// no rule. The key `ref` is the old name of `branch`, as cargo reads it. The
/// source of a resolved package adds the commit after `#`, so it is no source
/// this function reads. The primitive `git-source` of `aosabi-devtool` answers
/// this reading and the entry [`crate::generate::spec`] writes for it, one case
/// per kind of reference.
#[must_use]
pub fn git_source(source: &str) -> Option<(String, GitReference)> {
    let git = source.strip_prefix("git+")?;
    let (url, query) = git.split_once('?').unwrap_or((git, ""));
    let reference = query
        .split_once('=')
        .map_or(GitReference::DefaultBranch, |(key, value)| match key {
            "branch" | "ref" => GitReference::Branch(value.to_owned()),
            "tag" => GitReference::Tag(value.to_owned()),
            "rev" => GitReference::Rev(value.to_owned()),
            _ => GitReference::DefaultBranch,
        });
    Some((url.to_owned(), reference))
}

/// One normal dependency of a plain crate as the manifest of its generated
/// copy names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyDependency {
    /// The name the crate sees the dependency by, its rename or its name.
    pub key: String,
    /// The name of its package.
    pub package: String,
    /// Where it comes from, a path with the absolute directory of its
    /// package, a registry or a repository.
    pub source: CopySource,
    /// The `cfg` or the target it is named for, `None` for every target.
    pub target: Option<String>,
    /// Whether it is a build dependency, of the build script of the crate.
    pub build: bool,
    /// The features it turns on.
    pub features: Vec<String>,
    /// Whether it is optional.
    pub optional: bool,
    /// Whether it keeps its default features.
    pub default_features: bool,
}

/// Where a dependency of a plain crate comes from, as its manifest states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopySource {
    /// A path dependency, with the absolute directory of its package.
    Path(PathBuf),
    /// A dependency of a registry, with its requirement and its registry,
    /// `None` for crates.io.
    Registry {
        /// The version requirement.
        req: String,
        /// The URL of a registry other than crates.io.
        registry: Option<String>,
    },
    /// A dependency of a repository, with its URL and the reference its
    /// manifest names it by.
    Git {
        /// The URL of the repository.
        url: String,
        /// The reference.
        reference: GitReference,
    },
}

/// What the generated copy of a plain crate takes from its manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyFacts {
    /// The crate root, relative to the package.
    pub root: PathBuf,
    /// The crate types of the library.
    pub crate_types: Vec<String>,
    /// The build script of the package, relative to it, `None` for none.
    pub build_script: Option<PathBuf>,
    /// The `rust-version` of the package.
    pub rust_version: Option<String>,
    /// Every normal dependency and every build dependency, in the order of
    /// the metadata.
    pub dependencies: Vec<CopyDependency>,
    /// The features of the package, each with what it turns on, in the
    /// order of the metadata.
    pub features: Vec<(String, Vec<String>)>,
}

/// The service the command line works on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    /// The package name.
    pub name: String,
    /// The crate name of its library target.
    pub lib_name: String,
    /// The source path of its library target, the crate root: the path of
    /// its `[lib]` table, or cargo's default `src/lib.rs` where the manifest
    /// names none.
    pub lib_path: PathBuf,
    /// The package version.
    pub version: String,
    /// The package edition.
    pub edition: String,
    /// The directory of its `Cargo.toml`.
    pub root: PathBuf,
    /// Its `Cargo.toml`.
    pub manifest: PathBuf,
    /// The lock of its workspace.
    pub lock: PathBuf,
    /// The target directory of its workspace.
    pub target_dir: PathBuf,
    /// Its `aosabi.config.rs`, when the file exists, which a package whose
    /// facade names the hidden feature `mirror` alone may hold.
    pub config: Option<PathBuf>,
    /// The source of the facade.
    pub aosabi: DepSource,
    /// The source of its build dependency `aosabi-build`, `None` for a plain
    /// crate, which names no build side.
    pub aosabi_build: Option<DepSource>,
    /// Whether the crate is plain, which no fragment of `graphql!` holds, so
    /// its schema comes from its source and no build script writes it.
    pub plain: bool,
    /// What the generated copy of the package takes from its manifest, for a
    /// plain crate.
    pub copy: Option<CopyFacts>,
    /// The root of the workspace of a path facade, from which a card that
    /// compiles crates of the facade alone builds them.
    pub facade_workspace: Option<PathBuf>,
    /// The files a load reads beside the sources, `aosabi.config.rs`,
    /// `Cargo.toml` and `Cargo.lock` of the root, which the development loop
    /// watches beside them: the manifest of every path package of the graph
    /// but the package itself, the root manifest and the lock of its
    /// workspace, and the configuration files of cargo of the root, of every
    /// directory above it and of the cargo home, in the order of their paths.
    pub watched: Vec<PathBuf>,
}

/// Reads the service of `manifest_path`, or of `Cargo.toml` in `cwd`, under
/// the lock flags of `lock`.
///
/// # Errors
///
/// Returns `AO-C0108` first, for a package of the workspace that names a
/// crate of Aosabi from a registry, then `AO-C0302` for a missing lock under
/// `--locked` or `--frozen`, `AO-C0108` again for such a crate that the lock
/// of the project pins or the resolved graph holds,
/// `AO-C0101`, `AO-C0102` or `AO-C0103` for a package that is no service,
/// `AO-C0105` for a package that names no build side whose source the reader
/// cannot read, `AO-C0106` for a package of the derive path and `AO-C0107`
/// for a package with `aosabi.config.rs`, each beside a facade that names no
/// feature `mirror`, and the failure of a cargo call. A plain crate takes
/// `AO-C0101` for a missing facade alone and never `AO-C0102`.
pub fn load(
    cwd: &Path,
    manifest_path: Option<&Path>,
    lock: LockMode,
    cargo: &Cargo,
) -> Result<Project, CliError> {
    let manifest = manifest_path.map_or_else(|| cwd.join("Cargo.toml"), |path| cwd.join(path));
    // Cargo reads the configuration and rustup the toolchain of the directory
    // it runs in, so every call runs beside the manifest.
    let cwd = manifest.parent().filter(|dir| dir.is_dir()).unwrap_or(cwd);
    let workspace_root = cargo.locate_workspace(cwd, Some(&manifest))?;
    refuse_registry(&cargo.manifests(cwd, Some(&manifest))?)?;
    let lock_path = workspace_root.join(LOCK_FILE);
    if !lock_path.is_file() {
        match lock {
            LockMode::Locked => return Err(codes::lock_missing("--locked", None)),
            LockMode::Frozen => return Err(codes::lock_missing("--frozen", None)),
            LockMode::Normal | LockMode::Offline => {
                cargo.generate_lockfile(cwd, Some(&manifest), cargo::is_offline(lock))?;
                eprintln!("note: wrote {LOCK_FILE}");
            }
        }
    }
    refuse_registry_lock(&lock_path, CheckedLock::Project)?;
    let metadata = resolve(cargo, cwd, &manifest, lock, &lock_path)?;
    refuse_registry_graph(&metadata)?;
    let package = root_package(&metadata, &workspace_root)?;
    let name = cargo::text(package, "name").unwrap_or_default().to_owned();
    let facades = facade_entries(package);
    let facade = facades
        .iter()
        .copied()
        .find(|dep| cargo::text(dep, "kind").is_none());
    let build_side = cargo::items(package, "dependencies").iter().find(|dep| {
        cargo::text(dep, "name") == Some(BUILD_SIDE) && cargo::text(dep, "kind") == Some("build")
    });
    let manifest = PathBuf::from(cargo::text(package, "manifest_path").unwrap_or_default());
    let root = manifest
        .parent()
        .map_or_else(|| workspace_root.clone(), Path::to_path_buf);
    // The reader decides the path, over the active set of the host in the
    // dev profile. A crate whose source it cannot read and that names no
    // build side would be a plain crate, so it is answered with the error of
    // the reader and never asked for the build side.
    let cfg = cargo.cfg(&root, None, Compile::Host, &[], &[])?;
    let source = aosabi_build::source::read(&root, &cfg);
    let plain = source
        .as_ref()
        .is_ok_and(aosabi_build::source::Crate::is_plain);
    let unread = source.err().filter(|_| build_side.is_none());
    let Some(facade) = facade else {
        return Err(codes::not_a_service(&name, plain || unread.is_some()));
    };
    if let Some(error) = unread {
        return Err(codes::source_unread(&name, &error.to_string()));
    }
    let mirror = facades.iter().any(|dep| {
        cargo::items(dep, "features")
            .iter()
            .any(|feature| matches!(feature, JsValue::String(feature) if feature.as_str() == Some(MIRROR)))
    });
    // An older facade declares no feature mirror, and the derive path is its
    // one path, so the two refusals hold against a facade that declares the
    // feature alone.
    let retires = !mirror && declares_mirror(&metadata);
    if !plain && retires {
        return Err(codes::derive_path_retired(&name));
    }
    if !plain && build_side.is_none() {
        return Err(codes::not_a_service(&name, false));
    }
    let (lib_name, lib_path) = library(package, &name, plain)?;
    if facades.iter().any(|dep| identity(dep) != identity(facade)) {
        return Err(codes::facade_twice(&name));
    }
    let aosabi = source_of(&name, facade)?;
    let aosabi_build = build_side
        .map(|build_side| source_of(&name, build_side))
        .transpose()?;
    let facade_workspace = match &aosabi {
        DepSource::Path(dir) => Some(cargo.locate_workspace(cwd, Some(&dir.join("Cargo.toml")))?),
        DepSource::Git { .. } => None,
    };
    let config = Some(root.join(CONFIG_FILE)).filter(|path| path.is_file());
    if config.is_some() && retires {
        return Err(codes::config_retired(&name));
    }
    let copy = plain.then(|| copy_facts(package, &root, &lib_path));
    let watched = watched_files(&metadata, &manifest, &root, &workspace_root, cargo);
    Ok(Project {
        version: cargo::text(package, "version")
            .unwrap_or_default()
            .to_owned(),
        edition: cargo::text(package, "edition")
            .unwrap_or_default()
            .to_owned(),
        target_dir: PathBuf::from(cargo::text(&metadata, "target_directory").unwrap_or_default()),
        name,
        lib_name,
        lib_path,
        root,
        manifest,
        lock: lock_path,
        config,
        aosabi,
        aosabi_build,
        plain,
        copy,
        facade_workspace,
        watched,
    })
}

/// Returns the files a load of the package of the manifest `manifest`, whose
/// directory is `root`, reads beside that manifest: the manifest of every
/// other path package of the graph `metadata`, inside the root or outside
/// it, the root manifest and the lock of the workspace at `workspace_root`,
/// which lie above the root where the package is the member of a workspace
/// above it, and `config.toml` and `config` of the folder `.cargo` of the
/// root and of every directory above it and of the cargo home of `cargo`,
/// where cargo reads its configuration, each whether it exists or not, in
/// the order of their paths.
///
/// The set leaves out a file a configuration pulls in with `include`, the
/// root manifest of the workspace of a path package outside the root, and
/// the manifest of a path package the graph of the default features does
/// not reach. A change of one of them alone starts no reload, and the next
/// build refuses a crate of Aosabi from a registry it brings in all the
/// same, since every build reads the lock it compiles against with
/// [`refuse_registry_lock`].
fn watched_files(
    metadata: &JsValue,
    manifest: &Path,
    root: &Path,
    workspace_root: &Path,
    cargo: &Cargo,
) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = cargo::items(metadata, "packages")
        .iter()
        .filter(|package| matches!(cargo::field(package, "source"), None | Some(JsValue::Null)))
        .filter_map(|package| cargo::text(package, "manifest_path"))
        .map(PathBuf::from)
        .filter(|path| path != manifest)
        .collect();
    files.push(workspace_root.join("Cargo.toml"));
    files.push(workspace_root.join(LOCK_FILE));
    for dir in root.ancestors() {
        for name in ["config.toml", "config"] {
            files.push(dir.join(".cargo").join(name));
        }
    }
    if let Some(home) = cargo.home() {
        for name in ["config.toml", "config"] {
            files.push(home.join(name));
        }
    }
    files.sort();
    files.dedup();
    files
}

/// Returns what the generated copy of the plain crate of `package`, whose
/// directory is `root` and whose crate root is `lib_path`, takes from its
/// manifest, as cargo resolved it.
fn copy_facts(package: &JsValue, root: &Path, lib_path: &Path) -> CopyFacts {
    let crate_types = cargo::items(package, "targets")
        .iter()
        .find(|target| cargo::items(target, "kind").iter().any(is_library_kind))
        .map(|target| {
            cargo::items(target, "crate_types")
                .iter()
                .filter_map(|kind| match kind {
                    JsValue::String(kind) => kind.as_str().map(str::to_owned),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let build_script = cargo::items(package, "targets")
        .iter()
        .find(|target| {
            cargo::items(target, "kind")
                .iter()
                .any(|kind| matches!(kind, JsValue::String(kind) if kind.as_str() == Some("custom-build")))
        })
        .and_then(|target| cargo::text(target, "src_path"))
        .map(|path| {
            let path = Path::new(path);
            path.strip_prefix(root).unwrap_or(path).to_path_buf()
        });
    let dependencies = cargo::items(package, "dependencies")
        .iter()
        .filter(|dep| matches!(cargo::text(dep, "kind"), None | Some("build")))
        .filter_map(copy_dependency)
        .collect();
    let features = match cargo::field(package, "features") {
        Some(JsValue::Object(features)) => features
            .iter()
            .map(|(name, enabled)| {
                let enabled = match enabled {
                    JsValue::Array(items) => items
                        .iter()
                        .filter_map(|item| match item {
                            JsValue::String(item) => item.as_str().map(str::to_owned),
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                (name.as_str().unwrap_or_default().to_owned(), enabled)
            })
            .collect(),
        _ => Vec::new(),
    };
    CopyFacts {
        root: lib_path
            .strip_prefix(root)
            .unwrap_or(lib_path)
            .to_path_buf(),
        crate_types,
        build_script,
        rust_version: cargo::text(package, "rust_version").map(str::to_owned),
        dependencies,
        features,
    }
}

/// Returns the dependency entry `dep` of the metadata as the manifest of
/// the copy names it, `None` for an entry with no name.
fn copy_dependency(dep: &JsValue) -> Option<CopyDependency> {
    let package = cargo::text(dep, "name")?.to_owned();
    let key = cargo::text(dep, "rename").unwrap_or(&package).to_owned();
    let source = if let Some(path) = cargo::text(dep, "path") {
        CopySource::Path(PathBuf::from(path))
    } else if let Some((url, reference)) = cargo::text(dep, "source").and_then(git_source) {
        CopySource::Git { url, reference }
    } else {
        CopySource::Registry {
            req: cargo::text(dep, "req").unwrap_or("*").to_owned(),
            registry: cargo::text(dep, "registry").map(str::to_owned),
        }
    };
    let features = cargo::items(dep, "features")
        .iter()
        .filter_map(|feature| match feature {
            JsValue::String(feature) => feature.as_str().map(str::to_owned),
            _ => None,
        })
        .collect();
    let flag = |key: &str, default: bool| match cargo::field(dep, key) {
        Some(JsValue::Bool(value)) => *value,
        _ => default,
    };
    Some(CopyDependency {
        key,
        package,
        source,
        target: cargo::text(dep, "target").map(str::to_owned),
        build: cargo::text(dep, "kind") == Some("build"),
        features,
        optional: flag("optional", false),
        default_features: flag("uses_default_features", true),
    })
}

/// Returns the package of the manifest metadata answered for, refusing a
/// workspace that lists members beside it, `AO-C0103`.
///
/// A virtual manifest has no package and lists members, so it takes the same
/// refusal, named after the directory of the workspace. A service whose
/// folder is not the root of its workspace is the member of a workspace
/// above it, whose root manifest the refusal names, since the `[workspace]`
/// table stands there.
fn root_package<'a>(metadata: &'a JsValue, workspace_root: &Path) -> Result<&'a JsValue, CliError> {
    let directory_name = || {
        workspace_root
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    };
    let root_id =
        cargo::field(metadata, "resolve").and_then(|resolve| cargo::text(resolve, "root"));
    let Some(root_id) = root_id else {
        return Err(codes::workspace_with_members(&directory_name()));
    };
    let package = cargo::items(metadata, "packages")
        .iter()
        .find(|package| cargo::text(package, "id") == Some(root_id))
        .ok_or_else(|| codes::workspace_with_members(&directory_name()))?;
    if cargo::items(metadata, "workspace_members").len() > 1 {
        let name = cargo::text(package, "name").unwrap_or_default();
        let above = cargo::text(package, "manifest_path")
            .and_then(|path| Path::new(path).parent())
            .is_some_and(|folder| folder != workspace_root);
        return Err(if above {
            codes::workspace_above_with_members(name, &workspace_root.join("Cargo.toml"))
        } else {
            codes::workspace_with_members(name)
        });
    }
    Ok(package)
}

/// Returns the crate name and the source path of the library target of
/// `package`, refusing a crate of the derive path whose library does not
/// stand at `src/index.rs`, `AO-C0102`, while a `plain` crate takes its
/// library where `[lib]` puts it or at cargo's default `src/lib.rs`.
fn library(package: &JsValue, name: &str, plain: bool) -> Result<(String, PathBuf), CliError> {
    let target = cargo::items(package, "targets")
        .iter()
        .find(|target| cargo::items(target, "kind").iter().any(is_library_kind))
        .ok_or_else(|| codes::no_library_root(name))?;
    let src_path = PathBuf::from(cargo::text(target, "src_path").unwrap_or_default());
    if !plain && !src_path.ends_with(ENTRY) {
        return Err(codes::no_library_root(name));
    }
    let lib_name = cargo::text(target, "name")
        .unwrap_or(name)
        .replace('-', "_");
    Ok((lib_name, src_path))
}

/// Returns whether the target kind `kind` is one of a library target.
fn is_library_kind(kind: &JsValue) -> bool {
    match kind {
        JsValue::String(kind) => matches!(
            kind.as_str(),
            Some("lib" | "rlib" | "dylib" | "cdylib" | "staticlib")
        ),
        _ => false,
    }
}

/// Returns whether the facade the metadata resolved declares the feature
/// `mirror`, which every newer facade declares.
fn declares_mirror(metadata: &JsValue) -> bool {
    cargo::items(metadata, "packages")
        .iter()
        .filter(|package| cargo::text(package, "name") == Some(FACADE))
        .any(|package| match cargo::field(package, "features") {
            Some(JsValue::Object(features)) => features.get(MIRROR).is_some(),
            _ => false,
        })
}

/// Returns the entries of the facade among the dependencies of `package`
/// that are no development dependency.
///
/// A normal entry, whose kind is null, makes the package a service, and an
/// entry under `[build-dependencies]` takes part in the comparison of
/// sources and requirements alone, so a facade named only there refuses
/// with `AO-C0101`.
fn facade_entries(package: &JsValue) -> Vec<&JsValue> {
    cargo::items(package, "dependencies")
        .iter()
        .filter(|dep| cargo::text(dep, "name") == Some(FACADE))
        .filter(|dep| cargo::text(dep, "kind") != Some("dev"))
        .collect()
}

/// Returns the source, the requirement and the path of the dependency entry
/// `dep`, which are one for every entry of one dependency.
fn identity(dep: &JsValue) -> (Option<&str>, Option<&str>, Option<&str>) {
    (
        cargo::text(dep, "source"),
        cargo::text(dep, "req"),
        cargo::text(dep, "path"),
    )
}

/// Refuses a package of the workspace that names a crate of Aosabi from a
/// registry, `AO-C0108`, over the manifests `manifests`, the answer of
/// `cargo metadata --no-deps`.
///
/// That answer states every dependency as its manifest declares it with
/// nothing resolved, the package name of a renamed key and the entry an
/// inherited key takes from the workspace among them, so the refusal comes
/// before the lock of the project is written or read and before cargo
/// resolves, fetches or builds a package of a registry.
fn refuse_registry(manifests: &JsValue) -> Result<(), CliError> {
    for package in cargo::items(manifests, "packages") {
        let name = cargo::text(package, "name").unwrap_or_default();
        for dep in cargo::items(package, "dependencies") {
            if cargo::text(dep, "name").is_some_and(is_framework_crate) {
                source_of(name, dep)?;
            }
        }
    }
    Ok(())
}

/// Returns whether the source `source` of a resolved package is a registry,
/// any source but a path, which has none, and a repository.
fn from_registry(source: Option<&str>) -> bool {
    source.is_some_and(|source| !source.starts_with("git+"))
}

/// Refuses a crate of Aosabi that the lock at `path`, the lock `checked`,
/// pins from a registry, `AO-C0108`, with [`refuse_registry_pins`] over its
/// tables.
///
/// The loader reads the lock of the project with it, and every build reads
/// the lock it compiles against with it right before it: the lock of the
/// project before a lock is derived from it, the derived lock before it is
/// compared with the lock of the project, and the lock of the project or of
/// the workspace of a path facade before a cargo call that compiles over it.
///
/// # Errors
///
/// Returns `AO-C0108` for such a pin, naming the lock `checked`, and the
/// failure of the read of the lock.
pub(crate) fn refuse_registry_lock(path: &Path, checked: CheckedLock<'_>) -> Result<(), CliError> {
    refuse_registry_pins(&lock::read(path)?, checked)
}

/// Refuses a crate of Aosabi that a lock, the lock `checked` whose tables
/// are `entries`, pins from a registry, `AO-C0108`, naming a package of the
/// lock that depends on it.
///
/// It reads the lock as it stands before cargo resolves the graph, and again
/// after [`resolve`] wrote the pins the lock lacked, so a pin that a package
/// outside the workspace, a patch or a dependency of a dependency brought in
/// is refused before cargo downloads the crate. A pin that no package of the
/// lock depends on, which an edit or a merge of the lock leaves, is refused
/// as unused.
fn refuse_registry_pins(
    entries: &[lock::LockEntry],
    checked: CheckedLock<'_>,
) -> Result<(), CliError> {
    let Some(pinned) = entries
        .iter()
        .find(|entry| is_framework_crate(&entry.name) && from_registry(entry.source.as_deref()))
    else {
        return Ok(());
    };
    // A lock names a dependency by its name, and adds the version where two
    // pins share the name and the source in parentheses where they share the
    // version as well, with no source for a path package, so a dependency
    // names this pin by its name alone only where no other pin shares the
    // name, and by the name and the version alone only where no other pin
    // shares both.
    let same_name = entries.iter().filter(|entry| entry.name == pinned.name);
    let name_shared = same_name.clone().count() > 1;
    let version_shared = same_name
        .filter(|entry| entry.version == pinned.version)
        .count()
        > 1;
    let names_pinned = |dependency: &String| {
        let mut parts = dependency.splitn(3, ' ');
        parts.next() == Some(pinned.name.as_str())
            && parts
                .next()
                .map_or(!name_shared, |version| version == pinned.version)
            && parts.next().map_or(!version_shared, |source| {
                source
                    .strip_prefix('(')
                    .and_then(|source| source.strip_suffix(')'))
                    == pinned.source.as_deref()
            })
    };
    let source = pinned.source.as_deref().unwrap_or_default();
    match entries
        .iter()
        .find(|entry| entry.dependencies.iter().any(names_pinned))
    {
        Some(dependent) => Err(codes::registry_resolved(
            &dependent.name,
            &pinned.name,
            &pinned.version,
            source,
            checked,
        )),
        None => Err(codes::registry_unused_pin(
            &pinned.name,
            &pinned.version,
            source,
            checked,
        )),
    }
}

/// Returns the full `cargo metadata` of the manifest `manifest` under the
/// lock flags of `lock`, the lock `lock_path` read by
/// [`refuse_registry_lock`] before cargo downloads a crate it pins.
///
/// Under the default lock mode, which lets cargo write the lock, the metadata
/// runs with `--locked`, which answers as the run without the flag wherever the
/// lock holds the graph, and stops before any download where it does not. Where
/// it fails, `cargo update --workspace` writes the pins the lock lacks from the
/// index alone, the pins are read again and the metadata runs with `--locked`
/// once more. Under `--locked` and `--frozen` cargo never writes the lock, so
/// the metadata runs as the flags ask. Under `--offline` it downloads nothing,
/// but writes the pins the lock lacks from the crates it keeps, those of every
/// feature beside the default ones the answer resolves, so the pins are read
/// again after it.
///
/// # Errors
///
/// Returns `AO-C0108` for a crate of Aosabi from a registry that the written
/// lock pins, and the failure of a cargo call.
fn resolve(
    cargo: &Cargo,
    cwd: &Path,
    manifest: &Path,
    lock: LockMode,
    lock_path: &Path,
) -> Result<JsValue, CliError> {
    if lock != LockMode::Normal {
        let metadata = cargo.metadata(cwd, Some(manifest), lock)?;
        if lock == LockMode::Offline {
            refuse_registry_lock(lock_path, CheckedLock::Project)?;
        }
        return Ok(metadata);
    }
    if let Ok(metadata) = cargo.metadata(cwd, Some(manifest), LockMode::Locked) {
        return Ok(metadata);
    }
    cargo.update_workspace(cwd, Some(manifest))?;
    refuse_registry_lock(lock_path, CheckedLock::Project)?;
    cargo.metadata(cwd, Some(manifest), LockMode::Locked)
}

/// Refuses a crate of Aosabi that the graph `metadata`, the answer of the
/// full `cargo metadata`, resolves from a registry, `AO-C0108`, naming a
/// package of the graph that depends on it.
///
/// `cargo metadata` compiles nothing and runs no build script, so the
/// refusal comes before the generated workspace is derived, before any cargo
/// call that compiles and before any code of the crate runs. Every package
/// of the graph stands in the lock [`refuse_registry_pins`] read last, after
/// the metadata wherever cargo may write the lock, so this check stands as a
/// second wall behind that one.
fn refuse_registry_graph(metadata: &JsValue) -> Result<(), CliError> {
    let packages = cargo::items(metadata, "packages");
    let Some(resolved) = packages.iter().find(|package| {
        cargo::text(package, "name").is_some_and(is_framework_crate)
            && from_registry(cargo::text(package, "source"))
    }) else {
        return Ok(());
    };
    let id = cargo::text(resolved, "id");
    let dependent = cargo::field(metadata, "resolve")
        .map(|resolve| cargo::items(resolve, "nodes"))
        .unwrap_or_default()
        .iter()
        .find(|node| {
            cargo::items(node, "deps")
                .iter()
                .any(|dep| cargo::text(dep, "pkg") == id)
        })
        .and_then(|node| cargo::text(node, "id"))
        .and_then(|node_id| {
            packages
                .iter()
                .find(|package| cargo::text(package, "id") == Some(node_id))
        })
        .and_then(|package| cargo::text(package, "name"))
        .unwrap_or_default();
    Err(codes::registry_resolved(
        dependent,
        cargo::text(resolved, "name").unwrap_or_default(),
        cargo::text(resolved, "version").unwrap_or_default(),
        cargo::text(resolved, "source").unwrap_or_default(),
        CheckedLock::Project,
    ))
}

/// Returns the source of the dependency `dep` of the package `package`, a
/// path or the repository and the reference its manifest names it by,
/// refusing a dependency of a registry, `AO-C0108`, with the key of its
/// entry, a renamed one among them.
fn source_of(package: &str, dep: &JsValue) -> Result<DepSource, CliError> {
    if let Some(path) = cargo::text(dep, "path") {
        return Ok(DepSource::Path(PathBuf::from(path)));
    }
    git_source(cargo::text(dep, "source").unwrap_or_default())
        .map(|(url, reference)| DepSource::Git { url, reference })
        .ok_or_else(|| {
            let name = cargo::text(dep, "name").unwrap_or_default();
            let key = cargo::text(dep, "rename").unwrap_or(name);
            codes::registry_source(package, key, name)
        })
}
