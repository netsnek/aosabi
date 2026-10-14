// SPDX-License-Identifier: MIT
/*
 * aoviewer-core tree index: the index of a tree.
 *
 * The design keeps the index under 64 kB, and above 2000 types it splits by
 * first letter.
 */

//! The index.
//!
//! The index of a tree is the file a reader opens first, `README.md` of the
//! split layout or `API.md` of the single layout, and the index chapter of
//! the Typst document.
//!
//! - `README.md` is the home page: the counts, the root tables, the clusters
//!   with their source and one alphabetical table of every type that has a
//!   page of its own. Above [`LETTER_TYPES`] types that table moves to one
//!   page per first letter, `md/index/<letter>.md`, and `README.md` links the
//!   letters at its end, so the table of the types grows with one letter
//!   alone. The root tables and the table of the clusters stay on
//!   `README.md`.
//! - `API.md` holds every page of the tree in the order of the document, the
//!   home page first.
//! - `typ/99-index.typ` lists every type a page documents in name order with
//!   its kind and the page that holds it, which the theme counts through a
//!   query of the document, and above [`LETTER_TYPES`] types the list moves
//!   to `typ/99-index/<letter>.typ`.
//!
//! A first letter is a letter or a digit of ASCII in lower case, and `_` for
//! every other start, which every type name has.
//!
//! Every loop runs over the types of one schema, whose count
//! [`ReadLimits`](crate::limits::ReadLimits) caps, over the pages of one
//! tree, over the blocks or rows of one page, or over the letters of one
//! index, of which there are at most 37.
//!
//! [`LETTER_TYPES`]: super::LETTER_TYPES
//! [specification]: https://spec.graphql.org/October2021/

use alloc::vec::Vec;

use super::plan::{
    Site,
    documented_types,
    first_letter, //
};

/// The title of the home page, the one crumb of the breadcrumb of a part of
/// the index.
#[cfg(feature = "markdown")]
const OVERVIEW: &str = "Overview";

/// The title of the index chapter.
#[cfg(feature = "typst")]
const TITLE: &str = "Index";

/// The sentence of an index whose list moves to its letters.
#[cfg(feature = "typst")]
const BY_LETTER: &str = "The index lists the types by their first letter in the parts that follow.";

/// Returns the first letters of the parts of the Markdown index, which the
/// split layout writes above [`LETTER_TYPES`](super::LETTER_TYPES) types, in
/// byte order, or none.
///
/// A part holds the rows of the table of the types of the home page, every
/// type with a page of its own and the roots.
#[cfg(feature = "markdown")]
pub(super) fn md_letters(site: &Site<'_>) -> Vec<u8> {
    use super::plan::own_types;
    use crate::page::LayoutMode;

    if !site.splits() || site.layout != LayoutMode::Split {
        return Vec::new();
    }
    let model = &site.schema.model;
    let roots = model
        .types()
        .iter()
        .copied()
        .filter(|&t| model.roots().kind_of(t).is_some());
    let names = own_types(site.schema)
        .into_iter()
        .chain(roots)
        .map(|t| model.name(model.ty(t).name));
    letters(names)
}

/// Returns the first letters of the parts of the Typst index, which the
/// document holds above [`LETTER_TYPES`](super::LETTER_TYPES) types, in byte
/// order, or none.
pub(super) fn typ_letters(site: &Site<'_>) -> Vec<u8> {
    if !site.splits() {
        return Vec::new();
    }
    let model = &site.schema.model;
    let names = documented_types(site.schema)
        .into_iter()
        .map(|t| model.name(model.ty(t).name));
    letters(names)
}

