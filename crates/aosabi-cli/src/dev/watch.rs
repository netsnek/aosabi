// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the watcher of the sources of the development loop.
 *
 * Derived from Pylon 3.0.0-canary by Nico Schett,
 * packages/pylon/src/cli/dev/dev-server.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The watcher.
//!
//! The reference watches the whole working directory with chokidar, leaves
//! `node_modules`, `.pylon` and `.git` out, reacts to a path that matches
//! `/\.([cm]?[jt]sx?|css)$/` alone, and ranks the kind of a path `pages` below
//! `server` below `config`. Its worker role watches `src` and the
//! configuration file.
//!
//! [`Watcher`] polls its set every [`POLL_INTERVAL`] with `std::fs::metadata`,
//! since an event interface of the kernel needs a binding in `unsafe` code.
//! Both roles watch `src/**`, `aosabi.config.rs`, `Cargo.toml` and
//! `Cargo.lock` of the project, the worker role the manifest and the lock
//! beyond the set of the reference, since only a reload reads the manifest
//! again with the refusals of the loader, `AO-C0108` among them, and a
//! directory named `target` or `.git` is never entered. Both roles also watch
//! the files the last load that succeeded lists beside those: the manifest of
//! every path package of the graph of the default features, a package outside
//! the root among them, the root manifest and the lock of the workspace of the
//! project where they lie above the root, and `config.toml` and `config` of
//! the folder `.cargo` of the root and of every directory above it and of the
//! cargo home, so an edit of one reads the project again with the refusals of
//! the loader before the next build. The set holds no file a configuration
//! pulls in with `include`, no root manifest of the workspace of a path
//! package outside the root and no manifest of a path package behind a feature
//! the default graph leaves off. A change of one of those alone, or one that
//! lands while a reload runs, starts no reload, and the next build reads the
//! lock of the project and the lock it derives with the pin check of the
//! loader, so it refuses a crate of Aosabi from a registry such a change
//! brings in with `AO-C0108` before cargo compiles. A path counts when it ends
//! in `.rs` or `.toml` or is `Cargo.lock`, so a file a request writes into the
//! project starts no rebuild. A path counts as changed when it appears,
//! vanishes, or changes its modification time with its nanoseconds, its length
//! or its inode, so a save that keeps the length and the time to the
//! nanosecond and writes the file in place is the one change a poll misses,
//! which the next build meets in the same way.
//!
//! [`classify`] answers [`Kind::Config`] for `aosabi.config.rs`, `Cargo.toml`
//! and `Cargo.lock` at the root and [`Kind::Server`] for every other path,
//! the watcher answers [`Kind::Config`] for every other file the last load
//! lists as well, a manifest of a path package under `src` among them, and
//! the order of [`Kind`] ranks `server` below `config`.

use std::{
    cmp::Ordering,
    fs,
    io,
    path::{
        Path,
        PathBuf, //
    },
    time::{
        Duration,
        SystemTime, //
    }, //
};

use crate::project::{
    CONFIG_FILE,
    LOCK_FILE, //
};

/// The time between two polls.
pub const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// The manifest of the project, a configuration path.
const MANIFEST_FILE: &str = "Cargo.toml";

/// The directory of the sources.
const SOURCE_DIR: &str = "src";

/// The directories the walk never enters.
const SKIPPED_DIRS: [&str; 2] = ["target", ".git"];

/// The kind of a changed path, in the order of its rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A source of the service, `server` in the reload line.
    Server,
    /// The configuration, the manifest or the lock, `config` in the reload
    /// line.
    Config,
}

impl Kind {
    /// Returns the name the reload line prints.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Config => "config",
        }
    }
}

/// Returns the kind of the path `path` of the project at `root`.
#[must_use]
pub fn classify(root: &Path, path: &Path) -> Kind {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if [CONFIG_FILE, MANIFEST_FILE, LOCK_FILE]
        .iter()
        .any(|name| relative == Path::new(name))
    {
        Kind::Config
    } else {
        Kind::Server
    }
}

/// What a poll compares of one watched file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    /// The modification time since the epoch, with its nanoseconds, `None`
    /// where the file system gives none.
    modified: Option<Duration>,
    /// The length in bytes.
    len: u64,
    /// The inode, 0 on a host that gives none.
    inode: u64,
}

/// The snapshot of the watched files of a project, `(path, modified, len,
/// inode)` per file.
#[derive(Clone, Debug)]
pub struct Watcher {
    /// The project root.
    root: PathBuf,
    /// The other files the last load lists, outside `src` and beside the
    /// three files of the root, which the walk does not stamp, in the order
    /// of their paths.
    loaded: Vec<PathBuf>,
    /// Every file the last load lists, those under `src` among them, which
    /// count as [`Kind::Config`], in the order of their paths.
    config: Vec<PathBuf>,
    /// The stamp of every watched file with its path, in the order of the
    /// paths.
    snapshot: Vec<(PathBuf, Stamp)>,
}

impl Watcher {
    /// Returns a watcher of the project at `root`, the same set in both
    /// roles, with the other files `loaded` a load of the project reads, and
    /// with the snapshot of the files as they stand.
    ///
    /// # Errors
    ///
    /// Returns the error of the root, which the walk cannot read.
    pub fn new(root: &Path, loaded: &[PathBuf]) -> io::Result<Self> {
        let mut watcher = Self {
            root: root.to_path_buf(),
            loaded: Vec::new(),
            config: sorted(loaded),
            snapshot: Vec::new(),
        };
        watcher.loaded = watcher.others(loaded);
        watcher.snapshot = watcher.scan()?;
        Ok(watcher)
    }

