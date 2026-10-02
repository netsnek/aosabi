// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the targets of a package that the declaration list is
 * written for.
 *
 * No analogue on the reference side. The reference scans the source files of
 * one TypeScript program, and a cargo package compiles one crate per target,
 * each with a module tree of its own.
 */

//! The targets of a package, read from its manifest and its directories.
//!
//! Cargo compiles the library, every binary, every example, every integration
//! test and every benchmark of a package as a crate of its own, and it finds
//! them in the manifest and in the directories of the target auto discovery. A
//! declaration names its constant by a path from its crate root, so every
//! target gets a list of its own.
//!
//! The manifest is read for the `path` and `name` of the tables `[lib]`,
//! `[[bin]]`, `[[example]]`, `[[test]]` and `[[bench]]`, and for the
//! `crate-type` of `[[example]]`, one key per line with a string value or an
//! array of strings on that line, which is the form an application of the
//! authoring surface writes.
//! A target written in another TOML form, an inline table or a dotted key,
//! is found only where the auto discovery finds it, and a `crate-type`
//! written over several lines reads as none, which makes its example a
//! binary example.
//!
//! [the cargo book on cargo targets]: https://doc.rust-lang.org/cargo/reference/cargo-targets.html

use std::{
    fs,
    path::{
        Path,
        PathBuf, //
    },
};

/// The kind of a target, which decides the name of its list.
///
/// Cargo sets `CARGO_BIN_NAME` for binaries and binary examples alone,
/// `CARGO_TARGET_TMPDIR` for integration tests and benchmarks alone, and
/// `CARGO_CRATE_NAME` for every target.
///
/// [the cargo book on environment variables]: https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-crates
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The library, or an example whose `crate-type` names a kind other than
    /// `bin`, neither of which gets `CARGO_BIN_NAME` or `CARGO_TARGET_TMPDIR`.
    Lib,
    /// A binary or a binary example, which gets `CARGO_BIN_NAME`.
    Bin,
    /// An integration test or a benchmark, which gets `CARGO_TARGET_TMPDIR`.
    Test,
}

/// One target of the package.
#[derive(Debug)]
pub(crate) struct Target {
    /// The kind of the target.
    pub(crate) kind: Kind,
    /// The name of the target as cargo names it.
    pub(crate) name: String,
    /// The root file of the crate of the target.
    pub(crate) root: PathBuf,
}

impl Target {
    /// Returns the file name of the list of the target.
    ///
    /// The derive finds the list by the same name, which it builds from the
    /// environment cargo gives the compiler. A binary is keyed by its name,
    /// which is `CARGO_BIN_NAME`. A library and a test are keyed by their
    /// crate name, which is `CARGO_CRATE_NAME`, the name of the target with
    /// every `-` turned into `_`.
    pub(crate) fn list_name(&self) -> String {
        let krate = self.name.replace('-', "_");
        match self.kind {
            Kind::Lib => format!("lib-{krate}.rs"),
            Kind::Bin => format!("bin-{}.rs", self.name),
            Kind::Test => format!("test-{krate}.rs"),
        }
    }
}

/// The keys of one table of a target in the manifest that the scan reads.
#[derive(Debug, Default)]
struct TableEntry {
    /// The value of `name`, when the table states one.
    name: Option<String>,
    /// The value of `path`, when the table states one.
    path: Option<String>,
    /// The value of `crate-type`, or of its older spelling `crate_type`, when
    /// the table states one.
    crate_types: Option<Vec<String>>,
}

impl TableEntry {
    /// Returns the kind of the example this table states.
    ///
    /// An example is a binary example when its `crate-type` is missing or
    /// names `bin` alone, and an example of a library crate type otherwise, as
    /// cargo decides it.
    fn example_kind(&self) -> Kind {
        match &self.crate_types {
            Some(types) if types.iter().any(|kind| kind != "bin") => Kind::Lib,
            _ => Kind::Bin,
        }
    }
}

