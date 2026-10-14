// SPDX-License-Identifier: MIT
/*
 * aoviewer-core layout route_edges: the routing of the edges of a scene.
 *
 * A curve leaves the right end of a field row and enters the side of the target
 * header, never a body.
 */

//! The edge routing.
//!
//! Every edge of a view becomes one link, and so does every stub the view
//! carries. A link that the rule of the layout draws in full (see
//! `layout/mod.rs`: adjacent ranks in the ego layout, a source row and a target
//! header that face each other across one gutter in the whole graph layout) is
//! a cubic curve from the side of its row, or of the header of its source when
//! it leaves no row, to the side of the header of its target, both tangents
//! horizontal and both control points halfway across the gutter, so the curve
//! stays inside the gutter. A forward link leaves the right side and enters the
//! left side, and a backward link of the ego layout leaves the left side of its
//! row and enters the right side of the header, which keeps the rule that a
//! curve enters a header and never a body. A reference of a type to itself
//! loops on the right of its box from its row to its header.
//!
//! A pair of opposite references between two boxes becomes one curve with two
//! heads, the forward one, and the backward one is merged into it. Every other
//! link is a stub, an arrow into a chip that names the target. The chips of the
//! stubs of a box hang under it in the order of their links, flush with the
//! right side of the column, and the arrow loops out of the right side of the
//! row into the right side of its chip like a loop, so it stays out of every
//! box and chip of the column and out of the gutter beyond its first 18 px. The
//! arrow runs down the side of the box, as long as the part of the box under
//! its row and the chips above its own, and not the 24 px arrow of the design.
//!
//! The id of a link is the id of its source, ` => ` and the id of its target
//! type. A field or an input field row writes its own id as the source, an
//! argument edge the id of its argument, the first argument of the field of its
//! row the graph shows whose type is the target, as the graph makes one
//! argument edge per row and target, and a header the id of its type, which the
//! grammar writes in Voyager's form of the relation of the two types, a union
//! member, an implemented interface or an implementer. A stub that leaves the
//! header of its box for a type the model relates to it in no way, which the
//! cluster view draws for a neighbour that references the cluster, has no id in
//! the grammar with its box as the source. Its link carries the id of the edge
//! of the graph it stands for, from the type its chip names into its box, and
//! its arrow carries the id of its chip, which a writer does not repeat on the
//! arrow.
//!
//! The curves that enter one side of one header merge over their last 24 px
//! into a trunk, whose width is `125 + 75 * floor(log2(n))` centipixels capped
//! at 500, from the bit length of the count.
//!
//! Every loop runs over the links, the chips or the boxes of one view, over
//! the edges that leave or enter one node of the graph, or over the
//! arguments of one field, and counts its steps.

use alloc::{
    boxed::Box,
    vec,
    vec::Vec, //
};

use super::{
    ChipKind,
    ChipPlan,
    LayoutError,
    Link,
    MARGIN,
    Mode,
    Piece,
    Plan,
    Shape,
    Steps,
    find,
    id32,
    measure::chip_width, //
};
use crate::{
    graph::{
        Edge,
        EdgeKind,
        NodeId, //
    },
    id::ElementId,
    model::{
        Kind,
        Model,
        TypeId, //
    },
    scene::{
        CHIP_CP,
        EdgeStyle,
        HEADER_CP,
        Point,
        Rect,
        Scene,
        SceneChip,
        SceneEdge,
        TRUNK_CP,
        Trunk, //
    },
    schema::Schema,
    text::cut,
    transform::Member,
    view::{
        Chip,
        View,
        drawn, //
    },
};

/// How far a loop and the arrow of a stub reach out of the right side of their
/// column with their control points, 24 px, a step of the 4 px base. The curve
/// reaches three quarters of that, 18 px, which the narrowest gutter of 70 px
/// holds.
const LOOP: i32 = 2400;

/// The space above a stub chip, under its box or under the chip before it, 4
/// px, the base step.
const STUB_GAP: i32 = 400;

/// The widest trunk, 5 px.
const TRUNK_MAX: i32 = 500;

