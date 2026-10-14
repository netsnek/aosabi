// SPDX-License-Identifier: MIT
/*
 * aoviewer-core diff changes: the page model of the changes chapter.
 */

//! The changes chapter.
//!
//! [`changes_page`] writes the differences [`diff`] finds as one page: a note
//! that says how a rename shows, then a table of the added elements, one of
//! the removed elements and one of the changed elements with the aspect that
//! changed, each in the order of the ids and left out when it is empty. A
//! page with no difference says so in its note and holds no table. Every id
//! is a text cell, because a removed element has no page in the newer tree
//! and a cell links an element only through an id of the model the page was
//! built from.
//!
//! The route grammar has no route of the changes, so the page answers the
//! route of the empty search. No tree writes that route as a page of its own
//! and no writer gives it an anchor or a label, so the page takes no anchor
//! of another page, neither in the one file of the single layout nor in
//! `api.typ`.
//!
//! Every loop runs over the differences the page is given, which [`diff`]
//! writes at most once per element and aspect of two models whose counts
//! [`ReadLimits`](crate::limits::ReadLimits) caps.
//!
//! [`diff`]: super::diff

use alloc::{
    string::String,
    vec,
    vec::Vec, //
};

use super::{
    Aspect,
    Change,
    Difference, //
};
use crate::{
    page::{
        Block,
        Cell,
        PageModel, //
    },
    route::Route,
};

/// The title of the changes page.
const TITLE: &str = "Changes";

/// The title of the home page, the one crumb of the breadcrumb.
const OVERVIEW: &str = "Overview";

/// The table of the elements only the newer schema holds.
const ADDED: &str = "Added";

/// The table of the elements only the older schema holds.
const REMOVED: &str = "Removed";

/// The table of the elements whose aspects differ.
const CHANGED: &str = "Changed";

/// The column of the stable id.
const ELEMENT: &str = "Element";

/// The column of the aspect that changed.
const ASPECT: &str = "Aspect";

/// The note of a page with differences, which says how a rename shows.
const RENAME_NOTE: &str = "The tables list every difference from the older schema, each element by its stable id. A renamed element is listed twice, removed under its old id and added under its new one.";

/// The note of a page without a difference.
const NO_CHANGE: &str = "No element differs from the older schema.";

/// Returns the page model of the changes chapter, tables and no diagram.
///
/// The differences are sorted by their order and their repeats dropped first,
/// so the page is the same whatever order the caller hands them in. A
/// difference that is changed and names no aspect shows an empty aspect cell,
/// and the aspect of an added or a removed element is not shown.
#[must_use]
pub fn changes_page(diffs: &[Difference]) -> PageModel {
    let mut sorted: Vec<&Difference> = diffs.iter().collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    for d in sorted {
        let id = Cell::Text(d.id.clone());
        match d.change {
            Change::Added => added.push(vec![id]),
            Change::Removed => removed.push(vec![id]),
            Change::Changed => {
                let aspect = d.aspect.map_or("", aspect_word);
                changed.push(vec![id, Cell::Text(String::from(aspect))]);
            }
        }
    }
    let empty = added.is_empty() && removed.is_empty() && changed.is_empty();
    let note = if empty { NO_CHANGE } else { RENAME_NOTE };
    let mut blocks = vec![Block::Note {
        text: String::from(note),
    }];
    push_table(&mut blocks, ADDED, vec![ELEMENT], added);
    push_table(&mut blocks, REMOVED, vec![ELEMENT], removed);
    push_table(&mut blocks, CHANGED, vec![ELEMENT, ASPECT], changed);
    PageModel {
        route: Route::Search {
            text: String::new(),
        },
        title: String::from(TITLE),
        kind: None,
        breadcrumb: vec![(String::from(OVERVIEW), Route::Home)],
        blocks,
    }
}

/// Appends the table `title` with its `columns` when it has a row.
fn push_table(
    blocks: &mut Vec<Block>,
    title: &'static str,
    columns: Vec<&'static str>,
    rows: Vec<Vec<Cell>>,
) {
    if !rows.is_empty() {
        blocks.push(Block::Table {
            title,
            columns,
            rows,
        });
    }
}

/// Returns the words the page writes for `aspect`.
fn aspect_word(aspect: Aspect) -> &'static str {
    match aspect {
        Aspect::Kind => "Kind",
        Aspect::Type => "Type",
        Aspect::Description => "Description",
        Aspect::Deprecation => "Deprecation",
        Aspect::DefaultValue => "Default value",
        Aspect::Implements => "Implemented interfaces",
        Aspect::Members => "Members",
        Aspect::Repeatable => "Repeatable",
        Aspect::Locations => "Locations",
    }
}
