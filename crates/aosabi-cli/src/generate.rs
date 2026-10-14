// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the generated workspace under the target directory, with its
 * server and inspect packages.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/bundler/emit-server-glue.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The generated workspace.
//!
//! [`generate`] writes the boot of a server, which imports the entry and the
//! configuration and which the author never edits, into one cargo workspace
//! under `<target_dir>/aosabi/`, the directory the artifacts of the build script
//! already use, with two members:
//!
//! - The lean link acts on the release build, which the command runs with
//!   `-C panic=abort` and `--target`, and links a dev build as before. The
//!   package names the facade with the features of its card, `serve`,
//!   `unix-socket`, `deflate` and `linux` for the cards that serve on Linux.
//! - `inspect`, the package `<name>-inspect`, whose main is the one call
//!   `aosabi_build::inspect_from`, the runner `aosabi inspect` reads the
//!   model with, which validates nothing, so it answers for a schema the
//!   build script of `server` refuses.
//!
//! The root of the workspace carries the release profile of the Aosabi
//! workspace and the size profile of ADR-0025, since cargo reads the
//! profiles of the root of a workspace alone and the service is a path
//! dependency of it. The application's own `build.rs`, the one call
//! `aosabi_build::declarations_main()`, stays in the application and runs
//! when cargo compiles it, so no second one is written here.
//!
//! Every file goes through `update_file_if_changed` of `aosabi-build`, so a
//! second build writes none of them and compiles nothing. Every path is written
//! relative to the file that names it, longer for a target directory outside
//! the project and absolute when none can be made relative. Every generated
//! file carries one `//` or `#` header and no `///`, because nothing in it
//! reaches the API.
//!
//! A plain crate, whose roots no fragment of `graphql!` holds, takes the same
//! workspace with three differences. The workspace holds the generated copy of
//! the crate, `app/`, which mirrors the package by links, copies its crate root
//! with one generated module added, `#[path]` to the glue `glue.rs` beside it,
//! and carries a manifest of its own that states the package as cargo resolved
//! it, with every path dependency named from the copy. The workspace leaves the
//! copy out of its members, so the copy is a workspace of its own a card that
//! compiles the library builds alone, and cargo names its files by their
//! absolute paths, which the build remaps to the package. The server and the
//! inspect package name the copy in place of the package. The build script of
//! `server` calls the lean link and takes the artifacts the command line wrote
//! from the source of the crate, `aosabi_build::take_artifacts`, and names no
//! build dependency on the crate, and its main serves the entry of the
//! generated module, which hands the app the glue of its parts. The runner of
//! `inspect` reads the source path through `aosabi_build::inspect_source_from`.
//! A plain crate names no build side, so both packages name `aosabi-build` from
//! where the facade comes: the package beside a path facade, or the version or
//! the commit of the facade.
//!
//! `Flavour::Verify` writes the same workspace under
//! `<target_dir>/aosabi-verify/` with the packages `<name>-verify` and
//! `<name>-verify-inspect`, so a `verify` never touches `target/aosabi/`
//! and never overwrites the binary of a build.

use std::{
    fmt::Write,
    fs,
    path::{
        Component,
        Path,
        PathBuf, //
    }, //
};

use aosabi_build::{
    source::{
        Glue,
        MODULE, //
    },
    update_file_if_changed, //
};

use crate::{
    codes,
    project::{
        CopyDependency,
        CopyFacts,
        CopySource,
        DepSource,
        Project, //
    },
    report::CliError, //
};

/// The directory of the generated workspace and of the artifacts inside the
/// target directory.
pub const GENERATED_DIR: &str = "aosabi";

/// The directory of the generated workspace of `verify` inside the target
/// directory.
pub const VERIFY_DIR: &str = "aosabi-verify";

/// The directory of the generated copy of a plain crate inside the generated
/// workspace.
pub const COPY_DIR: &str = "app";

/// The file of the glue of a plain crate inside the generated workspace,
/// which the crate root of the copy includes as its module `__aosabi`.
pub const GLUE_FILE: &str = "glue.rs";

