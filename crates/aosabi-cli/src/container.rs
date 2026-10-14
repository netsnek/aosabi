// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the container files and the image of the docker card.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/create-pylon/src/create-directory/files.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The container files and the image.
//!
//! The reference scaffold writes a `Dockerfile` of three stages on
//! `node:20-alpine` that installs, builds and runs the service as the user
//! `node` on port 3000. The `docker` card writes the same role on every build,
//! a multi stage build from the slim Rust image of the pinned toolchain to the
//! distroless runtime with the C runtime, port 3000, as the user `nonroot`,
//! both images pinned by the digests of [`super::cards::digests`]. The builder
//! stage compiles from source, because a binary linked against the glibc of the
//! host does not run on the older glibc of the runtime image, and the runtime
//! is the `cc` variant, because a Rust binary on glibc links `libgcc_s`.
//!
//! [`write_container_files`] writes `target/aosabi/docker/Containerfile` and
//! `target/aosabi/docker/Containerfile.dockerignore` through
//! `update_file_if_changed`. The context of the image is the project root, or
//! for a path facade inside the git checkout that holds the project the root of
//! that checkout, so the workspace of the facade and the forks of
//! `third_party/rust` are in it. The ignore file excludes every path with `*`
//! and then names what the builder reads: for a checkout the root `Cargo.toml`,
//! `Cargo.lock` and `clippy.toml` and the directories `crates` and
//! `third_party/rust`, and of the project its `Cargo.toml`, `Cargo.lock`,
//! `build.rs`, `aosabi.config.rs` and `src`, the generated workspace with its
//! derived lock, and every `--include` path. A plain crate adds the generated
//! copy of the crate with its glue and the three artifacts of the source path
//! its server takes, since no build script infers its schema, and the links of
//! the copy name their entries by relative paths, which resolve under `/build`
//! as on the host. The builder needs no `cargo-aosabi`, because the generated
//! workspace and its checked lock travel with the context. The builder names
//! the artifact directory, because cargo writes no `CACHEDIR.TAG` into a target
//! directory the context already holds, and builds as the release build of the
//! host does, for the target `rustc --print host-tuple` names, with
//! `-C panic=abort` and the remapped prefixes of its own paths in `RUSTFLAGS`,
//! so the lean link of the card acts there too. No flag of the environment of
//! the command line reaches the stage, so the reader reads the crate of the
//! card with the flags of the stage alone, and the build of the card on the
//! host passes them without the flags of its environment as well. It links with
//! the LLD of the toolchain, which packs the relative relocations the lean link
//! of `server` asks for, where the GNU ld 2.40 of the builder image ignores
//! `-z pack-relative-relocs` on aarch64. It copies the binary out of its target
//! directory through the `rust-objcopy --strip-all` of the toolchain, so the
//! image carries the binary stripped while the crates and their code stay those
//! of a build without the strip. Every path of the context stands in the file
//! unquoted, as its `COPY` lines need it, so a path with white space does not
//! build. The ignore file leaves the `rust-toolchain.toml` of a checkout out,
//! since rustup in the builder reads it and downloads the components and the
//! targets it names before the build, while the image of the builder carries
//! the pinned toolchain for the host the build targets.
//!
//! At the switch `AOSABI_STD` at `source` the builder stage builds the standard
//! library from source with the set of the card, as the release build of the
//! host does ([`crate::std_source`]). It adds the component `rust-src` of the
//! toolchain of the builder image in a layer of its own before the `COPY` of
//! the context, where the engine keeps it across builds, still with no
//! `rust-toolchain.toml` in the context, runs its cargo call with
//! `RUSTC_BOOTSTRAP=1`, the crates of `-Zbuild-std` and no feature of the
//! standard library, maps the source of the standard library in its sysroot to
//! `/rustc/<commit>` after its other prefixes, so the image carries no path
//! under the rustup home of the builder in a panic location, and runs rustc
//! through the wrapper of the options per crate that the build writes beside
//! the generated workspace with no wrapper of the host chained,
//! [`BUILDER_WRAPPER`], which the ignore file names. At `prebuilt` the stage
//! stays as above.
//!
//! [`find_engine`] reads `AOSABI_CONTAINER_ENGINE`, where `none` means no
//! engine, and then looks for `podman` and `docker` on the path, and
//! [`build_image`] runs the engine with absolute paths, podman with
//! `--ignorefile` and docker without it, since `BuildKit` reads
//! `<Containerfile>.dockerignore` beside the file it builds, and reads the size
//! of the image with `image inspect`. The image is built for the architecture
//! of the host.

