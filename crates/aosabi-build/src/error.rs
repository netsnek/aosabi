// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the error of the build step.
 *
 * No file of the reference is translated here. The reference ends a build by
 * letting an `Error` reach the top of the command, and this type is what a
 * build script receives in its place.
 */

//! The error of the build step.

use core::fmt;
use std::{
    io,
    path::PathBuf, //
};

use aosabi_infer::InferError;

use crate::{
    artifact_dirs::ARTIFACT_DIR_ENV,
    assert_schema_is_valid::SchemaInvalid, //
};

/// Says why a build wrote no artifact.
///
/// Every variant is fatal for the build script, and none of them leaves a
/// half written set of files, because the inference and the validation run
/// before the first write.
#[derive(Debug)]
pub enum BuildError {
    /// The inference refuses the application.
    Infer(InferError),
    /// The inferred schema text does not form a valid schema.
    SchemaInvalid(SchemaInvalid),
    /// The roots name a type of the crate `krate`, and the declaration list
    /// the roots carry holds no declaration of that type, so the crate has
    /// no build script that writes the list.
    NoDeclarationList {
        /// The application crate, as its roots descriptor names it.
        krate: String,
    },
    /// The scan of the build script of an application crate cannot write the
    /// declaration list, for the reason `refusal`, at `path`.
    Scan {
        /// The file the scan read, or the root file of a target.
        path: PathBuf,
        /// Why the scan cannot write the list.
        refusal: ScanRefusal,
    },
    /// The run is no cargo build script, because the environment variable the
    /// text names is not set.
    NoBuildScript(String),
    /// Neither route to the artifact directory gives one, and the text is
    /// what the second route lacks.
    NoArtifactDir(String),
    /// The lean link of a Linux card is not written, for the reason the text
    /// gives.
    LeanLink(String),
    /// The file system refuses the named path.
    Io(PathBuf, io::Error),
    /// The reader read no plain crate, for the reason the error gives.
    #[cfg(feature = "source")]
    Source(crate::source::SourceError),
    /// The plain source of the crate states constructs the builder cannot
    /// read, each refused at the place of the author.
    #[cfg(feature = "source")]
    Refused(Vec<crate::source::Refusal>),
    /// The lowering of a plain crate wrote a dump the descriptor graph does
    /// not read, a fault of the builder.
    #[cfg(feature = "source")]
    Dump(aosabi_infer::GraphError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Infer(error) => fmt::Display::fmt(error, f),
            Self::SchemaInvalid(error) => fmt::Display::fmt(error, f),
            Self::NoDeclarationList { krate } => write!(
                f,
                "the declaration list of the crate \"{krate}\" holds no declaration of its root type, so the crate has no build script that writes the list. Give the crate the build script fn main() -> std::process::ExitCode {{ aosabi_build::declarations_main() }} and aosabi-build under [build-dependencies]"
            ),
            Self::Scan { path, refusal } => write!(f, "{}: {refusal}", path.display()),
            Self::NoBuildScript(variable) => write!(
                f,
                "{variable} is not set, so the run is no cargo build script, and only a build script writes the declaration list"
            ),
            Self::NoArtifactDir(reason) => write!(
                f,
                "no artifact directory: set {ARTIFACT_DIR_ENV}, or build with cargo, which sets OUT_DIR below a target directory that holds CACHEDIR.TAG ({reason})"
            ),
            Self::LeanLink(reason) => {
                write!(
                    f,
                    "the lean link of the Linux card is not written: {reason}"
                )
            }
            Self::Io(path, error) => write!(f, "{}: {error}", path.display()),
            #[cfg(feature = "source")]
            Self::Source(error) => fmt::Display::fmt(error, f),
            #[cfg(feature = "source")]
            Self::Refused(refusals) => {
                let count = refusals.len();
                let constructs = if count == 1 {
                    String::from("1 construct")
                } else {
                    format!("{count} constructs")
                };
                write!(
                    f,
                    "the builder cannot read {constructs} of the plain source of the crate, each at its file and line:"
                )?;
                for refusal in refusals {
                    write!(f, "\n{refusal}")?;
                }
                Ok(())
            }
            #[cfg(feature = "source")]
            Self::Dump(error) => write!(
                f,
                "the builder wrote a descriptor dump of the plain crate that the descriptor graph does not read, a fault of the builder: {error}"
            ),
        }
    }
}