/// Returns the path of the glue of a plain crate as rustc reads it relative
/// to the generated copy, the directory of the crate root `root`, relative to
/// the package, joined to the path its module names the glue by, from the
/// directory of the crate root to the generated workspace that holds the
/// copy, as `src/../../glue.rs` for the crate root `src/lib.rs`.
#[must_use]
pub fn glue_in_copy(root: &Path) -> PathBuf {
    let dir = root.parent().unwrap_or_else(|| Path::new(""));
    let depth = dir
        .components()
        .filter(|part| matches!(part, Component::Normal(_)))
        .count();
    let mut path = dir.to_path_buf();
    for _ in 0..=depth {
        path.push("..");
    }
    path.push(GLUE_FILE);
    path
}

/// The features of the facade the cards that serve on Linux name, `server`,
/// `server-legacy` and `docker`.
pub const LINUX_FEATURES: &[&str] = &["serve", "unix-socket", "deflate", "linux"];

/// The release profile of the Aosabi workspace and the size profile of
/// ADR-0025, as every twin carries them.
const PROFILES: &str = "[profile.release]
lto = \"fat\"
codegen-units = 1
panic = \"abort\"

[profile.size]
inherits = \"release\"
opt-level = \"z\"
lto = \"fat\"
codegen-units = 1
panic = \"abort\"
strip = \"symbols\"
";

/// Which workspace a build writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavour {
    /// The workspace of `build` and `dev`, under `<target_dir>/aosabi/`.
    Build,
    /// The workspace of `verify`, under `<target_dir>/aosabi-verify/`.
    Verify,
}

/// The server package a card compiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerPackage {
    /// The features of the facade the card names.
    pub features: &'static [&'static str],
    /// The variant of `aosabi_build::LinuxCard` the lean link of the card
    /// names, `Server` or `ServerLegacy`.
    pub linux_card: &'static str,
}

/// What [`generate`] wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    /// The root of the generated workspace.
    pub root: PathBuf,
    /// The directory of the generated copy of a plain crate, `None` for a
    /// crate of the derive path.
    pub copy: Option<PathBuf>,
    /// The manifest of the server package.
    pub server_manifest: PathBuf,
    /// The manifest of the inspect package.
    pub inspect_manifest: PathBuf,
    /// The derived lock of the workspace.
    pub lock: PathBuf,
    /// The name of the server package and of its binary.
    pub server_bin: String,
    /// The name of the inspect package and of its binary.
    pub inspect_bin: String,
    /// The count of files written, 0 when every file held its bytes.
    pub written: usize,
}

/// Returns the root of the generated workspace of `project` in `flavour`.
#[must_use]
pub fn root(project: &Project, flavour: Flavour) -> PathBuf {
    project.target_dir.join(match flavour {
        Flavour::Build => GENERATED_DIR,
        Flavour::Verify => VERIFY_DIR,
    })
}

/// Writes the generated workspace of `project` for the server package
/// `package` in `flavour`, with the generated copy of a plain crate and its
/// glue `glue`.
///
/// # Errors
///
/// Returns a failure when a path is no UTF-8 text or a file or a link
/// cannot be written, and `AO-C0101` for a crate of the derive path that
/// names no build side.
pub fn generate(
    project: &Project,
    package: &ServerPackage,
    flavour: Flavour,
    glue: Option<&Glue>,
) -> Result<Generated, CliError> {
    if let (Some(copy), Some(glue)) = (&project.copy, glue) {
        return generate_source(project, copy, package, flavour, glue);
    }
    let root = root(project, flavour);
    let (server_bin, inspect_bin) = package_names(project, flavour);
    let server = root.join("server");
    let inspect = root.join("inspect");
    let files = [
        (root.join("Cargo.toml"), workspace_toml(project, &root)),
        (
            server.join("Cargo.toml"),
            server_toml(project, &server, &server_bin, package)?,
        ),
        (
            server.join("build.rs"),
            server_build_rs(project, &server, package),
        ),
        (
            server.join("src").join("main.rs"),
            server_main_rs(project, &server)?,
        ),
        (
            inspect.join("Cargo.toml"),
            inspect_toml(project, &inspect, &inspect_bin)?,
        ),
        (
            inspect.join("src").join("main.rs"),
            inspect_main_rs(project, &inspect),
        ),
    ];
    let mut written = 0;
    for (path, text) in &files {
        if write(path, text)? {
            written += 1;
        }
    }
    Ok(Generated {
        lock: root.join("Cargo.lock"),
        server_manifest: server.join("Cargo.toml"),
        inspect_manifest: inspect.join("Cargo.toml"),
        root,
        copy: None,
        server_bin,
        inspect_bin,
        written,
    })
}

