// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs error: the refusals of the generator, by the codes of the family
 * AO-D.
 *
 * Every refusal names its code, and a warning is printed and never a refusal.
 */

//! The refusals of the generator.
//!
//! [`DocsError`] has one variant per code of the family `AO-D`, which nothing
//! else in the workspace uses. Each variant carries the reason as data, and
//! `Display` writes the code first and the text after it, in one place.
//!
//! A tree the core does not draw takes `AO-D0023`, the first code no plan held
//! before it.

use core::fmt;
use std::{
    io,
    path::{
        Path,
        PathBuf, //
    },
};

use aoviewer_core::{
    json::JsonError,
    layout::{
        LayoutError,
        MAX_STEPS, //
    },
    model::ModelError,
    tree::TreeError,
    view::ViewError, //
};

/// A place in a text file, as its reader counts it.
///
/// Both numbers start at 1. The SDL path counts the column in UTF-16 code units,
/// as graphql-js does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// The line.
    pub line: u32,
    /// The column.
    pub column: u32,
}

/// One message of a failed Typst compilation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The file of the tree the message points into, if it points into one.
    pub file: Option<String>,
    /// The place in that file, if the message names one.
    pub position: Option<Position>,
    /// The text of the message.
    pub message: String,
}

/// What a JSON reader of the core refuses in the bytes of a file, before the
/// model builder reads the schema they describe.
///
/// The model builder refuses the schema of any input as
/// [`DocsError::Model`], so a refusal of a file holds no model error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsonRefusal {
    /// The bytes are no JSON document within the read limits.
    Bytes {
        /// The byte offset the parser stands at, counted from the start of
        /// the file.
        offset: u64,
        /// What the parser found.
        error: JsonError,
    },
    /// The document is JSON of another shape than the reader reads.
    Shape {
        /// The path of the value, such as `data.__schema.types[3].kind`,
        /// which is empty for the document itself.
        path: String,
        /// What the reader expected there.
        expected: &'static str,
    },
}

/// How a file of the tree on disk differs from the generated tree, which the
/// check mode refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mismatch {
    /// The generated file is missing on disk.
    Missing,
    /// The file on disk is not part of the generated tree.
    Extra,
    /// Both files exist and differ.
    Differs {
        /// The first byte offset at which the two files differ, which is
        /// the length of the shorter one when it is a prefix of the other.
        offset: u64,
    },
}

/// A reason the generator refuses a run.
///
/// The variants follow the codes, `AO-D0001` to `AO-D0015` and `AO-D0023`,
/// in order, and [`DocsError::code`] returns the code of each.
#[derive(Debug)]
pub enum DocsError {
    /// `AO-D0001`: an input file is missing or unreadable.
    ///
    /// The text names the path, and `cargo aosabi build`, the command that
    /// writes it, for a file a build writes. An SDL file, an introspection
    /// result and a path whose kind is not named yet come from no command,
    /// so their text names none.
    Unreadable {
        /// The file.
        path: PathBuf,
        /// Why it cannot be read.
        error: io::Error,
        /// Whether `cargo aosabi build` writes the file, the
        /// `schema.graphql` or the `ir.json` of a build directory or an
        /// `ir.json` the input names.
        from_build: bool,
    },
    /// `AO-D0002`: the introspection JSON is refused.
    ///
    /// The error is [`JsonRefusal::Bytes`] with its byte offset or
    /// [`JsonRefusal::Shape`] with the path of the value and what was
    /// expected there.
    Introspection {
        /// The file.
        path: PathBuf,
        /// What the reader refuses.
        error: JsonRefusal,
    },
    /// `AO-D0003`: the SDL is refused.
    ///
    /// The message is the one of `aosabi-lang` or of `build_schema`, or of
    /// the bounds the SDL path holds before either runs.
    Sdl {
        /// The file.
        path: PathBuf,
        /// The place of the first location the message names, if any.
        position: Option<Position>,
        /// The message.
        message: String,
    },
    /// `AO-D0004`: `ir.json` is refused.
    ///
    /// The error is a [`JsonRefusal`], as for `AO-D0002`, and a model the
    /// builder refuses is [`DocsError::Model`] whatever input describes it.
    Ir {
        /// The file.
        path: PathBuf,
        /// What the reader refuses.
        error: JsonRefusal,
    },
    /// `AO-D0005`: `schema.graphql` and `ir.json` of a build directory
    /// disagree.
    ///
    /// Two files of one build that disagree are a stale file and a defect of the
    /// build.
    Disagree {
        /// The build directory.
        dir: PathBuf,
        /// The first path that differs, `Type`, `Type.field` or
        /// `Type.field(argument:)`.
        path: String,
        /// What `schema.graphql` holds at that path.
        served: String,
        /// What `ir.json` holds at that path.
        ir: String,
    },
    /// `AO-D0006`: `aosabi.docs.toml` is refused.
    Config {
        /// The file.
        path: PathBuf,
        /// The place the reader stopped at.
        position: Position,
        /// What the reader refuses there.
        reason: String,
    },
    /// `AO-D0007`: the value of a flag or a key is outside its list.
    Value {
        /// The flag or the key.
        name: String,
        /// The value given.
        value: String,
        /// Every value the flag or the key takes.
        allowed: &'static [&'static str],
    },
    /// `AO-D0008`: the check mode found a difference.
    ///
    /// A file is missing, extra or different, and one of the three alone.
    Check {
        /// The path of the file inside the tree.
        path: String,
        /// How the file differs.
        mismatch: Mismatch,
    },
    /// `AO-D0009`: the model is refused.
    ///
    /// The model builder refuses the schema an input describes, or the SDL
    /// path refuses its counts before the build.
    Model(ModelError),
    /// `AO-D0010`: `--device`, which lands the device chapter.
    Device {
        /// The card the flag names.
        path: PathBuf,
    },
    /// `AO-D0011`: `pdf` is asked and the binary carries no `pdf` feature.
    NoPdf {
        /// The command line that builds a binary with the feature.
        compile_line: &'static str,
    },
    /// `AO-D0012`: the Typst compilation failed.
    Typst {
        /// Every message, with its position.
        diagnostics: Vec<Diagnostic>,
    },
    /// `AO-D0013`: the output directory cannot be written.
    Output {
        /// The path that cannot be written.
        path: PathBuf,
        /// Why it cannot be written.
        error: io::Error,
    },
    /// `AO-D0014`: `--since` names a schema that does not load.
    Since(Box<DocsError>),
    /// `AO-D0015`: `--svg-text outlines` before the face vendors.
    SvgOutlines,
    /// `AO-D0023`: the core draws no tree of a model its builder took.
    ///
    /// A view refuses a route of the tree, or a layout crosses its cap or
    /// draws nothing. The outlines of the SVG text, which the core refuses as
    /// well, are `AO-D0015` before an input is read.
    Tree(TreeError),
}