/// Says why the scan of the build script of an application crate cannot
/// write the declaration list.
///
/// The text of each reason, with what the author writes in its place, is
/// written once, by `Display`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanRefusal {
    /// The module tree is circular, so the file is its own module.
    Circular,
    /// The file does not lex as Rust tokens.
    NoLex,
    /// The derive of Aosabi or the scalar attribute on the item `name` stands
    /// under `cfg_attr`.
    CfgAttrDeclares {
        /// The name of the item.
        name: String,
    },
    /// A `cfg` under `cfg_attr` stands on the declaration `name`.
    CfgAttrCfg {
        /// The name of the declaration.
        name: String,
    },
    /// A `path` under `cfg_attr` stands on the module `module`.
    CfgAttrPath {
        /// The path of the module from the crate root.
        module: String,
    },
    /// The declaration `name` stands in a module the list cannot name, for
    /// the reason `reason`.
    Unnamed {
        /// The name of the declaration.
        name: String,
        /// Why the list cannot name an item of its module.
        reason: ModuleRefusal,
    },
    /// The value `literal` of a `#[path]` is no string literal the scan reads.
    BadPath {
        /// The literal as the source writes it.
        literal: String,
    },
    /// The target shares the list `list` with the target whose root file is
    /// `other`, and both declare types.
    SharedList {
        /// The file name of the list.
        list: String,
        /// The root file of the other target.
        other: PathBuf,
    },
}

impl fmt::Display for ScanRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Circular => {
                f.write_str("the module tree is circular, so the file is its own module")
            }
            Self::NoLex => f.write_str("the file does not lex as Rust tokens"),
            Self::CfgAttrDeclares { name } => write!(
                f,
                "the derive of Aosabi or the scalar attribute on `{name}` stands under cfg_attr, which the scan of the declaration list cannot see through. Write the derive plainly and put #[cfg(..)] on the item"
            ),
            Self::CfgAttrCfg { name } => write!(
                f,
                "a cfg under cfg_attr stands on the declaration `{name}`, which the scan of the declaration list cannot see through. Write #[cfg(..)] on the item"
            ),
            Self::CfgAttrPath { module } => write!(
                f,
                "a path under cfg_attr stands on the module `{module}`, which the scan of the declaration list cannot follow. Write #[path = \"..\"] plainly"
            ),
            Self::Unnamed { name, reason } => {
                write!(f, "the declaration `{name}` is not listed: {reason}")
            }
            Self::BadPath { literal } => write!(
                f,
                "the #[path] {literal} is no string literal the scan of the declaration list reads, which is one with no escape but \\\\ and \\\""
            ),
            Self::SharedList { list, other } => write!(
                f,
                "the target shares the declaration list {list} with {}, and a list names the declarations of one crate only. Rename one of the two targets",
                other.display()
            ),
        }
    }
}

/// Says why the declaration list cannot name an item of a module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleRefusal {
    /// The visibility of the module `module` keeps it inside the module
    /// `parent`.
    Private {
        /// The path of the module from the crate root.
        module: String,
        /// The path of the module that declares it.
        parent: String,
    },
    /// A `cfg` under `cfg_attr` stands on the module `module`.
    CfgAttrCfg {
        /// The path of the module from the crate root.
        module: String,
    },
}

impl fmt::Display for ModuleRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Private { module, parent } => write!(
                f,
                "the visibility of the module `{module}` keeps it inside `{parent}`, so the declaration list, which every derive of the crate reads, cannot name it. Declare the module pub(crate)"
            ),
            Self::CfgAttrCfg { module } => write!(
                f,
                "a cfg under cfg_attr stands on the module `{module}`, which the scan of the declaration list cannot see through. Write #[cfg(..)] on the module"
            ),
        }
    }
}

impl core::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Infer(error) => Some(error),
            Self::Io(_, error) => Some(error),
            #[cfg(feature = "source")]
            Self::Source(error) => Some(error),
            #[cfg(feature = "source")]
            Self::Dump(error) => Some(error),
            #[cfg(feature = "source")]
            Self::Refused(_) => None,
            Self::SchemaInvalid(_)
            | Self::NoDeclarationList { .. }
            | Self::Scan { .. }
            | Self::NoBuildScript(_)
            | Self::NoArtifactDir(_)
            | Self::LeanLink(_) => None,
        }
    }
}

impl From<InferError> for BuildError {
    fn from(error: InferError) -> Self {
        Self::Infer(error)
    }
}

impl From<SchemaInvalid> for BuildError {
    fn from(error: SchemaInvalid) -> Self {
        Self::SchemaInvalid(error)
    }
}