/// Returns the names of the server and the inspect package of `project` in
/// `flavour`.
fn package_names(project: &Project, flavour: Flavour) -> (String, String) {
    match flavour {
        Flavour::Build => (
            format!("{}-server", project.name),
            format!("{}-inspect", project.name),
        ),
        Flavour::Verify => (
            format!("{}-verify", project.name),
            format!("{}-verify-inspect", project.name),
        ),
    }
}

/// Writes the generated workspace of the plain crate of `project`, whose
/// manifest the copy takes from `copy`, for the server package `package` in
/// `flavour`, with the generated copy of the crate and its glue `glue`.
fn generate_source(
    project: &Project,
    copy: &CopyFacts,
    package: &ServerPackage,
    flavour: Flavour,
    glue: &Glue,
) -> Result<Generated, CliError> {
    let root = root(project, flavour);
    let (server_bin, inspect_bin) = package_names(project, flavour);
    let server = root.join("server");
    let inspect = root.join("inspect");
    let app = root.join(COPY_DIR);
    let artifacts = match flavour {
        Flavour::Build => root.clone(),
        Flavour::Verify => root.join(crate::build::VERIFY_ARTIFACTS),
    };
    let build_side = build_side_of_facade(project)?;
    let files = [
        (
            root.join("Cargo.toml"),
            source_workspace_toml(project, &root),
        ),
        (app.join("Cargo.toml"), copy_toml(project, copy, &app)?),
        (
            server.join("Cargo.toml"),
            source_server_toml(project, &server, &server_bin, package, &build_side)?,
        ),
        (
            server.join("build.rs"),
            source_server_build_rs(project, &server, package, &artifacts)?,
        ),
        (
            server.join("src").join("main.rs"),
            source_server_main_rs(project, &server)?,
        ),
        (
            inspect.join("Cargo.toml"),
            source_inspect_toml(project, &inspect, &inspect_bin, &build_side)?,
        ),
        (
            inspect.join("src").join("main.rs"),
            source_inspect_main_rs(project, &inspect)?,
        ),
    ];
    let mut written = 0;
    for (path, text) in &files {
        if write(path, text)? {
            written += 1;
        }
    }
    let copied = aosabi_build::source::write_copy(
        &project.root,
        &copy.root,
        &app,
        &root.join(GLUE_FILE),
        glue,
    )
    .map_err(|error| CliError::new(format!("the generated copy {}: {error}", app.display())))?;
    written += copied.written;
    Ok(Generated {
        lock: root.join("Cargo.lock"),
        server_manifest: server.join("Cargo.toml"),
        inspect_manifest: inspect.join("Cargo.toml"),
        root,
        copy: Some(app),
        server_bin,
        inspect_bin,
        written,
    })
}

/// Writes the generated copy of the plain crate of `project` with its glue
/// `glue` under the generated workspace of `flavour`, its manifest among it,
/// and answers its directory, for a card that compiles the library of the
/// copy alone.
///
/// # Errors
///
/// Returns a failure when a path is no UTF-8 text or a file or a link
/// cannot be written, and `AO-C0101` for a crate that is not plain.
pub fn write_source_copy(
    project: &Project,
    flavour: Flavour,
    glue: &Glue,
) -> Result<PathBuf, CliError> {
    let copy = project
        .copy
        .as_ref()
        .ok_or_else(|| codes::not_a_service(&project.name, false))?;
    let root = root(project, flavour);
    let app = root.join(COPY_DIR);
    write(&app.join("Cargo.toml"), &copy_toml(project, copy, &app)?)?;
    aosabi_build::source::write_copy(&project.root, &copy.root, &app, &root.join(GLUE_FILE), glue)
        .map_err(|error| CliError::new(format!("the generated copy {}: {error}", app.display())))?;
    Ok(app)
}

