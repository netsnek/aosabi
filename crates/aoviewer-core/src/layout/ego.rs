// SPDX-License-Identifier: MIT
/*
 * aoviewer-core layout ego: the layout of an ego view.
 *
 * Referrers stand left of the focus and referenced types right of it.
 */

//! The ego layout.
//!
//! The ego layout keeps one column per rank, so the referrers of the focus
//! stand in the column on its left and the types it references in the column
//! on its right, and an expanded neighbour's ring one column further out. It
//! never wraps a column, since wrapping belongs to the whole graph mode
//! alone. It draws in full the edges between adjacent ranks and turns every
//! other span into a stub, and the views it serves are the ones
//! [`EgoLayout`] names.

use alloc::vec::Vec;

use super::{
    Column,
    EgoLayout,
    Layout,
    LayoutError,
    Mode,
    Seed,
    order::Band,
    run, //
};
use crate::{
    scene::Scene,
    schema::Schema,
    view::View, //
};

impl Layout for EgoLayout {
    fn layout(
        &self,
        view: &View,
        schema: &Schema,
        seed: Option<Seed<'_>>,
    ) -> Result<Scene, LayoutError> {
        run(view, schema, seed, Mode::Ego)
    }
}

/// Returns one column per band, in the order of the ranks.
pub(super) fn columns(bands: Vec<Band>) -> Vec<Column> {
    bands
        .into_iter()
        .map(|band| Column {
            rank: band.rank,
            items: band.items,
            x: 0,
            w: 0,
            first: true,
            last: true,
        })
        .collect()
}
