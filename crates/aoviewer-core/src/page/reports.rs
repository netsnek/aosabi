// SPDX-License-Identifier: MIT
/*
 * aoviewer-core page reports: the report pages of the unreachable, deprecated,
 * scalar and Relay types.
 *
 * The documents unreachable.md, scalars.md and relay.md are report pages, and
 * so is the deprecated route.
 */

//! The report pages.
//!
//! - The unreachable page holds the types no root reaches under the walk of
//!   aoviewer and the two lints of `docs/aoviewer.md` section 2.3, so it
//!   doubles as a lint report. The rule of each lint is the one of the home
//!   page counts. Each list is a name grid in name order, written even when it
//!   is empty, so the report has one shape.
//! - The deprecated page lists every deprecated member with its reason, the
//!   types in name order and the members of a type in the order of the
//!   transforms, each field before its arguments and the values last. A member
//!   of an introspection type is left out, since no page documents its type,
//!   and the deprecated members the home page counts are the ones this page
//!   lists.
//! - The scalars page holds the built in scalars in one table and every custom
//!   scalar with the specification URL it declares in another. The built in
//!   scalars are documented there, so their table carries their anchors, while
//!   a custom scalar has a page of its own and its row links to it.
//! - The Relay page holds the wrapper types the collapse finds, one table row
//!   each with its role, the node type it wraps and its description. A wrapper
//!   names the role and the node of the first collapsed field that passes
//!   through it, in the order of the fields.

use alloc::{
    string::String,
    vec,
    vec::Vec, //
};

use super::{
    Block,
    Cell,
    Lint,
    PageModel,
    WALK_NOTE,
    by_name,
    description_cell,
    empty,
    has_lint,
    overview_crumb,
    page_of_type,
    push_details,
    sort_names,
    summary_cell,
    title,
    type_link,
    type_page::deprecated_rows, //
};
use crate::{
    graph::NodeId,
    id::ElementId,
    model::{
        Kind,
        TypeId, //
    },
    route::Route,
    schema::Schema,
};

/// Returns the unreachable page.
pub(super) fn unreachable(schema: &Schema) -> PageModel {
    let (model, graph) = (&schema.model, &schema.graph);
    let walks = graph.walks();
    let grid = |title: &'static str, keep: &dyn Fn(NodeId) -> bool| {
        let mut names: Vec<(String, ElementId)> = graph
            .node_ids()
            .filter(|&n| keep(n))
            .map(|n| {
                let ty = graph.nodes[n.index()].ty;
                (
                    String::from(model.name(model.ty(ty).name)),
                    ElementId::of_type(model, ty),
                )
            })
            .collect();
        sort_names(&mut names);
        Block::Grid { title, names }
    };
    let blocks = vec![
        Block::Note {
            text: String::from(WALK_NOTE),
        },
        grid(title::UNREACHABLE, &|n| !walks.aoviewer.contains(n)),
        grid(title::IMPLEMENTATION_ONLY, &|n| {
            has_lint(schema, n, Lint::ImplementationOnly)
        }),
        grid(title::NOT_VOYAGER, &|n| {
            has_lint(schema, n, Lint::NotVoyager)
        }),
        Block::ViewerLink {
            route: Route::Unreachable,
        },
    ];
    report(Route::Unreachable, "Unreachable types", blocks)
}

/// Returns the deprecated page, which leaves out the members of the types no
/// page documents, the introspection types.
pub(super) fn deprecated(schema: &Schema) -> PageModel {
    let model = &schema.model;
    let documented = model
        .types()
        .iter()
        .copied()
        .filter(|&t| page_of_type(schema, t).is_some());
    let rows: Vec<Vec<Cell>> = by_name(schema, documented)
        .into_iter()
        .flat_map(|t| deprecated_rows(schema, t, true))
        .collect();
    let blocks = vec![
        Block::Table {
            title: title::DEPRECATED_MEMBERS,
            columns: vec!["Member", "Reason"],
            rows,
        },
        Block::ViewerLink {
            route: Route::Deprecated,
        },
    ];
    report(Route::Deprecated, "Deprecated members", blocks)
}