/// Returns the source of `aosabi-build` for a plain crate, which names no
/// build side: the package beside a path facade, or the version or the
/// commit of the facade.
///
/// # Errors
///
/// Returns `AO-C0101` for a path facade with no package `aosabi-build`
/// beside it.
fn build_side_of_facade(project: &Project) -> Result<DepSource, CliError> {
    if let Some(side) = &project.aosabi_build {
        return Ok(side.clone());
    }
    Ok(match &project.aosabi {
        DepSource::Path(dir) => {
            let side = dir
                .parent()
                .map(|parent| parent.join(crate::project::BUILD_SIDE))
                .filter(|side| side.join("Cargo.toml").is_file())
                .ok_or_else(|| codes::not_a_service(&project.name, true))?;
            DepSource::Path(side)
        }
        DepSource::Registry { version } => DepSource::Registry {
            version: version.clone(),
        },
        DepSource::Git { url, rev } => DepSource::Git {
            url: url.clone(),
            rev: rev.clone(),
        },
    })
}

/// Returns the text of the root manifest of the workspace at `root` of a
/// plain crate, which leaves the generated copy out of its members.
fn source_workspace_toml(project: &Project, root: &Path) -> String {
    format!(
        "{}[workspace]\nresolver = \"3\"\nmembers = [\"server\", \"inspect\"]\n\
         # The generated copy of the crate is a workspace of its own, which a card\n\
         # that compiles the library builds alone.\n\
         exclude = [\"{COPY_DIR}\"]\n\n{PROFILES}",
        header(project, "#", &root.join("Cargo.toml"))
    )
}

/// Returns the text of the manifest of the generated copy in `dir` of the
/// plain crate of `project`, whose package `copy` states as cargo resolved
/// it.
///
/// The copy is a workspace of its own with the resolver `3` of the generated
/// workspace, so a card that builds it alone takes the features of a table of a
/// platform where its platform applies, as the generated workspace and the
/// reader take them, where cargo takes resolver 1 for a package of edition 2015
/// or 2018 and unites the features of every table whatever its platform.
fn copy_toml(project: &Project, copy: &CopyFacts, dir: &Path) -> Result<String, CliError> {
    let mut text = header(project, "#", &dir.join("Cargo.toml"));
    let _ = writeln!(
        text,
        "[package]\nname = {}\nversion = {}\nedition = {}",
        toml_string(&project.name),
        toml_string(&project.version),
        toml_string(&project.edition),
    );
    if let Some(rust_version) = &copy.rust_version {
        let _ = writeln!(text, "rust-version = {}", toml_string(rust_version));
    }
    text.push_str("publish = false\n");
    match &copy.build_script {
        Some(script) => {
            let _ = writeln!(text, "build = {}", toml_string(&utf8(script)?));
        }
        None => text.push_str("build = false\n"),
    }
    text.push_str(
        "autobins = false\nautoexamples = false\nautotests = false\nautobenches = false\n\n",
    );
    let _ = writeln!(
        text,
        "[lib]\nname = {}\npath = {}",
        toml_string(&project.lib_name),
        toml_string(&utf8(&copy.root)?),
    );
    if copy.crate_types != ["lib"] && !copy.crate_types.is_empty() {
        let kinds: Vec<String> = copy
            .crate_types
            .iter()
            .map(|kind| toml_string(kind))
            .collect();
        let _ = writeln!(text, "crate-type = [{}]", kinds.join(", "));
    }
    let mut tables: Vec<(Option<&str>, bool)> = Vec::new();
    for dependency in &copy.dependencies {
        let table = (dependency.target.as_deref(), dependency.build);
        if !tables.contains(&table) {
            tables.push(table);
        }
    }
    for table in tables {
        let kind = if table.1 {
            "build-dependencies"
        } else {
            "dependencies"
        };
        match table.0 {
            None => {
                let _ = writeln!(text, "\n[{kind}]");
            }
            Some(target) => {
                let _ = writeln!(text, "\n[target.{}.{kind}]", toml_string(target));
            }
        }
        for dependency in copy
            .dependencies
            .iter()
            .filter(|dependency| (dependency.target.as_deref(), dependency.build) == table)
        {
            text.push_str(&dependency_line(dependency, dir)?);
        }
    }
    if !copy.features.is_empty() {
        text.push_str("\n[features]\n");
        for (name, enabled) in &copy.features {
            let enabled: Vec<String> = enabled.iter().map(|item| toml_string(item)).collect();
            let _ = writeln!(text, "{} = [{}]", toml_key(name), enabled.join(", "));
        }
    }
    text.push_str(&copy_lints(project)?);
    text.push_str(
        "\n# A workspace of its own, which the generated workspace leaves out, with\n\
         # the resolver of the generated workspace whatever the edition.\n\
         [workspace]\nresolver = \"3\"\n",
    );
    Ok(text)
}

