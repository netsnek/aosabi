// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs-pdf: the PDF of a generated reference, behind the pdf
 * feature.
 *
 * The PDF is optional and host only, and a build without the feature writes
 * api.typ and prints the line that compiles it.
 */

//! The PDF of a generated reference.
//!
//! The function `emit_pdf` compiles `api.typ` of a tree held in memory into a
//! PDF. A build without the feature `pdf` answers [`PdfError::FeatureOff`] with
//! [`COMPILE_LINE`], and the constant `PDF_AVAILABLE` tells a caller which
//! build it links. The PDF is byte identical across processes, because the
//! identity is fixed, no clock is read and the compiler version is pinned.
//!
//! The judging notebook is `tests/notebooks/76_docs_pdf.ipynb`. There is no
//! test in this crate, which is the rule of CONTRIBUTING.md section 3.

#[cfg(feature = "pdf")]
mod compile;
#[cfg(feature = "pdf")]
mod options;
#[cfg(not(feature = "pdf"))]
mod stub;
#[cfg(feature = "pdf")]
mod world;

#[cfg(not(feature = "pdf"))]
pub use crate::stub::{
    PDF_AVAILABLE,
    emit_pdf, //
};

/// The line that compiles `api.typ` by hand, which a build without the
/// feature `pdf` prints in place of a PDF.
///
/// Without the feature the generator writes `api.typ` and prints this line. It
/// runs from the root of the tree, so every file the source imports resolves
/// inside it, and it ignores the fonts of the system, so the PDF uses the faces
/// the compiler embeds.
pub const COMPILE_LINE: &str = "typst compile --root . --ignore-system-fonts api.typ api.pdf";

/// Holds what one compilation reads.
#[derive(Clone, Copy, Debug)]
pub struct PdfRequest<'t> {
    /// The files of the tree by their path inside it, `api.typ` among them.
    pub files: &'t [(String, Vec<u8>)],
    /// The bytes of `template.typ` when the tree imports a template.
    pub template: Option<&'t [u8]>,
    /// The files the source attaches, served under their names.
    pub attachments: &'t [(String, Vec<u8>)],
    /// The identity of the document, which is the schema hash of the
    /// manifest.
    pub ident: &'t str,
    /// The PDF standard the document follows.
    pub standard: PdfStandard,
    /// The date of the document as Unix seconds, which a standard that needs
    /// one reads.
    pub date_epoch: Option<i64>,
}

/// Names the PDF standard a document follows.
///
/// The standard is `V17` by default, and `A3b` with the attachment of a device
/// card is the switch the device chapter turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfStandard {
    /// PDF 1.7.
    V17,
    /// PDF/A-2b.
    A2b,
    /// PDF/A-3b, which honours an attached file.
    A3b,
}

/// Holds one compiled document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pdf {
    /// The bytes of the PDF.
    pub bytes: Vec<u8>,
    /// Every warning of the compilation with its position, printed and never
    /// a refusal.
    pub warnings: Vec<String>,
    /// The number of pages.
    pub pages: u32,
}

/// Names why a compilation wrote no PDF.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PdfError {
    /// The build carries no feature `pdf`, and `compile_line` compiles the
    /// tree by hand.
    FeatureOff {
        /// The line that compiles `api.typ`, which is [`COMPILE_LINE`].
        compile_line: &'static str,
    },
    /// The compilation failed, with every message and its position.
    Compile {
        /// The messages of the compiler.
        messages: Vec<String>,
    },
    /// The document does not meet the standard it asks for.
    Standard {
        /// Why the standard is not met.
        message: String,
    },
}
