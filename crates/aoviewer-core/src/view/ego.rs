// SPDX-License-Identifier: MIT
/*
 * aoviewer-core view ego: the neighbourhood view of one type.
 *
 * The ego view has a ring budget of 24 boxes per side and no automatic distance
 * two.
 */

//! The ego view.
//!
//! The focus sits at rank 0 at [`Level::L3`]. The types it references form the
//! ring on its right at rank 1, in the order of the edges that leave it, which
//! is the order of its rows, and the types that reference it form the ring on
//! its left at rank -1 by relevance, which is roots first, then the types of
//! the focus's own cluster, then the count of fields that reference the focus,
//! then the neighbour count, then [`name_order`]. A ring box is at
//! [`Level::L1`] and draws the rows that link it to the focus. A type on both
//! sides is drawn once, on the right, with its edges in both directions, which
//! the layout draws as one curve with two heads. Each side takes at most the
//! ring, and the rest of a side is one [`Chip::More`].
//!
//! An interface with at least four implementations shows them as one
//! [`Chip::Implementations`], and an implementation joins the ring then only
//! when it references the interface by a row as well, as a referrer in the ring
//! or in its `+N more` chip. The first reduction of a page folds a ring by the
//! same rule also when leaving out the implementations that reference the
//! interface by no row empties at least two places of the ring that no other
//! referrer fills, so the group box stands for at least two boxes. An
//! implementation the fold leaves out is counted by the group box and by no
//! `+N more` chip.
//!
//! A focus that leaves rows out, past its first 32 drawn rows or by the leaf
//! fold of a page, carries a [`Chip::MoreFields`] with their count, which the
//! layout draws as its last row. The view counts it once it is built, by the
//! rule the layout draws the focus by.
//!
//! A type of `x=` that the view draws as a ring box expands. It is drawn at
//! [`Level::L2`] and adds its own ring by the same rules, both of its sides one
//! rank further out on its own side, under the boxes the budget leaves, the
//! types it references first. The types the view already draws stay where they
//! are, so an expanded ring joins its centre to the boxes it adds. The names of
//! `x=` are read in their order, so a name may expand a box an earlier name
//! added, and a name that is no box of the view, the focus and a name read
//! twice change nothing.
//!
//! Every loop runs over the edges that leave or enter one node of one graph,
//! over the boxes of one ring, or over at most [`MAX_EXPAND`] names of `x=`.

use alloc::vec::Vec;
use core::cmp::Ordering;

use super::{
    Bits,
    Build,
    Chip,
    Fold,
    Level,
    Params,
    View,
    ViewKind,
    drawn,
    id32,
    implementers,
    is_box,
    type_of, //
};
use crate::{
    cluster::Clustering,
    graph::{
        EdgeKind,
        NodeId, //
    },
    model::Kind,
    route::{
        MAX_EXPAND,
        Options, //
    },
    schema::Schema,
    sort::name_order,
};

/// A box whose ring the view adds, with the ranks and the room of the ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Centre {
    /// The box.
    node: NodeId,
    /// The rank of the types it references.
    out_rank: i32,
    /// The rank of the types that reference it.
    in_rank: i32,
    /// The most boxes the ring adds on both sides together.
    room: usize,
}

/// A type that references a centre, with the keys of its relevance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Referrer {
    /// The type.
    node: NodeId,
    /// Whether it is a root.
    root: bool,
    /// Whether it is in the cluster of the centre.
    home: bool,
    /// The count of its rows that reference the centre.
    fields: u32,
    /// Its neighbour count.
    degree: u32,
}

/// Returns the ego view of `focus` under `options` and `params`.
pub(super) fn ego(
    schema: &Schema,
    clustering: &Clustering,
    focus: NodeId,
    options: &Options,
    params: Params,
) -> View {
    let mut b = Build::new(schema, ViewKind::Ego, Some(focus));
    b.push(focus, Level::L3, 0);
    let ring = usize::from(params.ring);
    let centre = Centre {
        node: focus,
        out_rank: 1,
        in_rank: -1,
        room: usize::MAX,
    };
    add_ring(&mut b, clustering, centre, ring, params.fold);
    let mut expanded: Vec<NodeId> = Vec::new();
    for name in options.expand.iter().take(MAX_EXPAND) {
        let Some(node) = schema
            .model
            .type_by_name(name)
            .and_then(|ty| schema.graph.node_of(ty))
        else {
            continue;
        };
        let Some(i) = b.position(node) else {
            continue;
        };
        if node == focus || expanded.contains(&node) {
            continue;
        }
        expanded.push(node);
        b.nodes[i].1 = Level::L2;
        let outer = b.ranks[i] + b.ranks[i].signum();
        let centre = Centre {
            node,
            out_rank: outer,
            in_rank: outer,
            room: usize::from(params.boxes).saturating_sub(b.boxes()),
        };
        add_ring(&mut b, clustering, centre, ring, params.fold);
    }
    b.finish()
}

