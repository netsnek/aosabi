// SPDX-License-Identifier: MIT
/*
 * xtask lock: the writer of assets.lock.
 *
 * The lock is JSON with sorted keys, one entry per product with its bytes, its
 * toolchain, its features, its profile, its SHA-256 and the SHA-256 of every
 * source file, so a stale product names the file that moved.
 */

//! The asset lock.
//!
//! `assets.lock` is
//! `{"aoviewer.wasm":{"built_by":..,"bytes":..,"features":.., "profile":..,"sha256":..,"sources":{"<path>":"<sha256>",..}}}`
//! with the keys of every object in the byte order of their UTF-8, which is the
//! order of their code points. No key of the viewer's lock is an array index,
//! which the writer would place first, since every key holds a letter. It is
//! written by [`aosabi_json::to_bytes`], the one writer of JSON bytes of the
//! workspace, as one line ended by a line feed, and a committed lock is read
//! back by [`aosabi_json::parse`] to name the sources that moved. Every SHA-256
//! is [`sha256_hex`] of the core, so the lock and the manifest of a
//! documentation tree hash with one function.

use std::{
    fmt,
    fs,
    io,
    path::Path,
    process::{
        Command,
        ExitStatus,
        Stdio, //
    },
    str, //
};

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::sha256::sha256_hex;

use crate::FileError;

/// One product of the lock and what it was built from.
pub struct Product<'a> {
    /// The file name of the product, which is the key of its entry.
    pub name: &'a str,
    /// The bytes of the product as written.
    pub bytes: &'a [u8],
    /// The compiler, as `rustc` with its release, commit, host and LLVM.
    pub built_by: &'a str,
    /// The cargo features of the build.
    pub features: &'a str,
    /// The cargo profile of the build.
    pub profile: &'a str,
    /// Every source file by its path from the root, with its SHA-256, in the
    /// byte order of the paths as [`sources`] returns them, which is the order
    /// of the table of the lock.
    pub sources: &'a [(String, String)],
}

/// Why the sources of a product cannot be hashed.
#[derive(Debug)]
pub enum SourceError {
    /// A file cannot be read.
    Io(FileError),
    /// `git` cannot be started.
    Spawn(io::Error),
    /// `git ls-files` failed.
    Git(ExitStatus),
    /// A path git tracks is not UTF-8, with every byte that is not replaced.
    Name(String),
    /// A path git tracks is no regular file in the working tree.
    NotRegular(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Spawn(error) => write!(f, "cannot run git: {error}"),
            Self::Git(status) => write!(f, "git ls-files failed, {status}"),
            Self::Name(path) => write!(f, "{path} is not UTF-8"),
            Self::NotRegular(path) => write!(f, "{path} is no regular file"),
        }
    }
}

