// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-build: the entry of a build script and of a program that is no build
 * script, and the emitter that validates a schema and writes the artifacts.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/bundler/emit-server-glue.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * It is the validation, the comparison with the file on disk and the writes of
 * schema.graphql and resolvers.js, with the return. Three writes of that
 * function are not ported. schema.mjs at :185-188, server.mjs at :193-196 and
 * worker.mjs at :199-202 are JavaScript modules a Node process imports, and
 * they have no twin, because a Rust server links its application. resolvers.js
 * is written as the text the reference writes, a copy that no Rust server
 * imports, so the resolver source of every union and interface is compared as
 * bytes. The helpers of the three modules not ported, configLoadSource at
 * :75-81, workerBootstrap at :87-114 and serverBootstrap at :118-160, stay
 * untranslated with them.
 */

//! The entry of a build script and the emitter of the artifacts.
//!
//! [`emit_from`] is the whole `main` of a build script, and [`emit_cli_from`]
//! the whole `main` of a program that runs after the build. Each takes the
//! entry of the application, the function `default` that answers its app, and
//! reads the roots from the type the entry returns without calling it, which
//! is the twin of the reference reading the type of the default export and
//! executing nothing. Both split into [`emit()`], which reads the descriptors
//! of the application and runs the inference, and [`emit_schema`], which
//! validates and writes. The split is the seam of the post compile route: a
//! program that is no build script calls the same functions with
//! [`ArtifactDirs`] of its own.
//!
//! A build script of the entry writes three files into `OUT_DIR`, which the
//! server crate includes: [`SCHEMA_FILE`], [`RESOLVE_TYPES_FILE`], the table
//! of the type resolvers generated from the model that writes
//! `resolvers.js`, and [`NAME_FILE`], the package name of the application
//! crate, which names the socket of HTTP(Unix Socket).

use std::{
    env,
    ffi::OsString,
    fs::{
        self,
        File, //
    },
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode,
    time::SystemTime, //
};

use aosabi_core::descriptor::{
    AppRoots,
    Roots, //
};
use aosabi_infer::{
    Built,
    DescriptorSource,
    Graph,
    ResolveTypeCheck,
    ResolveTypeRule,
    SchemaBuilder, //
};

#[cfg(feature = "source")]
use crate::source::{
    self,
    Cfg,
    SourceTypes,
    Undescribed, //
};
use crate::{
    artifact_dirs::{
        ARTIFACT_DIR_ENV,
        ArtifactDirs, //
    },
    assert_schema_is_valid::assert_schema_is_valid,
    authoring::authoring_json,
    error::BuildError,
    prepare_object_injection::{
        Injected,
        prepare_object_injection, //
    },
    update_file_if_changed::update_file_if_changed, //
};

/// The file name of the schema text, which is the name the reference writes.
pub const SCHEMA_FILE: &str = "schema.graphql";

/// The file name of the text copy of the resolvers, which is the name the
/// reference writes.
pub const RESOLVERS_FILE: &str = "resolvers.js";

/// The file name of the intermediate representation as JSON.
pub const IR_FILE: &str = "ir.json";

/// The file name of the dump of the descriptor graph.
pub const DESCRIPTORS_FILE: &str = "descriptors.json";

/// The file name of the authoring map, which no file of the reference has a
/// twin of.
pub const AUTHORING_FILE: &str = "authoring.json";

/// The file name of the table of the type resolvers in `OUT_DIR`, an
/// expression of the type `&'static [ResolveTypeRule]` of `aosabi-core`.
pub const RESOLVE_TYPES_FILE: &str = "aosabi_resolve_types.rs";

/// The file name of the package name of the application crate in `OUT_DIR`,
/// with no newline.
pub const NAME_FILE: &str = "aosabi_name.txt";

/// Says what a build wrote and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emitted {
    /// The path of [`SCHEMA_FILE`] in the artifact directory.
    pub schema: PathBuf,
    /// The path of [`RESOLVERS_FILE`] in the artifact directory.
    pub resolvers: PathBuf,
    /// The path of [`IR_FILE`] in the artifact directory.
    pub ir: PathBuf,
    /// The path of [`DESCRIPTORS_FILE`] in the artifact directory.
    pub descriptors: PathBuf,
    /// The path of [`AUTHORING_FILE`] in the artifact directory.
    pub authoring: PathBuf,
    /// Whether the schema text differs from what the artifact directory held
    /// before the build, which is `schemaChanged` of the reference.
    pub schema_changed: bool,
    /// Every warning of the inference, in emission order.
    pub warnings: Vec<String>,
    /// The package name of the application crate, the crate of its first
    /// present root, empty for an application with no root.
    pub app_crate: String,
    /// The table of the type resolvers as the text of [`RESOLVE_TYPES_FILE`].
    pub resolve_types: String,
}