/// Returns the links of `view`, one per edge of the view in its order, then
/// one per stub of the view in the order of its chips, where `at` holds the
/// position of every box.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
pub(super) fn links(
    view: &View,
    schema: &Schema,
    at: &[(NodeId, usize)],
    steps: &mut Steps,
) -> Result<Vec<Link>, LayoutError> {
    let graph = &schema.graph;
    let mut out = Vec::with_capacity(view.edges.len());
    steps.take(view.edges.len() + view.chips.len(), "links")?;
    for &e in &view.edges {
        let edge = &graph.edges[e as usize];
        let (Some(from), Some(to)) = (find(at, edge.from), find(at, edge.to)) else {
            continue;
        };
        out.push(Link {
            id: edge_id(schema, edge, steps)?,
            style: style(edge.kind),
            from,
            row: edge.row,
            to: Some(to),
            target: edge.to,
            shape: Shape::Forward,
            two_heads: false,
            chip: None,
            bare: false,
        });
    }
    for chip in &view.chips {
        let Chip::Stub { from, row, to } = *chip else {
            continue;
        };
        let Some(source) = find(at, from) else {
            continue;
        };
        let (id, kind, bare) = stub_id(schema, from, row, to, steps)?;
        out.push(Link {
            id,
            style: style(kind),
            from: source,
            row,
            to: find(at, to),
            target: to,
            shape: Shape::Stub,
            two_heads: false,
            chip: None,
            bare,
        });
    }
    Ok(out)
}

/// Returns the id of the stub from the row `row` of `from`, or from its
/// header, to `to`, the kind of the edge it stands for, and whether it is
/// bare, a header stub between two types the model relates in no way.
///
/// The edge a stub stands for leaves its box, the first edge of the graph from
/// that row to that target, or, when none does, enters its box from the target,
/// the first such edge the view draws, which is the edge the cluster view took
/// the neighbour from. An entering edge is a bare stub unless the two types are
/// related, an interface and its implementer, whose header form names it.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn stub_id(
    schema: &Schema,
    from: NodeId,
    row: Option<u32>,
    to: NodeId,
    steps: &mut Steps,
) -> Result<(ElementId, EdgeKind, bool), LayoutError> {
    let graph = &schema.graph;
    let outbound = graph.outbound(from);
    steps.take(outbound.len(), "links")?;
    let own = outbound
        .iter()
        .map(|&e| &graph.edges[e as usize])
        .find(|e| e.row == row && e.to == to);
    if let Some(edge) = own {
        return Ok((edge_id(schema, edge, steps)?, edge.kind, false));
    }
    let inbound = graph.inbound(from);
    steps.take(inbound.len(), "links")?;
    let entering = inbound
        .iter()
        .map(|&e| &graph.edges[e as usize])
        .find(|e| e.from == to && drawn(schema, e));
    let kind = entering.map_or(EdgeKind::Field, |e| e.kind);
    let model = &schema.model;
    let (source, target) = (type_of(schema, from), type_of(schema, to));
    let header = ElementId::Edge(
        Box::new(ElementId::of_type(model, source)),
        model.ty(target).name,
    );
    if related(model, source, target, steps)? {
        return Ok((header, kind, false));
    }
    match entering {
        Some(edge) => Ok((edge_id(schema, edge, steps)?, kind, true)),
        None => Ok((header, kind, true)),
    }
}

/// Returns whether the model relates the type `source` to the type `target` in
/// one of the three ways the grammar writes a header source in, a union and its
/// member, a type and an interface it implements, or an interface and an
/// implementer.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn related(
    model: &Model,
    source: TypeId,
    target: TypeId,
    steps: &mut Steps,
) -> Result<bool, LayoutError> {
    let ty = model.ty(source);
    let implementers = model.implementers(source);
    steps.take(
        ty.members.len() + ty.implements.len() + implementers.len(),
        "links",
    )?;
    Ok((ty.kind == Kind::Union && ty.members.contains(&target))
        || ty.implements.contains(&target)
        || implementers.contains(&target))
}

/// Returns the id of `edge`.
///
/// A row writes the id of its field or input field as the source, and an
/// argument edge the id of the first argument of that field the graph shows
/// whose type is the target. A header writes the id of its type, which the
/// grammar writes in Voyager's form of the relation of the two types.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn edge_id(schema: &Schema, edge: &Edge, steps: &mut Steps) -> Result<ElementId, LayoutError> {
    let (model, t) = (&schema.model, &schema.transformed);
    let node = &schema.graph.nodes[edge.from.index()];
    let target_ty = type_of(schema, edge.to);
    let target = model.ty(target_ty).name;
    let Some(row) = edge.row.and_then(|r| node.rows.get(r as usize)) else {
        let header = ElementId::of_type(model, node.ty);
        return Ok(ElementId::Edge(Box::new(header), target));
    };
    let mut source = ElementId::of_field(model, row.field);
    if edge.kind == EdgeKind::Argument {
        let args = t.args(model, row.field);
        steps.take(2 * args.len(), "links")?;
        let arg = args
            .iter()
            .copied()
            .find(|&a| {
                !t.hides(model, Member::Arg(a)) && model.arg(a).type_ref.named() == target_ty
            })
            .and_then(|a| ElementId::of_arg(model, a));
        if let Some(arg) = arg {
            source = arg;
        }
    }
    Ok(ElementId::Edge(Box::new(source), target))
}

