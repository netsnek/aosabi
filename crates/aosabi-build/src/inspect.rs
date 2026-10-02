// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the entry of the inspect runner, which writes the
 * introspection of an app for the command inspect.
 *
 * No reference file.
 */

//! The inspect runner entry.
//!
//! The inspect command loads the project in a child process, which
//! constructs the app and answers the data of the app, and then runs the schema
//! builder over the entry. The function [`inspect_from`] is the whole `main` of
//! that child for an Aosabi app. It constructs the app once, runs the inference
//! the build runs, and writes one file the command line reads:
//!
//! ```text
//! {"ir":<the intermediate representation as ir.json writes it>,"authz":[],"queues":[]}
//! ```
//!
//! and a newline. The members `authz` and `queues` stay empty until the
//! harvests of the registry of the ORM and of the queues reach the same file.
//! The runner validates nothing, as the command validates nothing, so an app
//! whose schema the build refuses is still inspected.

use std::{
    env,
    ffi::OsString,
    fs,
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode, //
};

use aosabi_core::descriptor::{
    AppRoots,
    Roots, //
};
use aosabi_infer::{
    DescriptorSource,
    Graph,
    SchemaBuilder, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::error::BuildError;

/// [sysexits(3)]: https://man.freebsd.org/cgi/man.cgi?query=sysexits&sektion=3
const EX_USAGE: u8 = 64;

/// Runs as the whole `main` of the inspect runner: constructs the app once,
/// runs the inference and writes the introspection of the app to the path of
/// its one argument.
///
/// The argument `entry` is the function `default` of the application crate. It
/// is called once and the app it answers is dropped, which is what the runner
/// of the reference does when it imports the entry, so a panic there fails the
/// command. The inference is the one [`emit_from`](crate::emit_from) runs, over
/// the roots the type of the entry names, and nothing is validated.
///
/// The file holds the object `{"ir", "authz", "queues"}` of the module
/// documentation and a newline, and it is the one output of a run that
/// succeeds, which answers [`ExitCode::SUCCESS`]. A run with no argument,
/// with an empty one or with more than one prints `usage: <program> OUTPUT`
/// to stderr and answers 64, the `EX_USAGE` of `sysexits.h`, with `<program>`
/// the file name the program was started by. An error of the inference or of
/// the write prints its message to stderr and answers
/// [`ExitCode::FAILURE`].
///
/// # Examples
///
/// The whole runner of an app:
///
/// ```ignore
/// fn main() -> std::process::ExitCode {
///     aosabi_build::inspect_from(my_app::default)
/// }
/// ```
#[must_use]
pub fn inspect_from<A: AppRoots>(entry: fn() -> A) -> ExitCode {
    let mut args = env::args_os();
    let program = program_name(args.next());
    let output = match (args.next(), args.next()) {
        (Some(output), None) if !output.is_empty() => PathBuf::from(output),
        _ => {
            eprintln!("usage: {program} OUTPUT");
            return ExitCode::from(EX_USAGE);
        }
    };

    drop(entry());

    let written = introspect_app_data::<A::Roots>().and_then(|data| write_data(&output, &data));
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Returns the introspection of the app whose roots are `R`, the object the
/// runner writes.
///
/// # Errors
///
/// Returns [`BuildError::NoDeclarationList`] when a root is present and the
/// list holds no declaration of it, and [`BuildError::Infer`] when the
/// inference refuses the app, the two refusals of
/// [`emit`](crate::emit()) before it validates.
fn introspect_app_data<R: Roots>() -> Result<JsValue, BuildError> {
    let roots = R::ROOTS;
    let graph = Graph::from_roots(&roots, R::DECLARATIONS)?;
    if roots.has_root() && !graph.has_declaration_of_root() {
        let krate = graph.app_crate().unwrap_or_default().to_owned();
        return Err(BuildError::NoDeclarationList { krate });
    }
    let source = DescriptorSource::new(graph);
    let built = SchemaBuilder::new(&source).build()?;
    Ok(introspection(&built.ir))
}

/// Returns the introspection of the plain crate of the package in the
/// directory `package`, the object the inspect runner writes, from its
/// source as rustc compiles it over the active set `cfg`.
///
/// The inference is the one [󰀀](crate::build_source) runs, and nothing is
/// validated and nothing is compiled, so the command `inspect` answers for a
/// plain crate with no runner.
///
/// # Errors
///
/// Returns what [`build_source`](crate::build_source) returns.
#[cfg(feature = "source")]
pub fn introspect_source(package: &Path, cfg: &crate::source::Cfg) -> Result<JsValue, BuildError> {
    let build = crate::emit::build_source(package, cfg)?;
    Ok(introspection(&build.built.ir))
}

/// Runs as the whole `main` of the inspect runner of a plain crate:
/// constructs the app once through the entry of its generated module, reads
/// the source of the crate in the directory `package` and writes the
/// introspection of the app to the path of its one argument.
///
/// The runner of a plain crate reads the source path, the inference of
/// [`introspect_source`] over the source of the crate as rustc compiles it for
/// the host in the dev profile, the profile the runner builds in, whose active
/// set the rustc of `RUSTC` or of the path prints in the working directory,
/// since the type of its entry names no roots, and it constructs the app as
/// the runner of the reference does, so a panic there fails the run.
/// `package` is the directory of the package relative to the working
/// directory or absolute, which the command line writes into the runner. The
/// invocation, the file and the exit statuses are those of [`inspect_from`].
///
/// # Examples
///
/// The whole runner of a plain crate:
///
/// ```ignore
/// fn main() -> std::process::ExitCode {
///     aosabi_build::inspect_source_from(my_app::__aosabi::entry, "../..")
/// }
/// ```
#[cfg(feature = "source")]
#[must_use]
pub fn inspect_source_from<A>(entry: fn() -> A, package: &str) -> ExitCode {
    let mut args = env::args_os();
    let program = program_name(args.next());
    let output = match (args.next(), args.next()) {
        (Some(output), None) if !output.is_empty() => PathBuf::from(output),
        _ => {
            eprintln!("usage: {program} OUTPUT");
            return ExitCode::from(EX_USAGE);
        }
    };

    drop(entry());

    let cfg = match crate::source::Cfg::host(Path::new(".")) {
        Ok(cfg) => cfg,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let written =
        introspect_source(Path::new(package), &cfg).and_then(|data| write_data(&output, &data));
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Returns the object `{"ir", "authz", "queues"}` of the intermediate
/// representation `ir`, with the two harvests empty.
fn introspection(ir: &aosabi_ir::SchemaIr) -> JsValue {
    let mut data = JsObject::with_capacity(3);
    data.insert("ir", aosabi_ir::to_js_value(ir));
    data.insert("authz", JsValue::Array(Vec::new()));
    data.insert("queues", JsValue::Array(Vec::new()));
    JsValue::Object(data)
}

/// Writes `data` as JSON with a newline to `output`.
///
/// # Errors
///
/// Returns [`BuildError::Io`] with the path the file system refuses.
fn write_data(output: &Path, data: &JsValue) -> Result<(), BuildError> {
    // The writer answers `None` for an undefined value alone, and the data is
    // an object.
    let text = aosabi_json::to_string(data).unwrap_or_default();
    fs::write(output, format!("{text}\n"))
        .map_err(|error| BuildError::Io(output.to_path_buf(), error))
}

/// Returns the file name of the path a program was started by.
///
/// The usage line names the program the way a person types it, so a path
/// deep in a target directory shortens to its last component. A start with
/// no such path names the program `inspect`.
fn program_name(arg0: Option<OsString>) -> String {
    let Some(arg0) = arg0 else {
        return String::from("inspect");
    };
    let path = PathBuf::from(arg0);
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
