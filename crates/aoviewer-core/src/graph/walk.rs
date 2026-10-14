// SPDX-License-Identifier: MIT
/*
 * aoviewer-core graph walk: the three walks of reachability as bit sets over
 * the nodes.
 *
 * The walk of aoviewer, the walk of Voyager and the strict walk are computed
 * once, with the adjacency.
 */

//! The walks.
//!
//! [`Walks::compute`] seeds the three walks from the same roots, every root
//! that is a node, and they differ in the edges they follow, with the walk of
//! Voyager entering objects, interfaces and unions alone as well. The walk of
//! aoviewer and the strict walk start from all three roots, and giving the
//! walk of Voyager the same seeds is a choice of this module.
//!
//! - The walk of aoviewer follows every edge forward, field, argument,
//!   implements and union member, and an implements edge backward as well,
//!   from an interface to its implementations. It is the definition of
//!   reachable every count of the viewer and the documents uses.
//! - The walk of Voyager follows field and union member edges forward and an
//!   implements edge backward, from an interface to the interfaces and objects
//!   implementing it, and it enters objects, interfaces and unions alone,
//!   which is Voyager's walk as implemented. An object never reaches the
//!   interfaces it implements, and no argument is followed.
//! - The strict walk follows field and implements edges forward alone, so an
//!   object reaches its interfaces and an interface never reaches its
//!   implementations. A type the walk of aoviewer reaches and the strict walk
//!   does not is reached only as an implementation, which is one of the two
//!   lints of the section.
//!
//! Voyager seeds one root at a time, the query root by default or the root a
//! reader picks, so it never seeds the mutation root beside the query root.
//! [󰀀](super::Graph::voyager_form) walks from one root. [`Walks::voyager`]
//! seeds the query and the mutation root, because the exit number of the walks
//! counts 36 of 77 under stock Voyager with both roots seeded, and the
//! subscription root as well, as the other two walks do, so the three sets
//! answer over the same seeds.
//!
//! A walk runs over the graph it is given. The strict walk over the
//! uncollapsed node set of the section is the strict walk of a graph built
//! under `relay=0`.
//!
//! A node enters the stack of a walk once, when it enters the set, so a walk
//! pops at most one entry per node and its loop runs over the nodes of one
//! graph.
//!
//! [type-graph.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/graph/type-graph.ts

use alloc::{
    vec,
    vec::Vec, //
};

use super::{
    EdgeKind,
    Graph,
    NodeId,
    is_composite, //
};
use crate::model::{
    Model,
    RootKind, //
};

/// The three walks of `docs/aoviewer.md` section 2.3, each a bit set over the
/// nodes, computed once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Walks {
    /// The walk of aoviewer, the definition of reachable.
    pub aoviewer: ReachSet,
    /// Voyager's edge rule, seeded from every root.
    pub voyager: ReachSet,
    /// The strict walk over field and implements edges.
    pub strict: ReachSet,
}

/// A set of nodes as a bit set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReachSet {
    /// One bit per node, the node of id `i` at bit `i % 64` of word `i / 64`.
    bits: Vec<u64>,
}

/// The edges one walk follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rule {
    /// The walk of aoviewer.
    Aoviewer,
    /// Voyager's walk.
    Voyager,
    /// The strict walk.
    Strict,
}

impl Rule {
    /// Returns whether the walk follows an edge of `kind` from its source to
    /// its target.
    fn forward(self, kind: EdgeKind) -> bool {
        match self {
            Rule::Aoviewer => true,
            Rule::Voyager => matches!(kind, EdgeKind::Field | EdgeKind::UnionMember),
            Rule::Strict => matches!(kind, EdgeKind::Field | EdgeKind::Implements),
        }
    }

    /// Returns whether the walk follows an edge of `kind` from its target to
    /// its source.
    fn backward(self, kind: EdgeKind) -> bool {
        kind == EdgeKind::Implements && self != Rule::Strict
    }
}

impl Walks {
    /// Computes the three walks of `graph`, whose nodes are types of `model`,
    /// from every root that is a node.
    pub(super) fn compute(graph: &Graph, model: &Model) -> Walks {
        let seeds: Vec<NodeId> = [RootKind::Query, RootKind::Mutation, RootKind::Subscription]
            .into_iter()
            .filter_map(|kind| model.roots().get(kind))
            .filter_map(|ty| graph.node_of(ty))
            .collect();
        Walks {
            aoviewer: walk(graph, model, &seeds, Rule::Aoviewer),
            voyager: walk(graph, model, &seeds, Rule::Voyager),
            strict: walk(graph, model, &seeds, Rule::Strict),
        }
    }
}

impl ReachSet {
    /// Returns the empty set over `len` nodes.
    fn empty(len: usize) -> ReachSet {
        ReachSet {
            bits: vec![0; len.div_ceil(64)],
        }
    }

    /// Adds `node` and returns whether it was absent.
    fn insert(&mut self, node: NodeId) -> bool {
        let i = node.index();
        let (word, bit) = (&mut self.bits[i / 64], 1u64 << (i % 64));
        let absent = *word & bit == 0;
        *word |= bit;
        absent
    }

    /// Returns whether the set holds `node`.
    ///
    /// A node of a larger graph than the set's is not held.
    #[must_use]
    pub fn contains(&self, node: NodeId) -> bool {
        let i = node.index();
        self.bits
            .get(i / 64)
            .is_some_and(|word| word & (1u64 << (i % 64)) != 0)
    }

    /// Returns the count of the nodes the set holds.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.bits.iter().map(|word| word.count_ones()).sum()
    }

    /// Returns every node of `graph` the set does not hold, in order.
    #[must_use]
    pub fn orphans(&self, graph: &Graph) -> Vec<NodeId> {
        graph.node_ids().filter(|&n| !self.contains(n)).collect()
    }
}

/// Returns the nodes of `graph` the walk under `rule` reaches from `seeds`,
/// the seeds included.
///
/// Voyager's walk enters an object, an interface or a union alone, which
/// holds for its seeds too.
pub(super) fn walk(graph: &Graph, model: &Model, seeds: &[NodeId], rule: Rule) -> ReachSet {
    let len = graph.nodes.len();
    let enters =
        |n: NodeId| rule != Rule::Voyager || is_composite(model.ty(graph.nodes[n.index()].ty).kind);
    let mut set = ReachSet::empty(len);
    let mut stack = Vec::new();
    for &s in seeds {
        if enters(s) && set.insert(s) {
            stack.push(s);
        }
    }
    for _ in 0..len {
        let Some(n) = stack.pop() else {
            break;
        };
        for m in steps(graph, n, rule) {
            if enters(m) && set.insert(m) {
                stack.push(m);
            }
        }
    }
    set
}

/// Returns the nodes one step of the walk under `rule` leads to from `node`.
fn steps(graph: &Graph, node: NodeId, rule: Rule) -> impl Iterator<Item = NodeId> + use<'_> {
    let forward = graph
        .outbound(node)
        .iter()
        .map(|&e| graph.edge(e))
        .filter(move |e| rule.forward(e.kind))
        .map(|e| e.to);
    let backward = graph
        .inbound(node)
        .iter()
        .map(|&e| graph.edge(e))
        .filter(move |e| rule.backward(e.kind))
        .map(|e| e.from);
    forward.chain(backward)
}
