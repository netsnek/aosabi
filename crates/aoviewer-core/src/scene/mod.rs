// SPDX-License-Identifier: MIT
/*
 * aoviewer-core scene: the scene, a display list in integer centipixels.
 *
 * A scene is boxes with rows and ports, curves with a style, chips, stubs and
 * trunks.
 */

//! The scene.
//!
//! A [`Scene`] is what a layout answers for a view and what every scene writer
//! reads, the SVG writer and the Typst primitives alike, so the viewer and the
//! documents draw the same picture. Every coordinate is an `i32` in
//! centipixels, a hundredth of a pixel, with the origin at the top left corner
//! and `y` growing downwards, and no coordinate is ever rounded.
//!
//! A box has a header of `HEADER_CP` and one row of `ROW_CP` per drawn row,
//! then one row for its folded inherited fields when it folds any, then one
//! row that counts what it leaves out when it leaves out any. The texts of
//! those two rows are `SceneBox::inherited_label` and `SceneBox::more_label`,
//! which the layout measures and a writer prints. A box keeps `PADDING_CP` on
//! either side of its text.
//!
//! A curve is one cubic segment from `from` over `c1` and `c2` to `to`, whose
//! tangents at both ends are horizontal. A curve that belongs to a trunk ends
//! on the side of the header of its target like every other curve, and the
//! trunk it names is drawn over its last `TRUNK_CP` with one head for all of
//! them.
//!
//! A chip is a count or a name that stands for what the view leaves out, a
//! `+N more` chip, an `N implementations` chip, or the chip a stub ends in. A
//! chip is `CHIP_CP` high, lower than a header, and its text is set at
//! `CHIP_TEXT_CP`. A count chip carries the id of the box it hangs at. The
//! first stub chip that names a type the scene draws no box of carries the id
//! of that type, and every other stub chip the id of its edge. An arrow whose
//! id is the id of its chip carries no id of its own in the drawing. A writer
//! that draws a count chip, and a stub chip with the id of its edge, inside
//! the element of the box it hangs at keeps every id of the drawing unique.

use alloc::{
    format,
    string::String,
    vec::Vec, //
};

use crate::{
    id::ElementId,
    model::Kind,
    view::Level, //
};

/// The height of a box header, 30 px.
pub(crate) const HEADER_CP: i32 = 3000;

/// The height of a row, 22 px at 13 px type, which is the comfortable density.
///
/// The compact density of the viewer is a choice of its panel, and a scene is
/// the same on every machine and in every document, so the core draws the
/// comfortable one.
pub(crate) const ROW_CP: i32 = 2200;

/// The horizontal padding of a box on either side of its text, 12 px.
pub(crate) const PADDING_CP: i32 = 1200;

/// The size of the text of a box header and of its rows, 13 px.
pub(crate) const TEXT_CP: i32 = 1300;

/// The size of the text of a chip, 11 px.
pub(crate) const CHIP_TEXT_CP: i32 = 1100;

/// The height of a chip, 20 px, which is lower than a header so a reader of
/// the drawing tells a chip from a box.
pub(crate) const CHIP_CP: i32 = 2000;

/// The horizontal padding of a chip on either side of its text, 8 px, a step
/// of the 4 px base.
pub(crate) const CHIP_PADDING_CP: i32 = 800;

/// The length over which the edges into one target merge into a trunk, 24 px.
pub(crate) const TRUNK_CP: i32 = 2400;

/// The laid out picture of one view, a display list in integer centipixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    /// The width of the drawing.
    pub width: i32,
    /// The height of the drawing.
    pub height: i32,
    /// The boxes, in the order of the boxes of the view.
    pub boxes: Vec<SceneBox>,
    /// The curves in the order of the edges of the view, the arrow of a stub
    /// in the place of its edge and an edge merged into its opposite left
    /// out, then the arrows of the stubs of the view in the order of its
    /// chips. In the cluster map they are the arcs of its trunks, one per
    /// trunk in the order of the trunks.
    pub edges: Vec<SceneEdge>,
    /// The chips, the count chips in the order of the chips of the view, then
    /// the chips of the stubs in the order of their arrows.
    pub chips: Vec<SceneChip>,
    /// The trunks, in the order of their first curve.
    pub trunks: Vec<Trunk>,
    /// The position of the focus in [`Scene::boxes`], when the view has one.
    pub focus: Option<u32>,
    /// The steps the layout took, counted against
    /// [`MAX_STEPS`](crate::layout::MAX_STEPS).
    pub iterations: u32,
}

/// A box of a scene, one type or one cluster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneBox {
    /// The id, `TYPE::<name>` or, in the cluster map, `CLUSTER::<slug>`.
    pub id: ElementId,
    /// The kind of the type, which a cluster box of the map gives as
    /// [`Kind::Object`].
    pub kind: Kind,
    /// The level of detail the box is drawn at.
    pub level: Level,
    /// The place and the size.
    pub rect: Rect,
    /// The name, cut with an ellipsis after 48 cells.
    pub title: String,
    /// The drawn rows, in the order of the transforms.
    pub rows: Vec<BoxRow>,
    /// The count of the rows the box leaves out, which its last row counts.
    pub more: u32,
    /// Whether the type is one of the roots.
    pub root: bool,
    /// The marks of the box, of which only `unreachable` applies to a box.
    pub marks: Marks,
    /// The count of the inherited fields folded into one row.
    pub inherited_folded: u32,
}

