// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the reader of the plain source of an application crate, the
 * first part of the source path of aosabi-builder.
 *
 * Each module names what it stands in for in its own header.
 */

//! The reader of a plain Rust crate.
//!
//! This module is the first part of that path. [`read`] parses the crate with
//! `syn` 2.0.106, the version the Linux kernel carries, on the build host,
//! with the library of every path dependency the manifests name, strips it as
//! rustc strips it for the build, and keeps what the source states:
//!
//! | Module | Reads |
//! | --- | --- |
//! | `features` | the features cargo enables for the crate and for each path dependency, over their manifests |
//! | `cfg` | [`Cfg`], the active set `rustc --print cfg` prints for the target of the build in its profile, with the target information over which cargo takes the tables of a platform, and the strip of every file over that set and the features of its crate, as rustc strips a crate before it compiles it |
//! | `tree` | the module tree from the crate root, cargo's default `src/lib.rs` or the `path` of `[lib]`, through file modules, inline modules and `#[path]`, every struct, enum, impl block, function, constant, static, type alias, macro call and `extern crate` with its doc text, its file and its line, and every `use` with its file and its line, for the crate and for each path dependency, each file as the strip leaves it |
//! | `resolve` | the paths of the crate and of its path dependencies, `use`, `pub use`, globs, `self`, `super`, `crate` and `extern crate`, and the names of `std`, `core`, `alloc` and the facade |
//! | `entry` | the entry, the one public function of the crate root that answers an app, and the roots its chain registers |
//! | `reach` | every struct, enum and inherent impl block the roots reach, and every type the reader cannot resolve with the reason |
//! | [`names`] | the GraphQL name of a Rust name, [`names::graphql_name`] |
//! | `docs` and `jsdoc` | the description and the tags of a doc comment, read as the derive path reads them |
//! | `describe` | the lowering of the types the roots reach into the dump of a descriptor graph, refusing every construct the source cannot state at the file and the line of the author |
//! | `types` | [`SourceTypes`], the third implementation of the seam `TypeSource`, which answers the inference about those types |
//!
//! A crate whose roots no fragment of `graphql!` holds is a plain crate, and
//! [`SourceTypes::describe`] gives the inference its types, so the build
//! writes the schema of a plain crate from its source and compiles nothing for
//! it. A crate whose roots a fragment holds keeps the derive path.
//!
//! The reader is a feature of the crate, `source`, so `syn` stays out of every
//! build that leaves the feature off, the build script of every application
//! and every wasm build among them. It ships in no artifact, and it adds no
//! `unsafe`. The judging notebook is
//! `tests/notebooks/12_authoring_parity.ipynb`, which runs it through the
//! primitive `source` of `aosabi-devtool` over its builder cells under
//! `tests/fixtures/rs/c6/builder/`, over every twin and every case of the
//! builder corpus, and over the names of the goldens.

mod cfg;
mod copy;
mod describe;
mod docs;
mod entry;
mod features;
mod glue;
mod jsdoc;
mod model;
pub mod names;
mod reach;
mod resolve;
mod tree;
mod types;

use core::fmt;
use std::{
    fs,
    io,
    path::{
        Component,
        Path,
        PathBuf, //
    }, //
};

pub use self::reach::{
    InputName,
    Member,
    Reach,
    Reached,
    ReachedKind,
    Reason,
    Root,
    RootFrom,
    Start,
    Unresolved, //
};
use self::{
    cfg::Active,
    features::{
        Edge,
        Package, //
    },
    model::Library,
    resolve::{
        FACADE,
        Resolver, //
    },
    tree::Tree, //
};
pub use self::{
    cfg::Cfg,
    copy::{
        Copied,
        write_copy, //
    },
    glue::{
        Glue,
        MODULE,
        glue, //
    },
    types::{
        SourceTypes,
        Undescribed, //
    }, //
};
use crate::declarations::manifest::{
    self,
    Dependency, //
};

/// A file and a line of the source of a crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The file, relative to the directory of the package, which opens with
    /// the directory of a path dependency for a file of that dependency.
    pub file: PathBuf,
    /// The line, counted from 1.
    pub line: usize,
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file.display(), self.line)
    }
}

/// The entry of a crate, the one public function of its root that answers
/// an app.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The name of the function.
    pub function: String,
    /// The place of its name.
    pub place: Place,
    /// Whether it answers a `Result` of an app rather than an app.
    pub result: bool,
}

/// The root a registration adds a part to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    /// `Query`, which `.query(T)` registers.
    Query,
    /// `Mutation`, which `.mutation(T)` registers.
    Mutation,
    /// `Subscription`, which `.subscription(T)` registers.
    Subscription,
}

impl RootKind {
    /// Returns the name of the method that registers the root.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Subscription => "subscription",
        }
    }
}

