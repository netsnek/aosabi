// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs-pdf stub: the build without the feature pdf.
 *
 * It is the path every build takes until the feature is on, so it is written
 * whole. It compiles nothing and answers every request with the line that
 * compiles api.typ by hand.
 */

//! The build without the feature `pdf`.

use crate::{
    COMPILE_LINE,
    Pdf,
    PdfError,
    PdfRequest, //
};

/// Tells whether this build compiles a PDF, which a build without the feature
/// `pdf` does not.
pub const PDF_AVAILABLE: bool = false;

/// Answers a request for a PDF in a build without the feature `pdf`.
///
/// `request` is not read, because this build compiles nothing, and the
/// caller prints the line of the refusal in place of a PDF.
///
/// # Errors
///
/// Returns [`PdfError::FeatureOff`] with [`COMPILE_LINE`] for every request.
pub fn emit_pdf(request: &PdfRequest<'_>) -> Result<Pdf, PdfError> {
    let _ = request;
    Err(PdfError::FeatureOff {
        compile_line: COMPILE_LINE,
    })
}
