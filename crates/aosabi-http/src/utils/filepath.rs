// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the rules of hono that turn a request path into a file path.
 *
 * Derived from hono 4.11.9, dist/utils/filepath.js, MIT.
 */

//! The file path of a request path.
//!
//! The adapters of hono for Bun, Deno and Workers read a file at the path
//! [`get_file_path`] or [`get_file_path_without_default_document`] makes of
//! the request path and the root. Both refuse a path with a `..` segment,
//! which is the rule [󰀃](crate::middleware::serve_static) shares. An option
//! the reference reads with `||` is an empty string here where the caller
//! gives none, since `||` treats the empty string and `undefined` alike.

/// The document a directory path names, `index.html`.
pub const DEFAULT_DOCUMENT: &str = "index.html";

/// Returns the file path of `filename` below `root`, with `default_document`
/// after a path that names a directory.
///
/// A name that ends in `/` takes the document, a name with no extension takes
/// `/` and the document, and the result is the one of
/// [`get_file_path_without_default_document`]. An empty `default_document` is
/// [`DEFAULT_DOCUMENT`]. An extension is a `.` and one or more ASCII letters,
/// digits, `_` and `-` at the end, which is what `/\.[a-zA-Z0-9_-]+$/`
/// matches. `None` is the `undefined` of a refused path.
#[must_use]
pub fn get_file_path(filename: &str, root: &str, default_document: &str) -> Option<String> {
    let default_document = if default_document.is_empty() {
        DEFAULT_DOCUMENT
    } else {
        default_document
    };
    let filename = if filename.ends_with('/') {
        format!("{filename}{default_document}")
    } else if has_extension(filename) {
        String::from(filename)
    } else {
        format!("{filename}/{default_document}")
    };
    get_file_path_without_default_document(&filename, root)
}

/// Returns the file path of `filename` below `root`.
///
/// A name with a `..` segment is refused. The name loses one leading `/`, `\`,
/// `./` or `.\` and has its first `\` written `/`, the root loses one trailing
/// `/`, a non empty root is joined with `/`, and the path loses one leading
/// `/` or `./`. A path that starts with `/` below a root that does not is
/// refused. `None` is the `undefined` of a refused path.
#[must_use]
pub fn get_file_path_without_default_document(filename: &str, root: &str) -> Option<String> {
    if has_parent_segment(filename) {
        return None;
    }
    let filename = strip_leading_separator(filename).replacen('\\', "/", 1);
    let root = root.strip_suffix('/').unwrap_or(root);
    let path = if root.is_empty() {
        filename
    } else {
        format!("{root}/{filename}")
    };
    let path = path
        .strip_prefix("./")
        .or_else(|| path.strip_prefix('/'))
        .unwrap_or(&path);
    if !root.starts_with('/') && path.starts_with('/') {
        return None;
    }
    Some(String::from(path))
}

/// Returns whether `path` has a `..` segment between the start, the end, `/`
/// and `\`, which is what `/(?:^|[\/\\])\.\.(?:$|[\/\\])/` matches.
pub(crate) fn has_parent_segment(path: &str) -> bool {
    path.split(['/', '\\']).any(|segment| segment == "..")
}

/// Returns whether `filename` ends in an extension of `get_file_path`.
fn has_extension(filename: &str) -> bool {
    filename.rsplit_once('.').is_some_and(|(_, extension)| {
        !extension.is_empty()
            && extension
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    })
}

/// Returns `filename` without one leading `/`, `\`, `./` or `.\`, which is
/// what `filename.replace(/^\.?[\/\\]/, "")` removes.
fn strip_leading_separator(filename: &str) -> &str {
    let rest = filename.strip_prefix('.').unwrap_or(filename);
    match rest.strip_prefix(['/', '\\']) {
        Some(rest) => rest,
        None => filename,
    }
}
