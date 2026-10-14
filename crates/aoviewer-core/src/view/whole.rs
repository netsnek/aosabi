// SPDX-License-Identifier: MIT
/*
 * aoviewer-core view whole: the view of the whole graph from a root.
 *
 * The whole graph is the Voyager view and exists for parity.
 */

//! The whole graph view.
//!
//! `#/whole/<Root>` draws the types the walk of aoviewer reaches from the
//! root, ranked by their breadth first distance from it. The walk follows
//! every edge from its source to its target and an implements edge from the
//! interface to its implementation as well, which is the walk the graph
//! computes from all three roots, here from one root so the distance is known.
//! The walk reads no flag of the view, so `leaf=0`, which removes rows and no
//! type, keeps the node set and the ranks and takes only the edges of the leaf
//! rows out of the drawing. `#/whole/All` draws everything, the three roots at
//! rank 0, and a type no root reaches one rank after the last. The boxes
//! follow the order the walk settles them, which is rank order.
//!
//! The view draws every edge between two of its boxes that leaves no row
//! `leaf=0` hides. A box is at [`Level::L2`] when the view holds at most 80 of
//! them and at [`Level::L1`] above. The node set is the graph's, so
//! `nodes=composite` draws the composite types and the default of the option
//! grammar, `nodes=all`, draws every layer, which is 127 boxes for
//! `zitadel-gql`. The design opens the mode on the composite set with layer
//! chips for the inputs and the enums, and those chips are filters of the
//! viewer that set the node set before a view is asked for. Under `hideRoot=1`
//! the roots leave after the walk. The whole graph mode folds no
//! implementation in the viewer, since it is the Voyager view, and the first
//! reduction of a page folds them as in the ego and the cluster views.
//!
//! A walk settles each node once, so its loop runs over the nodes of one
//! graph.

use alloc::{
    vec,
    vec::Vec, //
};

use super::{
    Build,
    Level,
    NONE,
    Params,
    View,
    ViewError,
    ViewKind,
    WHOLE_FULL,
    drawn,
    is_root,
    is_scalar,
    rank32,
    reduce, //
};
use crate::{
    graph::{
        EdgeKind,
        NodeId, //
    },
    model::RootKind,
    route::WholeRoot,
    schema::Schema,
};

/// Returns the whole graph view of `root` under `params`.
///
/// # Errors
///
/// Returns [`ViewError::NoRoot`] when `root` names one root and the schema
/// declares none of that kind.
pub(super) fn whole(schema: &Schema, root: &WholeRoot, params: Params) -> Result<View, ViewError> {
    let Some(kind) = whole_kind(root) else {
        return Ok(every_root(schema, params));
    };
    let ty = schema
        .model
        .roots()
        .get(kind)
        .ok_or(ViewError::NoRoot(kind))?;
    let seeds: Vec<NodeId> = schema.graph.node_of(ty).into_iter().collect();
    Ok(walked(schema, &seeds, false, params))
}

/// Returns the whole graph of every root under `params`, which is
/// `#/whole/All`.
pub(super) fn every_root(schema: &Schema, params: Params) -> View {
    let mut seeds = Vec::new();
    for kind in [RootKind::Query, RootKind::Mutation, RootKind::Subscription] {
        let node = schema
            .model
            .roots()
            .get(kind)
            .and_then(|ty| schema.graph.node_of(ty));
        if let Some(node) = node.filter(|n| !seeds.contains(n)) {
            seeds.push(node);
        }
    }
    walked(schema, &seeds, true, params)
}

/// Returns the view of the types the walk reaches from `seeds`, and of every
/// other type when `all` is set.
///
/// The walk follows every edge whatever `leaf` says, and the drawing keeps the
/// edges [`drawn`] keeps.
fn walked(schema: &Schema, seeds: &[NodeId], all: bool, params: Params) -> View {
    let graph = &schema.graph;
    let len = graph.nodes.len();
    let mut rank = vec![NONE; len];
    let mut order = Vec::new();
    for &s in seeds {
        rank[s.index()] = 0;
        order.push(s);
    }
    for head in 0..len {
        let Some(&u) = order.get(head) else {
            break;
        };
        let next = rank[u.index()] + 1;
        let forward = graph
            .outbound(u)
            .iter()
            .map(|&e| graph.edges[e as usize].to);
        let backward = graph
            .inbound(u)
            .iter()
            .map(|&e| &graph.edges[e as usize])
            .filter(|e| e.kind == EdgeKind::Implements)
            .map(|e| e.from);
        for v in forward.chain(backward) {
            if rank[v.index()] == NONE && !is_scalar(schema, v) {
                rank[v.index()] = next;
                order.push(v);
            }
        }
    }
    if all {
        let last = order.last().map_or(0, |&n| rank[n.index()] + 1);
        for n in graph.node_ids() {
            if rank[n.index()] == NONE && !is_scalar(schema, n) {
                rank[n.index()] = last;
                order.push(n);
            }
        }
    }
    order.retain(|&n| !(schema.transforms.hide_root && is_root(schema, n)));
    let level = if order.len() <= WHOLE_FULL {
        Level::L2
    } else {
        Level::L1
    };
    let mut b = Build::new(schema, ViewKind::Whole, None);
    for &n in &order {
        b.push(n, level, rank32(rank[n.index()]));
    }
    for &n in &order {
        for &e in graph.outbound(n) {
            let edge = &graph.edges[e as usize];
            if drawn(schema, edge) && b.contains(edge.to) {
                b.edges.push(e);
            }
        }
    }
    let mut view = b.finish();
    if let Some(at) = params.fold.at(ViewKind::Whole) {
        reduce::fold(&mut view, schema, at);
    }
    view
}

/// Returns the root kind a whole graph route names, or `None` for all of
/// them.
fn whole_kind(root: &WholeRoot) -> Option<RootKind> {
    match root {
        WholeRoot::Query => Some(RootKind::Query),
        WholeRoot::Mutation => Some(RootKind::Mutation),
        WholeRoot::Subscription => Some(RootKind::Subscription),
        WholeRoot::All => None,
    }
}
