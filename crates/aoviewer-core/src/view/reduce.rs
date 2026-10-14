// SPDX-License-Identifier: MIT
/*
 * aoviewer-core view reduce: the reduction of a view that is over its budget.
 *
 * The four reductions run in a fixed order, and a split into numbered parts
 * comes last.
 */

//! The reduction order.
//!
//! A view placed inside a page answers under [`Budget::PAGE`]. A type with
//! fewer than two neighbours has no diagram. Its neighbours are counted as the
//! view draws them, over the edges `leaf` keeps and the boxes `hideRoot`
//! keeps, which under the default options is the count of the graph. A view
//! that fits answers itself, and a view over the budget runs the four
//! reductions in the order of [`Reduction`], each one only when the view is
//! still over the budget, so the result is deterministic and explainable. A
//! view fits when it holds at most the boxes and edges of the budget and the
//! scene the layout of its kind draws is at most 1400 px wide.
//!
//! 1. [`Reduction::FoldedImplementers`] rebuilds the view with the
//!    implementations of an interface folded into its group box wherever the
//!    fold takes at least two boxes away. A ring of an ego view folds when
//!    leaving out the implementations that reference the interface by no row
//!    empties at least two of its places that no other referrer fills. A whole
//!    graph and a cluster view fold a group of two or more implementations of
//!    one interface, where an implementation folds when every edge of the view
//!    it touches is its implements edge into that interface and no chip hangs
//!    at it, so no other edge and no stub is lost. The step is recorded when
//!    the view the reductions leave holds a group box at an interface that the
//!    view before them holds none at, since the smaller ring of step 3 judges
//!    the fold of an ego view anew and may leave it no two boxes to take.
//! 2. [`Reduction::FoldedLeafRows`] folds the leaf rows of every box into one
//!    row counting them. The rows are the layout's, so the step changes no box
//!    and no edge and records the fold for the layout, which measures the view
//!    with the fold from then on, and an edge that leaves a folded leaf row,
//!    such as one into an enum box under `nodes=all`, leaves the row that
//!    counts them.
//! 3. [`Reduction::DroppedRing`] rebuilds an ego or an operation view with a
//!    ring one box smaller per side until it fits or the ring is empty, and
//!    the `+N more` chips count the rest, which the table under the diagram
//!    lists. The other views have no ring.
//! 4. [`Reduction::Split`] splits what is left into numbered parts along
//!    cluster borders. The boxes but the focus are taken by cluster, in the
//!    order of the clustering with the types outside every cluster last, and
//!    in the order of the view inside a cluster. A part takes whole clusters
//!    while they fit and starts anew at a cluster that does not, and a cluster
//!    larger than a part is cut in the order of the view. The focus is in
//!    every part with the [`Chip::MoreFields`] its drawing in that part
//!    counts, and its other chips are in the first. An edge between two parts
//!    becomes a [`Chip::Stub`] in the part of its source, which a printed
//!    document gives the page of its target. The cluster map splits its
//!    cluster boxes in their order, and a trunk between two parts is left out.
//!    The split packs by boxes and edges alone and runs only for a view over
//!    them, so a view within them that the first three steps leave wider than
//!    the budget stays whole and wider.
//!
//! The reductions that shaped the view are recorded in every part, in their
//! order. A view no split brings under the budget, which is a box whose own
//! edges exceed it, answers empty with [`EmptyReason::OverBudget`].
//!
//! Every loop runs over the boxes, edges and chips of one view, over at most
//! [`MAX_RING`] smaller rings, or over the parts of one view, which are at
//! most its boxes.

use alloc::{
    vec,
    vec::Vec, //
};

use super::{
    Budget,
    Chip,
    EmptyReason,
    Fold,
    MapGroup,
    MapTrunk,
    NONE,
    Params,
    Reduction,
    View,
    ViewError,
    ViewKind,
    build,
    chip_node,
    drawn,
    find,
    id32,
    implementers,
    is_box,
    more_fields,
    positions,
    type_of, //
};
use crate::{
    cluster::Clustering,
    graph::{
        EdgeKind,
        NodeId, //
    },
    route::{
        MAX_RING,
        Options,
        Route, //
    },
    schema::Schema,
};

