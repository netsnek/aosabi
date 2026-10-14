// SPDX-License-Identifier: MIT
/*
 * aoviewer-core layout map: the layout of the cluster map.
 *
 * The cluster map is laid out once per schema and option set and never
 * rearranges during navigation.
 */

//! The cluster map layout.
//!
//! The cluster boxes stand in one column in the order of the clustering, the
//! other bucket last, so the map is the same picture whatever the focus. A
//! cluster box carries the name of the cluster as its title and the three
//! most referenced types inside it as its rows, at [`Level::L0`]. The box
//! carries its type count as the rest of its types, the `+N types` row under
//! the three. Its id is the id of the cluster, and the other bucket, which
//! has no slug, takes the slug `other`, numbered like a duplicate slug when a
//! cluster holds it already.
//!
//! One trunk joins each pair of cluster boxes that edges join, and carries
//! their count. It is drawn as an arc on the right of the column from the
//! right side of one header to the right side of the other, which crosses no
//! box, reaching further out the more boxes lie between its ends, and its
//! count chip sits at the middle of the arc. The id of the arc is the cluster
//! id of the two slugs joined by two hyphens, which no slug holds, since a
//! slug has single inner hyphens alone.
//!
//! Every loop runs over the cluster boxes, their rows and the trunks of one
//! map, or over the numbered candidates of one slug, of which there are at
//! most one more than the boxes, and counts its steps.

use alloc::{
    format,
    string::String,
    vec::Vec, //
};

use super::{
    GAP,
    LayoutError,
    MARGIN,
    Steps,
    measure::{
        chip_width,
        port,
        size, //
    },
    route_edges::{
        bounds,
        pt,
        trunk_width, //
    }, //
};
use crate::{
    id::ElementId,
    model::Kind,
    scene::{
        BoxRow,
        EdgeStyle,
        HEADER_CP,
        Marks,
        Rect,
        Scene,
        SceneBox,
        SceneEdge,
        Trunk, //
    },
    schema::Schema,
    text::cut,
    view::{
        Level,
        View, //
    },
};

/// How far the arc between two adjacent boxes reaches out with its control
/// points, 32 px, a step of the 4 px base.
const ARC_BASE: i32 = 3200;

/// How much further the arc reaches per box between its ends, 8 px.
const ARC_STEP: i32 = 800;

/// The furthest an arc reaches with its control points, 200 px, the widest
/// gutter.
const ARC_MAX: i32 = 20_000;

/// The name of the slug of the other bucket.
const OTHER: &str = "other";

/// Returns the scene of the cluster map `view`.
///
/// # Errors
///
/// Returns [`LayoutError::Empty`] for a map with no cluster box and
/// [`LayoutError::CapExceeded`] when the steps exceed the cap.
pub(super) fn layout(view: &View, schema: &Schema) -> Result<Scene, LayoutError> {
    if view.groups.is_empty() {
        return Err(LayoutError::Empty);
    }
    let mut steps = Steps::default();
    let slugs = slugs(view, &mut steps)?;
    let (groups, nodes) = (view.groups.len(), view.nodes.len());
    steps.take(3 * groups + 3 * nodes, "map")?;
    let mut boxes: Vec<SceneBox> = Vec::with_capacity(view.groups.len());
    for (group, slug) in view.groups.iter().zip(slugs) {
        boxes.push(cluster_box(view, schema, group, slug));
    }
    let width = boxes.iter().map(|b| b.rect.w).max().unwrap_or(0);
    let mut y = MARGIN;
    for b in &mut boxes {
        b.rect.x = MARGIN;
        b.rect.w = width;
        b.rect.y = y;
        for row in &mut b.rows {
            row.port_y = row.port_y.saturating_add(y);
        }
        y = y.saturating_add(b.rect.h).saturating_add(GAP);
    }
    steps.take(5 * view.trunks.len() + groups, "map")?;
    let (edges, trunks, reach) = arcs(view, &boxes);
    let (mut w, h) = bounds(&boxes, &[], &edges);
    w = w.max(
        MARGIN
            .saturating_add(width)
            .saturating_add(reach)
            .saturating_add(MARGIN),
    );
    Ok(Scene {
        width: w,
        height: h,
        boxes,
        edges,
        chips: Vec::new(),
        trunks,
        focus: None,
        iterations: steps.used,
    })
}

