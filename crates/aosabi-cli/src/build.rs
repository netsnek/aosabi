// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the command build, its build log and its build record.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/cli/index.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The command `build`.
//!
//! The command runs these steps in order:
//!
//! 1. `.env`, the arguments and [`project::load`], with its refusals.
//! 2. [`cards::resolve`], with its refusals.
//! 3. For a card that compiles the server package, [`prepare`]: the
//!    generated workspace, the derived lock and its check, and the bytes of
//!    `target/aosabi/schema.graphql` as they stand before the build.
//! 4. The cargo build, of the server package with `--locked`, the artifact
//!    directory in `AOSABI_ARTIFACT_DIR` and in the release profile the
//!    flags of the lean link, of the library of the service for `worker`
//!    with the facade's `platform-coding` alone, or of
//!    the kernel crates of a path facade for `esp32c3-devkitm1`. A failure of
//!    cargo prints what cargo printed, the message of an invalid schema
//!    among it, and the command exits 1.
//! 5. The build step of a card that serves, [`build_step::run_build_step`].
//! 6. The canonical model, `target/aosabi/manifest.json`, the bytes
//!    `aosabi inspect --json` prints, from the `ir.json` the build script
//!    wrote with no authz and no queue.
//! 7. The client, `aosabi_query::build_client` with `schema_changed` true
//!    when the schema of step 3 is absent or differs from the one the build
//!    wrote, the comparison `emitServerGlue` makes.
//! 8. For a server card in the release profile, and in the dev profile with
//!    `--include`, `target/aosabi/dist/` with the binary it ships, stripped in
//!    the release profile, and a copy of every included path at its project
//!    relative path, a link refused.
//! 9. For `docker`, the container files and the image of [`container`].
//! 10. `target/aosabi/build.json`, the build record, and the build log of
//!     `docs/cli-and-targets.md` section 5.0 on standard error.
//!
//! A plain crate, whose roots no fragment of `graphql!` holds, takes the source
//! path for the schema: the reader of `aosabi-build` reads the crate from its
//! root as rustc compiles it for the card, stripped over the active set
//! `rustc --print cfg` prints for the target of the card with the flags of the
//! compile that builds the binary the card ships, those cargo passes for the
//! profile of the build and then the flags of the environment on the host and
//! none of them in the builder stage of the image of `docker`, `print_flags`,
//! its third type source answers the inference about the types its roots reach,
//! and the five artifacts land in the artifact directory with no compile of the
//! crate, beside the table of the type resolvers and the package name, a refusal
//! of every construct the source cannot state printed at the file and the line
//! of the author under `AO-C0105`. The builder then writes the glue of every
//! root, type and inherent block the roots reach into the generated copy of the
//! crate, and every card that compiles the application builds that copy in place
//! of the package: the server package of step 3 names it and takes the artifacts
//! the source path wrote, the Worker builds the library of the copy, and the
//! device card builds the crates of its kernel as for any crate. The build
//! remaps the directory of the copy to the package, the package name in the
//! release profile and the project root in the dev profile, so a diagnostic, a
//! panic location and the boot report name the author's files. A crate whose
//! roots a fragment holds keeps the steps above.
//!
//! A Rust configuration is code compiled into the binary, so the build step
//! runs after the compile and a failing configuration leaves the schema of
//! that build on disk.
//!
//! Cargo sets `CARGO_CFG_PANIC` from those flags and not from the profile, so
//! the lean link the build script of the server package calls acts on that build
//! for `aarch64-unknown-linux-gnu` and `x86_64-unknown-linux-gnu`, the targets
//! it takes, and `--target` keeps the flags away from the build scripts and the
//! proc macros. The last of several matching prefixes wins, so the project root,
//! which may lie inside the workspace of its facade, stands after the other two.
//! The source of the standard library in the sysroot, `lib/rustlib/src/rust`,
//! which the component `rust-src` puts there and which may lie inside the
//! workspace of a facade as well, maps to `/rustc/<commit>`, the path the
//! prebuilt standard library names its source by, last, after the project root
//! and the files of the generated copy of a plain crate, so a build on a
//! toolchain with the component writes the panic locations of the inlined code
//! of the standard library with the path a toolchain without it writes, and a
//! build on a toolchain without it writes the bytes it wrote before. A rustc
//! that names no commit takes no such remap. The flags of the environment of the
//! command line stand before these in a build of a card on the host and stay out
//! of the build of `docker`, whose builder stage passes none, so its build on
//! the host compiles the parts its image compiles. The dev profile unwinds,
//! keeps the unwind tables and adds no flag, since a remapped path reaches the
//! diagnostics a developer reads as well.
//!
//! A release build of a card whose set the table of the built in cards names
//! reads the switch `AOSABI_STD` of [`crate::std_source`], and at `source`
//! builds the standard library from source with that set into a target
//! directory of its own and puts the binary where a build at `prebuilt` puts
//! it. The dev build reads no switch.
//!
//! A server card in the release profile ships its binary as the copy in
//! `target/aosabi/dist/` that `rust-objcopy --strip-all` writes, on every such
//! build, while `target/<triple>/release/` keeps the binary cargo wrote with its
//! symbols, which the symbol reads of the notebooks take. The program is the one
//! of the toolchain, in `lib/rustlib/<host>/bin/` of the sysroot that
//! `rustc --print sysroot` names, and it runs with the `lib/` of that sysroot
//! first on `LD_LIBRARY_PATH`, since it links the `libLLVM` of that directory,
//! which its run path `$ORIGIN/../lib` does not reach. The dev profile copies
//! the binary as cargo wrote it, and only under `--include`.
//!
//! Every file the build writes goes through `update_file_if_changed`, so a
//! second build of the same sources writes no file. The build script of the
//! server package gives every artifact it wrote the modification time of its own
//! executable, so cargo runs it no more and the second build compiles nothing. A
//! step whose child printed its own failure, cargo or the binary under
//! `AOSABI_BUILD=1`, ends the command with 1 and no further text, which
//! [`shown`] marks.

use std::{
    ffi::{
        OsStr,
        OsString, //
    },
    fmt::Write,
    fs,
    path::{
        Path,
        PathBuf, //
    },
    process::{
        Command,
        ExitCode,
        Stdio, //
    },
};

use aosabi_build::{
    ARTIFACT_DIR_ENV,
    AUTHORING_FILE,
    DESCRIPTORS_FILE,
    IR_FILE,
    NAME_FILE,
    RESOLVE_TYPES_FILE,
    RESOLVERS_FILE,
    SCHEMA_FILE,
    source::{
        Cfg,
        Glue, //
    },
    update_file_if_changed, //
};
use aosabi_ir::app_model::{
    app_model,
    app_model_to_js, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    to_string_with_gap, //
};

use crate::{
    args::{
        BuildArgs,
        Global,
        LockMode, //
    },
    build_step,
    cards::{
        self,
        Builds,
        Card,
        Class,
        Profile, //
    },
    cargo::{
        self,
        Artifact,
        BuildRun,
        Cargo,
        CargoRun,
        Compile,
        Output, //
    },
    codes::{
        self,
        target_missing, //
    },
    container,
    dotenv::DotEnv,
    env,
    generate::{
        self,
        Flavour,
        GENERATED_DIR,
        Generated,
        ServerPackage, //
    },
    lock::{
        self,
        LockReport, //
    },
    project::{
        self,
        Project, //
    },
    report::{
        self,
        CliError, //
    },
    std_source::{
        self,
        SourceSet,
        Std, //
    },
};

/// The canonical model the build writes, the bytes `aosabi inspect --json`
/// prints.
pub const MANIFEST_FILE: &str = "manifest.json";

