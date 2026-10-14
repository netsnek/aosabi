// SPDX-License-Identifier: MIT
/*
 * aoviewer-core page home: the home page of a schema.
 *
 * The home page is the index a tree opens with.
 */

//! The home page.
//!
//! The home page holds the schema description, the overview diagram, the
//! counts, one table per root, the cluster table with the source that
//! produced it, and one alphabetical table of every type that has a page of
//! its own with its kind and its cluster. The counts are the declared types
//! in all and per kind, the boxes of the graph, the built in scalars, the
//! Relay wrappers the collapse folds, the three counts of reachability under
//! the walk of aoviewer with the sentence that says so, the deprecated
//! members and the clusters. The declared types and the deprecated members
//! both leave out the introspection types, which no page documents, so the
//! deprecated members are the rows of the deprecated page.
//!
//! A root table lists the fields of one root in the order of the transforms
//! with the type each shows and the first paragraph of its description, and
//! the root page holds the rest. The table of the types leaves out the built
//! in scalars and the Relay wrappers, which share the scalars page and the
//! Relay page, so it lists the boxes of the graph and the types the graph
//! hides by name.

use alloc::{
    string::{
        String,
        ToString, //
    },
    vec,
    vec::Vec, //
};

use super::{
    Block,
    Cell,
    Lint,
    OVERVIEW,
    PageModel,
    WALK_NOTE,
    by_name,
    cluster_page::{
        OTHER,
        clusters_table, //
    },
    empty,
    field_ref,
    has_lint,
    kind_word,
    link,
    page_of_type,
    push_description,
    push_diagrams,
    shown_type,
    source_note,
    summary_cell,
    title,
    type_link,
    type_ref_cell, //
};
use crate::{
    cluster::Clustering,
    id::ElementId,
    model::{
        FieldId,
        Kind,
        RootKind,
        TypeId, //
    },
    route::{
        Options,
        Route, //
    },
    schema::Schema,
    transform::Member,
    view::ViewError,
};

/// The kinds in the order the counts list them, with the label of each.
const KINDS: [(Kind, &str); 6] = [
    (Kind::Object, "Objects"),
    (Kind::Interface, "Interfaces"),
    (Kind::Union, "Unions"),
    (Kind::Enum, "Enums"),
    (Kind::Input, "Inputs"),
    (Kind::Scalar, "Scalars"),
];

/// The three roots with the title of their table.
const ROOTS: [(RootKind, &str); 3] = [
    (RootKind::Query, title::QUERY),
    (RootKind::Mutation, title::MUTATION),
    (RootKind::Subscription, title::SUBSCRIPTION),
];

/// Returns the home page.
///
/// # Errors
///
/// Returns the errors of [`View::for_page`](crate::view::View::for_page) for
/// the home route.
pub(super) fn home(
    schema: &Schema,
    clustering: &Clustering,
    options: &Options,
) -> Result<PageModel, ViewError> {
    let route = Route::Home;
    let mut blocks = Vec::new();
    push_description(schema.model.schema_description(), &mut blocks);
    push_diagrams(schema, clustering, &route, options, &mut blocks)?;
    blocks.push(counts(schema, clustering));
    blocks.push(Block::Note {
        text: String::from(WALK_NOTE),
    });
    push_root_tables(schema, &mut blocks);
    if !clustering.clusters.is_empty() || !clustering.other.is_empty() {
        blocks.push(source_note(clustering));
        blocks.push(clusters_table(clustering));
    }
    blocks.push(types_table(schema, clustering));
    blocks.push(Block::ViewerLink {
        route: route.clone(),
    });
    Ok(PageModel {
        route,
        title: String::from(OVERVIEW),
        kind: None,
        breadcrumb: Vec::new(),
        blocks,
    })
}

