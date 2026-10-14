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
//! - A facade named under `[build-dependencies]` alone is no normal dependency.
//! - `AO-C0102` for a package of the derive path whose library target does
//!   not stand at `src/index.rs`, the entry of `docs/authoring.md` section 2.
//! - `AO-C0103` for a package that names `aosabi` from two sources or at two
//!   requirements, or whose workspace lists members beside it. The facade
//!   named in `[dependencies]` and again in `[target.'cfg(unix)'.dependencies]`
//!   with features, from one source at one requirement, is one dependency and
//!   passes, which is how a service that serves files names `serve` for its
//!   Unix builds alone.
//!
//! Both hold against a facade that declares the feature, so the command line
//! still builds a service over a facade before that item, whose one path is the
//! derive path:
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
    report::CliError, //
};

/// The name of the facade, which a service names under `[dependencies]`.
pub const FACADE: &str = "aosabi";

/// The packages of the framework whose names stand outside the prefix
/// `aosabi-`, which a facade from a registry or a git source counts among the
/// crates of the framework by name beside the facade and the packages named
/// `aosabi-*`: the HTTP layer of the copied Yoga, named after its role since
/// step 1.
pub const FRAMEWORK_PACKAGES: &[&str] = &["graphql-over-http"];

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

/// Where a dependency of the service comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepSource {
    /// A path dependency, with the absolute directory of its package.
    Path(PathBuf),
    /// A registry dependency, with the version the lock of the project pins.
    Registry {
        /// The pinned version.
        version: String,
    },
    /// A git dependency, with its repository and the commit the lock pins.
    Git {
        /// The URL of the repository.
        url: String,
        /// The commit.
        rev: String,
    },
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
    /// A dependency of a repository, with its URL and the reference after
    /// `?` of its source, `rev=..`, `tag=..` or `branch=..`.
    Git {
        /// The URL of the repository.
        url: String,
        /// The reference, as `key=value`.
        reference: Option<String>,
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
}

/// Reads the service of `manifest_path`, or of `Cargo.toml` in `cwd`, under
/// the lock flags of `lock`.
///
/// # Errors
///
/// Returns `AO-C0302` for a missing lock under `--locked` or `--frozen`,
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
    let lock_path = workspace_root.join(LOCK_FILE);
    if !lock_path.is_file() {
        match lock {
            LockMode::Locked => return Err(codes::lock_missing("--locked")),
            LockMode::Frozen => return Err(codes::lock_missing("--frozen")),
            LockMode::Normal | LockMode::Offline => {
                cargo.generate_lockfile(cwd, Some(&manifest), cargo::is_offline(lock))?;
                eprintln!("note: wrote {LOCK_FILE}");
            }
        }
    }
    let metadata = cargo.metadata(cwd, Some(&manifest), lock)?;
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
    let aosabi = source_of(&metadata, facade)?;
    let aosabi_build = build_side
        .map(|build_side| source_of(&metadata, build_side))
        .transpose()?;
    let facade_workspace = match &aosabi {
        DepSource::Path(dir) => Some(cargo.locate_workspace(cwd, Some(&dir.join("Cargo.toml")))?),
        DepSource::Registry { .. } | DepSource::Git { .. } => None,
    };
    let config = Some(root.join(CONFIG_FILE)).filter(|path| path.is_file());
    if config.is_some() && retires {
        return Err(codes::config_retired(&name));
    }
    let copy = plain.then(|| copy_facts(package, &root, &lib_path));
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
    })
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
    } else if let Some(git) =
        cargo::text(dep, "source").and_then(|source| source.strip_prefix("git+"))
    {
        let (url, reference) = git.split_once('?').map_or((git, None), |(url, query)| {
            (
                url,
                Some(query.split('#').next().unwrap_or(query).to_owned()),
            )
        });
        CopySource::Git {
            url: url.to_owned(),
            reference,
        }
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
/// refusal, named after the directory of the workspace.
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
        return Err(codes::workspace_with_members(name));
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
/// `mirror`, which every facade on declares.
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

/// Returns the source of the dependency `dep`, with the version or the
/// commit of the package the metadata resolved for it.
fn source_of(metadata: &JsValue, dep: &JsValue) -> Result<DepSource, CliError> {
    let name = cargo::text(dep, "name").unwrap_or_default();
    if let Some(path) = cargo::text(dep, "path") {
        return Ok(DepSource::Path(PathBuf::from(path)));
    }
    let declared = cargo::text(dep, "source").unwrap_or_default();
    let resolved = cargo::items(metadata, "packages").iter().find(|package| {
        cargo::text(package, "name") == Some(name)
            && cargo::text(package, "source").is_some_and(|source| same_origin(source, declared))
    });
    let Some(resolved) = resolved else {
        return Err(CliError::new(format!(
            "cargo metadata resolved no package for the dependency {name} from {declared}"
        )));
    };
    let source = cargo::text(resolved, "source").unwrap_or_default();
    if let Some(git) = source.strip_prefix("git+") {
        let url_end = git.find(['?', '#']).unwrap_or(git.len());
        let rev = git.rsplit_once('#').map_or("", |(_, rev)| rev);
        return Ok(DepSource::Git {
            url: git[..url_end].to_owned(),
            rev: rev.to_owned(),
        });
    }
    Ok(DepSource::Registry {
        version: cargo::text(resolved, "version")
            .unwrap_or_default()
            .to_owned(),
    })
}

/// Returns whether the source `resolved` of a package comes from the
/// declared source `declared` of a dependency, which differ by the query and
/// the commit of a git source alone.
fn same_origin(resolved: &str, declared: &str) -> bool {
    let base = |source: &str| -> String {
        let end = source.find(['?', '#']).unwrap_or(source.len());
        source[..end].to_owned()
    };
    base(resolved) == base(declared)
}
