// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs write: the writing of a tree into its output directory.
 *
 * A file is written only when it changed, the theme file is kept when it
 * exists as a regular file, and a file of the previous manifest that the new
 * tree lacks is removed.
 */

//! The writing of a tree.
//!
//! The function [`write()`] puts a [`Tree`] under its output directory in four
//! steps.
//!
//! 1. Every file the previous `manifest.json` lists and the new tree lacks is
//!    removed, and every empty directory of such a file that the new tree
//!    does not use, whether this run, an earlier run that failed or a reader
//!    emptied it. The removal comes first, so on a file system that folds
//!    case a page whose name changed only by case is removed before its new
//!    name is written to the same file.
//! 2. When the tree holds a file the previous manifest does not list, a
//!    pending manifest takes its place before any directory or file is made.
//!    It lists the files of both, each with an empty hash and a length of 0
//!    beside the member `"pending": true`, so a run that fails or stops after
//!    it leaves no file that no manifest lists.
//! 3. Every file of the tree is written when its bytes differ from the file on
//!    disk and left alone when they are equal, so a second run over an
//!    unchanged schema writes nothing and keeps every modification time. A
//!    file is written to the temporary file `.aoviewer-docs.new` in its
//!    directory, which a rename then puts in its place, so a reader at the
//!    same time and a run that stops see the old file or the new one and never
//!    a part of it, and a file that other names link to keeps its bytes under
//!    them. The theme `sabi-theme.typ` is kept whatever it holds when a
//!    regular file of that name exists, since a reader may have edited it. Any
//!    other entry there is replaced by the theme, or refused when it is a
//!    directory, as at every other file of the tree.
//! 4. The manifest `manifest.json` of the tree is written last, so a run that
//!    fails halfway leaves the manifest of the last whole tree or the pending
//!    one, and the next run removes the files they list that its tree lacks.
//!
//! Every path of the tree and of the previous manifest is relative, its
//! components joined by `/`, each a run of ASCII letters, digits, `_`, `-` and
//! `.` that does not begin with `.`, so no component is `.` or `..`. Every
//! path the core writes has that shape, since its formats in `tree/plan.rs`,
//! `tree/mod.rs` and `page_path` of `render_markdown/mod.rs` of
//! crates/aoviewer-core/src join fixed words, digits, slugs of `[a-z0-9-]` and
//! names the model builder holds to `[_A-Za-z][_0-9A-Za-z]*`. A path of
//! another shape in the tree is refused before a file is touched, and one in
//! the previous manifest is never removed, so nothing outside the output
//! directory is written or removed. The writer neither writes nor removes
//! through a symbolic link below the output directory: a link that stands
//! where the tree has a directory is refused, a link that stands where it has
//! a file is replaced by the file, and a listed file below such a link stays.
//! The output directory itself may be a link, since the caller names it. A
//! pipe, a socket or a device that stands where the tree has a file is
//! replaced by the file as a link is, so the writer never opens one and never
//! waits on its other end. A run removes the temporary file that a run which
//! stopped between the write and the rename left in the output directory, in
//! each directory of its tree and in each directory it empties, and one output
//! directory takes one run at a time.
//!
//! A previous manifest that is missing, no regular file, unreadable or of
//! another shape than the one the core writes lists nothing, so the run
//! removes nothing, and the new manifest replaces it. A manifest that is a
//! symbolic link, a pipe or a device is never opened.
//!
//! Every loop runs over the files of the tree, the directories of their
//! paths, the entries of the previous manifest, whose length
//! [`ReadLimits::DEFAULT`] caps, or the components of one path.

use std::{
    fs::{
        self,
        File,
        OpenOptions, //
    },
    io::{
        self,
        ErrorKind,
        Read as _,
        Write as _, //
    },
    path::{
        Path,
        PathBuf, //
    },
};

use aoviewer_core::{
    json::{
        Parser,
        Token, //
    },
    limits::ReadLimits,
    tree::{
        Tree,
        TreeFile, //
    },
};

use crate::{
    check::{
        Chunk,
        file_difference, //
    },
    error::DocsError, //
};

/// The path of the manifest in a tree.
pub(crate) const MANIFEST: &str = "manifest.json";

/// The path of the theme in a tree, which a run keeps once it exists.
pub(crate) const THEME: &str = "sabi-theme.typ";

/// The name of the file a changed file is written to in its directory before
/// a rename puts it in place.
///
/// The name begins with `.`, so it is no path of a tree and none a manifest
/// lists, and the check names one that a run left behind as extra.
pub(crate) const TEMPORARY: &str = ".aoviewer-docs.new";