/// Returns the bytes of `assets.lock` for `products`.
///
/// The result is `None` only where the writer finds no JSON form, which an
/// object never is, so the caller keeps that case as an error rather than
/// writing an empty lock.
pub fn render(products: &[Product<'_>]) -> Option<Vec<u8>> {
    let lock = sorted(
        products
            .iter()
            .map(|product| (String::from(product.name), JsValue::Object(entry(product)))),
    );
    let mut out = aosabi_json::to_bytes(&JsValue::Object(lock))?;
    out.push(b'\n');
    Some(out)
}

/// Returns the entry of one product.
fn entry(product: &Product<'_>) -> JsObject {
    let sources = product
        .sources
        .iter()
        .map(|(path, digest)| (path.clone(), JsValue::from(digest.as_str())));
    let bytes = u64::try_from(product.bytes.len()).unwrap_or(u64::MAX);
    sorted([
        (String::from("built_by"), JsValue::from(product.built_by)),
        (String::from("bytes"), JsValue::from(bytes)),
        (String::from("features"), JsValue::from(product.features)),
        (String::from("profile"), JsValue::from(product.profile)),
        (
            String::from("sha256"),
            JsValue::from(sha256_hex(product.bytes)),
        ),
        (String::from("sources"), JsValue::Object(sources.collect())),
    ])
}

/// Returns an object of `pairs` with its keys in byte order.
///
/// The lock and every entry are built here, so the order of their keys does
/// not rest on the order a caller hands them in, and the table of sources
/// keeps the order [`sources`] sorts once.
fn sorted(pairs: impl IntoIterator<Item = (String, JsValue)>) -> JsObject {
    let mut pairs: Vec<(String, JsValue)> = pairs.into_iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs.into_iter().collect()
}

/// Returns every file git tracks under `paths`, each a directory or a file
/// from `root`, by its path from `root` with `/` between its parts and with
/// the SHA-256 of its bytes in the working tree, in the byte order of the
/// paths.
///
/// The tracked files are the sources a clean checkout holds, so the swap file
/// of an editor, a backup or the `.orig` file of a merge never enters the
/// lock, while a tracked file of any name does, one that `include_str!` or a
/// `#[path]` attribute reads among them. A tracked file the working tree
/// lacks is no source, since the build cannot read it, and a new file is one
/// once git tracks it. The order makes two calls over an unchanged tree equal
/// and is the one [`changed`] walks.
///
/// # Errors
///
/// Returns [`SourceError::Spawn`] or [`SourceError::Git`] when `git ls-files`
/// cannot run or fails, [`SourceError::Name`] when a path is not UTF-8,
/// [`SourceError::NotRegular`] when a tracked path is a symbolic link, a
/// submodule or another entry that is no regular file in the working tree,
/// since a link would let the build read a file whose path the lock does not
/// name, and [`SourceError::Io`] when a file cannot be read.
pub fn sources(root: &Path, paths: &[&str]) -> Result<Vec<(String, String)>, SourceError> {
    let output = Command::new("git")
        .args(["--literal-pathspecs", "ls-files", "-z", "--"])
        .args(paths)
        .current_dir(root)
        .stderr(Stdio::inherit())
        .output()
        .map_err(SourceError::Spawn)?;
    if !output.status.success() {
        return Err(SourceError::Git(output.status));
    }
    let mut tracked = Vec::new();
    for path in output.stdout.split(|&byte| byte == 0) {
        if path.is_empty() {
            continue;
        }
        let path = str::from_utf8(path)
            .map_err(|_| SourceError::Name(String::from_utf8_lossy(path).into_owned()))?;
        tracked.push(String::from(path));
    }
    // A path in conflict stands once per stage of the merge.
    tracked.sort_unstable();
    tracked.dedup();
    let mut sources = Vec::with_capacity(tracked.len());
    for path in tracked {
        let file = root.join(&path);
        let bytes = match fs::symlink_metadata(&file) {
            Ok(metadata) if metadata.is_file() => fs::read(&file),
            Ok(_) => return Err(SourceError::NotRegular(path)),
            Err(error) => Err(error),
        };
        match bytes {
            Ok(bytes) => {
                let digest = sha256_hex(&bytes);
                sources.push((path, digest));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(SourceError::Io(FileError::new(&path, error))),
        }
    }
    Ok(sources)
}

/// Returns the paths that differ between `before` and `after`, a path whose
/// SHA-256 changed and a path that only one of them names, in byte order.
///
/// Both lists are in the byte order of their paths, as [`sources`] and
/// [`committed`] return them, so one walk over both finds every path.
pub fn changed(before: &[(String, String)], after: &[(String, String)]) -> Vec<String> {
    let mut paths = Vec::new();
    let (mut i, mut j) = (0, 0);
    loop {
        match (before.get(i), after.get(j)) {
            (None, None) => return paths,
            (Some(old), Some(new)) if old.0 == new.0 => {
                if old.1 != new.1 {
                    paths.push(old.0.clone());
                }
                i += 1;
                j += 1;
            }
            (Some(old), Some(new)) if old.0 < new.0 => {
                paths.push(old.0.clone());
                i += 1;
            }
            (Some(old), None) => {
                paths.push(old.0.clone());
                i += 1;
            }
            (_, Some(new)) => {
                paths.push(new.0.clone());
                j += 1;
            }
        }
    }
}

/// What a committed lock records of one product.
pub struct Committed {
    /// The compiler.
    pub built_by: String,
    /// Every source with its SHA-256, in the byte order of the paths.
    pub sources: Vec<(String, String)>,
}

/// Returns what the lock `bytes` records of the product `name`.
///
/// The result is `None` when the bytes are no JSON, hold no entry for `name`,
/// no string for `built_by` or no table of sources, or name a source with no
/// string for its SHA-256.
pub fn committed(bytes: &[u8], name: &str) -> Option<Committed> {
    let JsValue::Object(lock) = aosabi_json::parse(bytes).ok()? else {
        return None;
    };
    let Some(JsValue::Object(entry)) = lock.get(name) else {
        return None;
    };
    let Some(JsValue::String(built_by)) = entry.get("built_by") else {
        return None;
    };
    let Some(JsValue::Object(table)) = entry.get("sources") else {
        return None;
    };
    let mut sources = Vec::with_capacity(table.len());
    for (path, digest) in table.iter() {
        let JsValue::String(digest) = digest else {
            return None;
        };
        sources.push((String::from(path.as_str()?), String::from(digest.as_str()?)));
    }
    sources.sort_unstable();
    Some(Committed {
        built_by: String::from(built_by.as_str()?),
        sources,
    })
}