use std::{
    ffi::OsString,
    fmt::Write,
    fs,
    path::{
        Path,
        PathBuf, //
    },
    process::{
        Command,
        Stdio, //
    },
};

use aosabi_build::{
    NAME_FILE,
    RESOLVE_TYPES_FILE,
    SCHEMA_FILE,
    update_file_if_changed, //
};

use crate::{
    build::LEAN_FLAGS,
    cards::digests::{
        BUILDER_DIGEST,
        BUILDER_IMAGE,
        BUILDER_TOOLCHAIN,
        RUNTIME_DIGEST,
        RUNTIME_IMAGE, //
    },
    codes,
    dotenv::DotEnv,
    env::{
        self,
        CONTAINER_ENGINE, //
    },
    generate::{
        COPY_DIR,
        GENERATED_DIR,
        GLUE_FILE,
        glue_in_copy,
        utf8, //
    },
    project::{
        CONFIG_FILE,
        DepSource,
        FACADE,
        LOCK_FILE,
        Project, //
    },
    report::CliError,
    std_source::{
        self,
        SourceSet, //
    }, //
};

/// The directory of the container files inside the generated workspace.
pub const DOCKER_DIR: &str = "docker";

/// The container file.
pub const CONTAINERFILE: &str = "Containerfile";

/// The ignore file beside it, which `BuildKit` finds by its name.
pub const IGNORE_FILE: &str = "Containerfile.dockerignore";

/// The file the engine writes the id of the image into.
pub const IMAGE_ID_FILE: &str = "image.id";

/// The wrapper of rustc of the builder stage at the switch `AOSABI_STD` at
/// `source`, beside the generated workspace, which chains no wrapper of the
/// host, so the directory of the container files holds its two files alone.
pub const BUILDER_WRAPPER: &str = "rustc-std-source-builder.sh";

/// The port the image exposes and serves on.
pub const PORT: u16 = 3000;

/// The flags of the release build in the builder that link with the LLD of
/// the toolchain, `ld.lld` in `lib/rustlib/<triple>/bin/gcc-ld/` of its
/// sysroot, which the compiler driver takes under `-fuse-ld=lld` from the
/// directory `-B` names, with the variables `sysroot` and `triple` of the
/// `RUN` line.
///
/// The GNU ld 2.40 of the builder image ignores `-z pack-relative-relocs` on
/// aarch64 with a warning and links no `DT_RELR`, which this LLD 22.1.2 links.
/// The default script of GNU ld gathers the cold code of `.text.unlikely` at
/// the head of `.text`, while LLD mixes it into `.text` unless
/// `-z keep-text-section-prefix` keeps it in a section of its own, 74,276 bytes
/// of the binary of `in-context-app` for 80 bytes more of file and no more
/// loaded bytes.
const LLD_FLAGS: &str = "-C link-arg=-fuse-ld=lld -C link-arg=-B$sysroot/lib/rustlib/$triple/bin/gcc-ld -C link-arg=-Wl,-z,keep-text-section-prefix";

/// The paths of a checkout the builder reads beside the project.
const CHECKOUT_PATHS: [&str; 5] = [
    "Cargo.toml",
    LOCK_FILE,
    "clippy.toml",
    "crates",
    "third_party/rust",
];

/// What the builder stage of a build at the switch `source` takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuilderSource<'a> {
    /// The set of the card.
    pub set: SourceSet,
    /// The crates of the framework the wrapper of rustc names.
    pub crates: &'a [String],
}

/// What [`build_image`] built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    /// The id of the image.
    pub id: String,
    /// The tag of the image, `localhost/<name>:<version>`.
    pub tag: String,
    /// The size of the image in bytes, as the engine reports it.
    pub size: u64,
}

