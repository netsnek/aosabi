// SPDX-License-Identifier: MIT
/*
 * aoviewer-core cluster territory: the clusters of the root field territories.
 *
 * Each type belongs to the query root field whose breadth first walk reaches
 * it first.
 */

//! The territory clusters.
//!
//! Every field of the query root seeds one territory, named after the field,
//! with its return type, and a root field that returns an interface with more
//! than eight implementers, which is the node by id shape, seeds no territory.
//!
//! One breadth first walk runs from every return type at once, and a node
//! joins the territory that reaches it at the smallest distance, the first
//! root field in name order on a tie. A second walk of the same kind then runs
//! from the argument types of the same root fields over the nodes the first
//! walk leaves, so an input only a root field takes joins the territory of
//! that field rather than staying outside. Both walks follow field, argument
//! and union member edges forward and an implements edge backward, from an
//! interface to its implementations, so an object never reaches the interfaces
//! it implements. They never enter a root.
//!
//! Each interface then joins the territory that holds the most of its
//! implementers. The first territory in name order wins a tie, and an
//! interface none of whose implementers the walks place keeps what the walks
//! gave it. The nodes neither the walks nor the vote place stay outside, and
//! the territories under three members merge as [`super`] describes.
//!
//! The walks also rank every node, in the order they settle them, the first
//! walk before the second, a level at a time and inside a level by territory,
//! then by kind in the order of [`Kind`], then by name, with the nodes neither
//! walk reaches after them in name order. A split cluster follows that rank,
//! and the kind before the name lets an object rather than a scalar or an enum
//! head a part and name it.
//!
//! A walk runs a level per iteration and settles at least one node per level,
//! so its loop runs over the nodes of one graph.

use alloc::{
    string::String,
    vec,
    vec::Vec, //
};

use super::{
    OUTSIDE,
    Placement,
    id32,
    sort_by_name,
    type_of, //
};
use crate::{
    graph::{
        EdgeKind,
        Graph,
        NodeId, //
    },
    model::{
        Kind,
        TypeId, //
    },
    schema::Schema,
    sort::name_order,
};

/// The implementers above which an interface a root field returns seeds no
/// territory, the node by id shape.
const NODE_BY_ID_IMPLEMENTERS: usize = 8;

/// The seeds of the territories.
#[derive(Clone, Debug, Default)]
struct Seeds {
    /// The names of the territories in byte order.
    names: Vec<String>,
    /// The return type of every root field that seeds, as a territory and a
    /// node.
    returns: Vec<(u32, NodeId)>,
    /// The argument types of every root field that seeds, as a territory and
    /// a node.
    arguments: Vec<(u32, NodeId)>,
}

/// Returns the territories of `schema` with the breadth first rank of every
/// node, indexed by node.
///
/// The territories come in the byte order of their root field names, the
/// small ones merged into their neighbours.
pub(super) fn place(schema: &Schema) -> (Placement, Vec<u32>) {
    let graph = &schema.graph;
    let seeds = seeds(schema);
    let mut placement = Placement::new(seeds.names, graph.nodes.len());
    let mut order = walk(schema, &seeds.returns, &mut placement.at);
    let left: Vec<(u32, NodeId)> = seeds
        .arguments
        .into_iter()
        .filter(|&(_, n)| placement.at[n.index()] == OUTSIDE)
        .collect();
    order.extend(walk(schema, &left, &mut placement.at));
    let rank = rank(schema, &order);
    vote(schema, &mut placement.at);
    placement.merge_small(graph);
    (placement, rank)
}

/// Returns the names of the territories in byte order, the return type of
/// every root field that seeds and its argument types.
fn seeds(schema: &Schema) -> Seeds {
    let (model, graph) = (&schema.model, &schema.graph);
    let Some(query) = model.roots().query.and_then(|t| graph.node_of(t)) else {
        return Seeds::default();
    };
    let rows = &graph.nodes[query.index()].rows;
    let mut fields: Vec<(&str, usize)> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.target.is_some_and(|t| is_node_by_id(schema, t)))
        .map(|(r, row)| (model.name(model.field(row.field).name), r))
        .collect();
    fields.sort_unstable();
    let mut territory = vec![OUTSIDE; rows.len()];
    for (t, &(_, r)) in fields.iter().enumerate() {
        territory[r] = id32(t);
    }
    let mut seeds = Seeds {
        names: fields
            .into_iter()
            .map(|(name, _)| String::from(name))
            .collect(),
        ..Seeds::default()
    };
    let edges = graph
        .outbound(query)
        .iter()
        .map(|&e| &graph.edges[e as usize]);
    for e in edges {
        let list = match e.kind {
            EdgeKind::Field => &mut seeds.returns,
            EdgeKind::Argument => &mut seeds.arguments,
            EdgeKind::Implements | EdgeKind::UnionMember => continue,
        };
        let Some(row) = e.row else {
            continue;
        };
        let t = territory[row as usize];
        if t != OUTSIDE && !graph.nodes[e.to.index()].is_root {
            list.push((t, e.to));
        }
    }
    seeds
}