impl DocsError {
    /// Returns the code of the refusal, such as `AO-D0001`.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unreadable { .. } => "AO-D0001",
            Self::Introspection { .. } => "AO-D0002",
            Self::Sdl { .. } => "AO-D0003",
            Self::Ir { .. } => "AO-D0004",
            Self::Disagree { .. } => "AO-D0005",
            Self::Config { .. } => "AO-D0006",
            Self::Value { .. } => "AO-D0007",
            Self::Check { .. } => "AO-D0008",
            Self::Model(_) => "AO-D0009",
            Self::Device { .. } => "AO-D0010",
            Self::NoPdf { .. } => "AO-D0011",
            Self::Typst { .. } => "AO-D0012",
            Self::Output { .. } => "AO-D0013",
            Self::Since(_) => "AO-D0014",
            Self::SvgOutlines => "AO-D0015",
            Self::Tree(_) => "AO-D0023",
        }
    }
}

impl From<ModelError> for DocsError {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}

impl fmt::Display for DocsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.code())?;
        match self {
            Self::Unreadable {
                path,
                error,
                from_build,
            } => unreadable_text(path, error, *from_build, f),
            Self::Introspection { path, error } | Self::Ir { path, error } => {
                write!(f, "{} is refused, ", path.display())?;
                read_text(error, f)
            }
            Self::Sdl {
                path,
                position: Some(p),
                message,
            } => write!(f, "{}:{}:{}: {message}", path.display(), p.line, p.column),
            Self::Sdl {
                path,
                position: None,
                message,
            } => write!(f, "{}: {message}", path.display()),
            Self::Disagree {
                dir,
                path,
                served,
                ir,
            } => write!(
                f,
                "schema.graphql and ir.json of {} disagree at {path}, where schema.graphql has {served} and ir.json has {ir}, so one of them is stale and cargo aosabi build writes both again",
                dir.display()
            ),
            Self::Config {
                path,
                position,
                reason,
            } => write!(
                f,
                "{}:{}:{}: {reason}",
                path.display(),
                position.line,
                position.column
            ),
            Self::Value {
                name,
                value,
                allowed,
            } => write!(
                f,
                "{value:?} is no value of {name}, which takes {}",
                allowed.join(", ")
            ),
            Self::Check {
                path,
                mismatch: Mismatch::Missing,
            } => write!(f, "{path} is missing from the tree on disk"),
            Self::Check {
                path,
                mismatch: Mismatch::Extra,
            } => write!(f, "{path} is on disk and not part of the generated tree"),
            Self::Check {
                path,
                mismatch: Mismatch::Differs { offset },
            } => write!(
                f,
                "{path} differs from the generated file at byte offset {offset}"
            ),
            Self::Model(e) => {
                f.write_str("the model is refused, ")?;
                model_text(e, f)
            }
            Self::Device { path } => write!(
                f,
                "--device {} needs the device chapter, which DC8 of the device card track lands",
                path.display()
            ),
            Self::NoPdf { compile_line } => write!(
                f,
                "this binary carries no pdf feature, so it writes api.typ and no PDF, and {compile_line} builds one that does"
            ),
            Self::Typst { diagnostics } => {
                f.write_str("the Typst compilation failed")?;
                for d in diagnostics {
                    f.write_str("\n")?;
                    diagnostic_text(d, f)?;
                }
                Ok(())
            }
            Self::Output { path, error } => {
                write!(f, "{} cannot be written ({error})", path.display())
            }
            Self::Since(inner) => write!(f, "the schema of --since does not load, {inner}"),
            Self::SvgOutlines => f.write_str(
                "--svg-text outlines needs the font face that M13 vendors, so the SVG keeps its text",
            ),
            Self::Tree(e) => {
                f.write_str("the core draws no tree of the model, ")?;
                tree_text(e, f)
            }
        }
    }
}