/// Runs the build step as the `main` of a build script over the entry of the
/// application.
///
/// `entry` is the function `default` of the application crate, whose return
/// type names the roots, and it is never called. Naming it is what links the
/// application crate into the build script. The directives for cargo go to
/// stdout: the build reruns when [`ARTIFACT_DIR_ENV`] changes and when one of
/// the five artifacts changes or is missing, so a deleted artifact comes back,
/// and every line of every warning of the inference becomes one warning of the
/// build. Beside the copy of the schema the run writes [`RESOLVE_TYPES_FILE`]
/// and [`NAME_FILE`] into `OUT_DIR`, which the macro `build_output!` of the
/// facade includes into the server crate.
///
/// Cargo compares the modification time of a watched file with the time the
/// script last ran, and the script writes the files it watches after it
/// started. So every artifact this run wrote takes the modification time of
/// the build script's own executable, which cargo linked before it ran the
/// script, and the build after it runs the script no more and compiles
/// nothing. An artifact that was deleted comes back with the bytes it had.
///
/// On an error the message goes to stderr and the answer is
/// [`ExitCode::FAILURE`], which fails the build of the server crate, and no
/// file is written or removed.
///
/// # Examples
///
/// The whole build script of a server crate:
///
/// ```ignore
/// fn main() -> std::process::ExitCode {
///     aosabi_build::emit_from(my_app::default)
/// }
/// ```
///
/// [the cargo book on build scripts]: https://doc.rust-lang.org/cargo/reference/build-scripts.html
#[must_use]
pub fn emit_from<A: AppRoots>(entry: fn() -> A) -> ExitCode {
    // The type of the entry is read and the function is never called.
    let _: fn() -> A = entry;
    println!("cargo::rerun-if-env-changed={ARTIFACT_DIR_ENV}");

    let emitted = ArtifactDirs::from_env().and_then(|dirs| {
        let watched = watched_paths(&dirs.artifact_dir);
        let before = watched.each_ref().map(|path| modified(path));
        let emitted = emit::<A::Roots>(&dirs)?;
        settle_written(&watched, &before)?;
        Ok(emitted)
    });
    let emitted = match emitted {
        Ok(emitted) => emitted,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    print_directives(&emitted);
    ExitCode::SUCCESS
}

/// Returns the paths of the five artifacts in `artifact_dir` that a build
/// script names under `cargo::rerun-if-changed`, in the order of
/// [`print_directives`].
fn watched_paths(artifact_dir: &Path) -> [PathBuf; 5] {
    [
        SCHEMA_FILE,
        RESOLVERS_FILE,
        IR_FILE,
        DESCRIPTORS_FILE,
        AUTHORING_FILE,
    ]
    .map(|name| artifact_dir.join(name))
}

/// Returns the modification time of `path`, `None` when it has none or is
/// missing.
fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

/// Gives every file of `watched` that this run wrote the modification time
/// of the running executable.
///
/// `before` holds the modification times the files had before the run, so a
/// file whose time moved is one this run created or rewrote. Cargo built the
/// executable of a build script before it took the time it compares the
/// watched files with, so a file at that time or earlier reads as unchanged on
/// the next build. A file this run left alone keeps its time, and no time
/// moves when the time of the executable cannot be read.
///
/// [the cargo book on build scripts]: https://doc.rust-lang.org/cargo/reference/build-scripts.html#rerun-if-changed
fn settle_written(
    watched: &[PathBuf; 5],
    before: &[Option<SystemTime>; 5],
) -> Result<(), BuildError> {
    let Some(linked) = env::current_exe().ok().and_then(|exe| modified(&exe)) else {
        return Ok(());
    };
    for (path, was) in watched.iter().zip(before) {
        let now = modified(path);
        if now == *was || now.is_none_or(|time| time <= linked) {
            continue;
        }
        File::options()
            .write(true)
            .open(path)
            .and_then(|file| file.set_modified(linked))
            .map_err(|error| BuildError::Io(path.clone(), error))?;
    }
    Ok(())
}

/// Prints what cargo has to know about a finished build.
fn print_directives(emitted: &Emitted) {
    for path in [
        &emitted.schema,
        &emitted.resolvers,
        &emitted.ir,
        &emitted.descriptors,
        &emitted.authoring,
    ] {
        println!("cargo::rerun-if-changed={}", path.display());
    }
    // A directive ends at its newline, so a warning of several lines takes one
    // directive per line.
    for line in emitted.warnings.iter().flat_map(|warning| warning.lines()) {
        println!("cargo::warning={line}");
    }
}

/// Runs as the `main` of the build script of the server package of a plain
/// crate, which takes the artifacts the command line wrote from the source of
/// the crate.
///
/// A plain crate holds no descriptor, so no build script infers its schema:
/// the command line wrote it from the source before cargo runs, with the table
/// of the type resolvers and the package name beside it. `artifact_dir` is the
/// directory of those artifacts relative to the server package, which the
/// command line writes into the script, so a plain cargo build of the package
/// takes them as well. The run copies [`SCHEMA_FILE`], [`RESOLVE_TYPES_FILE`]
/// and [`NAME_FILE`] into `OUT_DIR`, where the macro `build_output!` of the
/// facade includes them, and the build reruns when one of the three changes.
///
/// On a missing artifact the message names it and the command that writes
/// it, goes to stderr, and the answer is [`ExitCode::FAILURE`].
///
/// # Examples
///
/// The whole build script of the server package of a plain crate:
///
/// ```ignore
/// fn main() -> std::process::ExitCode {
///     aosabi_build::lean_link(aosabi_build::LinuxCard::Server);
///     aosabi_build::take_artifacts("..")
/// }
/// ```
#[must_use]
pub fn take_artifacts(artifact_dir: &str) -> ExitCode {
    let taken = taken_dirs(artifact_dir).and_then(|(from, out_dir)| {
        for name in [SCHEMA_FILE, RESOLVE_TYPES_FILE, NAME_FILE] {
            let path = from.join(name);
            let bytes = fs::read(&path).map_err(|error| BuildError::Io(path.clone(), error))?;
            write_artifact(&out_dir.join(name), &bytes)?;
            println!("cargo::rerun-if-changed={}", path.display());
        }
        Ok(())
    });
    match taken {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{error}\nthe artifacts of a plain crate are written by `cargo aosabi build`, which builds this package after it"
            );
            ExitCode::FAILURE
        }
    }
}