/// Adds the ring of `centre`, at most `ring` boxes per side, and the edges
/// that join the centre to them.
///
/// The interface `centre` folds its implementations into one chip when
/// [`Fold::folds_ring`] says so for `fold`. The fold takes out of the ring
/// the places of its inbound side that the referrers leave empty once the
/// implementations that only implement the centre are gone.
fn add_ring(b: &mut Build<'_>, clustering: &Clustering, centre: Centre, ring: usize, fold: Fold) {
    let schema = b.schema;
    let graph = &schema.graph;
    let c = centre.node;
    let mut seen = Bits::new(graph.nodes.len());
    let mut outs = Vec::new();
    for &e in graph.outbound(c) {
        let edge = &graph.edges[e as usize];
        let to = edge.to;
        if to != c
            && drawn(schema, edge)
            && !b.contains(to)
            && is_box(schema, to)
            && seen.insert(to)
        {
            outs.push(to);
        }
    }
    let mut ins = Vec::new();
    for &e in graph.inbound(c) {
        let edge = &graph.edges[e as usize];
        let from = edge.from;
        if from != c
            && drawn(schema, edge)
            && !b.contains(from)
            && is_box(schema, from)
            && seen.insert(from)
        {
            ins.push(from);
        }
    }
    let taken_out = outs.len().min(ring).min(centre.room);
    let places = ring.min(centre.room.saturating_sub(taken_out));
    if schema.model.ty(type_of(schema, c)).kind == Kind::Interface {
        let count = implementers(schema, c);
        let users = ins.iter().filter(|&&s| uses(schema, s, c)).count();
        let freed = ins.len().min(places) - users.min(places);
        if fold.folds_ring(count, freed) {
            b.chips.push(Chip::Implementations { at: c, count });
            ins.retain(|&s| uses(schema, s, c));
        }
    }
    let home = clustering.of(type_of(schema, c));
    let mut referrers: Vec<Referrer> = ins
        .iter()
        .map(|&s| referrer(schema, clustering, home, s, c))
        .collect();
    referrers.sort_by(|x, y| by_relevance(schema, x, y));
    let taken_in = referrers.len().min(places);
    let mut added = Bits::new(graph.nodes.len());
    for &n in &outs[..taken_out] {
        b.push(n, Level::L1, centre.out_rank);
        added.insert(n);
    }
    for r in &referrers[..taken_in] {
        b.push(r.node, Level::L1, centre.in_rank);
        added.insert(r.node);
    }
    for &e in graph.outbound(c) {
        let edge = &graph.edges[e as usize];
        if drawn(schema, edge) && (edge.to == c || added.contains(edge.to)) {
            b.edges.push(e);
        }
    }
    for &e in graph.inbound(c) {
        let edge = &graph.edges[e as usize];
        if drawn(schema, edge) && edge.from != c && added.contains(edge.from) {
            b.edges.push(e);
        }
    }
    if outs.len() > taken_out {
        b.chips.push(Chip::More {
            at: c,
            count: id32(outs.len() - taken_out),
            inbound: false,
        });
    }
    if referrers.len() > taken_in {
        b.chips.push(Chip::More {
            at: c,
            count: id32(referrers.len() - taken_in),
            inbound: true,
        });
    }
}

/// Returns whether `node` references `centre` by a drawn row, which an
/// implementation that only implements it does not.
fn uses(schema: &Schema, node: NodeId, centre: NodeId) -> bool {
    let graph = &schema.graph;
    graph.outbound(node).iter().any(|&e| {
        let edge = &graph.edges[e as usize];
        edge.to == centre && edge.kind != EdgeKind::Implements && drawn(schema, edge)
    })
}

/// Returns the keys of the relevance of `node`, a type that references
/// `centre`, whose cluster is `home`.
///
/// The fields that reference the centre are the rows of `node` a drawn edge
/// leaves for it. The edges of one row are adjacent in the graph, so a row is
/// counted where it starts.
fn referrer(
    schema: &Schema,
    clustering: &Clustering,
    home: Option<u32>,
    node: NodeId,
    centre: NodeId,
) -> Referrer {
    let graph = &schema.graph;
    let mut fields = 0;
    let mut last = None;
    for &e in graph.outbound(node) {
        let edge = &graph.edges[e as usize];
        if edge.to == centre && edge.row.is_some() && edge.row != last && drawn(schema, edge) {
            fields += 1;
            last = edge.row;
        }
    }
    Referrer {
        node,
        root: graph.nodes[node.index()].is_root,
        home: home.is_some() && clustering.of(type_of(schema, node)) == home,
        fields,
        degree: graph.neighbours(node),
    }
}

/// Orders two referrers by relevance, roots first, then the types of the
/// cluster of the centre, then more fields, then a larger neighbour count,
/// then [`name_order`].
fn by_relevance(schema: &Schema, x: &Referrer, y: &Referrer) -> Ordering {
    y.root
        .cmp(&x.root)
        .then(y.home.cmp(&x.home))
        .then(y.fields.cmp(&x.fields))
        .then(y.degree.cmp(&x.degree))
        .then_with(|| {
            name_order(
                &schema.model,
                type_of(schema, x.node),
                type_of(schema, y.node),
            )
        })
}
