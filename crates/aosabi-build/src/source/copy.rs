// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the generated copy of a plain crate, the crate the generated
 * packages build in place of the application's own.
 *
 * A Rust crate compiles its glue only inside the crate, which the
 * application never writes.
 */

//! The generated copy of a plain crate.
//!
//! The glue of a plain crate implements the traits of the facade for the
//! types of the crate, which only the crate itself may do for a type without
//! a public path, and the application holds no attribute or include that
//! could take the glue in. So the builder compiles the glue into a copy of
//! the crate under the target directory and never writes into the
//! application's tree. [`write_copy`] mirrors the package by links, one link
//! per entry of each directory, copies the crate root, which gains one
//! generated module, `#[path]` to the glue, and writes the glue beside the
//! copy. Every module file of the copy is the application's own file through
//! its link, so a module the crate root declares resolves as it does in the
//! package, and the crate root keeps every line where the author wrote it,
//! the generated module standing after the last one. A link names its entry
//! by a path relative to the link, so the copy resolves wherever the tree of
//! the package and its target directory stand together, in the builder stage
//! of the image of the docker card as on the host.
//!
//! The command line writes the manifest of the copy and builds it with the
//! directory of the copy remapped to the directory of the package, so a
//! diagnostic, a panic location and the boot report name the author's files
//! and lines. A second build writes a file or a link only where it changed,
//! so cargo compiles nothing again.

use std::{
    fs,
    io,
    path::{
        Component,
        Path,
        PathBuf, //
    }, //
};

use crate::update_file_if_changed::update_file_if_changed;

use super::glue::{
    Glue,
    MODULE, //
};

/// The entries of the root of a package the copy does not mirror: the
/// directory cargo builds into, the manifest and the lock, which the command
/// line writes for the copy, and the repository of the package.
const LEFT_OUT: [&str; 4] = ["target", "Cargo.toml", "Cargo.lock", ".git"];

/// What [`write_copy`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    /// The crate root of the copy.
    pub root: PathBuf,
    /// The count of files and links written, 0 when every one stood.
    pub written: usize,
}

/// Writes the generated copy of the plain crate of the package in the
/// directory `package`, whose crate root is `root` relative to it, into the
/// directory `copy`, with the glue `glue` in the file `glue_path` outside
/// the copy.
///
/// Every entry of the package but the directory `target`, the manifest, the
/// lock and the repository is a link in the copy,
/// and every directory on the way to the crate root a directory of the copy
/// whose entries are links but the next one on the way, so the copy reads
/// every file of the package where the package keeps it. A link whose entry
/// the package no longer holds goes. The crate root of the copy is the text
/// of the package's crate root with the generated module after its last
/// line.
///
/// # Errors
///
/// Returns the error of a read of the package, of a write of the copy and
/// of a link, which a host without links of the file system answers with
/// [`io::ErrorKind::Unsupported`].
pub fn write_copy(
    package: &Path,
    root: &Path,
    copy: &Path,
    glue_path: &Path,
    glue: &Glue,
) -> io::Result<Copied> {
    let package = fs::canonicalize(package)?;
    fs::create_dir_all(copy)?;
    let copy = fs::canonicalize(copy)?;
    let glue_path = match (glue_path.parent(), glue_path.file_name()) {
        (Some(dir), Some(name)) => {
            fs::create_dir_all(dir)?;
            fs::canonicalize(dir)?.join(name)
        }
        _ => glue_path.to_path_buf(),
    };
    let mut written = 0;
    let mut dirs: Vec<&std::ffi::OsStr> = root
        .parent()
        .map(|parent| {
            parent
                .components()
                .filter_map(|part| match part {
                    Component::Normal(name) => Some(name),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let file = root.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the crate root {} names no file", root.display()),
        )
    })?;
    let mut from = package.clone();
    let mut to = copy;
    let mut keep: Vec<&std::ffi::OsStr> = LEFT_OUT.iter().map(std::ffi::OsStr::new).collect();
    dirs.reverse();
    loop {
        fs::create_dir_all(&to)?;
        let next = dirs.pop();
        let own = next.unwrap_or(file);
        keep.push(own);
        written += mirror(&from, &to, &keep)?;
        keep.clear();
        let Some(next) = next else {
            break;
        };
        from.push(next);
        to.push(next);
    }
    let root_copy = to.join(file);
    let text = fs::read_to_string(from.join(file))?;
    let module = relative(&to, &glue_path);
    let mut copied = text;
    if !copied.ends_with('\n') {
        copied.push('\n');
    }
    copied.push_str(&module_lines(&module));
    if update_file_if_changed(&root_copy, copied.as_bytes())? {
        written += 1;
    }
    if update_file_if_changed(&glue_path, glue.text.as_bytes())? {
        written += 1;
    }
    Ok(Copied {
        root: root_copy,
        written,
    })
}

/// Returns the lines the crate root of the copy gains after its last line,
/// the generated module at the path `module` relative to the crate root.
fn module_lines(module: &Path) -> String {
    let path = format!("{:?}", module.to_string_lossy());
    [
        "\n// The glue of the crate, which the generated copy of cargo aosabi adds after the last line of the author.\n",
        "#[doc(hidden)]\n#[path = ",
        &path,
        "]\npub mod ",
        MODULE,
        ";\n",
    ]
    .concat()
}

/// Links every entry of the directory `from` into the directory `to` but
/// the names `keep`, which the copy holds of its own, each by its path
/// relative to `to`, drops every link of `to` whose entry `from` no longer
/// holds, and answers the count of links written. Both directories are
/// canonical, so the relative path names the entry.
fn mirror(from: &Path, to: &Path, keep: &[&std::ffi::OsStr]) -> io::Result<usize> {
    let mut written = 0;
    let mut names = Vec::new();
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if keep.contains(&name.as_os_str()) {
            continue;
        }
        let link = to.join(&name);
        let target = relative(to, &entry.path());
        names.push(name);
        if fs::read_link(&link).is_ok_and(|current| current == target) {
            continue;
        }
        if fs::symlink_metadata(&link).is_ok() {
            remove(&link)?;
        }
        link_to(&target, &link)?;
        written += 1;
    }
    for entry in fs::read_dir(to)? {
        let entry = entry?;
        let name = entry.file_name();
        if keep.contains(&name.as_os_str()) || names.contains(&name) {
            continue;
        }
        if entry.file_type()?.is_symlink() {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(written)
}

/// Removes the entry `path` of the copy, a link, a file or a directory.
fn remove(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// Makes `link` a link of the file system to `target`.
#[cfg(unix)]
fn link_to(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// Answers that a host with no links of the file system takes no copy.
#[cfg(not(unix))]
fn link_to(target: &Path, link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!(
            "the generated copy links {} to {}, and this host has no links of the file system",
            link.display(),
            target.display()
        ),
    ))
}

/// Returns the path of `to` relative to the directory `from`, both absolute,
/// or `to` itself when the two share no prefix.
fn relative(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<Component<'_>> = from.components().collect();
    let parts: Vec<Component<'_>> = to.components().collect();
    let common = from.iter().zip(&parts).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return to.to_path_buf();
    }
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &parts[common..] {
        out.push(part);
    }
    out
}
