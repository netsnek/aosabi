// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool viewer view: the operation view, the boxes, edges, chips,
 * reductions and parts of the view of a route.
 *
 * A view is a node set, an edge set and a level per node, plus what the
 * budget removed.
 */

//! The `view` operation.
//!
//! The request `{input, route, routes, budget, declared}` answers `views`,
//! one entry per route with its `route` and its `parts`, or with `refused`
//! and `message` when the core refuses the route. The budget `page`, the
//! default for every route but the whole graph, answers the parts
//! [`View::for_page`] reduces and splits, and `whole` and `open` answer one
//! view of [`View::of`] under [`Budget::WHOLE`](aoviewer_core::view::Budget)
//! or under a budget that admits every view. The options of a route come
//! from its hash, and its schema is built under their transforms.
//!
//! A part carries `part`, its number and the count of the parts or null,
//! `empty`, why it draws nothing or null, `focus`, `nodes` as pairs of a
//! type and its level `L0` to `L3`, `edges` as `[from, row, to, kind]` with
//! the field of the row or null, `chips` and `reductions`.

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aoviewer_core::{
    graph::{
        EdgeKind,
        NodeId, //
    },
    schema::Schema,
    view::{
        Chip,
        Level,
        Reduction,
        View, //
    }, //
};

use super::{
    Stop,
    declared,
    each_route,
    empty_json,
    load,
    node_name,
    planned,
    refused_by,
    routes,
    view_refused,
    views, //
};

/// Answers the operation `view`.
pub(super) fn run(request: &JsObject) -> Result<JsObject, Stop> {
    let asked = planned(request, routes(request)?)?;
    let declared = declared(request)?;
    let model = load(request)?.model;
    let out = each_route(model, declared, &asked, |schema, clustering, (a, plan)| {
        let mut entry = match views(schema, clustering, a, *plan) {
            Ok(parts) => {
                let mut entry = JsObject::with_capacity(2);
                let parts = parts.iter().map(|v| view_json(schema, v)).collect();
                entry.insert("parts", JsValue::Array(parts));
                entry
            }
            Err(e) => refused_by(&view_refused(e)),
        };
        entry.insert("route", a.hash.as_str());
        JsValue::Object(entry)
    });
    let mut payload = JsObject::with_capacity(1);
    payload.insert("views", JsValue::Array(out));
    Ok(payload)
}

/// Returns one view of `schema` as JSON.
fn view_json(schema: &Schema, view: &View) -> JsValue {
    let name = |n: NodeId| JsValue::from(node_name(schema, n));
    let nodes = view
        .nodes
        .iter()
        .map(|&(n, level)| JsValue::Array(vec![name(n), JsValue::from(level_word(level))]))
        .collect();
    let edges = view.edges.iter().map(|&e| edge_json(schema, e)).collect();
    let chips = view.chips.iter().map(|c| chip_json(schema, c)).collect();
    let reductions = view
        .reductions
        .iter()
        .map(|&r| JsValue::from(reduction_word(r)))
        .collect();
    let mut out = JsObject::with_capacity(7);
    out.insert(
        "part",
        view.part.map_or(JsValue::Null, |(n, count)| {
            JsValue::Array(vec![
                JsValue::from(u32::from(n)),
                JsValue::from(u32::from(count)),
            ])
        }),
    );
    out.insert("empty", view.empty.map_or(JsValue::Null, empty_json));
    out.insert("focus", view.focus.map_or(JsValue::Null, name));
    out.insert("nodes", JsValue::Array(nodes));
    out.insert("edges", JsValue::Array(edges));
    out.insert("chips", JsValue::Array(chips));
    out.insert("reductions", JsValue::Array(reductions));
    JsValue::Object(out)
}

/// Returns the edge at `index` of the graph of `schema` as
/// `[from, row, to, kind]`.
fn edge_json(schema: &Schema, index: u32) -> JsValue {
    let Some(edge) = schema.graph.edges.get(index as usize) else {
        return JsValue::Null;
    };
    JsValue::Array(vec![
        JsValue::from(node_name(schema, edge.from)),
        row_json(schema, edge.from, edge.row),
        JsValue::from(node_name(schema, edge.to)),
        JsValue::from(edge_word(edge.kind)),
    ])
}

/// Returns the name of the field of the row `row` of the node `node`, or
/// null for no row.
fn row_json(schema: &Schema, node: NodeId, row: Option<u32>) -> JsValue {
    let model = &schema.model;
    row.and_then(|r| schema.graph.nodes[node.index()].rows.get(r as usize))
        .map_or(JsValue::Null, |row| {
            JsValue::from(model.name(model.field(row.field).name))
        })
}

/// Returns one chip of a view as JSON, its `kind` and its fields.
fn chip_json(schema: &Schema, chip: &Chip) -> JsValue {
    let name = |n: NodeId| JsValue::from(node_name(schema, n));
    let mut out = JsObject::with_capacity(4);
    match *chip {
        Chip::More { at, count, inbound } => {
            out.insert("kind", "more");
            out.insert("at", name(at));
            out.insert("count", count);
            out.insert("inbound", inbound);
        }
        Chip::Implementations { at, count } => {
            out.insert("kind", "implementations");
            out.insert("at", name(at));
            out.insert("count", count);
        }
        Chip::MoreFields { at, count } => {
            out.insert("kind", "more_fields");
            out.insert("at", name(at));
            out.insert("count", count);
        }
        Chip::Stub { from, row, to } => {
            out.insert("kind", "stub");
            out.insert("from", name(from));
            out.insert("row", row_json(schema, from, row));
            out.insert("to", name(to));
        }
    }
    JsValue::Object(out)
}

/// Returns the word of a level, `L0` to `L3`.
fn level_word(level: Level) -> &'static str {
    match level {
        Level::L0 => "L0",
        Level::L1 => "L1",
        Level::L2 => "L2",
        Level::L3 => "L3",
    }
}

/// Returns the word of the kind of an edge.
fn edge_word(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Field => "field",
        EdgeKind::Argument => "argument",
        EdgeKind::Implements => "implements",
        EdgeKind::UnionMember => "union_member",
    }
}

/// Returns the word of a reduction.
fn reduction_word(reduction: Reduction) -> &'static str {
    match reduction {
        Reduction::FoldedImplementers => "folded_implementers",
        Reduction::FoldedLeafRows => "folded_leaf_rows",
        Reduction::DroppedRing => "dropped_ring",
        Reduction::Split => "split",
    }
}