/// The build record.
pub const BUILD_FILE: &str = "build.json";

/// The directory of the binary a server card ships and of the runtime data
/// next to it.
pub const DIST_DIR: &str = "dist";

/// The program of the toolchain that strips the binary a release build ships,
/// in `lib/rustlib/<host>/bin/` of its sysroot.
const OBJCOPY: &str = "rust-objcopy";

/// The variable the loader of Linux reads the directories of shared libraries
/// from before the run path of a program (, the manual page ld.so(8)).
const LIBRARY_PATH_ENV: &str = "LD_LIBRARY_PATH";

/// The directory of the artifacts of `verify` inside its generated
/// workspace.
pub const VERIFY_ARTIFACTS: &str = "artifacts";

/// The gap of the JSON files the build writes, the one of
/// `JSON.stringify(value, null, 2)`.
const GAP: &str = "  ";

/// The client files, in the order the build record lists them.
const CLIENT_FILES: [&str; 3] = ["client/types.ts", "client/index.ts", "client/index.js"];

/// What the log says of a `worker` build.
const WORKER_NOTE: &str = "the Workers entry lands at M12";

/// What the log says of an `esp32c3-devkitm1` build.
const MCU_NOTE: &str =
    "the GraphQL stack enters the image at round 3 of M12 and the card build at DC9";

/// What the log says of an `esp32c3-devkitm1` build over a facade that is no
/// path facade, which compiles nothing.
const MCU_NO_PATH_NOTE: &str = "the kernel crates build from the workspace of a path facade alone, and the card build lands at DC9";

/// What [`run`] did.
#[derive(Clone, Debug, Default)]
pub struct BuildOutcome {
    /// The served binary, `None` for a card that serves nothing.
    pub binary: Option<PathBuf>,
    /// Whether the schema the build wrote differs from the one before it.
    pub schema_changed: bool,
    /// Whether the build ran in the release profile.
    pub release: bool,
    /// The count of the packages whose build script cargo ran, every package
    /// but the service and the packages the build generates.
    pub build_scripts: usize,
    /// The generated workspace, for the server package.
    pub generated: Option<Generated>,
    /// What the check of the derived lock found, for the server package.
    pub lock: Option<LockReport>,
    /// The library the build compiled for a card that serves nothing.
    pub library: Option<PathBuf>,
    /// The files the build holds under the artifact directory.
    pub artifacts: Vec<PathBuf>,
    /// What the card does not build yet, as the log says it.
    pub note: Option<&'static str>,
    /// The crates of the framework the wrapper of rustc of a release build
    /// at the switch `source` names, `None` for a build that took no set.
    pub source_crates: Option<Vec<String>>,
}

/// What [`prepare`] made ready for the cargo build of a server package.
#[derive(Clone, Debug)]
pub struct Prepared {
    /// The generated workspace.
    pub generated: Generated,
    /// What the check of its derived lock found.
    pub lock: LockReport,
    /// The directory the build script writes the artifacts into.
    pub artifact_dir: PathBuf,
    /// The bytes of the schema before the build, `None` when there was none.
    pub previous_schema: Option<Vec<u8>>,
}

/// What a build of a plain crate reads from its source: the glue of its
/// generated copy and the active set of the target of the card, over which
/// the reader strips the crate.
struct Plain {
    /// The generated module of the copy.
    glue: Glue,
    /// The active set of the build.
    cfg: Cfg,
}

/// One build of a card, the facts every step reads.
struct Job<'a> {
    project: &'a Project,
    card: &'a Card,
    release: bool,
    std: Std,
    lock: LockMode,
    flavour: Flavour,
    output: Output,
    cargo: &'a Cargo,
}

/// Runs `build` with `args` under `global` and the pairs `env_file` of
/// `.env`, and answers the exit code.
#[must_use]
pub fn command(args: &BuildArgs, global: &Global, env_file: &DotEnv) -> ExitCode {
    let verbose = global.verbose || env::debug(env_file);
    match build(args, global, env_file) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if !is_shown(&error) {
                report::report_failure(&error, verbose);
            }
            ExitCode::FAILURE
        }
    }
}

/// Runs steps 3 to 7 of the build of `project` for `card`, in `flavour`,
/// with the standard error of every child as `output` says.
///
/// A release build reads the switch `AOSABI_STD`, and a dev build reads
/// none.
///
/// # Errors
///
/// Returns the refusals of the derived lock, of a missing target, of a value
/// of the switch that names neither form and of a build at `source` on a
/// toolchain with no `rust-src`, a failed build, which is [`shown`] under
/// [`Output::Inherit`], a failed build step, and a failure of the model or
/// of the client.
pub fn run(
    project: &Project,
    card: &Card,
    args: &BuildArgs,
    flavour: Flavour,
    output: Output,
    cargo: &Cargo,
) -> Result<BuildOutcome, CliError> {
    let release = args.release || card.profile == Profile::Release;
    let std = if release {
        Std::read(&DotEnv {
            pairs: cargo.env.clone(),
        })?
    } else {
        Std::Prebuilt
    };
    let job = Job {
        project,
        card,
        release,
        std,
        lock: args.lock,
        flavour,
        output,
        cargo,
    };
    let plain = if project.plain {
        Some(plain_of(&job)?)
    } else {
        None
    };
    match card.builds {
        Builds::ServerPackage(package) => run_server(&job, &package, plain.as_ref()),
        Builds::AppForTarget { features } => match &plain {
            Some(plain) => run_source_app(&job, features, plain),
            None => run_app(&job, features),
        },
        Builds::KernelForTarget { packages } => {
            let Some(plain) = &plain else {
                return run_kernel(&job, packages);
            };
            // The card compiles the crates of its kernel and no crate of the
            // application, so a plain crate writes its schema alone.
            let artifact_dir = artifact_dir(project, job.flavour);
            let previous_schema = fs::read(artifact_dir.join(SCHEMA_FILE)).ok();
            emit_source(project, &plain.cfg, &artifact_dir)?;
            let mut outcome = run_kernel(&job, packages)?;
            outcome.schema_changed = finish_source(&artifact_dir, previous_schema.as_ref())?;
            outcome.artifacts = present_artifacts(&artifact_dir);
            Ok(outcome)
        }
    }
}

/// Returns what the build `job` of a plain crate reads from its source, the
/// active set of the target of its card in its profile and the glue of the
/// crate as rustc compiles it over that set, the generated module of its
/// generated copy.
///
/// # Errors
///
/// Returns the failure of rustc that prints the active set, and `AO-C0105`
/// with the error of the reader, or with every refusal of the reader, of the
/// lowering and of the glue at the file and the line of the author.
fn plain_of(job: &Job<'_>) -> Result<Plain, CliError> {
    let project = job.project;
    let (profile, flags) = print_flags(job);
    let cfg = job.cargo.cfg(
        &project.root,
        job.card.triple,
        compile_of(job.card),
        &profile,
        &flags,
    )?;
    let krate = aosabi_build::source::read(&project.root, &cfg)
        .map_err(|error| codes::source_unread(&project.name, &error.to_string()))?;
    let glue = aosabi_build::source::glue(&krate)
        .map_err(|refusals| source_failure(project, aosabi_build::BuildError::Refused(refusals)))?;
    Ok(Plain { glue, cfg })
}

/// Returns the compile that builds the binary the card `card` ships: the
/// builder stage of the image for a card of the class container, and the
/// host for every other card.
fn compile_of(card: &Card) -> Compile {
    if card.class == Class::Container {
        Compile::BuilderStage
    } else {
        Compile::Host
    }
}