/// The tables of targets the manifest states.
#[derive(Debug, Default)]
struct Tables {
    /// The table `[lib]`.
    lib: TableEntry,
    /// The tables `[[bin]]`.
    bins: Vec<TableEntry>,
    /// The tables `[[example]]`.
    examples: Vec<TableEntry>,
    /// The tables `[[test]]` and `[[bench]]`.
    tests: Vec<TableEntry>,
}

/// Returns every target of the package in `dir` whose root file exists.
///
/// `package` is the name of the package, which names the library when
/// `[lib]` states no `name`, and the binary of `src/main.rs`. `manifest` is
/// the text of `Cargo.toml`. A target the manifest states and the auto
/// discovery finds as well is listed once, by the manifest. An example the
/// auto discovery finds takes the `crate-type` of the table of its name.
pub(crate) fn targets(dir: &Path, package: &str, manifest: &str) -> Vec<Target> {
    let tables = tables(manifest);
    let mut found = Vec::new();

    let lib = tables.lib.path.as_deref().unwrap_or("src/lib.rs");
    let lib_name = tables.lib.name.as_deref().unwrap_or(package);
    push(&mut found, Kind::Lib, lib_name, dir.join(lib));

    for entry in &tables.bins {
        push_stated(&mut found, Kind::Bin, dir, entry);
    }
    for entry in &tables.examples {
        push_stated(&mut found, entry.example_kind(), dir, entry);
    }
    for entry in &tables.tests {
        push_stated(&mut found, Kind::Test, dir, entry);
    }
    push(&mut found, Kind::Bin, package, dir.join("src/main.rs"));
    // The kind of a discovered example follows from its table, when it has
    // one, so the directory states none.
    for (kind, directory) in [
        (Some(Kind::Bin), "src/bin"),
        (None, "examples"),
        (Some(Kind::Test), "tests"),
        (Some(Kind::Test), "benches"),
    ] {
        for (name, root) in discovered(&dir.join(directory)) {
            let kind = kind.unwrap_or_else(|| example_kind(&tables.examples, &name));
            push(&mut found, kind, &name, root);
        }
    }
    found
}

/// Returns the kind of the example `name`, from the table of that name when
/// the manifest states one.
fn example_kind(examples: &[TableEntry], name: &str) -> Kind {
    examples
        .iter()
        .find(|entry| entry.name.as_deref() == Some(name))
        .map_or(Kind::Bin, TableEntry::example_kind)
}

/// Returns the directories of the auto discovery that exist, so a target
/// that appears in one of them asks for a new list.
///
/// A directory that does not exist is left out, because cargo runs a build
/// script again on every build while a path it watches is missing. So a
/// directory made after the last run is not watched, and its first target gets
/// a list only when the manifest or a watched path changes. The derive refuses
/// a target with no list in a crate that writes lists.
pub(crate) fn discovery_dirs(dir: &Path) -> Vec<PathBuf> {
    ["src", "examples", "tests", "benches"]
        .into_iter()
        .map(|directory| dir.join(directory))
        .filter(|directory| directory.is_dir())
        .collect()
}

/// Adds the target of `entry` when the table states a path.
///
/// A table with no `path` names a file the auto discovery finds, and a table
/// with no `name` names no target cargo accepts.
fn push_stated(found: &mut Vec<Target>, kind: Kind, dir: &Path, entry: &TableEntry) {
    if let (Some(name), Some(path)) = (&entry.name, &entry.path) {
        push(found, kind, name, dir.join(path));
    }
}

/// Adds one target unless its root file is missing or already listed.
fn push(found: &mut Vec<Target>, kind: Kind, name: &str, root: PathBuf) {
    if !root.is_file() || found.iter().any(|target| target.root == root) {
        return;
    }
    found.push(Target {
        kind,
        name: String::from(name),
        root,
    });
}