/// A drawn row of a box, one field, input field, enum value or, in the
/// cluster map, one type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxRow {
    /// The id of the member the row draws.
    pub id: ElementId,
    /// The name, cut with an ellipsis after 48 cells, with the count of the
    /// arguments in parentheses at the focus.
    pub name: String,
    /// The type expression, such as `[Project!]!`, or the empty string.
    pub type_text: String,
    /// The height of the port of the row, the middle of its band, where an
    /// edge leaves it.
    pub port_y: i32,
    /// The marks of the row, which the focus draws alone.
    pub marks: Marks,
}

/// A curve of a scene, one edge of the view, the arrow of a stub, or the arc
/// of a trunk of the cluster map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneEdge {
    /// The id, the source id, ` => ` and the id of the target type, or, for
    /// the arrow of a stub that the grammar names no edge from its box for,
    /// the id of its chip, which a writer does not repeat on the arrow. The
    /// arc of a trunk of the cluster map carries `CLUSTER::<a>--<b>`, the
    /// slugs of the two cluster boxes it joins, which is no edge of the
    /// grammar, and its style is [`EdgeStyle::Field`].
    pub id: ElementId,
    /// The style the kind of the edge gives.
    pub style: EdgeStyle,
    /// The point the curve leaves, on a side of a row or of a header.
    pub from: Point,
    /// The first control point, level with `from`.
    pub c1: Point,
    /// The second control point, level with `to`.
    pub c2: Point,
    /// The point the curve enters, on a side of a header or of a chip.
    pub to: Point,
    /// Whether the curve stands for a pair of opposite references and carries
    /// a head at both ends.
    pub two_heads: bool,
    /// The position in [`Scene::trunks`] of the trunk the curve merges into.
    pub trunk: Option<u32>,
}

/// A chip of a scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneChip {
    /// The id, the box a count chip hangs at, or the type or the edge a stub
    /// chip stands for.
    pub id: ElementId,
    /// The place and the size.
    pub rect: Rect,
    /// The text, `+N more`, `N implementations` or the name of a type.
    pub text: String,
    /// The element a click on the chip opens.
    pub target: Option<ElementId>,
}

/// The end of the curves that merge into one target over their last 24 px.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trunk {
    /// The point where the curves meet, 24 px before the side they enter, or
    /// the middle of the arc between two cluster boxes of the map.
    pub at: Point,
    /// The count of the curves, or of the edges between two clusters.
    pub count: u32,
    /// The width of the stroke.
    pub width_cp: i32,
}

/// The marks of a box or a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
// Each mark is drawn on its own, a strike, a hatch, a hollow mark or a
// corner, and any of them may meet, so the four flags are no state machine.
#[expect(clippy::struct_excessive_bools)]
pub struct Marks {
    /// The row is a Relay connection the collapse folds.
    pub relay: bool,
    /// The member is deprecated.
    pub deprecated: bool,
    /// The deprecation comes from a line of the description, the hollow mark.
    pub from_tag: bool,
    /// No root reaches the type, the grey corner mark.
    pub unreachable: bool,
}

/// The style of a curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeStyle {
    /// A field edge, solid.
    Field,
    /// An argument edge, dash dot with a hollow head.
    Argument,
    /// An implements edge, dotted with a hollow triangle head.
    Implements,
    /// A union member edge, dashed.
    UnionMember,
}

/// An axis aligned rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// The left side.
    pub x: i32,
    /// The top side.
    pub y: i32,
    /// The width.
    pub w: i32,
    /// The height.
    pub h: i32,
}

/// A point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    /// The horizontal coordinate.
    pub x: i32,
    /// The vertical coordinate.
    pub y: i32,
}

impl SceneBox {
    /// Returns the text of the row that counts the rows the box leaves out,
    /// when it leaves out any.
    ///
    /// The row reads `+N fields`, `+N values` in an enum and `+N types` in a
    /// cluster box of the map.
    #[must_use]
    pub(crate) fn more_label(&self) -> Option<String> {
        (self.more > 0).then(|| more_label(&self.id, self.kind, self.more))
    }

    /// Returns the text of the row that stands for the folded inherited
    /// fields, when the box folds any.
    #[must_use]
    pub(crate) fn inherited_label(&self) -> Option<String> {
        (self.inherited_folded > 0).then(|| inherited_label(self.inherited_folded))
    }
}

/// Returns the text of the row that counts `count` rows a box of `kind`
/// whose id is `id` leaves out.
pub(crate) fn more_label(id: &ElementId, kind: Kind, count: u32) -> String {
    let what = match (id, kind) {
        (ElementId::Cluster(_), _) => "types",
        (_, Kind::Enum) => "values",
        _ => "fields",
    };
    format!("+{count} {what}")
}

/// Returns the text of the row that stands for `count` folded inherited
/// fields.
pub(crate) fn inherited_label(count: u32) -> String {
    format!("{count} inherited")
}