/// Returns the flags cargo passes rustc for the profile of the build `job`
/// and the flags the build passes after those of the environment, with which
/// rustc prints the active set and the target information of the build, the
/// flags of the environment before them on the host and none of them in the
/// builder stage of the card `docker`, [`compile_of`].
///
/// The server package builds in the profiles of the generated workspace, whose
/// release profile aborts on a panic, and passes [`LEAN_FLAGS`] in the release
/// profile, on the host and in the builder stage of the image of `docker`, while
/// the Worker builds the generated copy alone, whose workspace states no
/// profile, so it keeps the profiles of cargo, as the device card does, which
/// compiles no crate of the application. The remaps of the paths of a build and
/// the link arguments of the builder stage change no pair, so they stay out.
fn print_flags(job: &Job<'_>) -> (Vec<String>, Vec<String>) {
    let lean = job.release && matches!(job.card.builds, Builds::ServerPackage(_));
    let flags = if lean {
        LEAN_FLAGS.iter().map(|flag| String::from(*flag)).collect()
    } else {
        Vec::new()
    };
    (Cfg::profile_flags(job.release, lean), flags)
}

/// Writes the five artifacts of the schema of the plain crate of `project` into
/// `artifact_dir` from its source, stripped over the active set `cfg` of the
/// build, with the table of the type resolvers and the package name beside them,
/// which the build script of its server package takes, and nothing of the crate
/// is compiled for the schema.
///
/// Every warning of the inference goes to standard error as
/// `warning: <line>`, as a program of the post compile route prints it.
///
/// # Errors
///
/// Returns `AO-C0105` with every refusal of the source path at the file and
/// the line of the author, the message of a schema that is no valid schema,
/// and the failure of a read or a write.
fn emit_source(project: &Project, cfg: &Cfg, artifact_dir: &Path) -> Result<(), CliError> {
    let dirs = aosabi_build::ArtifactDirs {
        artifact_dir: artifact_dir.to_path_buf(),
        out_dir: None,
    };
    let emitted = aosabi_build::emit_source(&project.root, cfg, &dirs)
        .map_err(|error| source_failure(project, error))?;
    for line in emitted.warnings.iter().flat_map(|warning| warning.lines()) {
        eprintln!("warning: {line}");
    }
    for (name, text) in [
        (RESOLVE_TYPES_FILE, emitted.resolve_types.as_str()),
        (NAME_FILE, emitted.app_crate.as_str()),
    ] {
        let path = artifact_dir.join(name);
        update_file_if_changed(&path, text.as_bytes())
            .map_err(|error| io_failure(&path, &error))?;
    }
    Ok(())
}

/// Writes the canonical model and the client of a plain crate from the
/// artifacts in `artifact_dir`, the client with `schema_changed` true when
/// `previous_schema` is absent or differs, and answers whether it differs.
fn finish_source(artifact_dir: &Path, previous_schema: Option<&Vec<u8>>) -> Result<bool, CliError> {
    write_manifest(artifact_dir)?;
    let schema = fs::read(artifact_dir.join(SCHEMA_FILE)).ok();
    let schema_changed = schema.is_none() || schema.as_ref() != previous_schema;
    aosabi_query::build_client(artifact_dir, schema_changed)
        .map_err(|error| CliError::new(error.to_string()))?;
    Ok(schema_changed)
}

/// Runs step 4 of a plain crate for a card that compiles the library of the
/// service for its target with the features `features` of the facade: the
/// artifacts of the source path, the generated copy of the crate with the glue
/// of `plain` and its lock, and the library of the copy for the target, remapped
/// to the project root.
fn run_source_app(
    job: &Job<'_>,
    features: &[&str],
    plain: &Plain,
) -> Result<BuildOutcome, CliError> {
    let project = job.project;
    let triple = target_of(job)?;
    let artifact_dir = artifact_dir(project, job.flavour);
    let previous_schema = fs::read(artifact_dir.join(SCHEMA_FILE)).ok();
    emit_source(project, &plain.cfg, &artifact_dir)?;
    let copy = generate::write_source_copy(project, job.flavour, &plain.glue)?;
    let manifest = copy.join("Cargo.toml");
    derive_copy_lock(project, &copy, job.lock, job.cargo)?;
    let rustflags = job.cargo.encoded_rustflags(
        compile_of(job.card),
        &copy_remaps(project, &copy, false, &project.root)?,
    );
    let compiled = job.cargo.build(&BuildRun {
        cwd: &project.root,
        manifest: Some(&manifest),
        target_dir: &project.target_dir,
        package: None,
        triple: Some(triple),
        no_default_features: false,
        features,
        lib: true,
        release: job.release,
        lock: generated_lock_mode(job.lock),
        output: job.output,
        env: &[(cargo::ENCODED_RUSTFLAGS, rustflags.as_os_str())],
    })?;
    check_compiled(&compiled, &project.name, job.output)?;
    let schema_changed = finish_source(&artifact_dir, previous_schema.as_ref())?;
    Ok(BuildOutcome {
        schema_changed,
        release: job.release,
        build_scripts: dependency_scripts(&compiled, &[project.name.as_str()]),
        library: library_of(&compiled, &project.lib_name),
        artifacts: present_artifacts(&artifact_dir),
        note: Some(WORKER_NOTE),
        ..BuildOutcome::default()
    })
}

/// Derives the lock of the generated copy at `copy` of the plain crate of
/// `project` and checks it, as [`derive_lock`] derives the lock of the
/// generated workspace, since the copy is a workspace of its own.
fn derive_copy_lock(
    project: &Project,
    copy: &Path,
    lock: LockMode,
    cargo: &Cargo,
) -> Result<LockReport, CliError> {
    let manifest = copy.join("Cargo.toml");
    let copy_lock = copy.join(project::LOCK_FILE);
    if copy_lock.is_file()
        && cargo
            .metadata(&project.root, Some(&manifest), generated_lock_mode(lock))
            .is_ok()
        && let Ok(report) = lock::check(&project.lock, &copy_lock)
    {
        return Ok(report);
    }
    lock::seed(&project.lock, &copy_lock)?;
    let resolve = if cargo::is_offline(lock) {
        LockMode::Offline
    } else {
        LockMode::Normal
    };
    cargo.metadata(&project.root, Some(&manifest), resolve)?;
    lock::check(&project.lock, &copy_lock)
}

/// Returns the flags that remap the files of the generated copy `copy` of
/// the plain crate of `project` to `to`, the directory that stands for the
/// package, and its glue to the file of the glue under `to` where the glue
/// lies inside the project root, or to its own path, in a build of the
/// generated workspace when `in_workspace` holds and in a build of the copy
/// alone otherwise.
///
/// Cargo hands rustc the files of a path package inside the directory of the
/// workspace it builds by paths relative to that directory, and the working
/// directory by its absolute path, so the files of the copy reach rustc as
/// `app/src/lib.rs` in a build of the generated workspace and as `src/lib.rs` in
/// a build of the copy alone, and the glue by the path the module of the crate
/// root names it, `app/src/../../glue.rs`. The last prefix that matches wins, so
/// the glue stands last.
fn copy_remaps(
    project: &Project,
    copy: &Path,
    in_workspace: bool,
    to: &Path,
) -> Result<Vec<String>, CliError> {
    let root = project
        .copy
        .as_ref()
        .map(|facts| facts.root.as_path())
        .ok_or_else(|| codes::not_a_service(&project.name, false))?;
    let mut glue_read = generate::glue_in_copy(root);
    if in_workspace {
        glue_read = Path::new(generate::COPY_DIR).join(glue_read);
    }
    let glue = copy.parent().unwrap_or(copy).join(generate::GLUE_FILE);
    let glue_to = match glue.strip_prefix(&project.root) {
        Ok(rest) => to.join(rest),
        Err(_) => glue,
    };
    let to = generate::utf8(to)?;
    let mut flags = vec![format!(
        "--remap-path-prefix={}={to}",
        generate::utf8(copy)?
    )];
    if in_workspace {
        flags.push(format!("--remap-path-prefix={}={to}", generate::COPY_DIR));
    }
    flags.push(format!(
        "--remap-path-prefix={}={}",
        generate::utf8(&glue_read)?,
        generate::utf8(&glue_to)?
    ));
    Ok(flags)
}