/// Returns the name and the root file of every target the auto discovery
/// finds in `dir`, sorted by name.
///
/// A target is `<name>.rs` or `<name>/main.rs`.
///
/// [the cargo book on target auto discovery]: https://doc.rust-lang.org/cargo/reference/cargo-targets.html#target-auto-discovery
fn discovered(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| discovered_one(&entry.path()))
        .collect();
    found.sort();
    found
}

/// Returns the name and the root file of the target at `path`, if it is one.
fn discovered_one(path: &Path) -> Option<(String, PathBuf)> {
    if path.is_dir() {
        let root = path.join("main.rs");
        let name = path.file_name()?.to_str()?;
        return root.is_file().then(|| (String::from(name), root));
    }
    if path.extension()? != "rs" {
        return None;
    }
    let name = path.file_stem()?.to_str()?;
    Some((String::from(name), path.to_path_buf()))
}

/// The library of a package and the crates it names as dependencies, which
/// the reader of the source of the crate reads.
#[cfg(feature = "source")]
#[derive(Debug)]
pub(crate) struct Library {
    /// The name of the package, as `CARGO_PKG_NAME` holds it.
    pub(crate) package: String,
    /// The crate name of the library, the `name` of `[lib]` or the name of the
    /// package, with every `-` turned into `_`.
    pub(crate) name: String,
    /// The root file of the library, the `path` of `[lib]` or cargo's default
    /// `src/lib.rs`, relative to the package.
    pub(crate) root: PathBuf,
    /// The value of `workspace` of `[package]`, the root of the workspace of
    /// the package relative to it, when the manifest states one.
    pub(crate) workspace: Option<PathBuf>,
    /// Whether the manifest holds a table `[workspace]`, which makes its
    /// package the root of a workspace.
    pub(crate) workspace_root: bool,
    /// The dependencies of the library, the entries of `[dependencies]` and of
    /// every `[target.<platform>.dependencies]`, each once per table, in the
    /// order the manifest names them.
    pub(crate) dependencies: Vec<Dependency>,
    /// The entries of `[workspace.dependencies]`, which a dependency of a
    /// member takes with `workspace = true`.
    pub(crate) workspace_dependencies: Vec<Dependency>,
    /// The features of `[features]`, each with what it enables, in the order
    /// the manifest names them.
    pub(crate) features: Vec<(String, Vec<String>)>,
}

/// One dependency a manifest names.
#[cfg(feature = "source")]
#[derive(Debug, Clone)]
pub(crate) struct Dependency {
    /// The key of the entry as the manifest writes it, which a feature names
    /// the dependency by, as `dep:<key>` and `<key>/<feature>`.
    pub(crate) key: String,
    /// The name the library sees the crate by, the key of the entry with every
    /// `-` turned into `_`.
    pub(crate) name: String,
    /// The package of the crate, the value of `package` or the key of the
    /// entry.
    pub(crate) package: String,
    /// The value of `path`, the directory of a path dependency relative to the
    /// manifest that names it.
    pub(crate) path: Option<PathBuf>,
    /// Whether the entry takes its source from the workspace, `workspace =
    /// true`.
    pub(crate) workspace: bool,
    /// The features of the dependency the entry enables, its `features`.
    pub(crate) features: Vec<String>,
    /// Whether the dependency is optional, `optional = true`.
    pub(crate) optional: bool,
    /// Whether the entry keeps the default features of the dependency, which
    /// `default-features = false` turns off.
    pub(crate) default_features: bool,
    /// The platform of the table `[target.<platform>.dependencies]` the entry
    /// stands in, a `cfg(..)` or a triple without its quotes, `None` for an
    /// entry of `[dependencies]` or of `[workspace.dependencies]`.
    pub(crate) platform: Option<String>,
}

