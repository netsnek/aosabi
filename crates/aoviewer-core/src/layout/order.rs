// SPDX-License-Identifier: MIT
/*
 * aoviewer-core layout order: the order inside the columns of a layered
 * drawing.
 *
 * The order comes from six barycenter sweeps over integer keys, seeded from
 * the previous scene when one exists.
 */

//! The order of the columns.
//!
//! The items of one rank form a band, the boxes of that rank and the count
//! chips that stand in it. [`SWEEPS`] sweeps order every band in turn. An
//! inward sweep runs from the outermost bands towards rank 0 and keys every
//! band by its neighbour one rank further out, rank 0 by both of its
//! neighbours, and an outward sweep runs from rank 0 outwards and keys every
//! band by its neighbour one rank nearer rank 0. The sweeps alternate and
//! start inward, so the last one orders every ring by the ring inside it and
//! the ring of the focus follows the rows of the focus.
//!
//! The key of an item is the mean of the keys of its pulls into the reference
//! band, the position of the other item times 64 plus the index of its port,
//! and an item with no pull there keeps its own position times 64. The mean
//! is an integer division whose remainder is dropped, and two equal keys keep
//! the order of the tie, which is the height of the box in the seed first,
//! then boxes before chips, then the stable id of a box and the order of a
//! chip.
//!
//! Every loop runs over the bands, the items or the pulls of one view, over
//! the boxes of the seed, or under [`SWEEPS`], and counts its steps.

use alloc::{
    vec,
    vec::Vec, //
};
use core::cmp::Ordering;

use super::{
    LayoutError,
    Piece,
    Plan,
    SWEEPS,
    Seed, //
};
use crate::id::ElementId;

/// The items of one rank, in their order.
#[derive(Clone, Debug)]
pub(super) struct Band {
    /// The rank.
    pub(super) rank: i32,
    /// The items, from the top down.
    pub(super) items: Vec<usize>,
}

/// Orders the items of every band of `plan` and returns the bands in the
/// order of their ranks.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
pub(super) fn bands(plan: &mut Plan<'_>, seed: Option<Seed<'_>>) -> Result<Vec<Band>, LayoutError> {
    let seeded = seed_heights(plan, seed)?;
    ties(plan, &seeded)?;
    plan.steps.take(5 * plan.items.len(), "sweeps")?;
    let mut bands = initial(plan, &seeded);
    let mut pos = vec![0i64; plan.items.len()];
    for band in &bands {
        for (p, &i) in band.items.iter().enumerate() {
            pos[i] = i64::try_from(p).unwrap_or(i64::MAX);
        }
    }
    plan.steps.take(2 * bands.len(), "sweeps")?;
    let inner_first = sweep_order(&bands);
    for sweep in 0..SWEEPS {
        plan.steps.take(3 * bands.len(), "sweeps")?;
        let outward = sweep % 2 == 1;
        let mut order = inner_first.clone();
        if !outward {
            order.reverse();
        }
        for b in order {
            if outward && bands[b].rank == 0 {
                continue;
            }
            sort_band(plan, &mut bands[b], &mut pos, outward)?;
        }
    }
    Ok(bands)
}

/// Returns the height every box has in the seed, or `i64::MAX` for a box the
/// seed does not draw and for a chip.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn seed_heights(plan: &mut Plan<'_>, seed: Option<Seed<'_>>) -> Result<Vec<i64>, LayoutError> {
    plan.steps.take(plan.items.len(), "sweeps")?;
    let mut out = vec![i64::MAX; plan.items.len()];
    let Some(Seed(scene)) = seed else {
        return Ok(out);
    };
    plan.steps
        .take(2 * scene.boxes.len() + plan.boxes.len(), "sweeps")?;
    let mut table: Vec<(&ElementId, i32)> = scene.boxes.iter().map(|b| (&b.id, b.rect.y)).collect();
    table.sort_unstable_by(|a, b| a.0.cmp(b.0));
    for (i, b) in plan.boxes.iter().enumerate() {
        if let Ok(k) = table.binary_search_by(|e| e.0.cmp(&b.id)) {
            out[i] = i64::from(table[k].1);
        }
    }
    Ok(out)
}