/// Returns the type of the node `node`.
fn type_of(schema: &Schema, node: NodeId) -> TypeId {
    schema.graph.nodes[node.index()].ty
}

/// Returns the style of an edge of `kind`.
fn style(kind: EdgeKind) -> EdgeStyle {
    match kind {
        EdgeKind::Field => EdgeStyle::Field,
        EdgeKind::Argument => EdgeStyle::Argument,
        EdgeKind::Implements => EdgeStyle::Implements,
        EdgeKind::UnionMember => EdgeStyle::UnionMember,
    }
}

/// Decides how every link is drawn, merges the opposite pairs, and adds the
/// chip of every stub.
///
/// The columns must hold their items, since the whole graph mode draws a
/// forward link in full only from the last sub column of a band into the
/// first of the next.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
pub(super) fn classify(plan: &mut Plan<'_>) -> Result<(), LayoutError> {
    plan.steps.take(plan.links.len(), "routing")?;
    for i in 0..plan.links.len() {
        let link = &plan.links[i];
        if link.shape == Shape::Stub {
            continue;
        }
        let shape = match link.to {
            Some(to) => shape_of(plan, link.from, to),
            None => Shape::Stub,
        };
        plan.links[i].shape = shape;
    }
    merge(plan)?;
    stub_chips(plan)
}

/// Returns how a link from the box `from` to the box `to` is drawn before
/// the opposite pairs merge.
fn shape_of(plan: &Plan<'_>, from: usize, to: usize) -> Shape {
    let (a, b) = (&plan.items[from], &plan.items[to]);
    if from == to {
        return match plan.mode {
            Mode::Ego => Shape::Loop,
            Mode::Whole => Shape::Stub,
        };
    }
    match i64::from(b.rank) - i64::from(a.rank) {
        1 if plan.mode == Mode::Ego => Shape::Forward,
        1 if plan.columns[a.column].last && plan.columns[b.column].first => Shape::Forward,
        -1 => Shape::Backward,
        _ => Shape::Stub,
    }
}

/// Merges every backward link into the first forward link of the opposite
/// direction between the same two boxes that carries no second head yet.
///
/// A backward link that finds none stays a backward curve in the ego layout
/// and becomes a stub in the whole graph layout.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn merge(plan: &mut Plan<'_>) -> Result<(), LayoutError> {
    let mut forward: Vec<(usize, usize, usize)> = plan
        .links
        .iter()
        .enumerate()
        .filter(|(_, l)| l.shape == Shape::Forward)
        .filter_map(|(i, l)| l.to.map(|t| (l.from, t, i)))
        .collect();
    forward.sort_unstable();
    plan.steps
        .take(forward.len() + 2 * plan.links.len(), "routing")?;
    for i in 0..plan.links.len() {
        let link = &plan.links[i];
        let (Shape::Backward, Some(to)) = (link.shape, link.to) else {
            continue;
        };
        let pair = (to, link.from);
        let start = forward.partition_point(|&(f, t, _)| (f, t) < pair);
        let same = forward[start..]
            .iter()
            .take_while(|&&(f, t, _)| (f, t) == pair)
            .count();
        plan.steps.take(same, "routing")?;
        let partner = forward[start..start + same]
            .iter()
            .map(|&(_, _, j)| j)
            .find(|&j| !plan.links[j].two_heads);
        match partner {
            Some(j) => {
                plan.links[j].two_heads = true;
                plan.links[i].shape = Shape::Merged;
            }
            None if plan.mode == Mode::Whole => plan.links[i].shape = Shape::Stub,
            None => {}
        }
    }
    Ok(())
}