/// Returns the library of the package whose manifest is the text `manifest`.
///
/// The manifest is read in the form of [`targets`], one key per line. A
/// dependency is a key whose value is a string or an inline table on its line,
/// the dotted keys `<name>.<key>`, or the dotted table
/// `[dependencies.<name>]`, the forms the cargo book gives. A feature is a key
/// of `[features]` whose value is an array of strings. A value whose array or
/// inline table goes on at later lines takes those lines up to its end,
/// without their comments.
///
/// [the cargo book on specifying dependencies]: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html
/// [the cargo book on features]: https://doc.rust-lang.org/cargo/reference/features.html
#[cfg(feature = "source")]
pub(crate) fn library(manifest: &str) -> Library {
    let tables = tables(manifest);
    let mut package = None;
    let mut workspace = None;
    let mut workspace_root = false;
    let mut features = Vec::new();
    let mut lists: [Vec<Dependency>; 2] = [Vec::new(), Vec::new()];
    let mut section = String::new();
    // The entry a dotted table `[dependencies.<name>]` opens, by its list,
    // the dependencies of the library or those of the workspace.
    let mut table: Option<(usize, usize)> = None;
    let lines: Vec<&str> = manifest.lines().map(str::trim).collect();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        at += 1;
        if line.starts_with('[') {
            section = line
                .split('#')
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .collect();
            workspace_root |= section == "[workspace]" || section.starts_with("[workspace.");
            table = dependency_table(&section).map(|(list, name, platform)| {
                (list, entry(&mut lists[list], name, platform.as_deref()))
            });
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.starts_with('#') {
            continue;
        }
        let value = whole_value(value.trim(), &lines, &mut at);
        let value = value.as_str();
        if let Some((list, entry)) = table {
            set(&mut lists[list][entry], key, value);
        } else if section == "[package]" {
            match key {
                "name" => package = string(value),
                "workspace" => workspace = string(value).map(PathBuf::from),
                _ => {}
            }
        } else if section == "[features]" {
            let name = key.trim_matches(['"', '\'']);
            features.push((String::from(name), strings(value).unwrap_or_default()));
        } else if let Some((list, platform)) = dependency_section(&section) {
            let (name, field) = match key.split_once('.') {
                Some((name, field)) => (name, Some(field)),
                None => (key, None),
            };
            let name = name.trim().trim_matches(['"', '\'']);
            if name.is_empty() {
                continue;
            }
            let entry = entry(&mut lists[list], name, platform.as_deref());
            match field {
                Some(field) => set(&mut lists[list][entry], field, value),
                None => inline(&mut lists[list][entry], value),
            }
        }
    }
    let package = package.unwrap_or_default();
    let name = tables
        .lib
        .name
        .unwrap_or_else(|| package.clone())
        .replace('-', "_");
    let root = PathBuf::from(tables.lib.path.as_deref().unwrap_or("src/lib.rs"));
    let [dependencies, workspace_dependencies] = lists;
    Library {
        package,
        name,
        root,
        workspace,
        workspace_root,
        dependencies,
        workspace_dependencies,
        features,
    }
}

/// Returns the value `first` of a key with the lines of `lines` from `at` on
/// that its array or its inline table goes on at, each without its comment,
/// and moves `at` past them.
#[cfg(feature = "source")]
fn whole_value(first: &str, lines: &[&str], at: &mut usize) -> String {
    let mut value = String::from(uncommented(first));
    while depth(&value) > 0 && *at < lines.len() {
        value.push(' ');
        value.push_str(uncommented(lines[*at]));
        *at += 1;
    }
    value
}

/// Returns the line `line` up to a `#` outside a string, which opens a
/// comment.
#[cfg(feature = "source")]
fn uncommented(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    for (at, c) in line.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if open == '"' && c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '#' => return line[..at].trim_end(),
            _ => {}
        }
    }
    line
}