/// Returns the directory of the artifacts at `artifact_dir` from the server
/// package and `OUT_DIR`, both as cargo hands them to a build script.
fn taken_dirs(artifact_dir: &str) -> Result<(PathBuf, PathBuf), BuildError> {
    let var = |name: &str| {
        env::var_os(name)
            .map(PathBuf::from)
            .ok_or_else(|| BuildError::NoBuildScript(String::from(name)))
    };
    let manifest_dir = var("CARGO_MANIFEST_DIR")?;
    Ok((manifest_dir.join(artifact_dir), var("OUT_DIR")?))
}

/// [sysexits(3)]: https://man.freebsd.org/cgi/man.cgi?query=sysexits&sektion=3
const EX_USAGE: u8 = 64;

/// Runs the build step as the `main` of a program that is no build script
/// over the entry of the application.
///
/// `entry` is the function `default` of the application crate, as for
/// [`emit_from`]. The one argument of the program names the artifact
/// directory, and a relative path resolves against the working directory.
/// This is the post compile route of
/// docs/decisions/ADR-0002-derive-inference-with-typesource-seam.md: the
/// program links the application crate after it is compiled, its roots carry
/// the declaration list as they do in a build script, and the harness writes
/// the artifacts of a library that has no server crate.
///
/// Nothing goes to stdout. There is no `OUT_DIR`, so the copy a server binary
/// embeds is not written, and no directive for cargo is printed, because no
/// cargo build reads the output. Every line of every warning of the inference
/// goes to stderr as `warning: <line>`, and the answer is
/// [`ExitCode::SUCCESS`]. On an error the message goes to stderr, the answer
/// is [`ExitCode::FAILURE`], and no file is written or removed. A run with no
/// argument, with an empty one or with more than one prints
/// `usage: <program> ARTIFACT_DIR` to stderr and answers 64, the `EX_USAGE`
/// of `sysexits.h`, with `<program>` the file name the program was started
/// by.
///
/// # Examples
///
/// The whole runner of a case of the builder corpus:
///
/// ```ignore
/// fn main() -> std::process::ExitCode {
///     aosabi_build::emit_cli_from(case_x::default)
/// }
/// ```
#[must_use]
pub fn emit_cli_from<A: AppRoots>(entry: fn() -> A) -> ExitCode {
    let _: fn() -> A = entry;
    let mut args = env::args_os();
    let program = program_name(args.next());
    let artifact_dir = match (args.next(), args.next()) {
        (Some(artifact_dir), None) if !artifact_dir.is_empty() => artifact_dir,
        _ => {
            eprintln!("usage: {program} ARTIFACT_DIR");
            return ExitCode::from(EX_USAGE);
        }
    };

    let dirs = ArtifactDirs {
        artifact_dir: PathBuf::from(artifact_dir),
        out_dir: None,
    };
    let emitted = match emit::<A::Roots>(&dirs) {
        Ok(emitted) => emitted,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    for line in emitted.warnings.iter().flat_map(|warning| warning.lines()) {
        eprintln!("warning: {line}");
    }
    ExitCode::SUCCESS
}

/// Returns the file name of the path a program was started by.
///
/// The usage line names the program the way a person types it, so a path
/// deep in a target directory shortens to its last component. A start with
/// no such path names the program `emit`.
fn program_name(arg0: Option<OsString>) -> String {
    let Some(arg0) = arg0 else {
        return String::from("emit");
    };
    let path = PathBuf::from(arg0);
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Infers the schema of the application whose roots are `R` and writes the
/// artifacts into `dirs`.
///
/// The roots reach every type through links, so the inference needs no list
/// for them. The declaration list serves the heritage sweep, which looks at
/// every declaration of the application crate, reached or not. The roots carry
/// the list the build script of the application crate wrote, as
/// [`Roots::DECLARATIONS`], so it holds the same declarations in every profile
/// and on every target.
///
/// # Errors
///
/// Returns [`BuildError::NoDeclarationList`] when a root is present and the
/// list holds no declaration of it, [`BuildError::Infer`] when the graph
/// refuses a spread or the inference refuses the application, and what
/// [`emit_schema`] returns.
pub fn emit<R: Roots>(dirs: &ArtifactDirs) -> Result<Emitted, BuildError> {
    let roots = R::ROOTS;
    let graph = Graph::from_roots(&roots, R::DECLARATIONS)?;
    if roots.has_root() && !graph.has_declaration_of_root() {
        let krate = graph.app_crate().unwrap_or_default().to_owned();
        return Err(BuildError::NoDeclarationList { krate });
    }

    // The dump of a graph is an object, and the writer answers `None` for an
    // undefined value alone.
    let descriptors = aosabi_json::to_string(&graph.to_js_value()).unwrap_or_default();
    let app_crate = graph.app_crate().unwrap_or_default().to_owned();
    let source = DescriptorSource::new(graph);
    let built = SchemaBuilder::new(&source).build()?;
    let authoring = authoring_json(&source.authoring_map(&built.ir));
    let mut emitted = emit_schema(&built, &descriptors, &authoring, dirs)?;
    emitted.resolve_types = resolve_types_rs(&built, &app_crate);
    if let Some(out_dir) = &dirs.out_dir {
        write_artifact(
            &out_dir.join(RESOLVE_TYPES_FILE),
            emitted.resolve_types.as_bytes(),
        )?;
        write_artifact(&out_dir.join(NAME_FILE), app_crate.as_bytes())?;
    }
    emitted.app_crate = app_crate;
    Ok(emitted)
}

/// What the source path makes of a plain crate before it validates and
/// writes: the inference and the texts of the descriptor dump and of the
/// authoring map.
#[cfg(feature = "source")]
#[derive(Debug, Clone)]
pub struct SourceBuild {
    /// The inference of the schema, the bytes of `schema.graphql`, the IR and
    /// the type resolvers.
    pub built: Built,
    /// The dump of the descriptor graph as JSON text with no newline at its
    /// end, which `descriptors.json` holds.
    pub descriptors: String,
    /// The authoring map as [`authoring_json`] writes it, which
    /// `authoring.json` holds.
    pub authoring: String,
    /// The name of the package of the crate.
    pub package: String,
}

/// Infers the schema of the plain crate of the package in the directory
/// `package` from its source, as rustc compiles it over the active set `cfg`
/// of the target of the build, and validates and writes nothing.
///
/// The reader of the module [`source`] reads the crate from its root, stripped
/// over `cfg` and the features of each crate it reads, finds the entry and the
/// roots it registers, and the third type source answers the inference about
/// the types they reach, so nothing of the application is compiled for the
/// schema.
///
/// # Errors
///
/// Returns [`BuildError::Source`] when the reader reads no crate,
/// [`BuildError::Refused`] with every construct the builder cannot read at
/// the place of the author, [`BuildError::Dump`] for a dump the descriptor
/// graph does not read, and [`BuildError::Infer`] when the inference refuses
/// the crate.
#[cfg(feature = "source")]
pub fn build_source(package: &Path, cfg: &Cfg) -> Result<SourceBuild, BuildError> {
    let krate = source::read(package, cfg).map_err(BuildError::Source)?;
    let types = SourceTypes::describe(&krate).map_err(|undescribed| match undescribed {
        Undescribed::Refused(refusals) => BuildError::Refused(refusals),
        Undescribed::Dump(error) => BuildError::Dump(error),
    })?;
    // The dump of a graph is an object, and the writer answers `None` for an
    // undefined value alone.
    let descriptors = aosabi_json::to_string(&types.dump()).unwrap_or_default();
    let built = SchemaBuilder::new(&types).build()?;
    let authoring = authoring_json(&types.authoring_map(&built.ir));
    Ok(SourceBuild {
        built,
        descriptors,
        authoring,
        package: String::from(krate.package()),
    })
}

/// Infers the schema of the plain crate of the package in the directory
/// `package` from its source over the active set `cfg`, validates it and
/// writes the five artifacts into `dirs`, as [`emit()`] writes those of a
/// crate of the derive path.
///
/// No build script runs for a plain crate, so nothing is written into
/// `OUT_DIR`, and the answer carries the table of the type resolvers and the
/// package name for the caller.
///
/// # Errors
///
/// Returns what [`build_source`] and [`emit_schema`] return.
#[cfg(feature = "source")]
pub fn emit_source(package: &Path, cfg: &Cfg, dirs: &ArtifactDirs) -> Result<Emitted, BuildError> {
    let build = build_source(package, cfg)?;
    let mut emitted = emit_schema(&build.built, &build.descriptors, &build.authoring, dirs)?;
    emitted.resolve_types = resolve_types_rs(&build.built, &build.package);
    emitted.app_crate = build.package;
    Ok(emitted)
}

/// Returns the text of [`RESOLVE_TYPES_FILE`] for `built`, the type resolver
/// of every union and interface of `app_crate` as a literal of the table of
/// `aosabi-core`.
///
/// The table is generated from the model [`resolvers_js`] writes, so the
/// resolver a server binds answers the type the text answers. The literal
/// names every item through `::aosabi::__private::glue::`, the path the facade
/// gives the glue, and every string is a Rust literal the compiler reads back.
#[must_use]
pub fn resolve_types_rs(built: &Built, app_crate: &str) -> String {
    let mut lines = vec![
        format!("// The type resolvers of {app_crate:?}, which aosabi_build::emit_from writes."),
        String::from("&["),
    ];
    for (name, rule) in &built.resolve_type_rules {
        lines.push(String::from(
            "    ::aosabi::__private::glue::ResolveTypeRule {",
        ));
        lines.push(format!("        type_name: {name:?},"));
        lines.push(format!(
            "        valid_names: &[{}],",
            quoted_list(&rule.valid_names)
        ));
        lines.push(String::from("        checks: &["));
        lines.extend(rule.checks.iter().map(check_line));
        lines.push(String::from("        ],"));
        lines.push(String::from("    },"));
    }
    lines.push(String::from("]\n"));
    lines.join("\n")
}

/// Returns the line of one check of a type resolver.
fn check_line(check: &ResolveTypeCheck) -> String {
    let (variant, fields, name) = match check {
        ResolveTypeCheck::AnyField { fields, name } => ("AnyField", fields, name),
        ResolveTypeCheck::AllFields { fields, name } => ("AllFields", fields, name),
    };
    format!(
        "            ::aosabi::__private::glue::ResolveTypeCheck::{variant} {{ fields: &[{}], name: {name:?} }},",
        quoted_list(fields)
    )
}

/// Returns the strings of `items` as Rust string literals joined by a comma.
fn quoted_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("{item:?}"))
        .collect::<Vec<String>>()
        .join(", ")
}