/// Returns the table of the counts of the home page.
fn counts(schema: &Schema, clustering: &Clustering) -> Block {
    let (model, tr, graph) = (&schema.model, &schema.transformed, &schema.graph);
    let declared: Vec<TypeId> = model
        .types()
        .iter()
        .copied()
        .filter(|&t| page_of_type(schema, t).is_some())
        .collect();
    let walks = graph.walks();
    let nodes = graph.nodes.len();
    let reachable = graph
        .node_ids()
        .filter(|&n| walks.aoviewer.contains(n))
        .count();
    let implementation_only = graph
        .node_ids()
        .filter(|&n| has_lint(schema, n, Lint::ImplementationOnly))
        .count();
    let mut rows = vec![row("Declared types", declared.len())];
    for (kind, label) in KINDS {
        let n = declared
            .iter()
            .filter(|&&t| model.ty(t).kind == kind)
            .count();
        rows.push(row(label, n));
    }
    let builtin = declared.iter().filter(|&&t| tr.is_builtin(t)).count();
    let relay = declared.iter().filter(|&&t| tr.is_relay_type(t)).count();
    rows.extend([
        row("Types in the graph", nodes),
        row("Built in scalars", builtin),
        row("Relay wrappers folded", relay),
        row("Reachable", reachable),
        row("Unreachable", nodes.saturating_sub(reachable)),
        row(title::IMPLEMENTATION_ONLY, implementation_only),
        row("Deprecated members", deprecated_members(schema)),
        row(title::CLUSTERS, clustering.clusters.len()),
    ]);
    Block::Table {
        title: title::COUNTS,
        columns: vec!["Count", "Number"],
        rows,
    }
}

/// Returns the number of deprecated members whose type a page documents, the
/// rows of the deprecated page. A member of an introspection type is left
/// out, as the declared types leave out its type.
fn deprecated_members(schema: &Schema) -> usize {
    let (model, tr) = (&schema.model, &schema.transformed);
    tr.marks
        .iter()
        .filter_map(|&(member, _)| match member {
            Member::Field(f) => Some(model.field(f).owner),
            Member::Arg(a) => {
                let f = model.arg(a).owner;
                (f != FieldId::DIRECTIVE).then(|| model.field(f).owner)
            }
            Member::Value(v) => Some(model.value(v).owner),
        })
        .filter(|&t| page_of_type(schema, t).is_some())
        .count()
}

/// Returns a row of the counts, its label and its number.
fn row(label: &str, n: usize) -> Vec<Cell> {
    vec![Cell::Text(String::from(label)), Cell::Text(n.to_string())]
}

/// Appends one table per root the schema declares, its fields with the type
/// each shows and the first paragraph of its description.
fn push_root_tables(schema: &Schema, blocks: &mut Vec<Block>) {
    let (model, tr) = (&schema.model, &schema.transformed);
    for (kind, table) in ROOTS {
        let Some(root) = model.roots().get(kind) else {
            continue;
        };
        let rows = tr
            .fields(model, root)
            .into_iter()
            .map(|f| {
                let (_, id) = field_ref(model, f);
                vec![
                    link(String::from(model.name(model.field(f).name)), id),
                    type_ref_cell(model, shown_type(schema, f)),
                    summary_cell(model.field(f).description.as_deref()),
                ]
            })
            .collect();
        blocks.push(Block::Table {
            title: table,
            columns: vec!["Field", "Type", "Description"],
            rows,
        });
    }
}

/// Returns the alphabetical table of every type that has a page of its own,
/// with its kind and its cluster.
fn types_table(schema: &Schema, clustering: &Clustering) -> Block {
    let model = &schema.model;
    let own = model
        .types()
        .iter()
        .copied()
        .filter(|&t| matches!(page_of_type(schema, t), Some(Route::Type { .. })));
    let mut other = clustering.other.clone();
    other.sort_unstable();
    let rows = by_name(schema, own)
        .into_iter()
        .map(|t| {
            let cluster = match clustering.of(t) {
                Some(i) => {
                    let c = &clustering.clusters[i as usize];
                    link(c.name.clone(), ElementId::Cluster(c.slug.clone()))
                }
                None if other.binary_search(&t).is_ok() => Cell::Text(String::from(OTHER)),
                None => empty(),
            };
            vec![
                type_link(model, t),
                Cell::Text(String::from(kind_word(model.ty(t).kind))),
                cluster,
            ]
        })
        .collect();
    Block::Table {
        title: title::TYPES,
        columns: vec!["Type", "Kind", "Cluster"],
        rows,
    }
}