/// Returns the slug of every cluster box of `view`, the one of its cluster,
/// or a free slug for the other bucket.
///
/// # Errors
///
/// Returns [`LayoutError::CapExceeded`] when the steps exceed the cap.
fn slugs(view: &View, steps: &mut Steps) -> Result<Vec<String>, LayoutError> {
    steps.take(2 * view.groups.len(), "map")?;
    let taken: Vec<&str> = view
        .groups
        .iter()
        .filter_map(|g| g.slug.as_deref())
        .collect();
    let mut other = String::from(OTHER);
    for n in 2..=taken.len() + 2 {
        steps.take(taken.len(), "map")?;
        if !taken.contains(&other.as_str()) {
            break;
        }
        other = format!("{OTHER}-{n}");
    }
    Ok(view
        .groups
        .iter()
        .map(|g| g.slug.clone().unwrap_or_else(|| other.clone()))
        .collect())
}

/// Returns the measured cluster box of `group`, whose slug is `slug`, at the
/// origin.
fn cluster_box(
    view: &View,
    schema: &Schema,
    group: &crate::view::MapGroup,
    slug: String,
) -> SceneBox {
    let model = &schema.model;
    let first = group.first as usize;
    let shown = &view.nodes[first..first + group.shown as usize];
    let rows = shown
        .iter()
        .enumerate()
        .map(|(k, &(node, _))| {
            let name = model.ty(schema.graph.nodes[node.index()].ty).name;
            BoxRow {
                id: ElementId::Type(name),
                name: cut(model.name(name)),
                type_text: String::new(),
                port_y: port(k),
                marks: Marks::default(),
            }
        })
        .collect();
    let mut b = SceneBox {
        id: ElementId::Cluster(slug),
        kind: Kind::Object,
        level: Level::L0,
        rect: Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        },
        title: cut(&group.name),
        rows,
        more: group.types.saturating_sub(group.shown),
        root: false,
        marks: Marks::default(),
        inherited_folded: 0,
    };
    size(&mut b);
    b
}

/// Returns the arcs of the trunks of `view` between the placed `boxes`, the
/// trunks they carry, and how far the furthest of them reaches out of the
/// column with its count chip.
fn arcs(view: &View, boxes: &[SceneBox]) -> (Vec<SceneEdge>, Vec<Trunk>, i32) {
    let mut edges = Vec::with_capacity(view.trunks.len());
    let mut trunks = Vec::with_capacity(view.trunks.len());
    let mut reach = 0;
    for trunk in &view.trunks {
        let (a, b) = (trunk.a as usize, trunk.b as usize);
        let (Some(from), Some(to)) = (boxes.get(a), boxes.get(b)) else {
            continue;
        };
        let right = from.rect.x.saturating_add(from.rect.w);
        let y1 = from.rect.y.saturating_add(HEADER_CP / 2);
        let y2 = to.rect.y.saturating_add(HEADER_CP / 2);
        let between = i32::try_from(b.abs_diff(a).saturating_sub(1)).unwrap_or(i32::MAX);
        let bulge = ARC_BASE
            .saturating_add(ARC_STEP.saturating_mul(between))
            .min(ARC_MAX);
        let (ElementId::Cluster(sa), ElementId::Cluster(sb)) = (&from.id, &to.id) else {
            continue;
        };
        let at = pt(right.saturating_add(bulge / 4 * 3), y1 / 2 + y2 / 2);
        let chip = chip_width(&format!("{}", trunk.count)) / 2;
        reach = reach.max(bulge / 4 * 3 + chip);
        edges.push(SceneEdge {
            id: ElementId::Cluster(format!("{sa}--{sb}")),
            style: EdgeStyle::Field,
            from: pt(right, y1),
            c1: pt(right.saturating_add(bulge), y1),
            c2: pt(right.saturating_add(bulge), y2),
            to: pt(right, y2),
            two_heads: false,
            trunk: Some(super::id32(trunks.len())),
        });
        trunks.push(Trunk {
            at,
            count: trunk.count,
            width_cp: trunk_width(trunk.count),
        });
    }
    (edges, trunks, reach)
}