/// Returns the views of `route` for a page, reduced under [`Budget::PAGE`].
///
/// # Errors
///
/// Returns the errors of [`View::of`].
pub(super) fn for_page(
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    options: &Options,
) -> Result<Vec<View>, ViewError> {
    let budget = Budget::PAGE;
    let mut params = Params {
        ring: options.ring.min(budget.ring),
        fold: Fold::Viewer,
        boxes: budget.boxes,
    };
    let first = build(schema, clustering, route, options, params)?;
    if first.empty.is_some() {
        return Ok(vec![first]);
    }
    if let Some(reason) = skipped(schema, route, &first) {
        return Ok(vec![View::empty(first.kind, reason)]);
    }
    if first.fits_page(schema, &budget) {
        return Ok(vec![first]);
    }
    params.fold = Fold::Page;
    let mut view = build(schema, clustering, route, options, params)?;
    let mut fits = view.fits_page(schema, &budget);
    let leaf_rows = !fits && view.kind != ViewKind::ClusterMap;
    if leaf_rows {
        view.reductions.push(Reduction::FoldedLeafRows);
        fits = view.fits_page(schema, &budget);
    }
    let full = params.ring;
    if matches!(view.kind, ViewKind::Ego | ViewKind::Operation) {
        for _ in 0..=MAX_RING {
            if params.ring == 0 || fits {
                break;
            }
            params.ring -= 1;
            view = build(schema, clustering, route, options, params)?;
            if leaf_rows {
                view.reductions.push(Reduction::FoldedLeafRows);
            }
            fits = view.fits_page(schema, &budget);
        }
    }
    let mut reductions = Vec::new();
    if folded(&first, &view) {
        reductions.push(Reduction::FoldedImplementers);
    }
    if leaf_rows {
        reductions.push(Reduction::FoldedLeafRows);
    }
    if params.ring < full {
        reductions.push(Reduction::DroppedRing);
    }
    let mut views = if view.fits(&budget) {
        vec![view]
    } else if let Some(parts) = split(schema, clustering, &view, &budget) {
        reductions.push(Reduction::Split);
        parts
    } else {
        let (boxes, edges) = view.size();
        return Ok(vec![View::empty(
            view.kind,
            EmptyReason::OverBudget { boxes, edges },
        )]);
    };
    for v in &mut views {
        v.reductions.clone_from(&reductions);
        more_fields(schema, v);
    }
    Ok(views)
}

/// Returns why the page of `route` draws no diagram of `view`, when a type
/// has fewer than two neighbours.
///
/// A type the graph joins to no other that is no root is one that no field
/// reaches and that references nothing. The neighbours of any other type are
/// counted by [`drawn_neighbours`].
fn skipped(schema: &Schema, route: &Route, view: &View) -> Option<EmptyReason> {
    if !matches!(route, Route::Type { .. }) {
        return None;
    }
    let focus = view.focus?;
    let graph = &schema.graph;
    if graph.neighbours(focus) == 0 && !graph.nodes[focus.index()].is_root {
        return Some(EmptyReason::Unreachable);
    }
    (drawn_neighbours(schema, focus) < 2).then_some(EmptyReason::FewerThanTwoNeighbours)
}

/// Returns the count of the neighbours of `focus` its ego view draws before
/// any ring budget, the other nodes joined to it in either direction by an
/// edge that [`drawn`] keeps and that [`is_box`] keeps, each counted once.
///
/// Under `leaf=1` and `hideRoot=0` it is [`Graph::neighbours`], which counts
/// every other end of an edge but a custom scalar of a node that is none.
///
/// [`Graph::neighbours`]: crate::graph::Graph::neighbours
fn drawn_neighbours(schema: &Schema, focus: NodeId) -> u32 {
    let graph = &schema.graph;
    let outs = graph
        .outbound(focus)
        .iter()
        .map(|&e| (e, graph.edges[e as usize].to));
    let ins = graph
        .inbound(focus)
        .iter()
        .map(|&e| (e, graph.edges[e as usize].from));
    let mut others: Vec<NodeId> = outs
        .chain(ins)
        .filter(|&(e, n)| {
            n != focus && drawn(schema, &graph.edges[e as usize]) && is_box(schema, n)
        })
        .map(|(_, n)| n)
        .collect();
    others.sort_unstable();
    others.dedup();
    id32(others.len())
}

