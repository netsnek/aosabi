// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs input detect: the kind of an input file by its first byte and
 * its name.
 *
 * The seam of the notebooks names a path and the loader decides its kind
 * here.
 */

//! The input kind.
//!
//! A directory is a build directory, a file called `ir.json` is the
//! intermediate representation, a file whose first byte is `{` is an
//! introspection result, and every other file is SDL. The first byte is the
//! first one after a byte order mark and the whitespace of JSON. A GraphQL
//! document that opens with `{` opens with the shorthand of a query, an
//! operation and no type definition, so a schema file has no reason to, and
//! the first byte tells JSON from SDL.
//!
//! The mark is passed over here and by the loader, while the JSON readers of
//! the core refuse it, which [RFC 8259] section 8.1 allows them to, so a
//! result saved with the mark loads as JSON.
//!
//! [`detect()`] reads a file up to that byte and [`load_path`] scans the
//! bytes it reads anyway, so the seam names and loads a file with one read of
//! it.
//!
//! [`load_path`]: super::load_path
//!
//! [specification]: https://spec.graphql.org/October2021/
//! [RFC 8259]: https://www.rfc-editor.org/rfc/rfc8259

use std::{
    fs::{
        self,
        File, //
    },
    io::{
        BufReader,
        Read as _, //
    },
    path::Path,
};

use aoviewer_core::limits::ReadLimits;

use super::{
    BOM,
    IR_FILE,
    Input,
    window, //
};
use crate::error::DocsError;

/// Returns the input `path` holds, reading at most what `limits` admits of
/// a file.
///
/// # Errors
///
/// Returns [`DocsError::Unreadable`] when the path does not exist or the file
/// cannot be read.
pub fn detect(path: &Path, limits: &ReadLimits) -> Result<Input, DocsError> {
    if let Some(input) = named(path)? {
        return Ok(input);
    }
    Ok(by_first_byte(path, scan(path, limits)?))
}

/// Returns the first byte of the file `path` after a byte order mark and the
/// whitespace of JSON, reading at most what `limits` admits of it.
///
/// # Errors
///
/// Returns [`DocsError::Unreadable`] when the file cannot be read.
pub(super) fn scan(path: &Path, limits: &ReadLimits) -> Result<Option<u8>, DocsError> {
    let unreadable = |error| DocsError::Unreadable {
        path: path.to_path_buf(),
        error,
        from_build: false,
    };
    let file = File::open(path).map_err(unreadable)?;
    // A file whose first other byte stands past the bytes the loader reads
    // holds more than `max_input_bytes` past any mark, which every reader
    // refuses whatever its kind, so the scan reads no further.
    let bytes = BufReader::new(file.take(window(limits))).bytes();
    first_byte(bytes).map_err(unreadable)
}

/// Returns the input `path` names by its kind of file and its name alone, a
/// directory or a file called `ir.json`, or `None` for a file whose bytes
/// decide.
///
/// # Errors
///
/// Returns [`DocsError::Unreadable`] when the path does not exist.
pub(super) fn named(path: &Path) -> Result<Option<Input>, DocsError> {
    let metadata = fs::metadata(path).map_err(|error| DocsError::Unreadable {
        path: path.to_path_buf(),
        error,
        from_build: false,
    })?;
    if metadata.is_dir() {
        return Ok(Some(Input::Build {
            dir: path.to_path_buf(),
        }));
    }
    if path.file_name().is_some_and(|name| name == IR_FILE) {
        return Ok(Some(Input::Ir(path.to_path_buf())));
    }
    Ok(None)
}

/// Returns the input of the file `path` whose first byte after a byte order
/// mark and the whitespace of JSON is `first`.
pub(super) fn by_first_byte(path: &Path, first: Option<u8>) -> Input {
    if first == Some(b'{') {
        Input::Introspection(path.to_path_buf())
    } else {
        Input::Schema(path.to_path_buf())
    }
}

/// Returns the first of `bytes` after a byte order mark and the whitespace
/// of JSON, or the first error of reading them.
pub(super) fn first_byte<E>(
    mut bytes: impl Iterator<Item = Result<u8, E>>,
) -> Result<Option<u8>, E> {
    let mut head = Vec::with_capacity(BOM.len());
    for b in bytes.by_ref().take(BOM.len()) {
        head.push(b?);
    }
    let head = head.strip_prefix(BOM).unwrap_or(&head);
    if let Some(&b) = head.iter().find(|&&b| !is_blank(b)) {
        return Ok(Some(b));
    }
    for b in bytes {
        let b = b?;
        if !is_blank(b) {
            return Ok(Some(b));
        }
    }
    Ok(None)
}

/// Returns whether `b` is whitespace of JSON.
///
/// [RFC 8259]: https://www.rfc-editor.org/rfc/rfc8259
fn is_blank(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}