/// Returns the lint tables of the package of `project` for the manifest of
/// its generated copy, so every level the package sets holds in the copy
/// every card compiles, `forbid` of `unsafe_code` among them.
///
/// Cargo metadata states no lint, so the tables come from the text of the
/// manifest: `[lints]` and every `[lints.<tool>]` as written, or for a package
/// that inherits them with `workspace = true` in `[lints]`, the tables
/// `[workspace.lints]` and `[workspace.lints.<tool>]` of the manifest of its
/// workspace, the directory of its lock, as `[lints]` tables, since the copy is
/// a workspace of its own. A table written as an inline table or by dotted keys
/// before the first header is not read.
///
/// # Errors
///
/// Returns a failure when a manifest cannot be read.
fn copy_lints(project: &Project) -> Result<String, CliError> {
    let read = |path: &Path| {
        fs::read_to_string(path)
            .map_err(|error| CliError::new(format!("{}: {error}", path.display())))
    };
    let mut tables = tables_under(&read(&project.manifest)?, &["lints"]);
    let inherits = tables
        .iter()
        .any(|(keys, lines)| keys.is_empty() && lines.iter().any(|line| is_workspace_true(line)));
    if inherits && let Some(root) = project.lock.parent() {
        tables = tables_under(&read(&root.join("Cargo.toml"))?, &["workspace", "lints"]);
    }
    let mut text = String::new();
    for (keys, lines) in tables {
        let header = if keys.is_empty() {
            String::from("[lints]")
        } else {
            format!("[lints.{}]", keys.join("."))
        };
        let lines: Vec<&String> = lines
            .iter()
            .filter(|line| !is_workspace_true(line))
            .collect();
        // The comments after the last key stand above the next table, so
        // they stay with it.
        let end = lines
            .iter()
            .rposition(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('#')
            })
            .map_or(0, |last| last + 1);
        let _ = writeln!(text, "\n{header}");
        for line in &lines[..end] {
            let _ = writeln!(text, "{line}");
        }
    }
    Ok(text)
}

/// Returns every table of the manifest text `text` whose header opens with
/// the keys `prefix`, each as the keys of its header after the prefix and
/// its lines up to the next header.
fn tables_under(text: &str, prefix: &[&str]) -> Vec<(Vec<String>, Vec<String>)> {
    let mut tables: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    let mut current = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            current = header_keys(trimmed)
                .filter(|keys| keys.len() >= prefix.len() && keys[..prefix.len()] == *prefix)
                .map(|keys| {
                    tables.push((keys[prefix.len()..].to_vec(), Vec::new()));
                    tables.len() - 1
                });
            continue;
        }
        if let Some(index) = current {
            tables[index].1.push(line.to_owned());
        }
    }
    tables
}

/// Returns the keys of the table header `line`, `None` for the header of an
/// array of tables.
fn header_keys(line: &str) -> Option<Vec<String>> {
    if line.starts_with("[[") {
        return None;
    }
    let inner = line.strip_prefix('[')?;
    let end = inner.find(']')?;
    Some(
        inner[..end]
            .split('.')
            .map(|key| key.trim().trim_matches('"').to_owned())
            .collect(),
    )
}

/// Returns whether the manifest line `line` is `workspace = true`, the key
/// by which a package inherits the lints of its workspace.
fn is_workspace_true(line: &str) -> bool {
    let code = line.split('#').next().unwrap_or_default();
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    compact == "workspace=true"
}

