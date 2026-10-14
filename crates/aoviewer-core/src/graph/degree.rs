// SPDX-License-Identifier: MIT
/*
 * aoviewer-core graph degree: the neighbour counts a view and a page read.
 *
 * A type with fewer than two neighbours has no neighbourhood diagram.
 */

//! The degrees.
//!
//! The count of every node is computed once with the graph, from the other
//! ends of its outbound and inbound edges, so [`Graph::neighbours`] is a
//! lookup. The loops run over the nodes and edges of one graph.

use alloc::vec::Vec;

use super::{
    Graph,
    NodeId,
    id32, //
};
use crate::model::{
    Kind,
    Model, //
};

/// Returns the neighbour count of every node of `graph`, indexed by node.
///
/// A neighbour is another node joined by an edge in either direction, counted
/// once. A custom scalar is never counted as the neighbour of another node,
/// and a custom scalar's own count is the count of the nodes that use it.
pub(super) fn neighbours(graph: &Graph, model: &Model) -> Vec<u32> {
    let scalar: Vec<bool> = graph
        .nodes
        .iter()
        .map(|n| model.ty(n.ty).kind == Kind::Scalar)
        .collect();
    let mut others = Vec::new();
    graph
        .node_ids()
        .map(|n| {
            others.clear();
            others.extend(
                ends(graph, n).filter(|&m| m != n && (scalar[n.index()] || !scalar[m.index()])),
            );
            others.sort_unstable();
            others.dedup();
            id32(others.len())
        })
        .collect()
}

/// Returns the other end of every edge that leaves or enters `node`.
fn ends(graph: &Graph, node: NodeId) -> impl Iterator<Item = NodeId> + use<'_> {
    let outs = graph.outbound(node).iter().map(|&e| graph.edge(e).to);
    let ins = graph.inbound(node).iter().map(|&e| graph.edge(e).from);
    outs.chain(ins)
}
