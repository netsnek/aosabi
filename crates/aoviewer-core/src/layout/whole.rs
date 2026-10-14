// SPDX-License-Identifier: MIT
/*
 * aoviewer-core layout whole: the layout of the whole graph with its wrapping
 * and its stubs.
 *
 * Wrapping, and the rule that stubs every edge that does not cross exactly
 * one gutter, belong to the whole graph mode alone.
 */

//! The whole graph layout.
//!
//! A column taller than its budget wraps into sub columns inside the same
//! rank band. The wrap aims at a picture between one to two and three to one,
//! the range the design names for the rule and step 13 of `51_viewer_core`
//! measures. The budget is one height for every band. A picture that is not
//! taller than two times its width keeps its columns whole, and a taller one
//! searches the height by bisection between the tallest item and the tallest
//! band for at most [`WRAP_PASSES`] passes, each of which wraps every band
//! greedily from the top and measures the picture with gutters of 70 px, and
//! takes the first height that brings that picture into the range, or the
//! last one tried.
//!
//! The picture the search measures is the stacked one, so the range is an
//! aim and not a bound of the scene. The placement then moves each item to
//! the ports it connects to, which can push a column far below its stacked
//! height, the gutters grow by the edges that cross them, the chips of the
//! stubs lengthen their boxes, and an item taller than the range allows
//! cannot wrap at all.
//!
//! An edge is drawn in full when it crosses exactly one gutter, from the last
//! sub column of a band into the first sub column of the next, and every
//! other edge becomes a stub, which the routing decides from the sub columns
//! this module makes.
//!
//! Every loop runs over the bands and the items of one view, or under
//! [`WRAP_PASSES`], and counts its steps.

use alloc::vec::Vec;

use super::{
    Column,
    GAP,
    Layout,
    LayoutError,
    MARGIN,
    Mode,
    Plan,
    Seed,
    WRAP_PASSES,
    WholeLayout,
    order::Band,
    place::GUTTER_BASE,
    run, //
};
use crate::{
    scene::Scene,
    schema::Schema,
    view::View, //
};

impl Layout for WholeLayout {
    fn layout(
        &self,
        view: &View,
        schema: &Schema,
        seed: Option<Seed<'_>>,
    ) -> Result<Scene, LayoutError> {
        run(view, schema, seed, Mode::Whole)
    }
}

/// Returns the sub columns of `bands`, every band wrapped at the height the
/// passes find.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
pub(super) fn wrap(plan: &mut Plan<'_>, bands: &[Band]) -> Result<Vec<Column>, LayoutError> {
    plan.steps
        .take(2 * plan.items.len() + bands.len(), "wrap")?;
    let tallest = bands
        .iter()
        .map(|b| stack(plan, &b.items))
        .max()
        .unwrap_or(0);
    let lowest = plan.items.iter().map(|i| i.h).max().unwrap_or(0);
    let mut limit = tallest;
    let (w, h) = extent(plan, bands, limit)?;
    if !too_tall(w, h) {
        return split(plan, bands, limit);
    }
    let (mut lo, mut hi) = (lowest, tallest);
    for _ in 0..WRAP_PASSES {
        limit = lo.saturating_add(hi.saturating_sub(lo) / 2);
        let (w, h) = extent(plan, bands, limit)?;
        if too_tall(w, h) {
            hi = limit;
        } else if too_wide(w, h) {
            lo = limit;
        } else {
            break;
        }
    }
    split(plan, bands, limit)
}

/// Returns whether a picture of width `w` and height `h` is taller than two
/// times its width.
fn too_tall(w: i64, h: i64) -> bool {
    w.saturating_mul(2) < h
}

/// Returns whether a picture of width `w` and height `h` is wider than three
/// times its height.
fn too_wide(w: i64, h: i64) -> bool {
    w > h.saturating_mul(3)
}

/// Returns the height of `items` stacked with the gap between two of them.
fn stack(plan: &Plan<'_>, items: &[usize]) -> i32 {
    let mut h: i32 = 0;
    for (k, &i) in items.iter().enumerate() {
        if k > 0 {
            h = h.saturating_add(GAP);
        }
        h = h.saturating_add(plan.items[i].h);
    }
    h
}

/// Returns the ranges of `items` that wrap into sub columns no taller than
/// `limit`, each holding at least one item.
fn subs(plan: &Plan<'_>, items: &[usize], limit: i32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let (mut start, mut h) = (0, 0i32);
    for (k, &i) in items.iter().enumerate() {
        let item = plan.items[i].h;
        let grown = if k == start {
            item
        } else {
            h.saturating_add(GAP).saturating_add(item)
        };
        if k > start && grown > limit {
            out.push((start, k));
            start = k;
            h = item;
        } else {
            h = grown;
        }
    }
    if start < items.len() {
        out.push((start, items.len()));
    }
    out
}

/// Returns the width and the height of the picture of `bands` wrapped at
/// `limit`, with 70 px gutters and the margins.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn extent(plan: &mut Plan<'_>, bands: &[Band], limit: i32) -> Result<(i64, i64), LayoutError> {
    plan.steps
        .take(3 * plan.items.len() + bands.len(), "wrap")?;
    let mut w = i64::from(MARGIN) * 2;
    let mut h = 0i64;
    for (b, band) in bands.iter().enumerate() {
        if b > 0 {
            w += i64::from(GUTTER_BASE);
        }
        for (s, &(from, to)) in subs(plan, &band.items, limit).iter().enumerate() {
            if s > 0 {
                w += i64::from(GUTTER_BASE);
            }
            let part = &band.items[from..to];
            let widest = part.iter().map(|&i| plan.items[i].w).max().unwrap_or(0);
            w += i64::from(widest);
            h = h.max(i64::from(stack(plan, part)));
        }
    }
    Ok((w, h + i64::from(MARGIN) * 2))
}

/// Returns the sub columns of `bands` wrapped at `limit`, in the order of the
/// ranks and of the sub columns inside each band.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn split(plan: &mut Plan<'_>, bands: &[Band], limit: i32) -> Result<Vec<Column>, LayoutError> {
    plan.steps
        .take(2 * plan.items.len() + bands.len(), "wrap")?;
    let mut out = Vec::new();
    for band in bands {
        let parts = subs(plan, &band.items, limit);
        let last = parts.len().saturating_sub(1);
        for (s, &(from, to)) in parts.iter().enumerate() {
            out.push(Column {
                rank: band.rank,
                items: band.items[from..to].to_vec(),
                x: 0,
                w: 0,
                first: s == 0,
                last: s == last,
            });
        }
    }
    Ok(out)
}