/// Returns the model of the type resolvers of `built` as the build keeps it.
#[must_use]
pub fn resolve_type_rules(built: &Built) -> Vec<(&str, &ResolveTypeRule)> {
    built
        .resolve_type_rules
        .iter()
        .map(|(name, rule)| (name.as_str(), rule))
        .collect()
}

/// Validates the schema of `built` and writes the artifacts into `dirs`.
///
/// `descriptors` is the dump of the descriptor graph as JSON text with no
/// newline at its end, and `authoring` the text of the authoring map
/// [`authoring_json`] makes, with its
/// newline. The files are [`SCHEMA_FILE`] with the bytes of
/// `built.type_defs`, [`RESOLVERS_FILE`] with the text [`resolvers_js`]
/// makes of `built`, [`IR_FILE`] with the JSON text of `built.ir` and one
/// newline, [`DESCRIPTORS_FILE`] with `descriptors` and one newline,
/// [`AUTHORING_FILE`] with `authoring`, and,
/// when `dirs.out_dir` is set, a second [`SCHEMA_FILE`] there, which is the
/// file a server binary embeds. Each one goes through
/// [`update_file_if_changed()`], so a build that changes nothing touches no
/// file.
///
/// # Errors
///
/// Returns [`BuildError::SchemaInvalid`] before anything touches the disk
/// when the text does not form a valid schema, and [`BuildError::Io`] with
/// the path the file system refuses.
pub fn emit_schema(
    built: &Built,
    descriptors: &str,
    authoring: &str,
    dirs: &ArtifactDirs,
) -> Result<Emitted, BuildError> {
    let type_defs = built.type_defs.as_bytes();

    // Fail before writing a byte if the schema is invalid.
    assert_schema_is_valid(&built.type_defs)?;

    let emitted = Emitted {
        schema: dirs.artifact_dir.join(SCHEMA_FILE),
        resolvers: dirs.artifact_dir.join(RESOLVERS_FILE),
        ir: dirs.artifact_dir.join(IR_FILE),
        descriptors: dirs.artifact_dir.join(DESCRIPTORS_FILE),
        authoring: dirs.artifact_dir.join(AUTHORING_FILE),
        schema_changed: schema_changed(&dirs.artifact_dir.join(SCHEMA_FILE), type_defs),
        warnings: built.warnings.clone(),
        app_crate: String::new(),
        resolve_types: String::new(),
    };

    fs::create_dir_all(&dirs.artifact_dir)
        .map_err(|error| BuildError::Io(dirs.artifact_dir.clone(), error))?;
    write_artifact(&emitted.schema, type_defs)?;
    write_artifact(&emitted.resolvers, resolvers_js(built).as_bytes())?;
    write_artifact(&emitted.ir, &json_artifact(&aosabi_ir::to_json(&built.ir)))?;
    write_artifact(&emitted.descriptors, &json_artifact(descriptors))?;
    write_artifact(&emitted.authoring, authoring.as_bytes())?;
    if let Some(out_dir) = &dirs.out_dir {
        write_artifact(&out_dir.join(SCHEMA_FILE), type_defs)?;
    }

    Ok(emitted)
}