/// Returns the line of the dependency `dependency` in the manifest of the
/// generated copy in `dir`, with its source, its package, its features and
/// its flags as the package states them.
fn dependency_line(dependency: &CopyDependency, dir: &Path) -> Result<String, CliError> {
    let mut parts = vec![match &dependency.source {
        CopySource::Path(path) => {
            format!("path = {}", toml_string(&utf8(&relative_path(dir, path))?))
        }
        CopySource::Registry { req, registry } => {
            let mut spec = format!("version = {}", toml_string(req));
            if let Some(registry) = registry {
                let _ = write!(spec, ", registry-index = {}", toml_string(registry));
            }
            spec
        }
        CopySource::Git { url, reference } => {
            let mut spec = format!("git = {}", toml_string(url));
            if let Some((key, value)) = reference.as_deref().and_then(|r| r.split_once('=')) {
                let _ = write!(spec, ", {key} = {}", toml_string(value));
            }
            spec
        }
    }];
    if dependency.package != dependency.key {
        parts.push(format!("package = {}", toml_string(&dependency.package)));
    }
    if !dependency.features.is_empty() {
        let features: Vec<String> = dependency
            .features
            .iter()
            .map(|feature| toml_string(feature))
            .collect();
        parts.push(format!("features = [{}]", features.join(", ")));
    }
    if !dependency.default_features {
        parts.push(String::from("default-features = false"));
    }
    if dependency.optional {
        parts.push(String::from("optional = true"));
    }
    Ok(format!(
        "{} = {{ {} }}\n",
        toml_key(&dependency.key),
        parts.join(", ")
    ))
}

/// Returns the text of the manifest of the server package in `dir` of a
/// plain crate, which names the generated copy and `aosabi-build` from
/// `build_side`.
fn source_server_toml(
    project: &Project,
    dir: &Path,
    name: &str,
    package: &ServerPackage,
    build_side: &DepSource,
) -> Result<String, CliError> {
    let app = toml_key(&project.name);
    let copy = toml_string(&format!("../{COPY_DIR}"));
    let features: Vec<String> = package
        .features
        .iter()
        .map(|feature| toml_string(feature))
        .collect();
    Ok(format!(
        "{}[package]\nname = {}\nversion = {}\nedition = {}\npublish = false\nbuild = \"build.rs\"\n\n\
         [dependencies]\n{app} = {{ path = {copy} }}\naosabi = {{ {}, features = [{}] }}\n\n\
         [build-dependencies]\naosabi-build = {{ {} }}\n",
        header(project, "#", &dir.join("Cargo.toml")),
        toml_string(name),
        toml_string(&project.version),
        toml_string(&project.edition),
        spec(&project.aosabi, dir)?,
        features.join(", "),
        spec(build_side, dir)?,
    ))
}

/// Returns the text of the build script of the server package in `dir` of
/// a plain crate, which takes the artifacts of `artifacts`.
fn source_server_build_rs(
    project: &Project,
    dir: &Path,
    package: &ServerPackage,
    artifacts: &Path,
) -> Result<String, CliError> {
    let from = utf8(&relative_path(dir, artifacts))?;
    Ok(format!(
        "{}fn main() -> std::process::ExitCode {{\n    \
         aosabi_build::lean_link(aosabi_build::LinuxCard::{});\n    \
         aosabi_build::take_artifacts({})\n}}\n",
        header(project, "//", &dir.join("build.rs")),
        package.linux_card,
        rust_string(&from),
    ))
}

/// Returns the text of the main of the server package in `dir` of a plain
/// crate, which serves the entry of the generated module, with the
/// configuration the app carries where the project holds no
/// `aosabi.config.rs`.
fn source_server_main_rs(project: &Project, dir: &Path) -> Result<String, CliError> {
    let main = dir.join("src").join("main.rs");
    let head = header(project, "//", &main);
    let entry = format!("{}::{MODULE}::entry", project.lib_name);
    let Some(config) = &project.config else {
        return Ok(format!(
            "{head}fn main() -> std::process::ExitCode {{\n    \
             aosabi::serve_app({entry}, aosabi::build_output!())\n}}\n"
        ));
    };
    let config_path = utf8(&relative_path(&dir.join("src"), config))?;
    Ok(format!(
        "{head}#[path = {}]\nmod config;\n\nfn main() -> std::process::ExitCode {{\n    \
         aosabi::serve({entry}, config::config, aosabi::build_output!())\n}}\n",
        rust_string(&config_path)
    ))
}