/// Numbers every item in the order that breaks a tie between two equal keys.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn ties(plan: &mut Plan<'_>, seeded: &[i64]) -> Result<(), LayoutError> {
    let n = plan.items.len();
    plan.steps.take(3 * n, "sweeps")?;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        seeded[a]
            .cmp(&seeded[b])
            .then_with(|| by_piece(plan, plan.items[a].piece, plan.items[b].piece))
    });
    for (t, &i) in order.iter().enumerate() {
        plan.items[i].tie = super::id32(t);
    }
    Ok(())
}

/// Orders two pieces, boxes before chips, boxes by their stable ids and
/// chips by their order.
fn by_piece(plan: &Plan<'_>, a: Piece, b: Piece) -> Ordering {
    match (a, b) {
        (Piece::Box(x), Piece::Box(y)) => plan.boxes[x].id.cmp(&plan.boxes[y].id),
        (Piece::Box(_), Piece::Chip(_)) => Ordering::Less,
        (Piece::Chip(_), Piece::Box(_)) => Ordering::Greater,
        (Piece::Chip(x), Piece::Chip(y)) => x.cmp(&y),
    }
}

/// Returns the bands in the order of their ranks, the items of each in the
/// order of the seed and then in their own order, the boxes in the order of
/// the view and the chips after them.
fn initial(plan: &Plan<'_>, seeded: &[i64]) -> Vec<Band> {
    let mut order: Vec<usize> = (0..plan.items.len()).collect();
    order.sort_by_key(|&i| (plan.items[i].rank, seeded[i], i));
    let mut bands: Vec<Band> = Vec::new();
    for i in order {
        let rank = plan.items[i].rank;
        match bands.last_mut() {
            Some(band) if band.rank == rank => band.items.push(i),
            _ => bands.push(Band {
                rank,
                items: vec![i],
            }),
        }
    }
    bands
}

/// Returns the positions of the bands from rank 0 outwards, each distance on
/// the left before the same distance on the right.
fn sweep_order(bands: &[Band]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..bands.len()).collect();
    order.sort_by_key(|&b| (bands[b].rank.unsigned_abs(), bands[b].rank));
    order
}

/// Sorts the items of `band` by their keys against the reference band of an
/// outward or an inward sweep, and updates `pos`.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn sort_band(
    plan: &mut Plan<'_>,
    band: &mut Band,
    pos: &mut [i64],
    outward: bool,
) -> Result<(), LayoutError> {
    let rank = band.rank;
    let toward = i64::from(rank) - i64::from(rank.signum());
    let away = i64::from(rank) + i64::from(rank.signum());
    let refers = |r: i32| -> bool {
        let r = i64::from(r);
        match (outward, rank) {
            (true, _) => r == toward,
            (false, 0) => r == -1 || r == 1,
            (false, _) => r == away,
        }
    };
    let mut keyed: Vec<(i64, u32, usize)> = Vec::with_capacity(band.items.len());
    let mut examined = 3 * band.items.len();
    for &i in &band.items {
        let (mut sum, mut count) = (0i64, 0i64);
        for pull in &plan.pulls[i] {
            if refers(plan.items[pull.other].rank) {
                sum = sum
                    .saturating_add(pos[pull.other].saturating_mul(64).saturating_add(pull.port));
                count += 1;
            }
        }
        examined += plan.pulls[i].len();
        let key = if count > 0 {
            sum / count
        } else {
            pos[i].saturating_mul(64)
        };
        keyed.push((key, plan.items[i].tie, i));
    }
    plan.steps.take(examined, "sweeps")?;
    keyed.sort_unstable();
    for (p, &(_, _, i)) in keyed.iter().enumerate() {
        band.items[p] = i;
        pos[i] = i64::try_from(p).unwrap_or(i64::MAX);
    }
    Ok(())
}
