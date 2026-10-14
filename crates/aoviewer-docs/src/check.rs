// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs check: the check mode, the tree in memory against the tree on
 * disk.
 *
 * A project that commits its reference adds the check mode to its own pipeline,
 * and a one byte change makes it exit non zero.
 */

//! The check mode.
//!
//! The function [`check`] compares a tree in memory with the tree under
//! `--out`, file by file, writes nothing, and answers the first difference with
//! its path and the first differing byte offset, or a missing or an extra file.
//! The command refuses the difference with `AO-D0008`, so [`CheckDiff`] turns
//! into [`DocsError::Check`].
//!
//! The tree on disk is what stands below the output directory. The check
//! passes exactly when the files of the tree stand there with their bytes in
//! the directories of the tree and nothing else does, apart from three files
//! beside the tree that the writer leaves to the reader.
//!
//! - The PDF `api.pdf` is not compared, since the PDF is compiled from the tree
//!   and is no file of it.
//! - The template `template.typ` is not compared either, since
//!   `--typst-template` places it beside `api.typ` and the tree imports it and
//!   does not write it.
//! - The theme `sabi-theme.typ` is compared only when it equals neither the
//!   file the generator writes nor the kept file. The writer keeps any regular
//!   file of that name, so such a file on disk is the kept file whatever it
//!   holds, and the check reports the theme only as missing, when the tree
//!   holds one and no regular file of that name stands on disk.
//!
//! So a check right after [`write`](crate::write::write) of the same tree
//! passes, unless the directory holds entries no manifest lists, which the
//! writer leaves where they are and the check reports. Any other file of the
//! tree that stands on disk as a symbolic link or a directory, or that
//! cannot be read, is missing, since the writer would write it. An output
//! directory that is missing or no directory holds no entry, and every file
//! of the tree is missing there.
//!
//! The walk lists the output directory and the directories of the tree, and
//! descends into nothing else. An entry the tree lacks is an extra at its own
//! path, a directory as much as a file, empty or not, and nothing below it is
//! read, since its path comes before every path below it. A directory of the
//! tree that stands on disk as a file or a symbolic link is missing at its
//! own path in the same way, since the writer refuses it, so the check reads
//! no file through such a link. A directory of the tree that cannot be
//! listed adds no entry, and its files are read one by one, so they are
//! missing when it cannot be searched either, and an extra file in a
//! directory that can be searched and not listed passes unseen.
//!
//! The differences are ordered by the bytes of their paths, as the files of a
//! tree are, and the first one is answered. Past [`MAX_EXTRA`] entries the
//! tree lacks, the walk stops, and the check answers the first difference
//! among the entries it read.
//!
//! Every loop runs over the files of the tree, the entries of its
//! directories, which [`MAX_EXTRA`] bounds beyond the tree, the chunks of one
//! file, whose length the file of the tree bounds, or the bytes of one chunk.

use std::{
    fs::{
        self,
        File, //
    },
    io::{
        self,
        ErrorKind,
        Read as _, //
    },
    path::Path,
};

use aoviewer_core::tree::{
    Tree,
    TreeFile, //
};

use crate::{
    error::{
        DocsError,
        Mismatch, //
    },
    write::{
        THEME,
        directories, //
    },
};

/// The most entries the tree lacks that the walk reads before it stops.
///
/// The walk reads the files and the directories of the tree and at most this
/// many entries more, so a directory of the tree that holds an unbounded
/// number of other entries costs a bounded walk.
pub const MAX_EXTRA: usize = 1 << 16;

/// The files beside a tree that the check does not compare and does not
/// count as extra.
const BESIDE: [&str; 3] = ["api.pdf", "template.typ", THEME];

/// The length of the chunks a file on disk is read and compared in.
const CHUNK: usize = 1 << 16;

/// The buffer of [`CHUNK`] bytes that [`file_difference`] reads a file into,
/// made once per run and used for each file.
pub(crate) struct Chunk(Box<[u8]>);

impl Chunk {
    /// Returns a buffer of [`CHUNK`] bytes.
    pub(crate) fn new() -> Chunk {
        Chunk(vec![0; CHUNK].into_boxed_slice())
    }
}

/// The first difference between a tree in memory and the tree on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckDiff {
    /// The path inside the tree, components joined by `/`.
    pub path: String,
    /// How the file differs, missing, extra or at a byte offset.
    pub mismatch: Mismatch,
}

impl From<CheckDiff> for DocsError {
    /// Returns the refusal `AO-D0008` of the difference.
    fn from(d: CheckDiff) -> Self {
        DocsError::Check {
            path: d.path,
            mismatch: d.mismatch,
        }
    }
}