    /// Takes `loaded` as the other files a load of the project reads, as the
    /// last load that succeeded lists them, and changes the snapshot for the
    /// files that join or leave that set alone, stamped as they stand now, so
    /// a change of any other watched file since the last poll still counts at
    /// the next one.
    pub fn watch_loaded(&mut self, loaded: &[PathBuf]) {
        self.config = sorted(loaded);
        let loaded = self.others(loaded);
        if loaded == self.loaded {
            return;
        }
        let was = std::mem::replace(&mut self.loaded, loaded);
        self.snapshot.retain(|(path, _)| {
            was.binary_search(path).is_err() || self.loaded.binary_search(path).is_ok()
        });
        for path in &self.loaded {
            if was.binary_search(path).is_err()
                && let Some(stamp) = stamp_of(path)
            {
                self.snapshot.push((path.clone(), stamp));
            }
        }
        self.snapshot.sort_by(|a, b| a.0.cmp(&b.0));
    }

    /// Returns the files of `loaded` that the walk of `src` and the three
    /// files of the root leave out, in the order of their paths.
    fn others(&self, loaded: &[PathBuf]) -> Vec<PathBuf> {
        let sources = self.root.join(SOURCE_DIR);
        let mut others: Vec<PathBuf> = loaded
            .iter()
            .filter(|path| {
                !path.starts_with(&sources) && classify(&self.root, path) == Kind::Server
            })
            .cloned()
            .collect();
        others.sort();
        others.dedup();
        others
    }

    /// Returns the kind of the watched path `path`: [`Kind::Config`] for a
    /// file the last load lists, and the answer of [`classify`] for every
    /// other.
    fn kind(&self, path: &Path) -> Kind {
        if self
            .config
            .binary_search_by(|loaded| loaded.as_path().cmp(path))
            .is_ok()
        {
            Kind::Config
        } else {
            classify(&self.root, path)
        }
    }

    /// Returns the source paths that appeared, vanished or changed since the
    /// last call, in the order of their paths, each with its kind.
    ///
    /// # Errors
    ///
    /// Returns the error of the root, which the walk cannot read. A file or a
    /// directory below it that vanishes during the walk counts as vanished.
    // An `inotify` binding would see a save without a poll.
    pub fn poll(&mut self) -> io::Result<Vec<(PathBuf, Kind)>> {
        let next = self.scan()?;
        let mut out = Vec::new();
        let (mut old, mut new) = (self.snapshot.iter().peekable(), next.iter().peekable());
        loop {
            let changed = match (old.peek(), new.peek()) {
                (None, None) => break,
                (Some((path, _)), None) => {
                    old.next();
                    path
                }
                (None, Some((path, _))) => {
                    new.next();
                    path
                }
                (Some((was, before)), Some((is, after))) => match was.cmp(is) {
                    Ordering::Less => {
                        old.next();
                        was
                    }
                    Ordering::Greater => {
                        new.next();
                        is
                    }
                    Ordering::Equal => {
                        old.next();
                        new.next();
                        if before == after {
                            continue;
                        }
                        is
                    }
                },
            };
            out.push((changed.clone(), self.kind(changed)));
        }
        self.snapshot = next;
        Ok(out)
    }

    /// Returns the stamp of every watched file as it stands, in the order of
    /// the paths.
    fn scan(&self) -> io::Result<Vec<(PathBuf, Stamp)>> {
        fs::metadata(&self.root)?;
        let mut out = Vec::new();
        walk(&self.root.join(SOURCE_DIR), &mut out);
        for name in [CONFIG_FILE, MANIFEST_FILE, LOCK_FILE] {
            let path = self.root.join(name);
            if let Some(stamp) = stamp_of(&path) {
                out.push((path, stamp));
            }
        }
        for path in &self.loaded {
            if let Some(stamp) = stamp_of(path) {
                out.push((path.clone(), stamp));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
}

/// Returns the paths `paths` in their order, each once.
fn sorted(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = paths.to_vec();
    paths.sort();
    paths.dedup();
    paths
}

/// Adds the stamp of every source below the directory `dir` to `out`,
/// entering no directory of [`SKIPPED_DIRS`] and following no link to a
/// directory.
fn walk(dir: &Path, out: &mut Vec<(PathBuf, Stamp)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            let skipped = SKIPPED_DIRS.iter().any(|name| entry.file_name() == *name);
            if !skipped {
                walk(&path, out);
            }
        } else if is_source(&path)
            && let Some(stamp) = stamp_of(&path)
        {
            out.push((path, stamp));
        }
    }
}

/// Returns whether `path` passes the source filter: it ends in `.rs` or
/// `.toml` or is named `Cargo.lock`.
fn is_source(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "rs" || extension == "toml")
        || path.file_name().is_some_and(|name| name == LOCK_FILE)
}

/// Returns the stamp of the file `path`, `None` when it is no file or
/// cannot be read.
fn stamp_of(path: &Path) -> Option<Stamp> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok());
    Some(Stamp {
        modified,
        len: metadata.len(),
        inode: inode(&metadata),
    })
}

/// Returns the inode of `metadata`.
#[cfg(unix)]
fn inode(metadata: &fs::Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::ino(metadata)
}

/// Returns 0, since the host gives no inode.
#[cfg(not(unix))]
fn inode(_metadata: &fs::Metadata) -> u64 {
    0
}