// Every variant writes its cause into its text, so no variant returns a
// source as well and a reporter that walks the chain prints it once.
impl std::error::Error for DocsError {}

/// Writes that the file `path` cannot be read for `error`, naming the
/// command that writes it when `from_build` holds.
fn unreadable_text(
    path: &Path,
    error: &io::Error,
    from_build: bool,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    if from_build {
        write!(
            f,
            "{} is missing or unreadable ({error}), and cargo aosabi build writes the schema.graphql and the ir.json of a build",
            path.display()
        )
    } else {
        write!(f, "{} is missing or unreadable ({error})", path.display())
    }
}

/// Writes what a JSON reader of the core refuses.
fn read_text(e: &JsonRefusal, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match e {
        JsonRefusal::Bytes { offset, error } => {
            write!(f, "at byte offset {offset}, ")?;
            f.write_str(json_text(*error))
        }
        JsonRefusal::Shape { path, expected } if path.is_empty() => {
            write!(f, "the document is not {expected}")
        }
        JsonRefusal::Shape { path, expected } => write!(f, "at {path}, expected {expected}"),
    }
}

/// Returns what the JSON parser of the core refuses.
fn json_text(e: JsonError) -> &'static str {
    match e {
        JsonError::Eof => "the document ends inside a value",
        JsonError::Byte(_) => "a byte stands where the grammar of JSON allows none of its kind",
        JsonError::Utf8 => "a string is not valid UTF-8",
        JsonError::Escape => "a backslash escape is not one RFC 8259 defines",
        JsonError::Depth => "the containers nest deeper than the read limits allow",
        JsonError::Number => "a number does not follow the grammar of JSON numbers",
        JsonError::TrailingData => "bytes other than whitespace follow the value",
        JsonError::TooLarge => "the document is longer than the read limits allow",
    }
}

/// Writes what the model builder refuses.
fn model_text(e: &ModelError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match e {
        ModelError::Duplicate { name } => write!(f, "{name} is declared twice"),
        ModelError::Dangling { from, name } => {
            write!(f, "{from} names {name}, which the schema does not declare")
        }
        ModelError::NotAnInterface { name } => {
            write!(
                f,
                "{name} is implemented as an interface and is no interface"
            )
        }
        ModelError::NotAnObject { name } => {
            write!(
                f,
                "{name} is named as a union member or a root and is no object"
            )
        }
        ModelError::InvalidName { name } => {
            write!(f, "the name {name:?} does not match [_A-Za-z][_0-9A-Za-z]*")
        }
        ModelError::Limit { what, limit } => {
            write!(f, "the {what} exceed the read limit of {limit}")
        }
    }
}

/// Writes why the writers of the core draw no tree.
///
/// The generator refuses the outlines of the SVG text before it reads an
/// input and hands the core the options it parsed, so the core refuses
/// neither the text mode nor the options in a run of the generator, and one
/// sentence covers both.
fn tree_text(e: &TreeError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match e {
        TreeError::View(e) => view_text(e, f),
        TreeError::Layout(LayoutError::CapExceeded { loop_name }) => write!(
            f,
            "a layout would take more than {MAX_STEPS} steps and crosses the cap in the loop {loop_name}"
        ),
        TreeError::Layout(LayoutError::Empty) => f.write_str("a diagram of the tree draws nothing"),
        TreeError::Svg(_) | TreeError::Option(_) => {
            f.write_str("the core refuses the options of the tree")
        }
    }
}

/// Writes why a view refuses a route of the tree.
fn view_text(e: &ViewError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match e {
        ViewError::UnknownType(name) => write!(
            f,
            "a route names the type {name}, which the schema does not declare"
        ),
        ViewError::UnknownField { ty, field } => write!(
            f,
            "a route names {ty}.{field}, which the schema does not declare"
        ),
        ViewError::UnknownArg { ty, field, arg } => write!(
            f,
            "a route names {ty}.{field}({arg}:), which the schema does not declare"
        ),
        ViewError::NoRoot(r) => write!(
            f,
            "a route names the {} root, which the schema does not declare",
            r.word()
        ),
        ViewError::UnknownCluster(slug) => write!(
            f,
            "a route names the cluster {slug}, which the clustering does not hold"
        ),
        ViewError::NoGraph => f.write_str("a route without a graph is drawn"),
    }
}

/// Writes one message of the Typst compilation with its position.
fn diagnostic_text(d: &Diagnostic, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match (&d.file, d.position) {
        (Some(file), Some(p)) => write!(f, "{file}:{}:{}: ", p.line, p.column)?,
        (Some(file), None) => write!(f, "{file}: ")?,
        (None, _) => {}
    }
    f.write_str(&d.message)
}