/// Returns the first letters of `names`, sorted and without repeats.
fn letters<'a>(names: impl Iterator<Item = &'a str>) -> Vec<u8> {
    let mut out: Vec<u8> = names.map(first_letter).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Writes `README.md` of the split layout or `API.md` of the single layout,
/// with the facts of its diagrams in `drawn`.
///
/// # Errors
///
/// Returns [`TreeError::View`](super::TreeError::View) when a page of the
/// file is refused by the view of its route.
#[cfg(feature = "markdown")]
pub(super) fn markdown_index(
    site: &Site<'_>,
    drawn: &[super::Drawn],
    out: &mut alloc::string::String,
) -> Result<(), super::TreeError> {
    use core::fmt::Write;

    use super::{
        TreeError,
        markdown_page, //
    };
    use crate::{
        escape::md_text,
        page::{
            LayoutMode,
            title, //
        },
        render_markdown::page_path,
        route::Route, //
    };

    let from = page_path(site.schema, &Route::Home, site.layout).unwrap_or_default();
    if site.layout == LayoutMode::Single {
        for page in &site.pages {
            let model = page
                .model
                .as_ref()
                .map_err(|e| TreeError::View(e.clone()))?;
            markdown_page(site, model, &from, drawn, out);
        }
        return Ok(());
    }
    let Some(home) = site.pages.iter().find(|p| p.route == Route::Home) else {
        return Ok(());
    };
    let model = home
        .model
        .as_ref()
        .map_err(|e| TreeError::View(e.clone()))?;
    let letters = md_letters(site);
    if letters.is_empty() {
        markdown_page(site, model, &from, drawn, out);
        return Ok(());
    }
    let mut model = model.clone();
    model.blocks.retain(|b| types_rows(b).is_none());
    markdown_page(site, &model, &from, drawn, out);
    out.push_str("\n## ");
    md_text(title::TYPES, out);
    out.push_str("\n\n");
    for (i, &letter) in letters.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let mut shown = [0; 1];
        let shown = char::from(letter.to_ascii_uppercase()).encode_utf8(&mut shown);
        out.push('[');
        md_text(shown, out);
        // Writing into a `String` cannot fail.
        let _ = write!(out, "](md/index/{}.md)", char::from(letter));
    }
    out.push('\n');
    Ok(())
}

/// Writes `md/index/<letter>.md`, the rows of the table of the types of the
/// home page whose type starts with `letter`.
///
/// # Errors
///
/// Returns [`TreeError::View`](super::TreeError::View) when the home page is
/// refused by the view of its route.
#[cfg(feature = "markdown")]
pub(super) fn markdown_letter(
    site: &Site<'_>,
    letter: u8,
    out: &mut alloc::string::String,
) -> Result<(), super::TreeError> {
    use alloc::{
        format,
        string::String,
        vec, //
    };

    use super::{
        TreeError,
        markdown_page, //
    };
    use crate::{
        page::{
            Block,
            PageModel,
            title, //
        },
        route::Route,
    };

    let Some(home) = site.pages.iter().find(|p| p.route == Route::Home) else {
        return Ok(());
    };
    let model = home
        .model
        .as_ref()
        .map_err(|e| TreeError::View(e.clone()))?;
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    for block in &model.blocks {
        if let Some((c, r)) = types_rows(block) {
            columns.clone_from(c);
            rows.extend(
                r.iter()
                    .filter(|row| row_letter(row) == Some(letter))
                    .cloned(),
            );
        }
    }
    let shown = char::from(letter.to_ascii_uppercase());
    let page = PageModel {
        route: Route::Search {
            text: String::from(char::from(letter)),
        },
        title: format!("Types starting with {shown}"),
        kind: None,
        breadcrumb: vec![(String::from(OVERVIEW), Route::Home)],
        blocks: vec![Block::Table {
            title: title::TYPES,
            columns,
            rows,
        }],
    };
    let path = format!("md/index/{}.md", char::from(letter));
    markdown_page(site, &page, &path, &[], out);
    Ok(())
}

