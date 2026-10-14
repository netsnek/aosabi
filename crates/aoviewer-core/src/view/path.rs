// SPDX-License-Identifier: MIT
/*
 * aoviewer-core view path: the shortest path between two types.
 *
 * A path route draws the shortest path between two types.
 */

//! The path view.
//!
//! A breadth first search runs over field, argument, union member and
//! implements edges, first along their direction and then, when no directed
//! path exists, in both directions, and the chain is drawn left to right at
//! [`Level::L1`] with the rows that link it. The first type is the focus at
//! rank 0 and each next type one rank to the right. An edge the search
//! follows against its direction is drawn as it is, so its head points back
//! along the chain, which is the mark of its direction.
//!
//! The search takes the edges of a node in the order of the graph, so of the
//! shortest paths it finds the first in that order, and the other shortest
//! paths the status line offers are the viewer's. A custom scalar ends a path
//! and never passes one on, because it is a chip inside a row. Under
//! `hideRoot=1` a root is the same, a type the route may name as an end and
//! no search passes through, because no root is a box of a view but the ones
//! a route names. Two types without a path answer an empty view.
//!
//! A search settles each node once and the walk back takes one edge per
//! settled node, so both loops run over the nodes of one graph.

use alloc::{
    vec,
    vec::Vec, //
};

use super::{
    Bits,
    Build,
    EmptyReason,
    Level,
    NONE,
    View,
    ViewError,
    ViewKind,
    drawn,
    id32,
    is_box,
    rank32,
    type_named, //
};
use crate::{
    graph::NodeId,
    schema::Schema, //
};

/// Returns the path view from the type called `from` to the type called `to`.
///
/// # Errors
///
/// Returns [`ViewError::UnknownType`] when the schema declares no type of one
/// of the two names.
pub(super) fn path(schema: &Schema, from: &str, to: &str) -> Result<View, ViewError> {
    let (model, graph) = (&schema.model, &schema.graph);
    let first = type_named(model, from)?;
    let last = type_named(model, to)?;
    let (Some(first), Some(last)) = (graph.node_of(first), graph.node_of(last)) else {
        return Ok(View::empty(ViewKind::Path, EmptyReason::Unreachable));
    };
    let steps =
        shortest(schema, first, last, false).or_else(|| shortest(schema, first, last, true));
    let Some(steps) = steps else {
        return Ok(View::empty(ViewKind::Path, EmptyReason::Unreachable));
    };
    let mut b = Build::new(schema, ViewKind::Path, Some(first));
    b.push(first, Level::L1, 0);
    let mut at = first;
    for (i, &e) in steps.iter().enumerate() {
        let edge = &graph.edges[e as usize];
        at = if edge.from == at { edge.to } else { edge.from };
        b.push(at, Level::L1, rank32(id32(i + 1)));
        b.edges.push(e);
    }
    Ok(b.finish())
}

/// Returns the edges of the shortest path from `first` to `last`, along the
/// direction of the edges or, when `undirected` is set, in both directions.
///
/// The search goes on from `first` and from no other node that is no box of
/// a view, a custom scalar or a root under `hideRoot=1`, so such a node is
/// never inside the path.
fn shortest(schema: &Schema, first: NodeId, last: NodeId, undirected: bool) -> Option<Vec<u32>> {
    let graph = &schema.graph;
    let len = graph.nodes.len();
    let mut via = vec![NONE; len];
    let mut seen = Bits::new(len);
    seen.insert(first);
    let mut queue = vec![first];
    for head in 0..len {
        let Some(&u) = queue.get(head) else {
            break;
        };
        if u == last {
            break;
        }
        if u != first && !is_box(schema, u) {
            continue;
        }
        let forward = graph
            .outbound(u)
            .iter()
            .map(|&e| (e, graph.edges[e as usize].to));
        let backward = graph
            .inbound(u)
            .iter()
            .filter(|_| undirected)
            .map(|&e| (e, graph.edges[e as usize].from));
        for (e, v) in forward.chain(backward) {
            if drawn(schema, &graph.edges[e as usize]) && seen.insert(v) {
                via[v.index()] = e;
                queue.push(v);
            }
        }
    }
    if !seen.contains(last) {
        return None;
    }
    let mut steps = Vec::new();
    let mut at = last;
    for _ in 0..len {
        if at == first {
            break;
        }
        let e = via[at.index()];
        steps.push(e);
        let edge = &graph.edges[e as usize];
        at = if edge.to == at { edge.from } else { edge.to };
    }
    steps.reverse();
    Some(steps)
}