/// Returns the failure of the source path of `project` for `error`, the
/// refusal `AO-C0105` with every construct the builder cannot read, each at
/// its file and line relative to the project root, or with the error of the
/// reader, which reads no crate.
#[must_use]
pub fn source_failure(project: &Project, error: aosabi_build::BuildError) -> CliError {
    match error {
        aosabi_build::BuildError::Refused(refusals) => {
            let lines: Vec<String> = refusals.iter().map(ToString::to_string).collect();
            codes::source_refused(&project.name, &lines)
        }
        aosabi_build::BuildError::Source(error) => {
            codes::source_unread(&project.name, &error.to_string())
        }
        error => CliError::new(error.to_string()),
    }
}

/// Runs step 3 of the build of `project` for the server package `package`
/// in `flavour`: writes the generated workspace, derives its lock and checks
/// it, and reads the schema as it stands.
///
/// # Errors
///
/// Returns `AO-C0301` for a package the lock of the project does not pin,
/// and the failure of a write or of a cargo call.
pub fn prepare(
    project: &Project,
    package: &ServerPackage,
    flavour: Flavour,
    lock: LockMode,
    cargo: &Cargo,
    glue: Option<&Glue>,
) -> Result<Prepared, CliError> {
    let generated = generate::generate(project, package, flavour, glue)?;
    let report = derive_lock(project, &generated, lock, cargo)?;
    let artifact_dir = artifact_dir(project, flavour);
    let previous_schema = fs::read(artifact_dir.join(SCHEMA_FILE)).ok();
    Ok(Prepared {
        generated,
        lock: report,
        artifact_dir,
        previous_schema,
    })
}

/// Returns the directory the build script of `project` writes the artifacts
/// into in `flavour`.
#[must_use]
pub fn artifact_dir(project: &Project, flavour: Flavour) -> PathBuf {
    match flavour {
        Flavour::Build => project.target_dir.join(GENERATED_DIR),
        Flavour::Verify => generate::root(project, flavour).join(VERIFY_ARTIFACTS),
    }
}

/// Returns the failure of a step whose child printed its own failure, for
/// which [`command`] prints no line.
#[must_use]
pub fn shown() -> CliError {
    CliError::new(String::new())
}

/// Returns whether `error` is the failure [`shown`] makes.
#[must_use]
pub fn is_shown(error: &CliError) -> bool {
    error.code.is_none()
        && error.message.is_empty()
        && error.detail.is_none()
        && error.hint.is_none()
}

/// Runs the ten steps of `build`.
fn build(args: &BuildArgs, global: &Global, env_file: &DotEnv) -> Result<(), CliError> {
    let cwd = std::env::current_dir()
        .map_err(|error| CliError::new(format!("the working directory is unreadable: {error}")))?;
    let cargo = Cargo::from_env(env_file);
    let project = project::load(&cwd, global.manifest_path.as_deref(), args.lock, &cargo)?;
    let card = cards::resolve(args.device.as_deref(), &project)?;
    let includes = includes(&project, &args.include)?;
    let mut outcome = run(
        &project,
        card,
        args,
        Flavour::Build,
        Output::Inherit,
        &cargo,
    )?;
    let artifact_dir = artifact_dir(&project, Flavour::Build);
    let triple = card
        .triple
        .map(str::to_owned)
        .or_else(|| cargo.host_triple(&project.root));
    if card.class == Class::Server
        && (outcome.release || !includes.is_empty())
        && let Some(binary) = &outcome.binary
    {
        let strip = if outcome.release {
            Some(Strip::of(&project, &cargo, triple.as_deref())?)
        } else {
            None
        };
        let copied = copy_dist(&project, &artifact_dir, binary, strip.as_ref(), &includes)?;
        outcome.artifacts.extend(copied);
    }
    let mut image = None;
    // A plain crate builds no binary yet, and the image carries one.
    if card.class == Class::Container && outcome.binary.is_some() {
        let source = card
            .source
            .zip(outcome.source_crates.as_deref())
            .map(|(set, crates)| container::BuilderSource { set, crates });
        let dir = container::write_container_files(&project, &includes, source)?;
        outcome.artifacts.extend([
            dir.join(container::CONTAINERFILE),
            dir.join(container::IGNORE_FILE),
        ]);
        image = Some(match container::find_engine(env_file) {
            Some(engine) => Ok(container::build_image(&engine, &project)?),
            None => Err(container::command_line(Path::new("podman"), &project)),
        });
    }
    let record = record(
        &project,
        card,
        &outcome,
        triple.as_deref(),
        &includes,
        image.as_ref(),
    );
    let record_path = artifact_dir.join(BUILD_FILE);
    write_json(&record_path, &record)?;
    eprint!("{}", log(&project, card, &outcome, image.as_ref()));
    Ok(())
}

/// Runs steps 3 to 7 for a card that compiles the server package, of a
/// plain crate with what `plain` reads from its source, whose artifacts the
/// source path writes after the generated workspace and before the cargo
/// build.
fn run_server(
    job: &Job<'_>,
    package: &ServerPackage,
    plain: Option<&Plain>,
) -> Result<BuildOutcome, CliError> {
    let project = job.project;
    let glue = plain.map(|plain| &plain.glue);
    let prepared = prepare(project, package, job.flavour, job.lock, job.cargo, glue)?;
    let copy = prepared.generated.copy.as_deref();
    if let Some(plain) = plain {
        emit_source(project, &plain.cfg, &prepared.artifact_dir)?;
    }
    let lean = if job.release {
        Some(lean_build(project, job.cargo, copy, compile_of(job.card))?)
    } else {
        None
    };
    // The dev profile remaps the copy of a plain crate alone, to the
    // project root, so a diagnostic names the author's file.
    let dev_flags = match (copy, &lean) {
        (Some(copy), None) => Some(job.cargo.encoded_rustflags(
            compile_of(job.card),
            &copy_remaps(project, copy, true, &project.root)?,
        )),
        _ => None,
    };
    let source = match (&lean, job.std, job.card.source) {
        (Some(_), Std::Source, Some(set)) => Some(source_build(job, &prepared.generated, set)?),
        _ => None,
    };
    let mut env_pairs: Vec<(&str, &OsStr)> =
        vec![(ARTIFACT_DIR_ENV, prepared.artifact_dir.as_os_str())];
    if let Some(lean) = &lean {
        env_pairs.push((cargo::ENCODED_RUSTFLAGS, lean.rustflags.as_os_str()));
    }
    if let Some(flags) = &dev_flags {
        env_pairs.push((cargo::ENCODED_RUSTFLAGS, flags.as_os_str()));
    }
    if let Some(source) = &source {
        env_pairs.extend(
            source
                .env
                .iter()
                .map(|(key, value)| (*key, value.as_os_str())),
        );
    }
    let target_dir = source
        .as_ref()
        .map_or(&project.target_dir, |source| &source.target_dir);
    let compiled = job.cargo.build(&BuildRun {
        cwd: &project.root,
        manifest: Some(&prepared.generated.server_manifest),
        target_dir,
        package: None,
        triple: lean.as_ref().map(|lean| lean.triple.as_str()),
        no_default_features: false,
        features: &[],
        lib: false,
        release: job.release,
        lock: generated_lock_mode(job.lock),
        output: job.output,
        env: &env_pairs,
    })?;
    check_compiled(&compiled, &prepared.generated.server_bin, job.output)?;
    let place = |dir: &Path| {
        let mut dir = dir.to_path_buf();
        if let Some(lean) = &lean {
            dir.push(&lean.triple);
        }
        dir.join(if job.release { "release" } else { "debug" })
            .join(&prepared.generated.server_bin)
    };
    let mut binary = executable_of(&compiled, &prepared.generated.server_bin)
        .unwrap_or_else(|| place(target_dir));
    if source.is_some() {
        let placed = place(&project.target_dir);
        std_source::place(&binary, &placed)?;
        binary = placed;
    }
    if job.card.serves {
        let env_file = DotEnv {
            pairs: job.cargo.env.clone(),
        };
        build_step::run_build_step(&binary, &project.root, &env_file, job.output)?;
    }
    write_manifest(&prepared.artifact_dir)?;
    let schema = fs::read(prepared.artifact_dir.join(SCHEMA_FILE)).ok();
    let schema_changed = schema.is_none() || schema != prepared.previous_schema;
    aosabi_query::build_client(&prepared.artifact_dir, schema_changed)
        .map_err(|error| CliError::new(error.to_string()))?;
    let own = [
        project.name.as_str(),
        prepared.generated.server_bin.as_str(),
        prepared.generated.inspect_bin.as_str(),
    ];
    Ok(BuildOutcome {
        binary: Some(binary),
        schema_changed,
        release: job.release,
        build_scripts: dependency_scripts(&compiled, &own),
        artifacts: present_artifacts(&prepared.artifact_dir),
        generated: Some(prepared.generated),
        lock: Some(prepared.lock),
        library: None,
        note: None,
        source_crates: source.map(|source| source.crates),
    })
}

