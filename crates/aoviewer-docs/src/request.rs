// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs request: the request of one run of the generator.
 *
 * A request carries the input, the output directory, the tree options, the PDF
 * switch, the old schema of --since, the device card and the check mode.
 */

//! The request.
//!
//! A [`Request`] holds everything one run of the generator reads besides the
//! files of its inputs. The build step fills it from `aosabi.docs.toml` alone
//! and the command from the file and its flags, where a flag wins over a key.
//! So the command applies the file to a new request through
//! [󰀂](crate::config::Config::apply) first and sets its flags after, since a
//! request keeps no mark of which of its fields a flag set.
//!
//! The PDF is a switch of the request and no format of the tree, since the
//! core never compiles one. The PDF standard and the date epoch of the PDF are
//! no field of the request. The caller of `emit_pdf` of `aoviewer-docs-pdf`
//! passes both in its `PdfRequest`, so this crate names no type of that crate
//! and its manifest keeps the four crates the plan gives it. The date the
//! Typst document states stays [`DocsOptions::document_date`].

use std::path::PathBuf;

use aoviewer_core::tree::DocsOptions;

use crate::input::Input;

/// The output directory of a request that names none, beside the schema a
/// build writes into `target/aosabi/`.
///
/// A project that commits its reference names another, such as `docs/api`.
pub const DEFAULT_OUT: &str = "target/aosabi/docs";

/// The request of one run of the generator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The schema to document.
    pub input: Input,
    /// The directory the tree is written into or checked against, `--out`,
    /// [`DEFAULT_OUT`] by default.
    pub out: PathBuf,
    /// The options of the tree.
    pub options: DocsOptions,
    /// Whether the run compiles `api.typ` into `api.pdf` beside the tree, set
    /// by the key `pdf` of the file or by the word `pdf` of `--format`.
    pub pdf: bool,
    /// The old schema of `--since`, whose differences the change log lists.
    pub since: Option<Input>,
    /// The device card of `--device`, refused lands the device chapter.
    pub device: Option<PathBuf>,
    /// Whether the run checks the tree under `out` rather than writing it,
    /// `--check`.
    pub check: bool,
}

impl Request {
    /// Returns the request of `input` with the default of every other field.
    ///
    /// The tree goes to [`DEFAULT_OUT`] with the default options of
    /// [`DocsOptions::default`], no PDF, no old schema, no device and no
    /// check.
    #[must_use]
    pub fn new(input: Input) -> Request {
        Request {
            input,
            out: PathBuf::from(DEFAULT_OUT),
            options: DocsOptions::default(),
            pdf: false,
            since: None,
            device: None,
            check: false,
        }
    }
}