/// One part of a root that the chain of the entry registers.
#[derive(Debug, Clone)]
pub struct Registration {
    /// The root the part belongs to.
    pub kind: RootKind,
    /// The path of the type from the crate root.
    pub path: String,
    /// The place of the call that registers it.
    pub place: Place,
    /// The paths of the functions `compose` reaches it through, the outermost
    /// first, empty for a part of the entry itself.
    pub through: Vec<String>,
    /// The type, as an index into the items of the crate.
    item: usize,
}

/// A form of the source the reader refuses, at the place of the author.
#[derive(Debug, Clone)]
pub struct Refusal {
    /// The place of the form.
    pub place: Place,
    /// What the reader refuses and what to write instead.
    pub message: String,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.place, self.message)
    }
}

/// Says why the reader read no crate.
#[derive(Debug)]
pub enum SourceError {
    /// The file system refuses the named path.
    Io(PathBuf, io::Error),
    /// A file does not parse as Rust, at the place, for the reason the text
    /// gives.
    Parse {
        /// The place `syn` names.
        place: Place,
        /// The message of `syn`.
        message: String,
    },
    /// The module tree is circular, so the file is its own module.
    Circular(PathBuf),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, error) => write!(f, "{}: {error}", path.display()),
            Self::Parse { place, message } => {
                write!(f, "{place}: the file does not parse as Rust: {message}")
            }
            Self::Circular(path) => write!(
                f,
                "{}: the module tree is circular, so the file is its own module",
                path.display()
            ),
        }
    }
}

impl core::error::Error for SourceError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io(_, error) => Some(error),
            Self::Parse { .. } | Self::Circular(_) => None,
        }
    }
}

/// A crate as its plain source states it.
pub struct Crate {
    /// The name of the package.
    package: String,
    /// The crate name of the library.
    name: String,
    /// The root file, relative to the package.
    root: PathBuf,
    /// The crates the reader read, the crate of the package first and its
    /// path dependencies after it.
    libraries: Vec<Library>,
    /// The modules, the items and the files of the crate and of its path
    /// dependencies.
    tree: Tree,
    /// The entry, the first of several.
    entry: Option<Entry>,
    /// The parts of the roots the chain of the entry registers.
    registrations: Vec<Registration>,
    /// The forms the reader refuses.
    refusals: Vec<Refusal>,
}

impl fmt::Debug for Crate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Crate")
            .field("package", &self.package)
            .field("name", &self.name)
            .field("root", &self.root)
            .field("files", &self.tree.files)
            .field("entry", &self.entry)
            .field("registrations", &self.registrations)
            .field("refusals", &self.refusals)
            .finish_non_exhaustive()
    }
}

/// Reads the library crate of the package in the directory `package`, as
/// rustc compiles it over the active set `cfg` of the target of the build.
///
/// The crate root is the `path` of `[lib]` in `Cargo.toml`, or cargo's default
/// `src/lib.rs`. The reader reads the manifest of the package and of every
/// path dependency the manifest of a crate it read names under
/// `[dependencies]` or under a `[target.<platform>.dependencies]` whose
/// platform holds over the target information of `cfg`, which no flag of the
/// profile changes, as cargo takes it, [`Cfg::platform_holds`], each once, and
/// the features cargo enables for each of them over those entries. It then
/// walks the module tree of each crate from its root, every file stripped as
/// rustc strips it over `cfg` and the features of its crate, finds the entry
/// and reads the roots its chain registers. A dependency takes its path from
/// its own entry or, with `workspace = true`, from the entry of
/// `[workspace.dependencies]` of the root of its workspace, as cargo takes it.
/// The facade and a dependency with no path, of a registry or of git, are not
/// read. A form the reader cannot read, two entries, a root chosen at run time
/// among them or a `cfg` with a key the reader cannot evaluate, is a
/// [`Refusal`] the crate carries and no error.
///
/// # Errors
///
/// Returns [`SourceError::Io`] when a manifest or a file of a tree cannot be
/// read, [`SourceError::Parse`] when a file does not parse as Rust, and
/// [`SourceError::Circular`] when a file is its own module.
pub fn read(package: &Path, cfg: &Cfg) -> Result<Crate, SourceError> {
    let read = manifests(package, cfg)?;
    let packages: Vec<Package<'_>> = read
        .manifests
        .iter()
        .zip(&read.edges)
        .map(|(manifest, edges)| Package {
            features: &manifest.features,
            edges,
        })
        .collect();
    let features = features::enabled(&packages);
    let mut tree = Tree::default();
    let mut libraries: Vec<Library> = Vec::with_capacity(read.manifests.len());
    for (krate, (library, externs)) in read.manifests.iter().zip(read.externs).enumerate() {
        let active = Active {
            cfg,
            features: &features[krate],
        };
        let root_file = normal(&read.dirs[krate].join(&library.root));
        let root = tree::walk(&mut tree, package, &root_file, krate, active)?;
        libraries.push(Library {
            root,
            name: library.name.clone(),
            package: library.package.clone(),
            externs,
        });
    }
    let main = &read.manifests[0];
    let mut krate = Crate {
        package: main.package.clone(),
        name: main.name.clone(),
        root: main.root.clone(),
        libraries,
        refusals: std::mem::take(&mut tree.refusals),
        tree,
        entry: None,
        registrations: Vec::new(),
    };
    let registered = entry::read(&krate.resolver());
    krate.entry = registered.entry;
    krate.registrations = registered.registrations;
    krate.refusals.extend(registered.refusals);
    Ok(krate)
}