/// Runs step 4 for a card that compiles the library of the service for its
/// target with the features `features` of the facade.
fn run_app(job: &Job<'_>, features: &[&str]) -> Result<BuildOutcome, CliError> {
    let project = job.project;
    let triple = target_of(job)?;
    let compiled = job.cargo.build(&BuildRun {
        cwd: &project.root,
        manifest: Some(&project.manifest),
        target_dir: &project.target_dir,
        package: None,
        triple: Some(triple),
        no_default_features: false,
        features,
        lib: true,
        release: job.release,
        lock: job.lock,
        output: job.output,
        env: &[],
    })?;
    check_compiled(&compiled, &project.name, job.output)?;
    Ok(BuildOutcome {
        release: job.release,
        build_scripts: dependency_scripts(&compiled, &[project.name.as_str()]),
        library: library_of(&compiled, &project.lib_name),
        note: Some(WORKER_NOTE),
        ..BuildOutcome::default()
    })
}

/// Runs step 4 for a card that compiles the packages `packages` of the
/// workspace of a path facade for its target, and nothing for a facade that
/// is no path facade.
fn run_kernel(job: &Job<'_>, packages: &[(&str, bool)]) -> Result<BuildOutcome, CliError> {
    let project = job.project;
    let Some(workspace) = &project.facade_workspace else {
        eprintln!(
            "note: {} compiles nothing, {MCU_NO_PATH_NOTE}",
            job.card.name
        );
        return Ok(BuildOutcome {
            release: job.release,
            note: Some(MCU_NO_PATH_NOTE),
            ..BuildOutcome::default()
        });
    };
    let triple = target_of(job)?;
    let manifest = workspace.join("Cargo.toml");
    let mut outcome = BuildOutcome {
        release: job.release,
        note: Some(MCU_NOTE),
        ..BuildOutcome::default()
    };
    for (package, no_default_features) in packages {
        let compiled = job.cargo.build(&BuildRun {
            cwd: workspace,
            manifest: Some(&manifest),
            target_dir: &project.target_dir,
            package: Some(package),
            triple: Some(triple),
            no_default_features: *no_default_features,
            features: &[],
            lib: false,
            release: job.release,
            lock: job.lock,
            output: job.output,
            env: &[],
        })?;
        check_compiled(&compiled, package, job.output)?;
        outcome.build_scripts += dependency_scripts(&compiled, &[package]);
        outcome.library = library_of(&compiled, &package.replace('-', "_"));
    }
    Ok(outcome)
}

