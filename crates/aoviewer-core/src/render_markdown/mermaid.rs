// SPDX-License-Identifier: MIT
/*
 * aoviewer-core render_markdown mermaid: the Mermaid form of a small view.
 *
 * A fenced class diagram stands beside a neighbourhood diagram of at most 12
 * boxes and nowhere else.
 */

//! The Mermaid writer.
//!
//! [`render_mermaid`] writes a [`View`] as the body of a Mermaid class
//! diagram, the text the Markdown writer puts in a fenced block with the info
//! string `mermaid`. Mermaid is an output format here and never a dependency.
//!
//! A view of more than 12 boxes is refused, since the diagram stands only
//! beside a neighbourhood diagram of at most 12 boxes, and so is a view that
//! draws nothing. A body over 400 relations or 45,000 bytes is refused as
//! well, under the caps of 500 edges and 50,000 characters Mermaid holds by
//! default.
//!
//! A box is a class whose id is its position, `n0` for the first box of the
//! view, with the name of its type as the label, so a type name that Mermaid
//! reads as a keyword, such as `class` or `note`, stays a name. A type that
//! is no object carries its kind as an annotation. The group box of folded
//! implementations is a class `g0` and up that realizes its interface. An
//! edge of a field is an association and one of an argument a dependency,
//! each labelled with the name of its field, an implementation a realization,
//! and a union member inherits from its union. A box carries no members,
//! since the edges name every field that joins two boxes and the table under
//! the diagram lists every member. The cluster map is one class `c0` and up
//! per cluster box, labelled with its name and its count of types, and one
//! link per trunk, labelled with its count of edges.
//!
//! A label keeps letters, digits, spaces and `_ . , ( ) -`, and writes every
//! other character as the entity code `#<decimal>;`, which Mermaid reads as
//! that character.
//!
//! Every loop runs over the at most `MAX_BOXES` boxes, the chips and the
//! edges of one view, over the cluster boxes and trunks of one map, or over
//! the characters of one label.

use alloc::string::String;
use core::fmt::Write;

use crate::{
    graph::{
        EdgeKind,
        NodeId, //
    },
    model::Kind,
    page::kind_word,
    schema::Schema,
    view::{
        Chip,
        View,
        ViewKind, //
    },
};

/// The most boxes a view the writer draws holds.
const MAX_BOXES: u32 = 12;

/// The most relations a body holds.
const MAX_RELATIONS: usize = 400;

/// The most bytes a body holds.
const MAX_BYTES: usize = 45_000;

/// The reason [`render_mermaid`] refuses a view, with the boxes the view
/// holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MermaidRefused {
    /// The boxes of the view, its type boxes and its group boxes, or its
    /// cluster boxes on the cluster map.
    pub boxes: u32,
}

/// Writes `view` as the body of a Mermaid class diagram into `out`.
///
/// The body starts with `classDiagram` and ends with a line feed, and the
/// caller puts it in a fenced block. `schema` is the bundle the view was
/// built from.
///
/// # Errors
///
/// Returns [`MermaidRefused`] with the boxes of the view when it holds more
/// than 12 boxes or none, or when its body would hold more than 400
/// relations or 45,000 bytes, and leaves `out` as it was.
///
/// # Panics
///
/// Panics when `view` was not built from the graph of `schema`.
pub fn render_mermaid(
    view: &View,
    schema: &Schema,
    out: &mut String,
) -> Result<(), MermaidRefused> {
    let boxes = boxes(view);
    let refused = MermaidRefused { boxes };
    if view.empty.is_some() || boxes == 0 || boxes > MAX_BOXES {
        return Err(refused);
    }
    let mark = out.len();
    out.push_str("classDiagram\n  direction LR\n");
    let relations = if view.kind == ViewKind::ClusterMap {
        map(view, out)
    } else {
        types(view, schema, out)
    };
    if relations > MAX_RELATIONS || out.len() - mark > MAX_BYTES {
        out.truncate(mark);
        return Err(refused);
    }
    Ok(())
}

/// Returns the boxes `view` draws, its cluster boxes on the cluster map and
/// its type boxes and group boxes of implementations otherwise, which is how
/// the budget of a view counts them.
fn boxes(view: &View) -> u32 {
    let count = if view.kind == ViewKind::ClusterMap {
        view.groups.len()
    } else {
        let groups = view
            .chips
            .iter()
            .filter(|c| matches!(c, Chip::Implementations { .. }))
            .count();
        view.nodes.len() + groups
    };
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Writes the boxes and edges of a view of types and returns the count of
/// its relations.
fn types(view: &View, schema: &Schema, out: &mut String) -> usize {
    let (model, graph) = (&schema.model, &schema.graph);
    for (i, (node, _)) in view.nodes.iter().enumerate() {
        let ty = model.ty(graph.nodes[node.index()].ty);
        // Writing into a `String` cannot fail.
        let _ = write!(out, "  class n{i}[\"");
        label(model.name(ty.name), out);
        out.push_str("\"]\n");
        if ty.kind != Kind::Object {
            let _ = writeln!(out, "  <<{}>> n{i}", kind_word(ty.kind));
        }
    }
    let mut relations = 0;
    let groups = view.chips.iter().filter_map(|c| match c {
        Chip::Implementations { at, count } => Some((*at, *count)),
        _ => None,
    });
    for (g, (at, count)) in groups.enumerate() {
        let _ = writeln!(out, "  class g{g}[\"{count} implementations\"]");
        if let Some(i) = position(view, at) {
            let _ = writeln!(out, "  g{g} ..|> n{i}");
            relations += 1;
        }
    }
    for &e in &view.edges {
        let edge = &graph.edges[e as usize];
        let (Some(a), Some(b)) = (position(view, edge.from), position(view, edge.to)) else {
            continue;
        };
        let field = edge
            .row
            .and_then(|r| graph.nodes[edge.from.index()].rows.get(r as usize))
            .map(|row| model.name(model.field(row.field).name));
        let _ = match edge.kind {
            EdgeKind::Field => write!(out, "  n{a} --> n{b}"),
            EdgeKind::Argument => write!(out, "  n{a} ..> n{b}"),
            EdgeKind::Implements => write!(out, "  n{a} ..|> n{b}"),
            EdgeKind::UnionMember => write!(out, "  n{a} <|-- n{b}"),
        };
        if let Some(field) = field {
            out.push_str(" : ");
            label(field, out);
        }
        out.push('\n');
        relations += 1;
    }
    relations
}

/// Writes the cluster boxes and trunks of the cluster map and returns the
/// count of its relations.
fn map(view: &View, out: &mut String) -> usize {
    for (i, group) in view.groups.iter().enumerate() {
        // Writing into a `String` cannot fail.
        let _ = write!(out, "  class c{i}[\"");
        label(&group.name, out);
        let _ = writeln!(out, " ({} types)\"]", group.types);
    }
    for trunk in &view.trunks {
        let _ = writeln!(out, "  c{} -- c{} : {}", trunk.a, trunk.b, trunk.count);
    }
    view.trunks.len()
}

/// Returns the position of the box of `node` among the boxes of `view`.
fn position(view: &View, node: NodeId) -> Option<usize> {
    view.nodes.iter().position(|(n, _)| *n == node)
}

/// Writes `text` as a label, every character outside the plain ones as its
/// entity code.
fn label(text: &str, out: &mut String) {
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '.' | ',' | '(' | ')' | '-') {
            out.push(c);
        } else {
            // Writing into a `String` cannot fail.
            let _ = write!(out, "#{};", u32::from(c));
        }
    }
}