/// What one run of [`write()`] did.
///
/// Every file of the tree is counted once, in `written` or in `unchanged`, so
/// the two add up to the files of the tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Written {
    /// The files written, new or changed.
    pub written: u32,
    /// The files whose bytes on disk were already the bytes of the tree, and
    /// the theme when it was kept.
    pub unchanged: u32,
    /// The files of the previous manifest that the tree lacks, removed.
    pub removed: u32,
    /// Whether `sabi-theme.typ` existed as a regular file and was kept as it
    /// is.
    pub theme_kept: bool,
}

/// Writes `tree` into the directory `out`, creating it when it is missing.
///
/// The files of the previous manifest that `tree` lacks are removed first,
/// a pending manifest is written when `tree` holds a file the previous one
/// does not list, each file of `tree` is written when it changed, and
/// `manifest.json` last. The theme is kept when it exists as a regular file
/// and is never removed.
///
/// # Errors
///
/// Returns [`DocsError::Output`] with the path and the reason when a
/// directory cannot be created, a file cannot be written, renamed or removed,
/// a symbolic link stands where the tree has a directory, or a path of
/// `tree` has another shape than the paths the core writes. Files written or
/// removed before the failure stay as they are.
pub fn write(tree: &Tree, out: &Path) -> Result<Written, DocsError> {
    let mut paths: Vec<&str> = tree.files.iter().map(|f| f.path.as_str()).collect();
    if let Some(bad) = paths.iter().find(|p| !is_tree_path(p)) {
        return Err(output(
            out.join(bad),
            io::Error::new(
                ErrorKind::InvalidInput,
                "the path is no relative path of plain names inside the output directory",
            ),
        ));
    }
    paths.sort_unstable();
    let dirs = directories(&paths);
    fs::create_dir_all(out).map_err(|e| output(out.to_path_buf(), e))?;
    remove_temporary(out)?;
    let listed = previous(out);
    let mut written = Written {
        removed: remove_stale(out, &listed, &paths, &dirs)?,
        ..Written::default()
    };
    let manifest = tree.files.iter().find(|f| f.path == MANIFEST);
    let unlisted = paths
        .iter()
        .any(|p| *p != MANIFEST && listed.binary_search_by(|l| l.as_str().cmp(p)).is_err());
    if manifest.is_some() && unlisted {
        let path = out.join(MANIFEST);
        replace(&path, &pending(&listed, &paths)).map_err(|e| output(path, e))?;
    }
    make_directories(out, &dirs)?;
    let mut chunk = Chunk::new();
    for f in tree.files.iter().filter(|f| f.path != MANIFEST) {
        put(out, f, &mut chunk, &mut written)?;
    }
    if let Some(f) = manifest {
        put(out, f, &mut chunk, &mut written)?;
    }
    Ok(written)
}

/// Returns whether `path` is a relative path of plain names that stays
/// inside the directory it is joined to.
///
/// Each component is a nonempty run of ASCII letters, digits, `_`, `-` and
/// `.` that does not begin with `.`, so no component is `.`, `..` or the
/// name of a hidden file, and the path holds no root, no separator but `/`
/// and no byte a file system reads in a way of its own.
pub(crate) fn is_tree_path(path: &str) -> bool {
    path.split('/').all(|c| {
        !c.is_empty()
            && !c.starts_with('.')
            && c.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    })
}

/// Returns the refusal of the path `path` for `error`.
fn output(path: PathBuf, error: io::Error) -> DocsError {
    DocsError::Output { path, error }
}

/// Returns the directories of the sorted `paths`, each proper prefix that
/// ends before a `/`, sorted and without repeats.
///
/// A directory sorts before every path below it, since its name is a prefix
/// of theirs, so creating them in this order creates a parent first.
pub(crate) fn directories(paths: &[&str]) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for p in paths {
        for (i, _) in p.match_indices('/') {
            dirs.push(String::from(&p[..i]));
        }
    }
    dirs.sort_unstable();
    dirs.dedup();
    dirs
}