/// What a release build at the switch `source` adds to its cargo call.
struct SourceBuild {
    /// The target directory of the value, inside the one of the project.
    target_dir: PathBuf,
    /// The pairs the cargo call takes.
    env: Vec<(&'static str, OsString)>,
    /// The crates of the framework the wrapper names, none where the set
    /// holds no options per crate.
    crates: Vec<String>,
}

/// Returns what the release build of the server package of `job` at the
/// switch `source` adds to its cargo call for the set `set`, with the
/// generated workspace `generated`.
///
/// # Errors
///
/// Returns the refusal of a toolchain whose sysroot holds no `rust-src`, and
/// the failure of a cargo call or of the write of the wrapper.
fn source_build(
    job: &Job<'_>,
    generated: &Generated,
    set: SourceSet,
) -> Result<SourceBuild, CliError> {
    let project = job.project;
    let sysroot = sysroot(project, job.cargo)?;
    if !std_source::source_dir(&sysroot).is_dir() {
        let release = job
            .cargo
            .rustc_facts(&project.root)
            .map(|facts| facts.release)
            .unwrap_or_default();
        return Err(std_source::refusal(&release));
    }
    let env_file = DotEnv {
        pairs: job.cargo.env.clone(),
    };
    let crates = if set.per_crate {
        framework_crates(job, generated)?
    } else {
        Vec::new()
    };
    let wrapper = if set.per_crate {
        let chain = env::var(&env_file, std_source::WRAPPER_ENV).filter(|chain| !chain.is_empty());
        let path = generated.root.join(std_source::WRAPPER_FILE);
        std_source::write_program(&path, &std_source::wrapper(&crates, chain.as_deref()))?;
        Some(path)
    } else {
        None
    };
    Ok(SourceBuild {
        target_dir: project.target_dir.join(std_source::SOURCE_TARGET_DIR),
        env: std_source::cargo_env(&env_file, set, wrapper.as_deref()),
        crates,
    })
}

/// Returns the crate names of the framework the build of `job` links: every
/// library of a package that the generated workspace `generated` resolves
/// from a path inside the workspace of a path facade and outside the project
/// and the generated workspace, the members of the facade and its forks, and
/// for another facade every library of a package named `aosabi` or
/// `aosabi-*` or listed in [`project::FRAMEWORK_PACKAGES`], `graphql-over-http`
/// among them, that the generated workspace resolves, sorted.
///
/// The metadata of the generated workspace names the packages the build
/// compiles, so a facade whose workspace lists a member that is not on the
/// disk builds at `source` as it builds at `prebuilt`.
///
/// # Errors
///
/// Returns the failure of `cargo metadata`.
fn framework_crates(job: &Job<'_>, generated: &Generated) -> Result<Vec<String>, CliError> {
    let project = job.project;
    let metadata = job.cargo.metadata(
        &project.root,
        Some(&generated.root.join("Cargo.toml")),
        generated_lock_mode(job.lock),
    )?;
    let framework = |package: &JsValue| match &project.facade_workspace {
        Some(workspace) => {
            matches!(cargo::field(package, "source"), None | Some(JsValue::Null))
                && cargo::text(package, "manifest_path").is_some_and(|manifest| {
                    let manifest = Path::new(manifest);
                    manifest.starts_with(workspace)
                        && !manifest.starts_with(&project.root)
                        && !manifest.starts_with(&generated.root)
                })
        }
        None => cargo::text(package, "name").is_some_and(|name| {
            name == project::FACADE
                || name.starts_with("aosabi-")
                || project::FRAMEWORK_PACKAGES.contains(&name)
        }),
    };
    let mut names: Vec<String> = cargo::items(&metadata, "packages")
        .iter()
        .filter(|package| framework(package))
        .flat_map(|package| cargo::items(package, "targets"))
        .filter(|target| {
            cargo::items(target, "kind").iter().any(|kind| {
                matches!(kind, JsValue::String(kind) if matches!(kind.as_str(), Some("lib" | "rlib" | "proc-macro")))
            })
        })
        .filter_map(|target| cargo::text(target, "name"))
        .map(|name| name.replace('-', "_"))
        .collect();
    names.sort_unstable();
    names.dedup();
    Ok(names)
}

/// The flags the release build of a server package passes rustc after the
/// flags of the environment on the host and before the remaps of its paths,
/// which the builder stage of the image of `docker` passes first as well.
pub(crate) const LEAN_FLAGS: [&str; 2] = ["-C", "panic=abort"];

/// The target and the flags of the release build of a server package.
struct LeanBuild {
    /// The target of the host, which `--target` names.
    triple: String,
    /// The value of `CARGO_ENCODED_RUSTFLAGS`.
    rustflags: OsString,
}

/// Returns the target and the flags of the release build of the server
/// package of `project` for the compile `compile`: the host, [`LEAN_FLAGS`],
/// which pass `-C panic=abort`, and `--remap-path-prefix` for the cargo home
/// to `cargo`, the workspace of a path facade to `aosabi`, the project root
/// to the package name, the files of the generated copy `copy` of a plain
/// crate to the package name as well, [`copy_remaps`], and the source of the
/// standard library in the sysroot to `/rustc/<commit>` last, as the builder
/// stage of the card `docker` writes them, after the flags of the
/// environment for a card built on the host and with none of them for the
/// card `docker`, whose builder stage passes none, so the build on the host
/// compiles the parts its image compiles.
fn lean_build(
    project: &Project,
    cargo: &Cargo,
    copy: Option<&Path>,
    compile: Compile,
) -> Result<LeanBuild, CliError> {
    let facts = cargo
        .rustc_facts(&project.root)
        .filter(|facts| !facts.host.is_empty())
        .ok_or_else(|| {
            CliError::new(String::from(
                "rustc -vV names no host target, which the release build of the server package passes to --target",
            ))
        })?;
    let triple = facts.host;
    let mut prefixes: Vec<(PathBuf, &str)> = Vec::new();
    if let Some(home) = cargo.home() {
        prefixes.push((home, "cargo"));
    }
    if let Some(workspace) = &project.facade_workspace {
        prefixes.push((workspace.clone(), project::FACADE));
    }
    prefixes.push((project.root.clone(), &project.name));
    let mut flags: Vec<String> = LEAN_FLAGS.iter().map(|flag| String::from(*flag)).collect();
    for (from, to) in prefixes {
        flags.push(format!(
            "--remap-path-prefix={}={to}",
            generate::utf8(&from)?
        ));
    }
    // The copy lies inside the target directory of the project, so it
    // stands after the project root, and the last prefix that matches wins.
    if let Some(copy) = copy {
        flags.extend(copy_remaps(project, copy, true, Path::new(&project.name))?);
    }
    // The source of the standard library stands last, so its remap wins over
    // every other prefix a path of it matches, the project root of a project
    // whose toolchain lies inside it among them, while no prefix of the copy
    // matches a path of it, a relative one or one inside the project.
    if !facts.commit.is_empty() && facts.commit != "unknown" {
        let source = std_source::source_dir(&sysroot(project, cargo)?);
        flags.push(format!(
            "--remap-path-prefix={}={}{}",
            generate::utf8(&source)?,
            std_source::REMAPPED_PREFIX,
            facts.commit
        ));
    }
    Ok(LeanBuild {
        triple,
        rustflags: cargo.encoded_rustflags(compile, &flags),
    })
}

/// Returns the count of the packages of `compiled` whose build script ran,
/// leaving out the path packages named in `own`.
///
/// A build for `--target` runs the script of a package once for the host and
/// once for the target when both compile it, and the count names the package
/// once, as the build line of `docs/cli-and-targets.md` section 5.0 names
/// every dependency build script by package and version.
fn dependency_scripts(compiled: &CargoRun, own: &[&str]) -> usize {
    let mut packages: Vec<&str> = compiled
        .build_script_packages
        .iter()
        .map(String::as_str)
        .filter(|id| {
            let (name, is_path) = cargo::package_of(id);
            !(is_path && own.contains(&name))
        })
        .collect();
    packages.sort_unstable();
    packages.dedup();
    packages.len()
}

/// Returns the target of the card of `job`, refusing one whose standard
/// library the toolchain lacks, `AO-R0105`.
fn target_of<'a>(job: &Job<'a>) -> Result<&'a str, CliError> {
    let triple = job.card.triple.unwrap_or_default();
    if job.cargo.has_target(&job.project.root, triple) {
        Ok(triple)
    } else {
        Err(target_missing(triple))
    }
}

/// Derives the lock of the generated workspace `generated` of `project` and
/// checks it.
///
/// A derived lock that resolves under `--locked` and passes the check stands
/// as it is, so a second build writes it no more. Otherwise the lock is
/// seeded from the lock of the project, resolved by `cargo metadata`,
/// offline under `--frozen` and `--offline`, and checked.
fn derive_lock(
    project: &Project,
    generated: &Generated,
    lock: LockMode,
    cargo: &Cargo,
) -> Result<LockReport, CliError> {
    let manifest = generated.root.join("Cargo.toml");
    if generated.lock.is_file()
        && cargo
            .metadata(&project.root, Some(&manifest), generated_lock_mode(lock))
            .is_ok()
        && let Ok(report) = lock::check(&project.lock, &generated.lock)
    {
        return Ok(report);
    }
    lock::seed(&project.lock, &generated.lock)?;
    let resolve = if cargo::is_offline(lock) {
        LockMode::Offline
    } else {
        LockMode::Normal
    };
    cargo.metadata(&project.root, Some(&manifest), resolve)?;
    lock::check(&project.lock, &generated.lock)
}

/// Returns the lock mode of every cargo call on the generated workspace
/// after its check, `--locked`, and `--frozen` where `lock` forbids the
/// network.
fn generated_lock_mode(lock: LockMode) -> LockMode {
    if cargo::is_offline(lock) {
        LockMode::Frozen
    } else {
        LockMode::Locked
    }
}

/// Returns the failure of a cargo build of `what` that did not succeed,
/// [`shown`] when its standard error reached the terminal.
fn check_compiled(compiled: &CargoRun, what: &str, output: Output) -> Result<(), CliError> {
    if compiled.status.success() {
        return Ok(());
    }
    match output {
        Output::Inherit => Err(shown()),
        Output::Capture => Err(CliError::new(format!("the cargo build of {what} failed"))
            .with_detail(String::from_utf8_lossy(&compiled.stderr).into_owned())),
    }
}

/// Returns the executable of the binary target `name` of `compiled`.
fn executable_of(compiled: &CargoRun, name: &str) -> Option<PathBuf> {
    compiled
        .artifacts
        .iter()
        .filter(|artifact| artifact.target == name)
        .find_map(|artifact| artifact.executable.clone())
}

/// Returns the `.rlib` of the library target `name` of `compiled`.
fn library_of(compiled: &CargoRun, name: &str) -> Option<PathBuf> {
    compiled
        .artifacts
        .iter()
        .filter(|artifact| artifact.target == name && is_library(artifact))
        .flat_map(|artifact| artifact.filenames.iter())
        .find(|file| {
            file.extension()
                .is_some_and(|extension| extension == "rlib")
        })
        .cloned()
}