/// Returns whether the type `ty` is an interface with more than
/// [`NODE_BY_ID_IMPLEMENTERS`] implementers.
fn is_node_by_id(schema: &Schema, ty: TypeId) -> bool {
    let model = &schema.model;
    model.ty(ty).kind == Kind::Interface && model.implementers(ty).len() > NODE_BY_ID_IMPLEMENTERS
}

/// Walks breadth first from every seed of `seeds` at once over the nodes `at`
/// leaves outside, writes into `at` the territory of every node the walk
/// reaches, and returns the nodes in the order the walk settles them.
///
/// Every seed is outside in `at`. A node reached at the same distance by two
/// territories joins the first.
fn walk(schema: &Schema, seeds: &[(u32, NodeId)], at: &mut [u32]) -> Vec<NodeId> {
    let (model, graph) = (&schema.model, &schema.graph);
    let mut claim = vec![OUTSIDE; at.len()];
    let mut level = Vec::new();
    for &(t, n) in seeds {
        offer(&mut claim, &mut level, n, t);
    }
    let mut order = Vec::with_capacity(at.len());
    for _ in 0..at.len() {
        if level.is_empty() {
            break;
        }
        for n in &level {
            at[n.index()] = claim[n.index()];
        }
        level.sort_by(|&a, &b| {
            let (ta, tb) = (type_of(graph, a), type_of(graph, b));
            at[a.index()]
                .cmp(&at[b.index()])
                .then(model.ty(ta).kind.cmp(&model.ty(tb).kind))
                .then_with(|| name_order(model, ta, tb))
        });
        let mut next = Vec::new();
        for &n in &level {
            for m in steps(graph, n) {
                if at[m.index()] == OUTSIDE && !graph.nodes[m.index()].is_root {
                    offer(&mut claim, &mut next, m, at[n.index()]);
                }
            }
        }
        order.append(&mut level);
        level = next;
    }
    order
}

/// Offers the node `n` to the territory `t`, which claims it unless a
/// territory before it in name order already does, and adds `n` to `level`
/// the first time it is claimed.
fn offer(claim: &mut [u32], level: &mut Vec<NodeId>, n: NodeId, t: u32) {
    let slot = &mut claim[n.index()];
    if *slot == OUTSIDE {
        level.push(n);
    }
    *slot = (*slot).min(t);
}

/// Returns the nodes one step of the territory walk leads to from `node`.
fn steps(graph: &Graph, node: NodeId) -> impl Iterator<Item = NodeId> + use<'_> {
    let forward = graph
        .outbound(node)
        .iter()
        .map(|&e| &graph.edges[e as usize])
        .filter(|e| e.kind != EdgeKind::Implements)
        .map(|e| e.to);
    let backward = graph
        .inbound(node)
        .iter()
        .map(|&e| &graph.edges[e as usize])
        .filter(|e| e.kind == EdgeKind::Implements)
        .map(|e| e.from);
    forward.chain(backward)
}

/// Returns the rank of every node, the position of the nodes of `order` in
/// it, then the nodes the walks never reach in name order.
fn rank(schema: &Schema, order: &[NodeId]) -> Vec<u32> {
    let graph = &schema.graph;
    let mut rank = vec![OUTSIDE; graph.nodes.len()];
    for (i, n) in order.iter().enumerate() {
        rank[n.index()] = id32(i);
    }
    let mut rest: Vec<NodeId> = graph
        .node_ids()
        .filter(|n| rank[n.index()] == OUTSIDE)
        .collect();
    sort_by_name(schema, &mut rest);
    for (i, n) in rest.iter().enumerate() {
        rank[n.index()] = id32(order.len() + i);
    }
    rank
}

/// Moves every interface into the territory that holds the most of its
/// implementers in `at`, the first on a tie.
///
/// The votes count the territories the walks gave, so the order in which the
/// interfaces vote changes nothing.
fn vote(schema: &Schema, at: &mut [u32]) {
    let (model, graph) = (&schema.model, &schema.graph);
    let walked = at.to_vec();
    let mut held = Vec::new();
    for n in graph.node_ids() {
        let ty = type_of(graph, n);
        if model.ty(ty).kind != Kind::Interface || graph.nodes[n.index()].is_root {
            continue;
        }
        held.clear();
        held.extend(
            model
                .implementers(ty)
                .iter()
                .filter_map(|&i| graph.node_of(i))
                .map(|i| walked[i.index()])
                .filter(|&t| t != OUTSIDE),
        );
        if let Some(t) = most(&mut held) {
            at[n.index()] = t;
        }
    }
}

/// Returns the value `values` holds most often, the smallest on a tie, and
/// sorts `values` on the way.
fn most(values: &mut [u32]) -> Option<u32> {
    values.sort_unstable();
    let mut best: Option<(usize, u32)> = None;
    for run in values.chunk_by(|a, b| a == b) {
        if best.is_none_or(|(count, _)| run.len() > count) {
            best = Some((run.len(), run[0]));
        }
    }
    best.map(|(_, value)| value)
}