/// The manifests of the crates a read reads, the package first and its path
/// dependencies after it, each with its directory and its dependencies.
struct Crates {
    /// The library each manifest states.
    manifests: Vec<manifest::Library>,
    /// The directory of each, relative to the package.
    dirs: Vec<PathBuf>,
    /// The dependencies of each, as the features read them.
    edges: Vec<Vec<Edge>>,
    /// The crates each names, each by the name it sees the crate by, with
    /// the library the reader reads for a path dependency.
    externs: Vec<Vec<(String, Option<usize>)>>,
}

/// Returns the manifest of the package in the directory `package` and of
/// every path dependency the manifest of a crate it read names in a table
/// that cargo takes for the build of `cfg`, each once.
fn manifests(package: &Path, cfg: &Cfg) -> Result<Crates, SourceError> {
    let mut read = Crates {
        manifests: vec![manifest_of(package, Path::new(""))?],
        dirs: vec![PathBuf::new()],
        edges: Vec::new(),
        externs: Vec::new(),
    };
    // The canonical form of each directory tells a dependency two crates
    // name apart.
    let mut identities = vec![canonical(package)?];
    while read.edges.len() < read.dirs.len() {
        let krate = read.edges.len();
        let dir = read.dirs[krate].clone();
        let mut edges = Vec::new();
        let mut externs = Vec::new();
        for dependency in read.manifests[krate].dependencies.clone() {
            if let Some(platform) = &dependency.platform
                && !cfg.platform_holds(platform)
            {
                continue;
            }
            let (entry, found) =
                dependency_dir(package, &dir, &read.manifests[krate], &dependency)?;
            let target = match found {
                Some(found) => {
                    let identity = canonical(&package.join(&found))?;
                    let known = identities.iter().position(|seen| *seen == identity);
                    if known.is_none() {
                        read.manifests.push(manifest_of(package, &found)?);
                        read.dirs.push(found);
                        identities.push(identity);
                    }
                    Some(known.unwrap_or(read.dirs.len() - 1))
                }
                None => None,
            };
            externs.push((dependency.name.clone(), target));
            edges.push(Edge {
                key: dependency.key,
                target,
                optional: dependency.optional,
                default_features: entry.default_features,
                features: entry.features,
            });
        }
        read.edges.push(edges);
        read.externs.push(externs);
    }
    Ok(read)
}

/// Returns the library the manifest of the package in the directory `dir`,
/// relative to `package`, states.
fn manifest_of(package: &Path, dir: &Path) -> Result<manifest::Library, SourceError> {
    let path = package.join(dir).join("Cargo.toml");
    let text = fs::read_to_string(&path).map_err(|error| SourceError::Io(path, error))?;
    Ok(manifest::library(&text))
}

/// Returns the canonical form of the directory `dir`.
fn canonical(dir: &Path) -> Result<PathBuf, SourceError> {
    fs::canonicalize(dir).map_err(|error| SourceError::Io(dir.to_path_buf(), error))
}

/// Returns the entry of the dependency `dependency` of the library `library`
/// in the directory `dir`, with the features of the entry of the workspace
/// it takes, and the directory, relative to `package`, of the library it
/// names, `None` for the facade, which the reader maps by its table, and for
/// a crate of a registry or of git, whose source it does not read.
///
/// An entry with `workspace = true` takes the source and the default features
/// of the entry of the workspace and adds its own features to those of that
/// entry.
fn dependency_dir(
    package: &Path,
    dir: &Path,
    library: &manifest::Library,
    dependency: &Dependency,
) -> Result<(Dependency, Option<PathBuf>), SourceError> {
    if resolve::is_known_crate(&dependency.name) {
        return Ok((dependency.clone(), None));
    }
    let (base, entry) = if dependency.workspace {
        let Some((root, shared)) = workspace_of(package, dir, library)? else {
            return Ok((dependency.clone(), None));
        };
        let Some(mut entry) = shared
            .into_iter()
            .find(|entry| entry.name == dependency.name)
        else {
            return Ok((dependency.clone(), None));
        };
        entry.features.extend(dependency.features.iter().cloned());
        (root, entry)
    } else {
        (dir.to_path_buf(), dependency.clone())
    };
    if entry.package == FACADE {
        return Ok((entry, None));
    }
    let found = entry.path.as_ref().map(|path| normal(&base.join(path)));
    Ok((entry, found))
}