/// Returns the text of the manifest of the inspect package in `dir` of a
/// plain crate, which names the generated copy and `aosabi-build` from
/// `build_side` with the reader of the source path.
fn source_inspect_toml(
    project: &Project,
    dir: &Path,
    name: &str,
    build_side: &DepSource,
) -> Result<String, CliError> {
    let app = toml_key(&project.name);
    let copy = toml_string(&format!("../{COPY_DIR}"));
    Ok(format!(
        "{}[package]\nname = {}\nversion = {}\nedition = {}\npublish = false\n\n\
         [dependencies]\n{app} = {{ path = {copy} }}\naosabi-build = {{ {}, features = [\"source\"] }}\n",
        header(project, "#", &dir.join("Cargo.toml")),
        toml_string(name),
        toml_string(&project.version),
        toml_string(&project.edition),
        spec(build_side, dir)?,
    ))
}

/// Returns the text of the main of the inspect package in `dir` of a plain
/// crate, which constructs the app through the entry of the generated module
/// and reads the source of the package.
fn source_inspect_main_rs(project: &Project, dir: &Path) -> Result<String, CliError> {
    let package = utf8(&relative_path(dir, &project.root))?;
    Ok(format!(
        "{}fn main() -> std::process::ExitCode {{\n    \
         aosabi_build::inspect_source_from({}::{MODULE}::entry, concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/\", {}))\n}}\n",
        header(project, "//", &dir.join("src").join("main.rs")),
        project.lib_name,
        rust_string(&package),
    ))
}

/// Returns the path of `to` relative to the directory `from`, both absolute,
/// or `to` itself when the two share no prefix.
#[must_use]
pub fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<Component<'_>> = from.components().collect();
    let to_parts: Vec<Component<'_>> = to.components().collect();
    let common = from
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0 {
        return to.to_path_buf();
    }
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &to_parts[common..] {
        out.push(part);
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// Returns `text` as a TOML basic string.
#[must_use]
pub fn toml_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Returns `name` as a key of a TOML table, bare when it may stand bare.
#[must_use]
pub fn toml_key(name: &str) -> String {
    let bare = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if bare {
        name.to_owned()
    } else {
        toml_string(name)
    }
}

/// Returns the source of the build side of `project`, which every crate of
/// the derive path names and a plain crate does not.
///
/// # Errors
///
/// Returns `AO-C0101` for a project that names no build side, whose
/// workspace no command generates.
fn build_side(project: &Project) -> Result<&DepSource, CliError> {
    project
        .aosabi_build
        .as_ref()
        .ok_or_else(|| codes::not_a_service(&project.name, false))
}

/// Returns the dependency specification of `source` inside an inline table
/// written in the directory `dir`.
///
/// # Errors
///
/// Returns a failure when the path of a path source is no UTF-8 text.
pub fn spec(source: &DepSource, dir: &Path) -> Result<String, CliError> {
    Ok(match source {
        DepSource::Path(path) => {
            format!("path = {}", toml_string(&utf8(&relative_path(dir, path))?))
        }
        DepSource::Registry { version } => {
            format!("version = {}", toml_string(&format!("={version}")))
        }
        DepSource::Git { url, rev } => {
            format!("git = {}, rev = {}", toml_string(url), toml_string(rev))
        }
    })
}

/// Returns `path` as UTF-8 text.
///
/// # Errors
///
/// Returns a failure when `path` is no UTF-8 text, which no manifest can
/// name.
pub fn utf8(path: &Path) -> Result<String, CliError> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        CliError::new(format!(
            "the path {} is no UTF-8 text, which a generated manifest cannot name",
            path.display()
        ))
    })
}

/// Returns the header line of the generated file `path`, with `prefix`.
fn header(project: &Project, prefix: &str, path: &Path) -> String {
    let shown = path.strip_prefix(&project.root).unwrap_or(path);
    format!(
        "{prefix} {}, generated by cargo aosabi. The next build writes it again.\n",
        shown.display()
    )
}

/// Returns the text of the root manifest of the workspace at `root`.
fn workspace_toml(project: &Project, root: &Path) -> String {
    format!(
        "{}[workspace]\nresolver = \"3\"\nmembers = [\"server\", \"inspect\"]\n\n{PROFILES}",
        header(project, "#", &root.join("Cargo.toml"))
    )
}