/// Returns the interfaces of the group boxes of implementations of `view`.
fn group_boxes(view: &View) -> impl Iterator<Item = NodeId> + '_ {
    view.chips.iter().filter_map(|c| match *c {
        Chip::Implementations { at, .. } => Some(at),
        _ => None,
    })
}

/// Returns whether `view` holds a group box of implementations at an
/// interface that `first` holds none at, which is what the first reduction
/// adds.
fn folded(first: &View, view: &View) -> bool {
    let before: Vec<NodeId> = group_boxes(first).collect();
    group_boxes(view).any(|at| !before.contains(&at))
}

/// Folds every group of at least `at` implementations of one interface of
/// `view` into its group box.
///
/// An implementation folds when every edge of the view it touches is its
/// implements edge into the interface, and never when it is the focus or
/// when a chip hangs at it, so a stub keeps the box it leaves.
pub(super) fn fold(view: &mut View, schema: &Schema, at: u32) {
    let graph = &schema.graph;
    let len = view.nodes.len();
    let pos = positions(view.nodes.iter().map(|&(n, _)| n));
    let mut into: Vec<Option<NodeId>> = vec![None; len];
    let mut free = vec![true; len];
    for &e in &view.edges {
        let edge = &graph.edges[e as usize];
        let (Some(a), Some(b)) = (find(&pos, edge.from), find(&pos, edge.to)) else {
            continue;
        };
        if edge.kind == EdgeKind::Implements && a != b {
            free[b] = false;
            match into[a] {
                None => into[a] = Some(edge.to),
                Some(t) if t == edge.to => {}
                Some(_) => free[a] = false,
            }
        } else {
            free[a] = false;
            free[b] = false;
        }
    }
    for chip in &view.chips {
        if let Some(p) = find(&pos, chip_node(chip)) {
            free[p] = false;
        }
    }
    let target = |p: usize| {
        into[p]
            .filter(|_| free[p] && Some(view.nodes[p].0) != view.focus)
            .and_then(|t| find(&pos, t))
    };
    let mut count = vec![0u32; len];
    for p in 0..len {
        if let Some(i) = target(p) {
            count[i] += 1;
        }
    }
    let gone: Vec<bool> = (0..len)
        .map(|p| target(p).is_some_and(|i| count[i] >= at))
        .collect();
    if !gone.contains(&true) {
        return;
    }
    let kept = |n: NodeId| find(&pos, n).is_none_or(|p| !gone[p]);
    for (i, &c) in count.iter().enumerate() {
        if c >= at {
            let interface = view.nodes[i].0;
            view.chips.push(Chip::Implementations {
                at: interface,
                count: implementers(schema, interface),
            });
        }
    }
    let edges: Vec<u32> = view
        .edges
        .iter()
        .copied()
        .filter(|&e| {
            let edge = &graph.edges[e as usize];
            kept(edge.from) && kept(edge.to)
        })
        .collect();
    let mut nodes = Vec::with_capacity(len);
    let mut ranks = Vec::with_capacity(len);
    for p in (0..len).filter(|&p| !gone[p]) {
        nodes.push(view.nodes[p]);
        ranks.push(view.ranks[p]);
    }
    view.nodes = nodes;
    view.ranks = ranks;
    view.edges = edges;
}

/// The parts one view is packed into.
struct Packer<'p> {
    /// The part of every box, or [`NONE`].
    part_of: Vec<u32>,
    /// The boxes of the view each box shares an edge with, once per edge,
    /// a box itself once per loop.
    touching: &'p [Vec<usize>],
    /// The boxes every box counts, itself and its group boxes.
    weight: &'p [u32],
    /// The box every part holds, the focus.
    anchor: Option<usize>,
    /// The budget of every part.
    budget: Budget,
    /// The part being filled.
    current: u32,
    /// Its boxes.
    boxes: u32,
    /// Its edges.
    edges: u32,
    /// The boxes it holds but the anchor.
    members: usize,
}