/// Returns the columns and the rows of `block` when it is the table of the
/// types of the home page.
#[cfg(feature = "markdown")]
fn types_rows(
    block: &crate::page::Block,
) -> Option<(&Vec<&'static str>, &Vec<Vec<crate::page::Cell>>)> {
    match block {
        crate::page::Block::Table {
            title,
            columns,
            rows,
        } if *title == crate::page::title::TYPES => Some((columns, rows)),
        _ => None,
    }
}

/// Returns the first letter of the type a row of the table of the types
/// names in its first cell.
#[cfg(feature = "markdown")]
fn row_letter(row: &[crate::page::Cell]) -> Option<u8> {
    use crate::page::{
        Cell,
        TypeRefPart, //
    };

    let Some(Cell::TypeRef { parts }) = row.first() else {
        return None;
    };
    parts.iter().find_map(|part| match part {
        TypeRefPart::Name { text, .. } => Some(first_letter(text)),
        _ => None,
    })
}

/// Writes the index chapter `chapter` after its import line: the title and
/// the list of the types, or, above the letter split, the title with its
/// sentence, and in a part of one letter that letter and its list.
#[cfg(feature = "typst")]
pub(super) fn typst_index(
    site: &Site<'_>,
    chapter: super::Chapter,
    out: &mut alloc::string::String,
) {
    use alloc::string::String;

    use super::Chapter;
    use crate::render_typst::typst_str;

    let types = documented_types(site.schema);
    match chapter {
        Chapter::IndexLetter(letter) => {
            let model = &site.schema.model;
            let part: Vec<_> = types
                .into_iter()
                .filter(|&t| first_letter(model.name(model.ty(t).name)) == letter)
                .collect();
            let shown = String::from(char::from(letter.to_ascii_uppercase()));
            typst_title(2, &shown, out);
            typst_table(site, &part, out);
        }
        _ if site.splits() => {
            typst_title(1, TITLE, out);
            out.push_str("#sv-p((");
            typst_str(BY_LETTER, out);
            out.push_str(",))\n");
        }
        _ => {
            typst_title(1, TITLE, out);
            typst_table(site, &types, out);
        }
    }
}

/// Writes a title of the outline at `level` that reads `text`.
#[cfg(feature = "typst")]
fn typst_title(level: u8, text: &str, out: &mut alloc::string::String) {
    use core::fmt::Write;

    use crate::render_typst::typst_str;

    // Writing into a `String` cannot fail.
    let _ = write!(out, "#sv-title({level}, (");
    typst_str(text, out);
    out.push_str(",))\n");
}

/// Writes the table of `types`, one row per type with its name linked to
/// its label, its kind and the page of its label.
///
/// Every type a page documents carries its anchor on that page, the title of
/// its own page or of its root page, or the cell of its row on the scalars or
/// the Relay page, so every label the table names is one the document holds.
#[cfg(feature = "typst")]
fn typst_table(site: &Site<'_>, types: &[crate::model::TypeId], out: &mut alloc::string::String) {
    use crate::{
        id::ElementId,
        page::kind_word,
        render_typst::typst_str, //
    };

    let model = &site.schema.model;
    out.push_str("#sv-table((), ((\"Type\",), (\"Kind\",), (\"Page\",)), (");
    for (i, &t) in types.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let ty = model.ty(t);
        let anchor = site.schema.anchors.of(&ElementId::of_type(model, t));
        out.push_str("\n  ((");
        if !anchor.is_empty() {
            out.push_str("sv-link(label(");
            typst_str(anchor, out);
            out.push_str("), (");
        }
        out.push_str("sv-code(");
        typst_str(model.name(ty.name), out);
        out.push(')');
        if !anchor.is_empty() {
            out.push_str(",))");
        }
        out.push_str(",), (");
        typst_str(kind_word(ty.kind), out);
        out.push_str(",), (");
        if !anchor.is_empty() {
            out.push_str("sv-stub(label(");
            typst_str(anchor, out);
            out.push_str(")),");
        }
        out.push_str("))");
    }
    if types.len() == 1 {
        out.push(',');
    }
    out.push_str("))\n");
}