/// Returns the directory of the container files of `project`.
#[must_use]
pub fn docker_dir(project: &Project) -> PathBuf {
    project.target_dir.join(GENERATED_DIR).join(DOCKER_DIR)
}

/// Returns the tag of the image of `project`, `localhost/<name>:<version>`
/// with the name in lower case, which an image name needs.
#[must_use]
pub fn image_tag(project: &Project) -> String {
    format!(
        "localhost/{}:{}",
        project.name.to_ascii_lowercase(),
        project.version
    )
}

/// Returns the context of the image of `project`: the root of the git
/// checkout that holds the project and its path facade, else the project
/// root.
#[must_use]
pub fn context(project: &Project) -> PathBuf {
    let DepSource::Path(facade) = &project.aosabi else {
        return project.root.clone();
    };
    let checkout = project
        .root
        .ancestors()
        .find(|dir| dir.join(".git").exists());
    match checkout {
        Some(checkout) if facade.starts_with(checkout) => checkout.to_path_buf(),
        _ => project.root.clone(),
    }
}

/// Writes the container file and its ignore file of `project` with the
/// `--include` paths `includes`, and the wrapper of rustc of the builder
/// stage where `source` names the set of a build at the switch `source`, and
/// answers their directory.
///
/// # Errors
///
/// Returns a failure when the project or its target directory lies outside
/// the context, a path is no UTF-8 text, or a file cannot be written.
pub fn write_container_files(
    project: &Project,
    includes: &[PathBuf],
    source: Option<BuilderSource<'_>>,
) -> Result<PathBuf, CliError> {
    let dir = docker_dir(project);
    let context = context(project);
    let layout = Layout::new(project, &context, includes)?;
    let files = [
        (
            dir.join(CONTAINERFILE),
            containerfile(project, &layout, &dir, source),
        ),
        (
            dir.join(IGNORE_FILE),
            ignore_file(project, &layout, &dir, &context, source.is_some()),
        ),
    ];
    fs::create_dir_all(&dir).map_err(|error| io_failure(&dir, &error))?;
    for (path, text) in &files {
        update_file_if_changed(path, text.as_bytes()).map_err(|error| io_failure(path, &error))?;
    }
    if let Some(source) = source {
        std_source::write_program(
            &project.target_dir.join(GENERATED_DIR).join(BUILDER_WRAPPER),
            &std_source::wrapper(source.crates, None),
        )?;
    }
    Ok(dir)
}

/// Returns the container engine: the program `AOSABI_CONTAINER_ENGINE`
/// names, none for `none`, else `podman` and then `docker` from the path.
#[must_use]
pub fn find_engine(env_file: &DotEnv) -> Option<PathBuf> {
    match env::var(env_file, CONTAINER_ENGINE) {
        Some(value) if value == "none" => return None,
        Some(value) if !value.is_empty() => return Some(PathBuf::from(value)),
        _ => {}
    }
    let path = env::var(env_file, "PATH")?;
    ["podman", "docker"].into_iter().find_map(|name| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|file| is_program(file))
    })
}

/// Returns the arguments of the engine `engine` that build the image of
/// `project`, every path absolute.
#[must_use]
pub fn build_arguments(engine: &Path, project: &Project) -> Vec<OsString> {
    let dir = docker_dir(project);
    let mut args: Vec<OsString> = vec![
        "build".into(),
        "--file".into(),
        dir.join(CONTAINERFILE).into_os_string(),
    ];
    if is_podman(engine) {
        args.push("--ignorefile".into());
        args.push(dir.join(IGNORE_FILE).into_os_string());
    }
    args.extend([
        "--iidfile".into(),
        dir.join(IMAGE_ID_FILE).into_os_string(),
        "--tag".into(),
        image_tag(project).into(),
        context(project).into_os_string(),
    ]);
    args
}

/// Returns the command line that builds the image of `project` with the
/// engine `engine`, to print where no engine runs it.
#[must_use]
pub fn command_line(engine: &Path, project: &Project) -> String {
    let mut line = engine.display().to_string();
    for arg in build_arguments(engine, project) {
        line.push(' ');
        line.push_str(&arg.to_string_lossy());
    }
    line
}