impl Packer<'_> {
    /// Starts the part `current` with the anchor alone.
    fn open(&mut self) {
        let anchor = self.anchor;
        self.boxes = anchor.map_or(0, |a| self.weight[a]);
        self.edges = anchor.map_or(0, |a| {
            id32(self.touching[a].iter().filter(|&&q| q == a).count())
        });
        self.members = 0;
    }

    /// Closes the part being filled and opens the next.
    fn next(&mut self) {
        self.current += 1;
        self.open();
    }

    /// Returns whether the part being filled holds the box `p`.
    fn holds(&self, p: usize) -> bool {
        self.part_of[p] == self.current || Some(p) == self.anchor
    }

    /// Adds every box of `group` to the part being filled and returns
    /// whether they fit, leaving the part as it was when they do not.
    fn add(&mut self, group: &[usize]) -> bool {
        let (mut boxes, mut edges) = (self.boxes, self.edges);
        for (i, &p) in group.iter().enumerate() {
            self.part_of[p] = self.current;
            boxes += self.weight[p];
            edges += id32(self.touching[p].iter().filter(|&&q| self.holds(q)).count());
            if boxes > u32::from(self.budget.boxes) || edges > u32::from(self.budget.edges) {
                for &q in &group[..=i] {
                    self.part_of[q] = NONE;
                }
                return false;
            }
        }
        self.boxes = boxes;
        self.edges = edges;
        self.members += group.len();
        true
    }

    /// Adds `group` whole, to the part being filled or to a new one, or box
    /// by box when it fits no part whole, and returns whether every box
    /// found a part.
    fn place(&mut self, group: &[usize]) -> bool {
        if self.add(group) {
            return true;
        }
        if self.members > 0 {
            self.next();
            if self.add(group) {
                return true;
            }
        }
        for &p in group {
            if self.add(&[p]) {
                continue;
            }
            if self.members > 0 {
                self.next();
            }
            if !self.add(&[p]) {
                return false;
            }
        }
        true
    }
}

/// Returns the parts of `view` under `budget`, or `None` when a box does not
/// fit a part alone.
fn split(
    schema: &Schema,
    clustering: &Clustering,
    view: &View,
    budget: &Budget,
) -> Option<Vec<View>> {
    if view.kind == ViewKind::ClusterMap {
        return Some(split_map(view, budget));
    }
    let graph = &schema.graph;
    let len = view.nodes.len();
    let pos = positions(view.nodes.iter().map(|&(n, _)| n));
    let mut touching: Vec<Vec<usize>> = vec![Vec::new(); len];
    let mut ends: Vec<Option<(usize, usize)>> = Vec::with_capacity(view.edges.len());
    for &e in &view.edges {
        let edge = &graph.edges[e as usize];
        let pair = find(&pos, edge.from).zip(find(&pos, edge.to));
        if let Some((a, b)) = pair {
            touching[a].push(b);
            if a != b {
                touching[b].push(a);
            }
        }
        ends.push(pair);
    }
    let mut weight = vec![1u32; len];
    for chip in &view.chips {
        if let Chip::Implementations { at, .. } = *chip
            && let Some(p) = find(&pos, at)
        {
            weight[p] += 1;
        }
    }
    let anchor = view.focus.and_then(|f| find(&pos, f));
    let key = |p: usize| {
        clustering
            .of(type_of(schema, view.nodes[p].0))
            .unwrap_or(NONE)
    };
    let mut order: Vec<usize> = (0..len).filter(|&p| Some(p) != anchor).collect();
    order.sort_by_key(|&p| (key(p), p));
    let mut packer = Packer {
        part_of: vec![NONE; len],
        touching: &touching,
        weight: &weight,
        anchor,
        budget: *budget,
        current: 0,
        boxes: 0,
        edges: 0,
        members: 0,
    };
    packer.open();
    for group in order.chunk_by(|&x, &y| key(x) == key(y)) {
        if !packer.place(group) {
            return None;
        }
    }
    let parts = packer.current + 1;
    let cut = Cut {
        schema,
        view,
        pos: &pos,
        ends: &ends,
        part_of: &packer.part_of,
        anchor,
        count: u16::try_from(parts).unwrap_or(u16::MAX),
    };
    Some((0..parts).map(|k| cut.part(k)).collect())
}