/// Returns the text of [`RESOLVERS_FILE`] for `built`.
///
/// That object maps the name of every union and of every interface with a
/// field to an object whose one key `__resolveType` holds the type resolver,
/// and a build with neither writes `export const resolvers = {}` and one
/// newline. The parser gives every union and every interface a resolver before
/// it answers, so no value is undefined.
#[must_use]
pub fn resolvers_js(built: &Built) -> String {
    let resolvers: Vec<(&str, Injected<'_>)> = built
        .resolvers
        .iter()
        .map(|(name, resolve_type)| {
            let resolver = vec![("__resolveType", Injected::Function(resolve_type))];
            (name.as_str(), Injected::Object(resolver))
        })
        .collect();

    [
        "export const resolvers = ",
        &prepare_object_injection(&resolvers),
        "\n",
    ]
    .concat()
}

/// Returns whether `type_defs` differs from what `schema_path` holds.
///
/// The `catch` of the reference is empty, so a file that cannot be read counts
/// as the first build and the answer is `true`. The comparison is over bytes,
/// as in [`update_file_if_changed()`].
fn schema_changed(schema_path: &Path, type_defs: &[u8]) -> bool {
    let prev = fs::read(schema_path).ok();
    prev.as_deref() != Some(type_defs)
}

/// Returns the bytes of a JSON artifact, which are `text` and one newline.
fn json_artifact(text: &str) -> Vec<u8> {
    format!("{text}\n").into_bytes()
}

/// Writes one artifact and names its path in the error.
fn write_artifact(path: &Path, content: &[u8]) -> Result<(), BuildError> {
    update_file_if_changed(path, content)
        .map(|_| ())
        .map_err(|error| BuildError::Io(path.to_path_buf(), error))
}
