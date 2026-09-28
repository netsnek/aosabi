// SPDX-License-Identifier: MIT
/*
 * aosabi-http path: the POSIX join and normalize of the path module of Node.
 *
 * Derived from node v24.13.0, lib/path.js, MIT.
 */

//! `join` and `normalize`.
//!
//! The Node adapter of hono joins the root of `serveStatic` and the path of a
//! request with `node:path`, and the Zitadel integration joins the
//! working directory and the path of its key file with it. The functions read
//! text alone, so the module builds for every target.

/// Returns `path.join(...paths)` of Node on a POSIX platform.
///
/// [path.js]: https://github.com/nodejs/node/blob/v24.13.0/lib/path.js
#[must_use]
pub fn join(paths: &[&str]) -> String {
    let paths: Vec<&str> = paths
        .iter()
        .copied()
        .filter(|path| !path.is_empty())
        .collect();
    if paths.is_empty() {
        return String::from(".");
    }
    normalize(&paths.join("/"))
}

/// Returns `path.normalize(path)` of Node on a POSIX platform.
#[must_use]
pub fn normalize(path: &str) -> String {
    if path.is_empty() {
        return String::from(".");
    }
    let is_absolute = path.starts_with('/');
    let trailing_separator = path.ends_with('/');
    let mut normalized = normalize_string(path, !is_absolute);
    if normalized.is_empty() {
        if is_absolute {
            return String::from("/");
        }
        return String::from(if trailing_separator { "./" } else { "." });
    }
    if trailing_separator {
        normalized.push('/');
    }
    if is_absolute {
        return format!("/{normalized}");
    }
    normalized
}

/// Returns `path` with its `.` and `..` segments resolved and its runs of
/// `/` made one, with no leading and no trailing `/`.
///
/// An empty segment and a `.` add nothing, and a `..` is [`push_parent`]. The
/// lengths count bytes where the reference counts code units of UTF-16, which
/// picks the same branches, because a separator and a dot are one unit and
/// one byte, and a result of at most two units and one of more than two bytes
/// both hold no `/` and both become empty.
fn normalize_string(path: &str, allow_above_root: bool) -> String {
    let bytes = path.as_bytes();
    let mut res = String::new();
    let mut last_segment_length = 0;
    let mut last_slash: Option<usize> = None;
    let mut dots = 0;
    let mut code = 0;
    for i in 0..=bytes.len() {
        if let Some(&byte) = bytes.get(i) {
            code = byte;
        } else if code == b'/' {
            break;
        } else {
            code = b'/';
        }
        if code != b'/' {
            dots = if code == b'.' && dots != -1 {
                dots + 1
            } else {
                -1
            };
            continue;
        }
        let start = last_slash.map_or(0, |slash| slash + 1);
        if start != i && dots == 2 {
            push_parent(&mut res, &mut last_segment_length, allow_above_root);
        } else if start != i && dots != 1 {
            if !res.is_empty() {
                res.push('/');
            }
            res.push_str(&path[start..i]);
            last_segment_length = i - start;
        }
        last_slash = Some(i);
        dots = 0;
    }
    res
}

/// Applies a `..` segment to `res`, whose last segment is
/// `last_segment_length` long.
///
/// After a segment that is no `..` the segment and the `/` before it are
/// dropped, and a result with no `/` becomes empty, which is what both
/// branches of the reference do for it. With no segment before it, or after
/// a `..`, the `..` is kept where `allow_above_root` is set.
fn push_parent(res: &mut String, last_segment_length: &mut usize, allow_above_root: bool) {
    let after_parent = *last_segment_length == 2 && res.ends_with("..");
    if !after_parent && !res.is_empty() {
        match res.len().checked_sub(*last_segment_length + 1) {
            Some(last_slash_index) if res.len() > 2 => {
                res.truncate(last_slash_index);
                *last_segment_length = res
                    .rfind('/')
                    .map_or(res.len(), |slash| res.len() - 1 - slash);
            }
            _ => {
                res.clear();
                *last_segment_length = 0;
            }
        }
        return;
    }
    if allow_above_root {
        res.push_str(if res.is_empty() { ".." } else { "/.." });
        *last_segment_length = 2;
    }
}
