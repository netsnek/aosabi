// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the watcher of the sources of the development loop.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/dev/dev-server.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The watcher.
//!
//! [`Watcher`] polls the sets of its role every
//! [`POLL_INTERVAL`] with `std::fs::metadata`, since an event interface of the
//! kernel needs a binding in `unsafe` code. The web role watches `src/**`,
//! `aosabi.config.rs`, `Cargo.toml` and `Cargo.lock` of the project, the
//! worker role `src/**` and `aosabi.config.rs`, and a directory named `target`
//! or `.git` is never entered. A path counts when it ends in `.rs` or `.toml`
//! or is `Cargo.lock`, so a file a request writes into the project starts no rebuild. A path
//! counts as changed when it appears, vanishes, or changes its modification
//! time with its nanoseconds, its length or its inode, so a save that keeps
//! the length and the time to the nanosecond and writes the file in place is
//! the one change a poll misses.
//!
//! [`classify`] answers [`Kind::Config`] for `aosabi.config.rs`, `Cargo.toml`
//! and `Cargo.lock` at the root and [`Kind::Server`] for every other path, and
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
    /// Whether the set is the one of the worker role.
    worker: bool,
    /// The stamp of every watched file with its path, in the order of the
    /// paths.
    snapshot: Vec<(PathBuf, Stamp)>,
}

impl Watcher {
    /// Returns a watcher of the project at `root` for the worker role when
    /// `worker`, with the snapshot of the files as they stand.
    ///
    /// # Errors
    ///
    /// Returns the error of the root, which the walk cannot read.
    pub fn new(root: &Path, worker: bool) -> io::Result<Self> {
        let mut watcher = Self {
            root: root.to_path_buf(),
            worker,
            snapshot: Vec::new(),
        };
        watcher.snapshot = watcher.scan()?;
        Ok(watcher)
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
            out.push((changed.clone(), classify(&self.root, changed)));
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
        let roots: &[&str] = if self.worker {
            &[CONFIG_FILE]
        } else {
            &[CONFIG_FILE, MANIFEST_FILE, LOCK_FILE]
        };
        for name in roots {
            let path = self.root.join(name);
            if let Some(stamp) = stamp_of(&path) {
                out.push((path, stamp));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
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