/// Returns the text of the manifest of the server package in `dir`.
fn server_toml(
    project: &Project,
    dir: &Path,
    name: &str,
    package: &ServerPackage,
) -> Result<String, CliError> {
    let app = toml_key(&project.name);
    let app_path = toml_string(&utf8(&relative_path(dir, &project.root))?);
    let features: Vec<String> = package
        .features
        .iter()
        .map(|feature| toml_string(feature))
        .collect();
    Ok(format!(
        "{}[package]\nname = {}\nversion = {}\nedition = {}\npublish = false\nbuild = \"build.rs\"\n\n\
         [dependencies]\n{app} = {{ path = {app_path} }}\naosabi = {{ {}, features = [{}] }}\n\n\
         [build-dependencies]\n{app} = {{ path = {app_path} }}\naosabi-build = {{ {} }}\n",
        header(project, "#", &dir.join("Cargo.toml")),
        toml_string(name),
        toml_string(&project.version),
        toml_string(&project.edition),
        spec(&project.aosabi, dir)?,
        features.join(", "),
        spec(build_side(project)?, dir)?,
    ))
}

/// Returns the text of the build script of the server package in `dir`.
fn server_build_rs(project: &Project, dir: &Path, package: &ServerPackage) -> String {
    format!(
        "{}fn main() -> std::process::ExitCode {{\n    \
         aosabi_build::lean_link(aosabi_build::LinuxCard::{});\n    \
         aosabi_build::emit_from({}::default)\n}}\n",
        header(project, "//", &dir.join("build.rs")),
        package.linux_card,
        project.lib_name,
    )
}

/// Returns the text of the main of the server package in `dir`, which names
/// the configuration of the service when the service has one, and serves the
/// configuration its app carries when it has none.
fn server_main_rs(project: &Project, dir: &Path) -> Result<String, CliError> {
    let main = dir.join("src").join("main.rs");
    let head = header(project, "//", &main);
    let lib = &project.lib_name;
    let Some(config) = &project.config else {
        return Ok(format!(
            "{head}fn main() -> std::process::ExitCode {{\n    \
             aosabi::serve_app({lib}::default, aosabi::build_output!())\n}}\n"
        ));
    };
    let config_path = utf8(&relative_path(&dir.join("src"), config))?;
    Ok(format!(
        "{head}#[path = {}]\nmod config;\n\nfn main() -> std::process::ExitCode {{\n    \
         aosabi::serve({lib}::default, config::config, aosabi::build_output!())\n}}\n",
        rust_string(&config_path)
    ))
}

/// Returns the text of the manifest of the inspect package in `dir`.
fn inspect_toml(project: &Project, dir: &Path, name: &str) -> Result<String, CliError> {
    let app = toml_key(&project.name);
    let app_path = toml_string(&utf8(&relative_path(dir, &project.root))?);
    Ok(format!(
        "{}[package]\nname = {}\nversion = {}\nedition = {}\npublish = false\n\n\
         [dependencies]\n{app} = {{ path = {app_path} }}\naosabi-build = {{ {} }}\n",
        header(project, "#", &dir.join("Cargo.toml")),
        toml_string(name),
        toml_string(&project.version),
        toml_string(&project.edition),
        spec(build_side(project)?, dir)?,
    ))
}

/// Returns the text of the main of the inspect package in `dir`.
fn inspect_main_rs(project: &Project, dir: &Path) -> String {
    format!(
        "{}fn main() -> std::process::ExitCode {{\n    aosabi_build::inspect_from({}::default)\n}}\n",
        header(project, "//", &dir.join("src").join("main.rs")),
        project.lib_name,
    )
}

/// Returns `text` as a Rust string literal.
fn rust_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Writes `text` to `path` unless the file holds those bytes, creating its
/// directory, and answers whether it wrote.
fn write(path: &Path, text: &str) -> Result<bool, CliError> {
    let failure = |error: std::io::Error| CliError::new(format!("{}: {error}", path.display()));
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(failure)?;
    }
    update_file_if_changed(path, text.as_bytes()).map_err(failure)
}
