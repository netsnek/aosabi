// SPDX-License-Identifier: MIT
/*
 * aoviewer-core layout place: the placement of the columns of a layered
 * drawing.
 *
 * Columns nearest the focus are placed first, each box is pulled to the mean
 * height of the ports it connects to, overlaps resolve in one forward pass,
 * and gutters grow with the edges that cross them.
 */

//! The placement.
//!
//! Every box of a column takes the width of the widest item of the column,
//! and a column is wide enough to hold the text of every stub chip that hangs
//! under one of its boxes between the paddings of a box, since a writer draws
//! a stub chip into a box of the scene inside the element of the box its
//! arrow leaves.
//!
//! The vertical placement takes the columns by their distance from rank 0,
//! the nearer first, the left before the right, and the sub columns of a band
//! in their order. An item wants the mean of the heights its pulls into the
//! columns placed already ask for, which is the height of the port of the
//! other item less the height of its own port, as an integer division whose
//! remainder is dropped. An item with no such pull wants the height it has.
//! [`OVERLAP_PASSES`] forward pass then pushes every item below the one above
//! it by the gap of 18 px, and the column shifts back by the mean
//! displacement. [`ROUTE_ITERATIONS`] further rounds repeat that with the
//! pulls into both neighbours of every column, and a round that moves nothing
//! ends them. No round changes the order of a column.
//!
//! The horizontal placement puts the columns side by side from the left
//! margin. Two rank bands are 70 px apart plus 5 px per edge drawn in full
//! across the gutter, at most 200 px, and two sub columns of one band are 70
//! px apart, the gutter no edge crosses.
//!
//! Every loop runs over the columns, the items, the pulls or the links of
//! one view, or under [`OVERLAP_PASSES`] and [`ROUTE_ITERATIONS`], and counts
//! its steps.

use alloc::{
    vec,
    vec::Vec, //
};

use super::{
    ChipKind,
    GAP,
    LayoutError,
    MARGIN,
    OVERLAP_PASSES,
    Piece,
    Plan,
    ROUTE_ITERATIONS,
    Shape, //
    measure::chip_room,
};

/// The gutter no edge crosses, 70 px.
pub(super) const GUTTER_BASE: i32 = 7000;

/// The width a gutter grows by per edge drawn in full across it, 5 px.
const GUTTER_PER_EDGE: i32 = 500;

/// The widest gutter, 200 px.
const GUTTER_MAX: i32 = 20_000;

/// Sets the width of every column and the width of every box to the width of
/// its column.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
pub(super) fn widths(plan: &mut Plan<'_>) -> Result<(), LayoutError> {
    plan.steps.take(
        plan.columns.len() + 2 * plan.items.len() + plan.chips.len(),
        "placement",
    )?;
    for chip in &plan.chips {
        if chip.kind == ChipKind::Stub {
            let column = &mut plan.columns[plan.items[chip.at].column];
            column.w = column.w.max(chip_room(&chip.text));
        }
    }
    for column in &mut plan.columns {
        let mut w = column.w;
        for &i in &column.items {
            w = w.max(plan.items[i].w);
        }
        column.w = w;
        for &i in &column.items {
            if let Piece::Box(_) = plan.items[i].piece {
                plan.items[i].w = w;
            }
        }
    }
    Ok(())
}

/// Places every item of every column at its height.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
pub(super) fn vertical(plan: &mut Plan<'_>) -> Result<(), LayoutError> {
    plan.steps
        .take(2 * plan.columns.len() + 2 * plan.items.len(), "placement")?;
    let mut order: Vec<usize> = (0..plan.columns.len()).collect();
    order.sort_by_key(|&c| {
        let rank = plan.columns[c].rank;
        (rank.unsigned_abs(), rank, c)
    });
    let mut placed = vec![false; plan.items.len()];
    for &c in &order {
        settle(plan, c, &placed)?;
        for &i in &plan.columns[c].items {
            placed[i] = true;
        }
    }
    for _ in 0..ROUTE_ITERATIONS {
        plan.steps.take(order.len(), "placement")?;
        let mut moved = false;
        for &c in &order {
            moved |= settle(plan, c, &placed)?;
        }
        if !moved {
            break;
        }
    }
    Ok(())
}