/// Builds the image of `project` with the engine `engine` from the context
/// directory and reads its id and size.
///
/// # Errors
///
/// Returns `AO-C0701` with the output of the engine when the build or the
/// inspection fails.
pub fn build_image(engine: &Path, project: &Project) -> Result<ImageInfo, CliError> {
    let name = engine.display().to_string();
    let tag = image_tag(project);
    eprintln!("building the image {tag} with {name}");
    let built = Command::new(engine)
        .args(build_arguments(engine, project))
        .current_dir(context(project))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| codes::engine_failed(&name, error.to_string()))?;
    if !built.status.success() {
        return Err(codes::engine_failed(&name, both_streams(&built)));
    }
    let id_file = docker_dir(project).join(IMAGE_ID_FILE);
    let id = fs::read_to_string(&id_file)
        .map_err(|error| codes::engine_failed(&name, format!("{}: {error}", id_file.display())))?
        .trim()
        .to_owned();
    let inspected = Command::new(engine)
        .args(["image", "inspect", "--format", "{{.Size}}", &id])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| codes::engine_failed(&name, error.to_string()))?;
    if !inspected.status.success() {
        return Err(codes::engine_failed(&name, both_streams(&inspected)));
    }
    let size = String::from_utf8_lossy(&inspected.stdout)
        .trim()
        .parse()
        .map_err(|_| codes::engine_failed(&name, both_streams(&inspected)))?;
    Ok(ImageInfo { id, tag, size })
}

/// The paths of the project inside the context, each `/` separated and
/// relative to the context.
struct Layout {
    /// The project root, empty when it is the context.
    app: String,
    /// The target directory.
    target: String,
    /// Whether the context is a checkout with the facade's workspace.
    checkout: bool,
    /// The workspace of a path facade inside the context, empty when it is
    /// the context, `None` for another facade.
    facade: Option<String>,
    /// The generated copy of a plain crate, `None` for a crate of the
    /// derive path.
    copy: Option<PlainCopy>,
    /// Every `--include` path relative to the project root.
    includes: Vec<String>,
}

impl Layout {
    /// Returns the layout of `project` in `context` with the `--include`
    /// paths `includes`.
    fn new(project: &Project, context: &Path, includes: &[PathBuf]) -> Result<Self, CliError> {
        let inside = |path: &Path| -> Result<String, CliError> {
            let relative = path.strip_prefix(context).map_err(|_| {
                CliError::new(format!(
                    "{} lies outside the context {} of the image",
                    path.display(),
                    context.display()
                ))
            })?;
            utf8(relative)
        };
        let mut paths = Vec::with_capacity(includes.len());
        for include in includes {
            let relative = include
                .strip_prefix(&project.root)
                .unwrap_or(include.as_path());
            paths.push(utf8(relative)?);
        }
        let facade = project
            .facade_workspace
            .as_deref()
            .filter(|workspace| workspace.starts_with(context))
            .map(inside)
            .transpose()?;
        let copy = match (project.plain, project.copy.as_ref()) {
            (true, Some(facts)) => {
                let generated = project.target_dir.join(GENERATED_DIR);
                let glue = generated.join(GLUE_FILE);
                let glue_to = match glue.strip_prefix(&project.root) {
                    Ok(rest) => utf8(&Path::new(&project.name).join(rest))?,
                    Err(_) => in_builder(&inside(&glue)?),
                };
                Some(PlainCopy {
                    dir: inside(&generated.join(COPY_DIR))?,
                    glue_read: utf8(&Path::new(COPY_DIR).join(glue_in_copy(&facts.root)))?,
                    glue_to,
                })
            }
            _ => None,
        };
        Ok(Self {
            app: inside(&project.root)?,
            target: inside(&project.target_dir)?,
            checkout: context != project.root,
            facade,
            copy,
            includes: paths,
        })
    }

    /// Returns the path `rest` of the project relative to the context.
    fn app(&self, rest: &str) -> String {
        if self.app.is_empty() {
            rest.to_owned()
        } else {
            format!("{}/{rest}", self.app)
        }
    }