/// Returns whether `artifact` is of a library target.
fn is_library(artifact: &Artifact) -> bool {
    artifact
        .kinds
        .iter()
        .any(|kind| matches!(kind.as_str(), "lib" | "rlib"))
}

/// Writes `manifest.json` into `artifact_dir` from its `ir.json`, the
/// canonical model with no authz and no queue.
fn write_manifest(artifact_dir: &Path) -> Result<PathBuf, CliError> {
    let ir_path = artifact_dir.join(IR_FILE);
    let failure = |what: String| CliError::new(format!("{}: {what}", ir_path.display()));
    let bytes = fs::read(&ir_path).map_err(|error| failure(error.to_string()))?;
    let value = aosabi_json::parse(&bytes).map_err(|error| failure(error.to_string()))?;
    let ir = aosabi_ir::from_js_value(&value).map_err(|error| failure(error.to_string()))?;
    let model =
        app_model(ir, Vec::new(), Vec::new()).map_err(|error| failure(error.to_string()))?;
    let path = artifact_dir.join(MANIFEST_FILE);
    write_json(&path, &app_model_to_js(&model))?;
    Ok(path)
}

/// Writes `value` to `path` with the gap of two spaces and a newline, unless
/// the file holds those bytes.
fn write_json(path: &Path, value: &JsValue) -> Result<(), CliError> {
    let mut text = to_string_with_gap(value, GAP).unwrap_or_default();
    text.push('\n');
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|error| io_failure(dir, &error))?;
    }
    update_file_if_changed(path, text.as_bytes())
        .map(|_| ())
        .map_err(|error| io_failure(path, &error))
}

/// Returns the artifacts of the build in `artifact_dir` that exist, in the
/// order of the build record.
fn present_artifacts(artifact_dir: &Path) -> Vec<PathBuf> {
    [
        SCHEMA_FILE,
        RESOLVERS_FILE,
        IR_FILE,
        DESCRIPTORS_FILE,
        AUTHORING_FILE,
        MANIFEST_FILE,
    ]
    .into_iter()
    .chain(CLIENT_FILES)
    .map(|name| artifact_dir.join(name))
    .filter(|path| path.is_file())
    .collect()
}

/// Returns every `--include` path of `include` as an absolute path inside
/// the project root, refusing a path that does not exist, lies outside the
/// root or is a link.
fn includes(project: &Project, include: &[PathBuf]) -> Result<Vec<PathBuf>, CliError> {
    let mut out = Vec::with_capacity(include.len());
    for path in include {
        let full = normalize(&project.root.join(path));
        if !full.starts_with(&project.root) || full == project.root {
            return Err(CliError::new(format!(
                "the include {} lies outside the project {}, and an include keeps its project relative path",
                path.display(),
                project.root.display()
            )));
        }
        refuse_link(&full)?;
        out.push(full);
    }
    Ok(out)
}

/// Returns `path` with every `.` dropped and every `..` taken back, without
/// reading the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            part => out.push(part),
        }
    }
    out
}

/// Refuses `path` when it does not exist or is a link, which the copy does
/// not follow.
fn refuse_link(path: &Path) -> Result<(), CliError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_failure(path, &error))?;
    if metadata.file_type().is_symlink() {
        return Err(CliError::new(format!(
            "the include {} is a link, and the build copies no link",
            path.display()
        )));
    }
    Ok(())
}

/// The strip of the binary a release build of a server card ships.
struct Strip {
    /// The `rust-objcopy` of the toolchain.
    program: PathBuf,
    /// The value of `LD_LIBRARY_PATH` it runs with, the `lib/` of the sysroot
    /// first.
    library_path: OsString,
}

impl Strip {
    /// Returns the strip of the toolchain of `project` for the build of the
    /// host `host`, the `rust-objcopy` in `lib/rustlib/<host>/bin/` of the
    /// sysroot its rustc names.
    ///
    /// # Errors
    ///
    /// Returns a failure when `host` is `None`, rustc names no sysroot, the
    /// sysroot holds no `rust-objcopy`, or its `lib/` cannot stand in
    /// `LD_LIBRARY_PATH`.
    fn of(project: &Project, cargo: &Cargo, host: Option<&str>) -> Result<Self, CliError> {
        let host = host.ok_or_else(|| {
            CliError::new(String::from(
                "rustc -vV names no host target, whose rust-objcopy strips the binary a release build ships",
            ))
        })?;
        let sysroot = sysroot(project, cargo)?;
        let program = sysroot
            .join("lib")
            .join("rustlib")
            .join(host)
            .join("bin")
            .join(OBJCOPY);
        if !program.is_file() {
            return Err(CliError::new(format!(
                "the toolchain carries no {} for the strip of the binary a release build ships",
                program.display()
            )));
        }
        Ok(Self {
            program,
            library_path: library_path(&sysroot.join("lib"))?,
        })
    }

    /// # Errors
    ///
    /// Returns a failure when the program does not start or does not
    /// succeed, with its standard error as the detail.
    fn run(&self, binary: &Path) -> Result<Vec<u8>, CliError> {
        let failure = |what: String| {
            CliError::new(format!(
                "{} --strip-all {} {what}",
                self.program.display(),
                binary.display()
            ))
        };
        let output = Command::new(&self.program)
            .arg("--strip-all")
            .arg(binary)
            .arg("-")
            .env(LIBRARY_PATH_ENV, &self.library_path)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| failure(format!("does not start: {error}")))?;
        if !output.status.success() {
            return Err(failure(format!("fails with {}", output.status))
                .with_detail(String::from_utf8_lossy(&output.stderr).into_owned()));
        }
        Ok(output.stdout)
    }
}

/// Returns the value of `LD_LIBRARY_PATH` with `first` before the directories
/// of the value the command line inherited.
#[cfg(unix)]
fn library_path(first: &Path) -> Result<OsString, CliError> {
    use std::os::unix::ffi::{
        OsStrExt,
        OsStringExt, //
    };

    let separates = |byte: &u8| matches!(*byte, b':' | b';');
    let bytes = first.as_os_str().as_bytes();
    if bytes.is_empty() || bytes.iter().any(separates) {
        return Err(CliError::new(format!(
            "{} cannot stand in {LIBRARY_PATH_ENV}, which a colon and a semicolon split",
            first.display()
        )));
    }
    let mut value = bytes.to_vec();
    if let Some(inherited) = std::env::var_os(LIBRARY_PATH_ENV) {
        for dir in inherited.as_bytes().split(separates) {
            if !dir.is_empty() {
                value.push(b':');
                value.extend_from_slice(dir);
            }
        }
    }
    Ok(OsString::from_vec(value))
}

/// Returns `first` alone, since a host that is not Unix loads no library from
/// the directories of `LD_LIBRARY_PATH`.
#[cfg(not(unix))]
fn library_path(first: &Path) -> Result<OsString, CliError> {
    Ok(first.as_os_str().to_owned())
}

/// Returns the sysroot of the toolchain of `project`, as `rustc --print
/// sysroot` names it in the project root.
fn sysroot(project: &Project, cargo: &Cargo) -> Result<PathBuf, CliError> {
    let output = Command::new(cargo.rustc())
        .args(["--print", "sysroot"])
        .current_dir(&project.root)
        .envs(cargo.env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| CliError::new(format!("rustc --print sysroot does not start: {error}")))?;
    let text = String::from_utf8(output.stdout).unwrap_or_default();
    let sysroot = text.trim_end();
    if !output.status.success() || sysroot.is_empty() {
        return Err(
            CliError::new(String::from("rustc --print sysroot names no sysroot"))
                .with_detail(String::from_utf8_lossy(&output.stderr).into_owned()),
        );
    }
    Ok(PathBuf::from(sysroot))
}

