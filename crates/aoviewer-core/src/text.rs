// SPDX-License-Identifier: MIT
/*
 * aoviewer-core text: the cell width of the monospace face and the width of a
 * run.
 *
 * Every ASCII glyph of the face has the same advance, so a width is an integer
 * product.
 */

//! The text metrics.
//!
//! Everything inside a diagram is set in Sabi Mono, the ASCII subset of
//! `DejaVu Sans Mono`, and every glyph of that face in both weights advances
//! [`CELL`] of [`EM`] units. The width of a run is therefore its cell count
//! times the cell times the size divided by the em, an integer product that is
//! the same on every machine and in the viewer and the documents alike. A box
//! keeps 8 percent of slack beside every run, because an SVG shown through an
//! image element cannot load the face and falls back to a monospace face of
//! the host.
//!
//! A size is given in centipixels, so 13 px is `1300`, and a width is
//! answered in centipixels. Every product is taken in `i64` and saturates, so
//! no input overflows, and the division drops its remainder.
//!
//! Every loop runs over the characters of one text, whose length the limits
//! of the reader cap.

use alloc::string::String;

/// The advance of every ASCII glyph of Sabi Mono and of `DejaVu Sans Mono`, in
/// units of the em.
pub const CELL: i32 = 1233;

/// The units per em of Sabi Mono and of `DejaVu Sans Mono`.
pub const EM: i32 = 2048;

/// The slack a box keeps beside a run, in percent.
const SLACK_PERCENT: i64 = 8;

/// The most cells a name takes inside a box before it is cut with an ellipsis.
pub(crate) const MAX_NAME_CELLS: u32 = 48;

/// The ellipsis that ends a cut name, one of the four glyphs beyond ASCII the
/// subset carries.
const ELLIPSIS: char = '\u{2026}';

/// Returns the cells a run draws.
///
/// A character outside ASCII counts one cell, and the ellipsis, the arrow, the
/// bullet and the middle dot are the four glyphs beyond ASCII the subset
/// carries. A GraphQL name matches `[_A-Za-z][_0-9A-Za-z]*`, so every name a
/// diagram draws is ASCII.
#[must_use]
pub fn cells(text: &str) -> u32 {
    u32::try_from(text.chars().count()).unwrap_or(u32::MAX)
}

/// Returns the width of a run of `cells` cells at the size `size_cp`, in
/// centipixels.
///
/// The width is the cell count times [`CELL`] times the size over [`EM`],
/// taken as an integer product whose remainder is dropped.
#[must_use]
pub fn run_width(cells: u32, size_cp: i32) -> i32 {
    scaled(cells, size_cp, 100)
}

/// Returns the width of a run of `cells` cells at the size `size_cp` with 8
/// percent of slack, in centipixels.
///
/// It is [`run_width`] times 108 over 100 as one integer product, which is the
/// room a box keeps for the run.
#[must_use]
pub fn run_width_with_slack(cells: u32, size_cp: i32) -> i32 {
    scaled(cells, size_cp, 100 + SLACK_PERCENT)
}

/// Returns `cells` times [`CELL`] times `size_cp` times `percent` over [`EM`]
/// times 100, saturated to an `i32`.
fn scaled(cells: u32, size_cp: i32, percent: i64) -> i32 {
    let product = i64::from(cells)
        .saturating_mul(i64::from(CELL))
        .saturating_mul(i64::from(size_cp))
        .saturating_mul(percent);
    let width = product / (i64::from(EM) * 100);
    i32::try_from(width).unwrap_or(if width < 0 { i32::MIN } else { i32::MAX })
}

/// Returns `name` as a box draws it, cut to [`MAX_NAME_CELLS`] cells with an
/// ellipsis in the last one when it is longer.
///
/// The title of a page and the page itself print the whole name.
pub(crate) fn cut(name: &str) -> String {
    if cells(name) <= MAX_NAME_CELLS {
        return String::from(name);
    }
    let keep = (MAX_NAME_CELLS - 1) as usize;
    let mut out: String = name.chars().take(keep).collect();
    out.push(ELLIPSIS);
    out
}