/// Adds the chip of every stub, which names the target of its link, and
/// makes the item of every box as tall as the box and the chips under it.
///
/// The first chip that names a type the view draws no box of carries the id
/// of that type, and every other chip the id of its edge, so no chip takes
/// the id of a box or of another chip of the scene. The arrow of a bare stub
/// takes the id of its chip.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn stub_chips(plan: &mut Plan<'_>) -> Result<(), LayoutError> {
    plan.steps
        .take(plan.links.len() + plan.boxes.len(), "routing")?;
    let model = &plan.schema.model;
    let mut slots = vec![0i32; plan.boxes.len()];
    let mut named: Vec<NodeId> = Vec::new();
    for i in 0..plan.links.len() {
        let link = &plan.links[i];
        if link.shape != Shape::Stub {
            continue;
        }
        let name = model
            .ty(plan.schema.graph.nodes[link.target.index()].ty)
            .name;
        let text = cut(model.name(name));
        let first = match named.binary_search(&link.target) {
            Err(k) if link.to.is_none() => {
                plan.steps.take(named.len() - k + 1, "routing")?;
                named.insert(k, link.target);
                true
            }
            _ => false,
        };
        let id = if first {
            ElementId::Type(name)
        } else {
            link.id.clone()
        };
        if link.bare {
            plan.links[i].id = id.clone();
        }
        let link = &plan.links[i];
        let at = link.from;
        let slot = slots[at];
        slots[at] = slot.saturating_add(1);
        let item = &mut plan.items[at];
        item.h = item.h.saturating_add(STUB_GAP).saturating_add(CHIP_CP);
        plan.links[i].chip = Some(plan.chips.len());
        plan.chips.push(ChipPlan {
            id,
            rect: Rect {
                x: 0,
                y: 0,
                w: chip_width(&text),
                h: CHIP_CP,
            },
            text,
            target: Some(ElementId::Type(name)),
            at,
            kind: ChipKind::Stub,
            slot,
        });
    }
    Ok(())
}

/// Returns the scene of `plan`, with its boxes and chips at their places,
/// its curves routed and its trunks.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
pub(super) fn finish(mut plan: Plan<'_>) -> Result<Scene, LayoutError> {
    let (items, chips, links) = (plan.items.len(), plan.chips.len(), plan.links.len());
    plan.steps.take(items + chips + 6 * links, "routing")?;
    place_boxes(&mut plan);
    let mut edges = Vec::with_capacity(plan.links.len());
    for link in &plan.links {
        if let Some(edge) = curve(&plan, link) {
            edges.push(edge);
        }
    }
    let trunks = trunks(&plan, &mut edges);
    let mut rows = 0;
    for b in &mut plan.boxes {
        rows += b.rows.len();
        for row in &mut b.rows {
            row.port_y = row.port_y.saturating_add(b.rect.y);
        }
    }
    let boxes = plan.boxes.len();
    plan.steps
        .take(2 * boxes + rows + 2 * chips + 4 * edges.len(), "routing")?;
    let chips: Vec<SceneChip> = plan
        .chips
        .into_iter()
        .map(|c| SceneChip {
            id: c.id,
            rect: c.rect,
            text: c.text,
            target: c.target,
        })
        .collect();
    let (width, height) = bounds(&plan.boxes, &chips, &edges);
    let focus = plan.view.focus.and_then(|f| find(&plan.at, f)).map(id32);
    Ok(Scene {
        width,
        height,
        boxes: plan.boxes,
        edges,
        chips,
        trunks,
        focus,
        iterations: plan.steps.used,
    })
}

/// Sets the rectangles of the boxes and of the chips from their items and
/// their columns, and hangs the chips of the stubs under their boxes.
fn place_boxes(plan: &mut Plan<'_>) {
    for item in &plan.items {
        let x = plan.columns[item.column].x;
        match item.piece {
            Piece::Box(i) => {
                let rect = &mut plan.boxes[i].rect;
                (rect.x, rect.y, rect.w) = (x, item.y, item.w);
            }
            Piece::Chip(k) => {
                let rect = &mut plan.chips[k].rect;
                (rect.x, rect.y, rect.w, rect.h) = (x, item.y, item.w, item.h);
            }
        }
    }
    for chip in &mut plan.chips {
        if chip.kind != ChipKind::Stub {
            continue;
        }
        let under = plan.boxes[chip.at].rect;
        let right = under.x.saturating_add(under.w);
        let step = STUB_GAP.saturating_add(CHIP_CP);
        chip.rect.x = right.saturating_sub(chip.rect.w);
        chip.rect.y = under
            .y
            .saturating_add(under.h)
            .saturating_add(STUB_GAP)
            .saturating_add(chip.slot.saturating_mul(step));
    }
}