/// A view and the part of every box, from which the parts are cut.
struct Cut<'c> {
    /// The schema whose graph the view draws.
    schema: &'c Schema,
    /// The view.
    view: &'c View,
    /// The table of [`positions`] of its boxes.
    pos: &'c [(NodeId, usize)],
    /// The positions of the two ends of every edge of the view.
    ends: &'c [Option<(usize, usize)>],
    /// The part of every box, or [`NONE`] for the anchor.
    part_of: &'c [u32],
    /// The box every part holds.
    anchor: Option<usize>,
    /// The count of the parts.
    count: u16,
}

impl Cut<'_> {
    /// Returns the part `k`, counted from 0.
    fn part(&self, k: u32) -> View {
        let view = self.view;
        let holds = |p: usize| self.part_of[p] == k || Some(p) == self.anchor;
        let number = u16::try_from(k + 1).unwrap_or(u16::MAX);
        let mut out = View::blank(view.kind, view.focus);
        out.part = Some((number, self.count));
        out.row = view.row;
        for p in (0..view.nodes.len()).filter(|&p| holds(p)) {
            out.nodes.push(view.nodes[p]);
            out.ranks.push(view.ranks[p]);
        }
        for chip in &view.chips {
            let owner = find(self.pos, chip_node(chip));
            if owner.is_some_and(|p| holds(p) && (k == 0 || Some(p) != self.anchor)) {
                out.chips.push(*chip);
            }
        }
        for (&e, &pair) in view.edges.iter().zip(self.ends) {
            let Some((a, b)) = pair else {
                continue;
            };
            if holds(a) && holds(b) {
                out.edges.push(e);
            } else if holds(a) && Some(a) != self.anchor {
                out.chips.push(Chip::Stub {
                    from: view.nodes[a].0,
                    row: self.schema.graph.edges[e as usize].row,
                    to: view.nodes[b].0,
                });
            }
        }
        out
    }
}

/// Returns the parts of the cluster map `view` under `budget`, its cluster
/// boxes taken in their order.
fn split_map(view: &View, budget: &Budget) -> Vec<View> {
    let count = view.groups.len();
    let mut into: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (t, trunk) in view.trunks.iter().enumerate() {
        into[trunk.b as usize].push(t);
    }
    let mut part_of = vec![0u32; count];
    let (mut current, mut boxes, mut edges) = (0u32, 0u32, 0u32);
    for g in 0..count {
        let joins = |part: u32| {
            id32(
                into[g]
                    .iter()
                    .filter(|&&t| part_of[view.trunks[t].a as usize] == part)
                    .count(),
            )
        };
        let mut added = joins(current);
        if boxes > 0
            && (boxes >= u32::from(budget.boxes) || edges + added > u32::from(budget.edges))
        {
            current += 1;
            boxes = 0;
            edges = 0;
            added = 0;
        }
        part_of[g] = current;
        boxes += 1;
        edges += added;
    }
    let parts = current + 1;
    let number = |k: u32| u16::try_from(k).unwrap_or(u16::MAX);
    (0..parts)
        .map(|k| {
            let mut out = View::blank(ViewKind::ClusterMap, None);
            out.part = Some((number(k + 1), number(parts)));
            let mut renumber = vec![NONE; count];
            for g in (0..count).filter(|&g| part_of[g] == k) {
                let group = &view.groups[g];
                renumber[g] = id32(out.groups.len());
                let first = group.first as usize;
                let shown = first..first + group.shown as usize;
                out.groups.push(MapGroup {
                    name: group.name.clone(),
                    slug: group.slug.clone(),
                    types: group.types,
                    first: id32(out.nodes.len()),
                    shown: group.shown,
                });
                out.nodes.extend_from_slice(&view.nodes[shown.clone()]);
                out.ranks.extend_from_slice(&view.ranks[shown]);
            }
            for trunk in &view.trunks {
                let (a, b) = (renumber[trunk.a as usize], renumber[trunk.b as usize]);
                if a != NONE && b != NONE {
                    out.trunks.push(MapTrunk {
                        a,
                        b,
                        count: trunk.count,
                    });
                }
            }
            out
        })
        .collect()
}