/// Compares `tree` with the tree under the directory `out`.
///
/// Every file of `tree` is read from `out` and compared byte by byte, and
/// every entry the walk of the module documentation finds below `out` that
/// `tree` lacks is an extra, but for the three files beside the tree. The
/// directory `out` is read and never written.
///
/// # Errors
///
/// Returns the [`CheckDiff`] of the first difference in the byte order of
/// the paths: [`Mismatch::Missing`] for a file or a directory of `tree` that
/// `out` lacks, [`Mismatch::Extra`] for an entry `tree` lacks, and
/// [`Mismatch::Differs`] with the first differing byte offset for a file
/// whose bytes differ.
pub fn check(tree: &Tree, out: &Path) -> Result<(), CheckDiff> {
    let mut files: Vec<&TreeFile> = tree.files.iter().collect();
    files.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut found = walk(out, &paths, &directories(&paths))
        .into_iter()
        .peekable();
    let mut chunk = Chunk::new();
    for f in files {
        if let Some(d) = found.next_if(|d| d.path.as_str() < f.path.as_str()) {
            return Err(d);
        }
        if let Some(m) = compare(out, f, &mut chunk) {
            return Err(difference(f.path.clone(), m));
        }
    }
    found.next().map_or(Ok(()), Err)
}

/// Returns the difference of `path` of kind `mismatch`.
fn difference(path: String, mismatch: Mismatch) -> CheckDiff {
    CheckDiff { path, mismatch }
}

/// Returns how the file `f` of the tree differs from its file under `out`,
/// or `None` when the two are equal.
///
/// A theme that is a regular file is equal whatever it holds, since the
/// writer keeps it.
fn compare(out: &Path, f: &TreeFile, chunk: &mut Chunk) -> Option<Mismatch> {
    let path = out.join(&f.path);
    let Ok(meta) = fs::symlink_metadata(&path) else {
        return Some(Mismatch::Missing);
    };
    if !meta.is_file() {
        return Some(Mismatch::Missing);
    }
    if f.path == THEME {
        return None;
    }
    match file_difference(&path, &f.bytes, chunk) {
        Ok(None) => None,
        Ok(Some(offset)) => Some(Mismatch::Differs { offset }),
        Err(_) => Some(Mismatch::Missing),
    }
}

/// Returns the first byte offset at which the file at `path` differs from
/// `bytes`, the length of the shorter one when it is a prefix of the other,
/// or `None` when the two are equal.
///
/// The file is read into `chunk` a chunk at a time, and at most one byte more
/// than `bytes` holds is read, so the length of `bytes` bounds the read
/// whatever the length of the file. The caller opens a regular file alone,
/// since opening a pipe waits for a writer.
///
/// # Errors
///
/// Returns the error of opening or reading the file.
pub(crate) fn file_difference(
    path: &Path,
    bytes: &[u8],
    chunk: &mut Chunk,
) -> io::Result<Option<u64>> {
    let mut file = File::open(path)?;
    let mut at = 0usize;
    loop {
        let rest = &bytes[at..];
        let n = match file.read(&mut chunk.0[..rest.len().clamp(1, CHUNK)]) {
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if n == 0 {
            return Ok((!rest.is_empty()).then(|| offset(at)));
        }
        let read = &chunk.0[..n];
        // A read asks for more than `rest` holds only when `rest` is empty,
        // so a byte beyond it is a file longer than the tree's.
        let Some(expected) = rest.get(..n) else {
            return Ok(Some(offset(at)));
        };
        if read != expected {
            let same = read
                .iter()
                .zip(expected)
                .take_while(|(x, y)| x == y)
                .count();
            return Ok(Some(offset(at + same)));
        }
        at += n;
    }
}

/// Returns the byte offset `at` as the offset of a [`Mismatch::Differs`].
fn offset(at: usize) -> u64 {
    u64::try_from(at).unwrap_or(u64::MAX)
}

/// Returns the differences the entries below `out` show against the tree of
/// the sorted `paths` and `dirs`, in the byte order of the paths.
///
/// A directory on disk is descended into when it is one of `dirs`, and any
/// other entry of that path is a missing directory. An entry that is neither
/// one of `dirs` nor one of `paths` nor a file beside the tree is an extra.
/// A name that is no UTF-8 is written with the replacement character, which
/// no path of a tree holds.
fn walk(out: &Path, paths: &[&str], dirs: &[String]) -> Vec<CheckDiff> {
    let most = paths.len() + dirs.len() + BESIDE.len() + MAX_EXTRA;
    let mut found: Vec<CheckDiff> = Vec::new();
    let mut pending = vec![String::new()];
    let mut read = 0usize;
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(out.join(&dir)) else {
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                break;
            };
            read += 1;
            if read > most {
                found.sort_unstable_by(|a, b| a.path.cmp(&b.path));
                return found;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let path = if dir.is_empty() {
                String::from(name)
            } else {
                format!("{dir}/{name}")
            };
            if dirs.binary_search(&path).is_ok() {
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    pending.push(path);
                } else {
                    found.push(difference(path, Mismatch::Missing));
                }
            } else if paths.binary_search(&path.as_str()).is_err()
                && !(dir.is_empty() && BESIDE.contains(&path.as_str()))
            {
                found.push(difference(path, Mismatch::Extra));
            }
        }
    }
    found.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    found
}