/// Returns how many arrays and inline tables the TOML text `text` opens and
/// does not close outside its strings.
#[cfg(feature = "source")]
fn depth(text: &str) -> usize {
    let mut depth = 0_usize;
    let mut quote = None;
    let mut escaped = false;
    for c in text.chars() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if open == '"' && c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '[' | '{' => depth += 1,
            ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// Returns the list a header `section` opens the entries of, 0 for
/// `[dependencies]` and `[target.<platform>.dependencies]`, the dependencies
/// of the library, and 1 for `[workspace.dependencies]`, with the platform of
/// a table of a target.
#[cfg(feature = "source")]
fn dependency_section(section: &str) -> Option<(usize, Option<String>)> {
    let inner = section.strip_prefix('[')?.strip_suffix(']')?;
    match inner {
        "dependencies" => Some((0, None)),
        "workspace.dependencies" => Some((1, None)),
        _ => platform_of(inner.strip_suffix(".dependencies")?).map(|platform| (0, Some(platform))),
    }
}

/// Returns the list, the name and the platform of the dotted table
/// `[dependencies.<name>]`, `[target.<platform>.dependencies.<name>]` or
/// `[workspace.dependencies.<name>]` the header `section` opens, in the
/// numbering of [`dependency_section`].
#[cfg(feature = "source")]
fn dependency_table(section: &str) -> Option<(usize, &str, Option<String>)> {
    let inner = section.strip_prefix('[')?.strip_suffix(']')?;
    let (before, name) = inner.rsplit_once('.')?;
    let (list, platform) = match before {
        "dependencies" => (0, None),
        "workspace.dependencies" => (1, None),
        _ => (0, Some(platform_of(before.strip_suffix(".dependencies")?)?)),
    };
    Some((list, name.trim_matches(['"', '\'']), platform))
}

/// Returns the platform of the key `target.<platform>` of a header, the
/// platform without the quotes of its key and with the escapes of a key in
/// double quotes read, which the header holds with no white space, or `None`
/// for a key of no table of a target.
#[cfg(feature = "source")]
fn platform_of(key: &str) -> Option<String> {
    let platform = key.strip_prefix("target.")?;
    if let Some(literal) = platform
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return Some(String::from(literal));
    }
    let Some(basic) = platform
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return Some(String::from(platform));
    };
    let mut text = String::with_capacity(basic.len());
    let mut chars = basic.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            text.extend(chars.next());
        } else {
            text.push(c);
        }
    }
    Some(text)
}

/// Returns the position of the entry of the key `key` of the table of the
/// platform `platform` in `list`, adding it when the list holds none, since
/// dotted keys name one entry on several lines, while the entries of one key
/// in two tables stay two, which cargo takes where their platforms apply.
#[cfg(feature = "source")]
fn entry(list: &mut Vec<Dependency>, key: &str, platform: Option<&str>) -> usize {
    let name = key.replace('-', "_");
    if let Some(at) = list
        .iter()
        .position(|known| known.name == name && known.platform.as_deref() == platform)
    {
        return at;
    }
    list.push(Dependency {
        key: String::from(key),
        name,
        package: String::from(key),
        path: None,
        workspace: false,
        features: Vec::new(),
        optional: false,
        default_features: true,
        platform: platform.map(String::from),
    });
    list.len() - 1
}

/// Sets the key `key` of the entry `dependency` to the TOML value `value`,
/// for the keys the reader needs, `path`, `package`, `workspace`, `features`,
/// `optional` and `default-features`.
#[cfg(feature = "source")]
fn set(dependency: &mut Dependency, key: &str, value: &str) {
    match key.trim().trim_matches(['"', '\'']) {
        "path" => dependency.path = string(value).map(PathBuf::from),
        "package" => {
            if let Some(package) = string(value) {
                dependency.package = package;
            }
        }
        "workspace" => dependency.workspace = value.starts_with("true"),
        "features" => dependency.features = strings(value).unwrap_or_default(),
        "optional" => dependency.optional = value.starts_with("true"),
        "default-features" | "default_features" => {
            dependency.default_features = !value.starts_with("false");
        }
        _ => {}
    }
}