/// Returns the scalars page.
pub(super) fn scalars(schema: &Schema) -> PageModel {
    let (model, tr) = (&schema.model, &schema.transformed);
    let scalars = by_name(
        schema,
        model
            .types()
            .iter()
            .copied()
            .filter(|&t| model.ty(t).kind == Kind::Scalar),
    );
    let mut details = Vec::new();
    let mut builtin = Vec::new();
    let mut custom = Vec::new();
    for t in scalars {
        let ty = model.ty(t);
        if tr.is_builtin(t) {
            let id = ElementId::of_type(model, t);
            builtin.push(vec![
                Cell::Name {
                    text: String::from(model.name(ty.name)),
                    id: id.clone(),
                },
                description_cell(ty.description.as_deref(), &id, &mut details),
            ]);
        } else {
            let url = ty.specified_by_url.clone().unwrap_or_default();
            custom.push(vec![
                type_link(model, t),
                Cell::Text(url),
                summary_cell(ty.description.as_deref()),
            ]);
        }
    }
    let mut blocks = vec![Block::Table {
        title: title::BUILTIN_SCALARS,
        columns: vec!["Scalar", "Description"],
        rows: builtin,
    }];
    push_details(details, &mut blocks);
    blocks.push(Block::Table {
        title: title::CUSTOM_SCALARS,
        columns: vec!["Scalar", "Specification", "Description"],
        rows: custom,
    });
    blocks.push(Block::ViewerLink {
        route: Route::Scalars,
    });
    report(Route::Scalars, "Scalars", blocks)
}

/// Returns the Relay page.
pub(super) fn relay(schema: &Schema) -> PageModel {
    let (model, tr) = (&schema.model, &schema.transformed);
    let mut blocks = Vec::new();
    if !schema.transforms.relay {
        blocks.push(Block::Note {
            text: String::from(
                "The Relay collapse is off, so every wrapper type has a page of its own.",
            ),
        });
    }
    let wrappers = by_name(
        schema,
        model
            .types()
            .iter()
            .copied()
            .filter(|&t| tr.is_relay_type(t)),
    );
    let mut details = Vec::new();
    let rows = wrappers
        .into_iter()
        .map(|t| {
            let ty = model.ty(t);
            let id = ElementId::of_type(model, t);
            let (role, node) = role_of(schema, t);
            vec![
                Cell::Name {
                    text: String::from(model.name(ty.name)),
                    id: id.clone(),
                },
                Cell::Text(String::from(role)),
                node.map_or_else(empty, |n| type_link(model, n)),
                description_cell(ty.description.as_deref(), &id, &mut details),
            ]
        })
        .collect();
    blocks.push(Block::Table {
        title: title::RELAY_WRAPPERS,
        columns: vec!["Type", "Role", "Node", "Description"],
        rows,
    });
    push_details(details, &mut blocks);
    blocks.push(Block::ViewerLink {
        route: Route::Relay,
    });
    report(Route::Relay, "Relay wrappers", blocks)
}

/// Returns the role of the Relay type `ty` and the node type it wraps, from
/// the first collapsed field that passes through it, or the empty role for a
/// Relay type no collapsed field names.
fn role_of(schema: &Schema, ty: TypeId) -> (&'static str, Option<TypeId>) {
    for c in &schema.transformed.collapsed {
        let node = Some(c.node.named());
        if c.connection == ty {
            return ("connection", node);
        }
        if c.edge == ty {
            return ("edge", node);
        }
        if c.page_info == Some(ty) {
            return ("page info", None);
        }
    }
    ("", None)
}

/// Returns a report page of `route` under `title` with `blocks`.
fn report(route: Route, title: &str, blocks: Vec<Block>) -> PageModel {
    PageModel {
        route,
        title: String::from(title),
        kind: None,
        breadcrumb: overview_crumb(),
        blocks,
    }
}