/// Returns the curve of `link`, or `None` for a link merged into another.
fn curve(plan: &Plan<'_>, link: &Link) -> Option<SceneEdge> {
    let source = plan.boxes[link.from].rect;
    let (_, port) = plan.port(link.from, link.row);
    let y = source.y.saturating_add(port);
    let right = source.x.saturating_add(source.w);
    let header = |r: Rect| r.y.saturating_add(HEADER_CP / 2);
    let (from, to) = match (link.shape, link.to, link.chip) {
        (Shape::Forward, Some(t), _) => {
            let target = plan.boxes[t].rect;
            (pt(right, y), pt(target.x, header(target)))
        }
        (Shape::Backward, Some(t), _) => {
            let target = plan.boxes[t].rect;
            (
                pt(source.x, y),
                pt(target.x.saturating_add(target.w), header(target)),
            )
        }
        (Shape::Loop, _, _) => (pt(right, y), pt(right, header(source))),
        (Shape::Stub, _, Some(k)) => {
            let chip = plan.chips[k].rect;
            let edge = chip.x.saturating_add(chip.w);
            (pt(right, y), pt(edge, chip.y.saturating_add(CHIP_CP / 2)))
        }
        _ => return None,
    };
    let (c1, c2) = if matches!(link.shape, Shape::Loop | Shape::Stub) {
        (
            pt(from.x.saturating_add(LOOP), from.y),
            pt(to.x.saturating_add(LOOP), to.y),
        )
    } else {
        let half = to.x.saturating_sub(from.x) / 2;
        (
            pt(from.x.saturating_add(half), from.y),
            pt(to.x.saturating_sub(half), to.y),
        )
    };
    Some(SceneEdge {
        id: link.id.clone(),
        style: link.style,
        from,
        c1,
        c2,
        to,
        two_heads: link.two_heads,
        trunk: None,
    })
}

/// Returns the trunks of the curves of `plan` and marks every curve that
/// merges into one.
///
/// The curves drawn in full that enter one side of one header form a trunk
/// when there are two or more of them. The trunks follow their first curve.
fn trunks(plan: &Plan<'_>, edges: &mut [SceneEdge]) -> Vec<Trunk> {
    let mut entering: Vec<(usize, bool, usize)> = Vec::new();
    let mut e = 0;
    for link in &plan.links {
        if link.shape == Shape::Merged {
            continue;
        }
        let side = match (link.shape, link.to) {
            (Shape::Forward, Some(t)) => Some((t, false)),
            (Shape::Backward, Some(t)) => Some((t, true)),
            (Shape::Loop, _) => Some((link.from, true)),
            _ => None,
        };
        if let Some((t, right)) = side {
            entering.push((t, right, e));
        }
        e += 1;
    }
    entering.sort_unstable();
    let mut groups: Vec<&[(usize, bool, usize)]> = entering
        .chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
        .filter(|g| g.len() > 1)
        .collect();
    groups.sort_unstable_by_key(|g| g[0].2);
    let mut out = Vec::with_capacity(groups.len());
    for group in groups {
        let (t, right, _) = group[0];
        let r = plan.boxes[t].rect;
        let x = if right {
            r.x.saturating_add(r.w).saturating_add(TRUNK_CP)
        } else {
            r.x.saturating_sub(TRUNK_CP)
        };
        let count = id32(group.len());
        for &(_, _, e) in group {
            edges[e].trunk = Some(id32(out.len()));
        }
        out.push(Trunk {
            at: pt(x, r.y.saturating_add(HEADER_CP / 2)),
            count,
            width_cp: trunk_width(count),
        });
    }
    out
}

/// Returns the width of a trunk of `count` curves, `125 + 75 * floor(log2(n))`
/// centipixels capped at 500.
///
/// The floor of the binary logarithm is the bit length of the count less one,
/// so no float is needed, and a count of 0 is taken as 1.
pub(super) fn trunk_width(count: u32) -> i32 {
    let log = count.max(1).ilog2();
    let log = i32::try_from(log).unwrap_or(0);
    (125 + 75 * log).min(TRUNK_MAX)
}

/// Returns the width and the height of a drawing that holds `boxes`, `chips`
/// and `edges`, with its margin on the right and at the bottom.
///
/// A curve lies inside the hull of its end and control points, so those
/// points bound it.
pub(super) fn bounds(
    boxes: &[crate::scene::SceneBox],
    chips: &[SceneChip],
    edges: &[SceneEdge],
) -> (i32, i32) {
    let rects = boxes
        .iter()
        .map(|b| b.rect)
        .chain(chips.iter().map(|c| c.rect));
    let corners = rects.map(|r| pt(r.x.saturating_add(r.w), r.y.saturating_add(r.h)));
    let points = edges.iter().flat_map(|e| [e.from, e.c1, e.c2, e.to]);
    let (mut w, mut h) = (0, 0);
    for p in corners.chain(points) {
        w = w.max(p.x);
        h = h.max(p.y);
    }
    (w.saturating_add(MARGIN), h.saturating_add(MARGIN))
}

/// Returns the point at `x` and `y`.
pub(super) fn pt(x: i32, y: i32) -> Point {
    Point { x, y }
}