    /// Returns the flags of the release build in the builder stage, the compile
    /// whose binary the image ships: [`LEAN_FLAGS`], the `-C panic=abort` the
    /// release build of the server package passes on the host after the flags
    /// of its environment, then `--remap-path-prefix` for the cargo home of the
    /// builder image, the workspace of a path facade, the project root and the
    /// files of the generated copy of a plain crate and its glue, as the build
    /// on the host remaps them and after the project root, since the last
    /// prefix that matches wins, and the link with the LLD of the toolchain,
    /// [`LLD_FLAGS`], which the host build does not take. No flag of the
    /// environment of the command line stands among them, so the reader reads
    /// the crate for the card `docker` with [`LEAN_FLAGS`] after the flags of
    /// the release profile, the flags of this stage that change a pair, and the
    /// build of the card on the host leaves the flags of its environment out as
    /// well.
    fn rustflags(&self, project: &Project) -> String {
        let mut flags = LEAN_FLAGS.join(" ");
        flags.push_str(" --remap-path-prefix=$CARGO_HOME=cargo");
        if let Some(facade) = &self.facade {
            let _ = write!(
                flags,
                " --remap-path-prefix={}={FACADE}",
                in_builder(facade)
            );
        }
        let _ = write!(
            flags,
            " --remap-path-prefix={}={}",
            in_builder(&self.app),
            project.name
        );
        if let Some(copy) = &self.copy {
            let _ = write!(
                flags,
                " --remap-path-prefix={}={name} --remap-path-prefix={COPY_DIR}={name} --remap-path-prefix={}={}",
                in_builder(&copy.dir),
                copy.glue_read,
                copy.glue_to,
                name = project.name,
            );
        }
        let _ = write!(flags, " {LLD_FLAGS}");
        flags
    }
}

/// The generated copy of a plain crate inside the context.
struct PlainCopy {
    /// The directory of the copy relative to the context.
    dir: String,
    /// The path of the glue as rustc reads it in the generated workspace,
    /// `app/src/../../glue.rs` for the crate root `src/lib.rs`.
    glue_read: String,
    /// What the glue stands for in the paths of the build.
    glue_to: String,
}

/// Returns the path `relative` of the context as the builder stage sees it,
/// below `/build`.
fn in_builder(relative: &str) -> String {
    if relative.is_empty() {
        String::from("/build")
    } else {
        format!("/build/{relative}")
    }
}

/// Returns the header of the file `name` of the directory `dir`.
fn header(project: &Project, dir: &Path, name: &str) -> String {
    let path = dir.join(name);
    let shown = path.strip_prefix(&project.root).unwrap_or(&path);
    format!(
        "# {}, written by cargo aosabi from the docker card. Do not edit.\n",
        shown.display()
    )
}