/// Writes the binary `binary` into `dist/` of `artifact_dir`, as `strip`
/// writes it where it is given and as it is otherwise, copies every path of
/// `includes` there at its project relative path, and answers the files it
/// holds.
fn copy_dist(
    project: &Project,
    artifact_dir: &Path,
    binary: &Path,
    strip: Option<&Strip>,
    includes: &[PathBuf],
) -> Result<Vec<PathBuf>, CliError> {
    let dist = artifact_dir.join(DIST_DIR);
    let mut copied = Vec::new();
    let shipped = dist.join(binary.file_name().unwrap_or_default());
    match strip {
        Some(strip) => write_copy(binary, &shipped, &strip.run(binary)?, &mut copied)?,
        None => copy_file(binary, &shipped, &mut copied)?,
    }
    for include in includes {
        let relative = include.strip_prefix(&project.root).unwrap_or(include);
        copy_tree(include, &dist.join(relative), &mut copied)?;
    }
    Ok(copied)
}

/// Copies the file or the directory `source` to `dest`, refusing a link.
fn copy_tree(source: &Path, dest: &Path, copied: &mut Vec<PathBuf>) -> Result<(), CliError> {
    refuse_link(source)?;
    if !source.is_dir() {
        return copy_file(source, dest, copied);
    }
    let entries = fs::read_dir(source).map_err(|error| io_failure(source, &error))?;
    let mut names: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    names.sort();
    for name in names {
        copy_tree(&source.join(&name), &dest.join(&name), copied)?;
    }
    Ok(())
}

/// Copies the file `source` to `dest` unless `dest` holds its bytes, with its
/// permissions, and records `dest`.
// A memory map of the file would read it with no copy.
fn copy_file(source: &Path, dest: &Path, copied: &mut Vec<PathBuf>) -> Result<(), CliError> {
    let bytes = fs::read(source).map_err(|error| io_failure(source, &error))?;
    write_copy(source, dest, &bytes, copied)
}

/// Writes `bytes` to `dest` unless `dest` holds them, with the permissions of
/// `source`, and records `dest`.
fn write_copy(
    source: &Path,
    dest: &Path,
    bytes: &[u8],
    copied: &mut Vec<PathBuf>,
) -> Result<(), CliError> {
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir).map_err(|error| io_failure(dir, &error))?;
    }
    if update_file_if_changed(dest, bytes).map_err(|error| io_failure(dest, &error))? {
        let permissions = fs::metadata(source)
            .map_err(|error| io_failure(source, &error))?
            .permissions();
        fs::set_permissions(dest, permissions).map_err(|error| io_failure(dest, &error))?;
    }
    copied.push(dest.to_path_buf());
    Ok(())
}

/// Returns the build record of `project` for `card`.
fn record(
    project: &Project,
    card: &Card,
    outcome: &BuildOutcome,
    triple: Option<&str>,
    includes: &[PathBuf],
    image: Option<&Result<container::ImageInfo, String>>,
) -> JsValue {
    let mut object = JsObject::new();
    object.insert("device", card.name);
    object.insert("class", card.class.name());
    object.insert("triple", triple.map_or(JsValue::Null, JsValue::from));
    let profile = if outcome.release {
        Profile::Release
    } else {
        Profile::Dev
    };
    object.insert("profile", profile.name());
    let carriers: Vec<JsValue> = card
        .carriers
        .iter()
        .map(|(carrier, protocols)| {
            let mut entry = JsObject::new();
            entry.insert("carrier", carrier.name());
            let names: Vec<JsValue> = protocols
                .iter()
                .map(|protocol| JsValue::from(protocol.name()))
                .collect();
            entry.insert("protocols", names);
            JsValue::from(entry)
        })
        .collect();
    object.insert("carriers", carriers);
    object.insert("encoding", card.encoding.name());
    object.insert("binary", file_value(project, outcome.binary.as_deref()));
    object.insert("library", file_value(project, outcome.library.as_deref()));
    object.insert("artifacts", paths_value(project, &outcome.artifacts));
    object.insert("include", paths_value(project, includes));
    object.insert(
        "dependency_build_scripts",
        u64::try_from(outcome.build_scripts).unwrap_or(u64::MAX),
    );
    object.insert("image", image_value(image));
    object.insert("sandbox", "none");
    JsValue::from(object)
}

/// Returns `{path, size}` of the file `path`, or null.
fn file_value(project: &Project, path: Option<&Path>) -> JsValue {
    let Some(path) = path else {
        return JsValue::Null;
    };
    let mut entry = JsObject::new();
    entry.insert("path", shown_path(project, path));
    entry.insert("size", file_size(path));
    JsValue::from(entry)
}

/// Returns the paths `paths` as a JSON array of their shown forms.
fn paths_value(project: &Project, paths: &[PathBuf]) -> JsValue {
    JsValue::from(
        paths
            .iter()
            .map(|path| JsValue::from(shown_path(project, path)))
            .collect::<Vec<_>>(),
    )
}

/// Returns `{tag, id, size}` of a built image, or null.
fn image_value(image: Option<&Result<container::ImageInfo, String>>) -> JsValue {
    let Some(Ok(info)) = image else {
        return JsValue::Null;
    };
    let mut entry = JsObject::new();
    entry.insert("tag", info.tag.as_str());
    entry.insert("id", info.id.as_str());
    entry.insert("size", info.size);
    JsValue::from(entry)
}

/// Returns the build log of `project` for `card` with the image line of
/// `image`.
fn log(
    project: &Project,
    card: &Card,
    outcome: &BuildOutcome,
    image: Option<&Result<container::ImageInfo, String>>,
) -> String {
    let scripts = match outcome.build_scripts {
        1 => String::from("1 dependency build script"),
        count => format!("{count} dependency build scripts"),
    };
    let artifact = match (&outcome.binary, &outcome.library, outcome.note) {
        (Some(binary), _, _) => sized(project, binary),
        (None, Some(library), Some(note)) => format!("{}, {note}", sized(project, library)),
        (None, _, Some(note)) => format!("none, {note}"),
        (None, _, None) => String::from("none"),
    };
    let mut text = format!(
        "  device    {}, built in\n  payload   none, a built in card\n  \
         build     0 declared build scripts, {scripts}, sandbox none, 0 network\n  \
         artifact  {artifact}\n  docs      none until M14\n",
        card.name
    );
    match image {
        Some(Ok(info)) => {
            let _ = writeln!(
                text,
                "  image     {}  {} bytes",
                info.tag,
                thousands(info.size)
            );
        }
        Some(Err(command)) => {
            let _ = writeln!(
                text,
                "  image     not built, no container engine, {command}"
            );
        }
        None => {}
    }
    text
}

/// Returns the shown path of `path` with its size.
fn sized(project: &Project, path: &Path) -> String {
    format!(
        "{}  {} bytes",
        shown_path(project, path),
        thousands(file_size(path))
    )
}

/// Returns the size of the file `path`, 0 when it cannot be read.
fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |metadata| metadata.len())
}

/// Returns `path` relative to the project root when it lies inside it, else
/// whole.
fn shown_path(project: &Project, path: &Path) -> String {
    path.strip_prefix(&project.root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Returns `value` with a comma between every group of three digits.
fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Returns the failure of a read or a write of `path`.
fn io_failure(path: &Path, error: &std::io::Error) -> CliError {
    CliError::new(format!("{}: {error}", path.display()))
}