/// Sets the keys of the inline table `value` on the entry `dependency`, and
/// nothing for a value that is a string, the requirement of a registry.
#[cfg(feature = "source")]
fn inline(dependency: &mut Dependency, value: &str) {
    let Some(mut rest) = value.strip_prefix('{') else {
        return;
    };
    loop {
        rest = rest.trim_start();
        if rest.is_empty() || rest.starts_with('}') {
            return;
        }
        let Some((key, after)) = rest.split_once('=') else {
            return;
        };
        let after = after.trim_start();
        let end = value_end(after);
        set(dependency, key, &after[..end]);
        match after[end..].trim_start().strip_prefix(',') {
            Some(tail) => rest = tail,
            None => return,
        }
    }
}

/// Returns the length of the TOML value `text` opens with inside an inline
/// table: a string, an array or a table, or a bare value up to the `,` or
/// the `}` that ends it.
#[cfg(feature = "source")]
fn value_end(text: &str) -> usize {
    let mut depth = 0_usize;
    let mut quote = None;
    let mut escaped = false;
    for (at, c) in text.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if open == '"' && c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '[' | '{' => depth += 1,
            ']' | '}' if depth > 0 => depth -= 1,
            ',' | '}' if depth == 0 => return at,
            _ => {}
        }
    }
    text.len()
}

/// Returns the tables of targets in the text of a manifest.
fn tables(manifest: &str) -> Tables {
    let mut tables = Tables::default();
    let mut current: Option<&mut TableEntry> = None;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            current = table(&mut tables, line);
            continue;
        }
        let (Some(entry), Some((key, value))) = (current.as_deref_mut(), line.split_once('='))
        else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "name" => entry.name = string(value),
            "path" => entry.path = string(value),
            "crate-type" | "crate_type" => entry.crate_types = strings(value),
            _ => {}
        }
    }
    tables
}

/// Returns the entry the header `line` opens, or `None` for a table that
/// states no target.
fn table<'a>(tables: &'a mut Tables, line: &str) -> Option<&'a mut TableEntry> {
    let header: String = line.split('#').next()?.split_whitespace().collect();
    let list = match header.as_str() {
        "[lib]" => return Some(&mut tables.lib),
        "[[bin]]" => &mut tables.bins,
        "[[example]]" => &mut tables.examples,
        "[[test]]" | "[[bench]]" => &mut tables.tests,
        _ => return None,
    };
    list.push(TableEntry::default());
    list.last_mut()
}

/// Returns the text of the TOML string `value` opens with, a basic string
/// whose one escapes are `\\` and `\"`, or a literal string.
///
/// Another escape answers `None`, and the key reads as not stated.
fn string(value: &str) -> Option<String> {
    leading_string(value).map(|(text, _)| text)
}

/// Returns the texts of the TOML array of strings `value`, which ends on its
/// own line.
///
/// An array that goes on at a later line or holds anything but strings
/// answers `None`, and the key reads as not stated.
fn strings(value: &str) -> Option<Vec<String>> {
    let mut rest = value.strip_prefix('[')?.trim_start();
    let mut texts = Vec::new();
    while !rest.starts_with(']') {
        let (text, tail) = leading_string(rest)?;
        texts.push(text);
        let tail = tail.trim_start();
        rest = match tail.strip_prefix(',') {
            Some(tail) => tail.trim_start(),
            None if tail.starts_with(']') => tail,
            None => return None,
        };
    }
    Some(texts)
}

/// Returns the text of the TOML string `value` opens with and the text behind
/// it, for the strings [`string`] reads.
fn leading_string(value: &str) -> Option<(String, &str)> {
    if let Some(literal) = value.strip_prefix('\'') {
        return literal
            .split_once('\'')
            .map(|(text, rest)| (String::from(text), rest));
    }
    let body = value.strip_prefix('"')?;
    let mut chars = body.char_indices();
    let mut text = String::new();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return Some((text, &body[at + 1..])),
            '\\' => text.push(
                chars
                    .next()
                    .map(|(_, c)| c)
                    .filter(|c| matches!(c, '\\' | '"'))?,
            ),
            c => text.push(c),
        }
    }
    None
}