/// Returns the text of the container file.
///
/// The builder writes the binary the image carries with the
/// `rust-objcopy --strip-all` of the toolchain, in `lib/rustlib/<triple>/bin/`
/// of its sysroot, with the `lib/` of the sysroot on `LD_LIBRARY_PATH`, since
/// the program links the `libLLVM` of that directory, which its run path
/// `$ORIGIN/../lib` does not reach.
fn containerfile(
    project: &Project,
    layout: &Layout,
    dir: &Path,
    source: Option<BuilderSource<'_>>,
) -> String {
    let binary_name = format!("{}-server", project.name);
    let binary = format!("/aosabi/{binary_name}");
    let mut text = header(project, dir, CONTAINERFILE);
    // At the switch source the component rust-src takes a layer of its own
    // before the copy of the context, and the build names the source of the
    // standard library, the bootstrap, the crates of -Zbuild-std with no
    // feature and the wrapper of the options per crate.
    let (component, commit, remap, source_env) = match source {
        Some(source) => (
            format!("RUN rustup component add rust-src --toolchain {BUILDER_TOOLCHAIN}\n"),
            String::from(" \\\n    && commit=\"$(rustc -vV | sed -n 's/^commit-hash: //p')\""),
            format!(
                " --remap-path-prefix=$sysroot/{}=/rustc/$commit",
                std_source::SOURCE_DIR.join("/")
            ),
            format!(
                "{}=1 {}={} {}= {}=/build/{}/{GENERATED_DIR}/{BUILDER_WRAPPER} \\\n       ",
                std_source::BOOTSTRAP_ENV,
                std_source::BUILD_STD_ENV,
                source.set.crates,
                std_source::BUILD_STD_FEATURES_ENV,
                std_source::WRAPPER_ENV,
                layout.target,
            ),
        ),
        None => (String::new(), String::new(), String::new(), String::new()),
    };
    let _ = write!(
        text,
        "FROM {BUILDER_IMAGE}@{BUILDER_DIGEST} AS builder\n\
         {component}\
         WORKDIR /build\n\
         COPY . .\n\
         RUN triple=\"$(rustc --print host-tuple)\" \\\n    \
         && sysroot=\"$(rustc --print sysroot)\"{commit} \\\n    \
         && RUSTFLAGS=\"{rustflags}{remap}\" \\\n       \
         {source_env}AOSABI_ARTIFACT_DIR=/build/{target}/{GENERATED_DIR} cargo build --release --locked \\\n        \
         --target \"$triple\" \\\n        \
         --manifest-path {target}/{GENERATED_DIR}/server/Cargo.toml \\\n        \
         --target-dir {target} \\\n    \
         && mkdir -p /aosabi \\\n    \
         && LD_LIBRARY_PATH=\"$sysroot/lib\" \"$sysroot/lib/rustlib/$triple/bin/rust-objcopy\" --strip-all \\\n        \
         \"{target}/$triple/release/{binary_name}\" {binary} \\\n    \
         && AOSABI_BUILD=1 {binary}\n\n\
         FROM {RUNTIME_IMAGE}@{RUNTIME_DIGEST}\n\
         WORKDIR /aosabi\n\
         COPY --from=builder {binary} {binary}\n",
        target = layout.target,
        rustflags = layout.rustflags(project),
    );
    for include in &layout.includes {
        let _ = writeln!(
            text,
            "COPY --from=builder /build/{} /aosabi/{include}",
            layout.app(include)
        );
    }
    let _ = write!(
        text,
        "EXPOSE {PORT}/tcp\nUSER nonroot\nENV PORT={PORT}\nENTRYPOINT [\"/aosabi/{binary_name}\"]\n"
    );
    text
}

/// Returns the text of the ignore file, which names the wrapper of rustc of
/// the builder stage where `source` is true.
fn ignore_file(
    project: &Project,
    layout: &Layout,
    dir: &Path,
    context: &Path,
    source: bool,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    if layout.checkout {
        lines.extend(
            CHECKOUT_PATHS
                .iter()
                .filter(|path| context.join(path).exists())
                .map(|path| (*path).to_owned()),
        );
    }
    for path in ["Cargo.toml", LOCK_FILE, "build.rs", CONFIG_FILE, "src"] {
        lines.push(layout.app(path));
    }
    for path in ["Cargo.toml", LOCK_FILE, "server", "inspect"] {
        lines.push(format!("{}/{GENERATED_DIR}/{path}", layout.target));
    }
    if source {
        lines.push(format!(
            "{}/{GENERATED_DIR}/{BUILDER_WRAPPER}",
            layout.target
        ));
    }
    if layout.copy.is_some() {
        for path in [
            COPY_DIR,
            GLUE_FILE,
            SCHEMA_FILE,
            RESOLVE_TYPES_FILE,
            NAME_FILE,
        ] {
            lines.push(format!("{}/{GENERATED_DIR}/{path}", layout.target));
        }
    }
    lines.extend(layout.includes.iter().map(|include| layout.app(include)));
    let mut text = header(project, dir, IGNORE_FILE);
    text.push_str("*\n");
    for line in lines {
        if !text.contains(&format!("\n!{line}\n")) {
            text.push('!');
            text.push_str(&line);
            text.push('\n');
        }
    }
    text
}

/// Returns whether `engine` is podman, which takes `--ignorefile`.
fn is_podman(engine: &Path) -> bool {
    engine
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("podman"))
}

/// Returns whether `file` is a program the process may run.
fn is_program(file: &Path) -> bool {
    let Ok(metadata) = fs::metadata(file) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}

/// Returns the standard error and the standard output of `output`.
fn both_streams(output: &std::process::Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text
}

/// Returns the failure of a write of `path`.
fn io_failure(path: &Path, error: &std::io::Error) -> CliError {
    CliError::new(format!("{}: {error}", path.display()))
}