/// Returns the directory of the root of the workspace of the library
/// `library` in the directory `dir`, relative to `package`, with the entries
/// of its `[workspace.dependencies]`, or `None` for a package in no
/// workspace.
///
/// The root is the package itself when its manifest holds `[workspace]`, the
/// directory the key `workspace` of `[package]` names, or else the nearest
/// directory above it whose manifest holds `[workspace]`, as cargo finds it.
fn workspace_of(
    package: &Path,
    dir: &Path,
    library: &manifest::Library,
) -> Result<Option<(PathBuf, Vec<Dependency>)>, SourceError> {
    if library.workspace_root {
        return Ok(Some((
            dir.to_path_buf(),
            library.workspace_dependencies.clone(),
        )));
    }
    if let Some(root) = &library.workspace {
        let root = normal(&dir.join(root));
        let named = manifest_of(package, &root)?;
        return Ok(Some((root, named.workspace_dependencies)));
    }
    let start = canonical(&package.join(dir))?;
    let mut up = dir.to_path_buf();
    for ancestor in start.ancestors().skip(1) {
        up = normal(&up.join(".."));
        let path = ancestor.join("Cargo.toml");
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let named = manifest::library(&text);
        if named.workspace_root {
            return Ok(Some((up, named.workspace_dependencies)));
        }
    }
    Ok(None)
}

/// Returns `path` with every `.` left out and every `..` that follows a name
/// taken together with that name, so a file of a path dependency is named
/// from the package by the shortest relative path.
fn normal(path: &Path) -> PathBuf {
    let mut parts: Vec<Component<'_>> = Vec::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir if matches!(parts.last(), Some(Component::Normal(_))) => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.iter().collect()
}

impl Crate {
    /// Returns the name of the package, as `CARGO_PKG_NAME` holds it.
    #[must_use]
    pub fn package(&self) -> &str {
        &self.package
    }

    /// Returns the crate name of the library.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns whether the crate is plain, which is whether no macro call of
    /// `graphql!` stands at the level of the items of its modules, as the
    /// strip of the build leaves them.
    ///
    /// The roots of a crate of the derive path stand in a fragment of
    /// `graphql!`, which the reader cannot read, and that crate keeps the path
    /// through `emit_from` and `inspect_from`.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        !self.macro_calls().iter().any(|(path, _)| {
            path.rsplit("::")
                .next()
                .is_some_and(|last| last == "graphql")
        })
    }

    /// Returns the root file of the crate, relative to the package.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns every file the reader read, relative to the package, in the
    /// order of the walk, the files of the crate first and those of its path
    /// dependencies after them.
    #[must_use]
    pub fn files(&self) -> &[PathBuf] {
        &self.tree.files
    }

    /// Returns the entry, or `None` for a crate whose root holds no public
    /// function that answers an app.
    #[must_use]
    pub fn entry(&self) -> Option<&Entry> {
        self.entry.as_ref()
    }

    /// Returns the parts of the roots the chain of the entry registers, the
    /// parent's first and the children's of `compose` after them.
    #[must_use]
    pub fn registrations(&self) -> &[Registration] {
        &self.registrations
    }

    /// Returns the forms the reader refuses, in the order it meets them.
    #[must_use]
    pub fn refusals(&self) -> &[Refusal] {
        &self.refusals
    }

    /// Returns every macro call at the level of the items of a module of the
    /// crate, with its path and its place, whose expansion the reader cannot
    /// see.
    #[must_use]
    pub fn macro_calls(&self) -> Vec<(String, Place)> {
        self.tree
            .items
            .iter()
            .filter(|item| self.tree.modules[item.module].krate == 0)
            .filter_map(|item| match &item.kind {
                model::Kind::Macro(call) if call.defines.is_none() => {
                    Some((call.path.clone(), item.place.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// Returns what the roots of `start` reach.
    #[must_use]
    pub fn reach(&self, start: &Start) -> Reach {
        reach::reach(&self.resolver(), &self.registrations, start)
    }

    /// Returns the resolver of the paths of the crate.
    fn resolver(&self) -> Resolver<'_> {
        Resolver {
            modules: &self.tree.modules,
            items: &self.tree.items,
            libraries: &self.libraries,
        }
    }
}
