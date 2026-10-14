// SPDX-License-Identifier: MIT
/*
 * aoviewer-core graph adjacency: the outbound and inbound edge lists over one
 * edge index.
 *
 * The adjacency is built once per schema, which keeps the pass over every view
 * linear.
 */

//! The adjacency.
//!
//! Both directions are lists of positions in the one edge list of the
//! [`Graph`](super::Graph), grouped by node with a start per node, so a lookup
//! is a slice and costs nothing per call. A counting sort groups them, which
//! keeps the edge order inside each group. Its loops run over the nodes and
//! the edges of one graph.

use alloc::{
    vec,
    vec::Vec, //
};

use super::{
    Edge,
    NodeId,
    id32, //
};

/// The outbound and inbound edge indexes of every node.
#[derive(Clone, Debug, Default)]
pub(super) struct Adjacency {
    /// The start of each node's run inside `outs`, one per node and one more.
    out_starts: Vec<usize>,
    /// The indexes of the edges grouped by the node they leave.
    outs: Vec<u32>,
    /// The start of each node's run inside `ins`, one per node and one more.
    in_starts: Vec<usize>,
    /// The indexes of the edges grouped by the node they enter.
    ins: Vec<u32>,
}

impl Adjacency {
    /// Builds the adjacency of `edges` over `nodes` nodes.
    ///
    /// # Panics
    ///
    /// Panics when an edge names a node at or past `nodes`.
    pub(super) fn build(nodes: usize, edges: &[Edge]) -> Adjacency {
        let (out_starts, outs) = group(nodes, edges, |e| e.from);
        let (in_starts, ins) = group(nodes, edges, |e| e.to);
        Adjacency {
            out_starts,
            outs,
            in_starts,
            ins,
        }
    }

    /// Returns the indexes of the edges that leave `node`.
    pub(super) fn outbound(&self, node: NodeId) -> &[u32] {
        let i = node.index();
        &self.outs[self.out_starts[i]..self.out_starts[i + 1]]
    }

    /// Returns the indexes of the edges that enter `node`.
    pub(super) fn inbound(&self, node: NodeId) -> &[u32] {
        let i = node.index();
        &self.ins[self.in_starts[i]..self.in_starts[i + 1]]
    }
}

/// Returns the starts and the edge indexes of `edges` grouped by the node
/// `key` names, each group in edge order.
fn group(nodes: usize, edges: &[Edge], key: impl Fn(&Edge) -> NodeId) -> (Vec<usize>, Vec<u32>) {
    let mut starts = vec![0; nodes + 1];
    for e in edges {
        starts[key(e).index() + 1] += 1;
    }
    for i in 0..nodes {
        starts[i + 1] += starts[i];
    }
    let mut next = starts.clone();
    let mut list = vec![0; edges.len()];
    for (i, e) in edges.iter().enumerate() {
        let slot = &mut next[key(e).index()];
        list[*slot] = id32(i);
        *slot += 1;
    }
    (starts, list)
}