/// Places the items of the column `c` by their pulls into the items
/// `placed` marks, and returns whether an item moved.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn settle(plan: &mut Plan<'_>, c: usize, placed: &[bool]) -> Result<bool, LayoutError> {
    let items = plan.columns[c].items.clone();
    let mut wanted = Vec::with_capacity(items.len());
    let passes = usize::try_from(OVERLAP_PASSES).unwrap_or(usize::MAX);
    let mut examined = items.len().saturating_mul(passes.saturating_add(5));
    for &i in &items {
        let (mut sum, mut count) = (0i64, 0i64);
        for pull in &plan.pulls[i] {
            if placed[pull.other] {
                let other = &plan.items[pull.other];
                let y = i64::from(other.y) + i64::from(pull.other_off) - i64::from(pull.own_off);
                sum = sum.saturating_add(y);
                count += 1;
            }
        }
        examined += plan.pulls[i].len();
        wanted.push(if count > 0 {
            sum / count
        } else {
            i64::from(plan.items[i].y)
        });
    }
    plan.steps.take(examined, "placement")?;
    let mut ys = wanted.clone();
    for _ in 0..OVERLAP_PASSES {
        let mut floor = i64::MIN;
        for (k, &i) in items.iter().enumerate() {
            ys[k] = ys[k].max(floor);
            floor = ys[k]
                .saturating_add(i64::from(plan.items[i].h))
                .saturating_add(i64::from(GAP));
        }
    }
    let shift = if items.is_empty() {
        0
    } else {
        let moved: i64 = ys.iter().zip(&wanted).map(|(y, w)| y - w).sum();
        moved / i64::try_from(items.len()).unwrap_or(i64::MAX)
    };
    let mut changed = false;
    for (k, &i) in items.iter().enumerate() {
        let y = clamp(ys[k] - shift);
        changed |= plan.items[i].y != y;
        plan.items[i].y = y;
    }
    Ok(changed)
}

/// Returns `v` as an `i32`, saturated.
fn clamp(v: i64) -> i32 {
    i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX })
}

/// Places every column at its left side, from the left margin.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
pub(super) fn horizontal(plan: &mut Plan<'_>) -> Result<(), LayoutError> {
    plan.steps
        .take(plan.columns.len() + 2 * plan.links.len(), "placement")?;
    let crossing = crossings(plan);
    let mut x = MARGIN;
    for c in 0..plan.columns.len() {
        plan.columns[c].x = x;
        let column = &plan.columns[c];
        x = x.saturating_add(column.w);
        if let Some(next) = plan.columns.get(c + 1) {
            let gap = if next.rank == column.rank {
                GUTTER_BASE
            } else {
                gutter(&crossing, column.rank, next.rank)
            };
            x = x.saturating_add(gap);
        }
    }
    Ok(())
}

/// Returns the pair of ranks every edge drawn in full crosses between, the
/// lower first, sorted.
fn crossings(plan: &Plan<'_>) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = plan
        .links
        .iter()
        .filter(|l| matches!(l.shape, Shape::Forward | Shape::Backward))
        .filter_map(|l| l.to.map(|t| (plan.items[l.from].rank, plan.items[t].rank)))
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect();
    out.sort_unstable();
    out
}

/// Returns the width of the gutter between the rank bands `left` and
/// `right`, from the pairs of ranks of `crossing`.
fn gutter(crossing: &[(i32, i32)], left: i32, right: i32) -> i32 {
    let pair = (left.min(right), left.max(right));
    let from = crossing.partition_point(|&p| p < pair);
    let to = crossing.partition_point(|&p| p <= pair);
    let count = i32::try_from(to - from).unwrap_or(i32::MAX);
    GUTTER_BASE
        .saturating_add(count.saturating_mul(GUTTER_PER_EDGE))
        .min(GUTTER_MAX)
}

/// Shifts every item so the highest of them starts at the top margin.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
pub(super) fn normalize(plan: &mut Plan<'_>) -> Result<(), LayoutError> {
    plan.steps.take(2 * plan.items.len(), "placement")?;
    let top = plan.items.iter().map(|i| i.y).min().unwrap_or(MARGIN);
    let shift = i64::from(MARGIN) - i64::from(top);
    for item in &mut plan.items {
        item.y = clamp(i64::from(item.y) + shift);
    }
    Ok(())
}