/// Returns whether every directory on the way from `out` to the relative
/// `path` is a directory and no symbolic link.
fn inside(out: &Path, path: &str) -> io::Result<bool> {
    for (i, _) in path.match_indices('/') {
        match fs::symlink_metadata(out.join(&path[..i])) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => return Ok(false),
            Err(e) if is_absent(&e) => return Ok(false),
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

/// Returns whether the relative `dir` names a directory under `out` with
/// every directory on the way to it, none of them a symbolic link.
fn is_directory(out: &Path, dir: &str) -> io::Result<bool> {
    if !inside(out, dir)? {
        return Ok(false);
    }
    match fs::symlink_metadata(out.join(dir)) {
        Ok(m) => Ok(m.is_dir()),
        Err(e) if is_absent(&e) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Returns whether `e` says that a path names nothing.
fn is_absent(e: &io::Error) -> bool {
    matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
}

/// Removes every file of the sorted `listed` paths of the previous manifest
/// under `out` that the sorted `paths` of the new tree lack, and then every
/// empty directory of such a file that is none of the sorted `dirs` of the
/// new tree, and returns the files removed.
///
/// A directory is tried whether or not its file was removed now, so one
/// that an earlier run or a reader emptied goes as well, and the temporary
/// file of a stopped run is removed from it first. The theme and the
/// manifest are never removed, and neither is a listed path of another
/// shape than a path of the tree, a directory, a file below a symbolic link
/// or a directory that is a link or stands below one.
fn remove_stale(
    out: &Path,
    listed: &[String],
    paths: &[&str],
    dirs: &[String],
) -> Result<u32, DocsError> {
    let mut removed = 0u32;
    let mut emptied: Vec<&str> = Vec::new();
    for p in listed {
        if p == MANIFEST || p == THEME || paths.binary_search(&p.as_str()).is_ok() {
            continue;
        }
        for (i, _) in p.match_indices('/') {
            emptied.push(&p[..i]);
        }
        let path = out.join(p);
        if !inside(out, p).map_err(|e| output(path.clone(), e))? {
            continue;
        }
        match fs::symlink_metadata(&path) {
            Ok(m) if m.is_dir() => continue,
            Ok(_) => {}
            Err(e) if is_absent(&e) => continue,
            Err(e) => return Err(output(path, e)),
        }
        fs::remove_file(&path).map_err(|e| output(path, e))?;
        removed = removed.saturating_add(1);
    }
    emptied.sort_unstable();
    emptied.dedup();
    // A directory sorts before the directories below it, so the reverse
    // order removes a child before its parent.
    for d in emptied.iter().rev() {
        if dirs.binary_search_by(|x| x.as_str().cmp(d)).is_ok() {
            continue;
        }
        let path = out.join(d);
        if !is_directory(out, d).map_err(|e| output(path.clone(), e))? {
            continue;
        }
        remove_temporary(&path)?;
        match fs::remove_dir(&path) {
            Ok(()) => {}
            Err(e) if is_absent(&e) || e.kind() == ErrorKind::DirectoryNotEmpty => {}
            Err(e) => return Err(output(path, e)),
        }
    }
    Ok(removed)
}

/// Returns the paths the manifest under `out` lists, each of the shape of a
/// path of the tree, sorted and without repeats, or none when the manifest
/// is missing, no regular file, unreadable or of another shape.
///
/// The manifest is opened only when it is a regular file and no symbolic
/// link, since opening a pipe waits for a writer and reading a terminal
/// waits for its input.
fn previous(out: &Path) -> Vec<String> {
    let limits = ReadLimits::DEFAULT;
    let path = out.join(MANIFEST);
    if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
        return Vec::new();
    }
    let Ok(file) = File::open(&path) else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    let most = u64::from(limits.max_input_bytes) + 1;
    if file.take(most).read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let mut listed = listed_files(&bytes, &limits).unwrap_or_default();
    listed.retain(|p| is_tree_path(p));
    listed.sort_unstable();
    listed.dedup();
    listed
}

/// Returns the pending manifest of the sorted `listed` paths of the previous
/// manifest and the sorted `paths` of the new tree.
///
/// It lists every path of both but the manifest, each as `[path, "", 0]`, in
/// the shape [`listed_files`] reads. Every path has the shape of a path of
/// the tree, so none holds a byte that a JSON string escapes.
fn pending(listed: &[String], paths: &[&str]) -> Vec<u8> {
    let mut all: Vec<&str> = listed
        .iter()
        .map(String::as_str)
        .chain(paths.iter().copied())
        .filter(|p| *p != MANIFEST)
        .collect();
    all.sort_unstable();
    all.dedup();
    let mut out = String::from("{\"version\":1,\"pending\":true,\"files\":[");
    for (i, p) in all.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("[\"");
        out.push_str(p);
        out.push_str("\",\"\",0]");
    }
    out.push_str("]}\n");
    out.into_bytes()
}

/// Returns the paths of the member `files` of the manifest `bytes`, or
/// `None` when the bytes are no JSON object whose `files` is an array of
/// `[path, sha256, bytes]` entries.
///
/// The other members are passed over whatever they hold, so a manifest of
/// another generator string or layout version lists its files as well.
fn listed_files(bytes: &[u8], limits: &ReadLimits) -> Option<Vec<String>> {
    let mut parser = Parser::new(bytes, limits);
    if parser.next().ok()?? != Token::BeginObject {
        return None;
    }
    let mut files = None;
    loop {
        match parser.next().ok()?? {
            Token::EndObject => break,
            Token::Key(key) if key == "files" && files.is_none() => {
                files = Some(entries(&mut parser)?);
            }
            Token::Key(_) => skip_value(&mut parser)?,
            _ => return None,
        }
    }
    if parser.next().ok()?.is_some() {
        return None;
    }
    files
}

/// Reads the array of `[path, sha256, bytes]` entries the parser stands
/// before and returns the paths.
fn entries(parser: &mut Parser<'_>) -> Option<Vec<String>> {
    if parser.next().ok()?? != Token::BeginArray {
        return None;
    }
    let mut paths = Vec::new();
    loop {
        match parser.next().ok()?? {
            Token::EndArray => return Some(paths),
            Token::BeginArray => {}
            _ => return None,
        }
        let Token::Str(path) = parser.next().ok()?? else {
            return None;
        };
        let Token::Str(_) = parser.next().ok()?? else {
            return None;
        };
        let Token::Number(_) = parser.next().ok()?? else {
            return None;
        };
        if parser.next().ok()?? != Token::EndArray {
            return None;
        }
        paths.push(path.into_owned());
    }
}

/// Reads past the value the parser stands before.
///
/// The parser checks that the containers close in order and caps their
/// depth, so counting the open containers finds the end of the value.
fn skip_value(parser: &mut Parser<'_>) -> Option<()> {
    let mut open = 0usize;
    loop {
        match parser.next().ok()?? {
            Token::BeginObject | Token::BeginArray => open += 1,
            Token::EndObject | Token::EndArray => open = open.checked_sub(1)?,
            _ => {}
        }
        if open == 0 {
            return Some(());
        }
    }
}

/// Creates every directory of `dirs` under `out` that does not exist, and
/// removes the temporary file of a run that stopped from each that does.
///
/// A file or a symbolic link where the tree has a directory is refused, so
/// the writer never writes through a link.
fn make_directories(out: &Path, dirs: &[String]) -> Result<(), DocsError> {
    for d in dirs {
        let path = out.join(d);
        match fs::symlink_metadata(&path) {
            Ok(m) if m.is_dir() => remove_temporary(&path)?,
            Ok(m) => {
                let what = if m.is_symlink() {
                    "a symbolic link stands where the tree has a directory"
                } else {
                    "a file stands where the tree has a directory"
                };
                return Err(output(path, io::Error::other(what)));
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                fs::create_dir(&path).map_err(|e| output(path, e))?;
            }
            Err(e) => return Err(output(path, e)),
        }
    }
    Ok(())
}

/// Writes the file `f` under `out` when its bytes changed, keeps the theme
/// when a regular file of that name exists, and counts the file in
/// `written`, comparing through `chunk`.
///
/// A regular file of the same bytes is unchanged, and any other entry of
/// that path, a file that cannot be read among them, is replaced through
/// [`replace`], which neither follows a link nor opens a pipe. A directory
/// stays, and the rename refuses it.
fn put(
    out: &Path,
    f: &TreeFile,
    chunk: &mut Chunk,
    written: &mut Written,
) -> Result<(), DocsError> {
    let path = out.join(&f.path);
    let on_disk = match fs::symlink_metadata(&path) {
        Ok(m) => Some(m),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(output(path, e)),
    };
    if f.path == THEME && on_disk.as_ref().is_some_and(fs::Metadata::is_file) {
        written.theme_kept = true;
        written.unchanged = written.unchanged.saturating_add(1);
        return Ok(());
    }
    let same_length = on_disk
        .is_some_and(|m| m.is_file() && u64::try_from(f.bytes.len()).is_ok_and(|n| n == m.len()));
    if same_length && file_difference(&path, &f.bytes, chunk).is_ok_and(|d| d.is_none()) {
        written.unchanged = written.unchanged.saturating_add(1);
        return Ok(());
    }
    replace(&path, &f.bytes).map_err(|e| output(path, e))?;
    written.written = written.written.saturating_add(1);
    Ok(())
}

/// Writes `bytes` to the file [`TEMPORARY`] beside `path` and renames it to
/// `path`.
///
/// The rename replaces the entry at `path` as a whole, a file that other
/// names link to, a symbolic link, a pipe, a socket or a device, and refuses
/// a directory. The temporary file is created anew, so a link at its name
/// makes the write fail and is never followed, and a write that fails
/// removes it.
fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_file_name(TEMPORARY);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let written = file.write_all(bytes);
    drop(file);
    let renamed = written.and_then(|()| fs::rename(&temporary, path));
    if renamed.is_err() {
        // The first error is the one to report, and a temporary file this
        // removal leaves goes with the next run.
        let _ = fs::remove_file(&temporary);
    }
    renamed
}

/// Removes the temporary file that a run which stopped left in `dir`.
fn remove_temporary(dir: &Path) -> Result<(), DocsError> {
    let path = dir.join(TEMPORARY);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if is_absent(&e) => Ok(()),
        Err(e) => Err(output(path, e)),
    }
}
