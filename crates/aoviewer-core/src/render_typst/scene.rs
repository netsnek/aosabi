// SPDX-License-Identifier: MIT
/*
 * aoviewer-core render_typst scene: the Typst form of a scene as data for the
 * theme.
 *
 * A diagram is one call of the theme with every box, row, edge, chip and trunk
 * as data in points.
 */

//! The Typst scene writer.
//!
//! [`render_typst_scene`] writes a [`Scene`] as the `#sv-diagram(..)` call of
//! `docs/documentation-generator.md` section 8.1, from the same scene the SVG
//! writer draws, so text stays text, selectable and searchable, and the
//! drawing functions of `sabi-theme.typ` turn the data into place, rectangle,
//! text and curve primitives.
//!
//! Every coordinate is a length in points, the centipixels of the scene times
//! 3/400, since a pixel is 0.75 pt. The quotient is written as an exact
//! decimal, because 400 divides ten thousand, so four decimals always suffice
//! and no coordinate is rounded.
//!
//! The call names `width` and `height` and four arrays of dictionaries.
//!
//! - `boxes` in the order of the scene, each with its `id`, its `kind`, its
//!   place `x`, `y`, `w` and `h`, its `name` and the `link` label of the
//!   page of its type, and its `rows`, each an array of the name, the type
//!   expression, the height of its port and the marks `relay`,
//!   `deprecated` and `tag` it carries. The count of its folded inherited
//!   fields is `inherited` and the count of the rows it leaves out is
//!   `more`, as in the example of the section, and the theme words both
//!   rows. The flags `root`, `focus`, `unreachable` and `cluster` are
//!   written only when they hold.
//! - `edges` in the order of the scene, each a cubic curve `from` over `c1`
//!   and `c2` to `to` with its `style`, the direction of the `head` at its end
//!   unless a trunk draws that head, and the direction of the `tail` head of a
//!   pair of opposite references. An arc of the cluster map carries the
//!   `width` of its trunk and no head.
//! - `trunks`, each the last 24 px of its curves from `from` to `to` in its
//!   `width`, with one `head` in the `style` of its first curve and its
//!   `count`.
//! - `chips` in the order of the scene, then the count chip at the middle of
//!   every arc of the cluster map, each with its place, its `text`, the
//!   `target` label of the element it opens and `stub` when it stands for what
//!   the drawing leaves out, which prints the page number of its target.
//!
//! The rules of the heads, of the trunks and of the chips are the ones of the
//! SVG writer, so both drawings of one scene agree. A label a scene names may
//! be one the document does not hold, a type drawn apart from its page, so the
//! theme links a box and prints a page number only when the document holds the
//! label.
//!
//! Every loop runs over the boxes, rows, edges, chips or trunks of one scene,
//! whose counts the steps of the layout cap, or over the four decimals of one
//! coordinate.

use alloc::{
    format,
    string::String,
    vec,
    vec::Vec, //
};
use core::fmt::Write;

use super::{
    Seq,
    element_label,
    push_label,
    push_number,
    typst_str, //
};
use crate::{
    id::ElementId,
    layout::measure::chip_width,
    model::Model,
    page::kind_word,
    scene::{
        BoxRow,
        CHIP_CP,
        EdgeStyle,
        Point,
        Rect,
        Scene,
        SceneBox,
        SceneEdge, //
    },
    schema::Schema,
};

/// The divisor of a length in centipixels that gives points, over the
/// factor [`PT_FACTOR`], since 1 cp is 3/400 pt.
const PT_DIVISOR: u64 = 400;

/// The factor of a length in centipixels that gives points, over
/// [`PT_DIVISOR`].
const PT_FACTOR: i64 = 3;

/// The decimals of a length in points, four, since 400 divides ten
/// thousand.
const DECIMALS: usize = 4;

/// The scale of the fraction of a length in points, ten thousand over
/// [`PT_DIVISOR`].
const FRACTION_SCALE: u64 = 25;

/// The indentation of an element of an array of the call.
const ELEMENT_INDENT: &str = "\n    ";

/// The indentation of the end of an array of the call.
const END_INDENT: &str = "\n  ";

/// The `#sv-diagram(..)` call of `<gen>` section 8.1, every box, row, edge,
/// chip and trunk as data in points, `cpx * 3 / 400` written as an exact
/// decimal.
///
/// The call is written at the start of a line of markup and ends with a line
/// feed, and the ids are written in the text form of the grammar, whose
/// names the model of `schema` holds, the schema the scene was laid out
/// from. The labels of the links are the anchors of `schema`, and a box or a
/// chip whose element no page of the tree documents carries none.
///
/// # Panics
///
/// Panics when an id of `scene` holds a name beyond the name table of the
/// model of `schema`, which a scene laid out from another schema can do.
pub fn render_typst_scene(scene: &Scene, schema: &Schema, out: &mut String) {
    let draw = Draw::new(scene, schema);
    out.push_str("#sv-diagram(width: ");
    push_pt(scene.width, out);
    out.push_str(", height: ");
    push_pt(scene.height, out);
    out.push_str(",\n  boxes: ");
    draw.boxes(out);
    out.push_str(",\n  edges: ");
    draw.edges(out);
    out.push_str(",\n  trunks: ");
    draw.trunks(out);
    out.push_str(",\n  chips: ");
    draw.chips(out);
    out.push_str(")\n");
}

/// Writes the length `cp` in centipixels as a Typst length in points.
///
/// The length is `cp * 3 / 400` points with no rounding, the integer part
/// and at most four decimals without trailing zeros, such as `0.9375pt` for
/// 125 cp and `-0.375pt` for -50 cp.
fn push_pt(cp: i32, out: &mut String) {
    let scaled = i64::from(cp).saturating_mul(PT_FACTOR);
    if scaled < 0 {
        out.push('-');
    }
    let magnitude = scaled.unsigned_abs();
    push_number(magnitude / PT_DIVISOR, out);
    let fraction = (magnitude % PT_DIVISOR) * FRACTION_SCALE;
    if fraction != 0 {
        // Writing into a `String` cannot fail.
        let mut digits = String::new();
        let _ = write!(digits, ".{fraction:0DECIMALS$}");
        out.push_str(digits.trim_end_matches('0'));
    }
    out.push_str("pt");
}

/// Writes the point `p` as an array of two lengths.
fn push_point(p: Point, out: &mut String) {
    out.push('(');
    push_pt(p.x, out);
    out.push_str(", ");
    push_pt(p.y, out);
    out.push(')');
}

/// Writes the place and the size of `r` as the keys `x`, `y`, `w` and `h`.
fn push_rect(r: Rect, out: &mut String) {
    for (key, value) in [("x: ", r.x), (", y: ", r.y), (", w: ", r.w), (", h: ", r.h)] {
        out.push_str(key);
        push_pt(value, out);
    }
}

/// Writes the key `key` with the label `target`, when there is one.
fn push_target(key: &str, target: Option<&str>, out: &mut String) {
    if let Some(target) = target {
        out.push_str(key);
        push_label(target, out);
    }
}

/// An array of the call being written, one element per line, each followed
/// by a comma, which also keeps an array of one element an array.
struct Lines {
    /// Whether an element was written.
    any: bool,
}

impl Lines {
    /// Opens an array of the call in `out`.
    fn open(out: &mut String) -> Lines {
        out.push('(');
        Lines { any: false }
    }

    /// Starts the next element in `out` on a line of its own.
    fn next(&mut self, out: &mut String) {
        out.push_str(ELEMENT_INDENT);
        self.any = true;
    }

    /// Ends the element just written in `out`.
    fn end(out: &mut String) {
        out.push(',');
    }

    /// Closes the array in `out`, its end on a line of its own when it holds
    /// an element.
    fn close(self, out: &mut String) {
        if self.any {
            out.push_str(END_INDENT);
        }
        out.push(')');
    }
}

/// Returns the word of the style of a curve, which the theme reads.
fn style_word(style: EdgeStyle) -> &'static str {
    match style {
        EdgeStyle::Field => "field",
        EdgeStyle::Argument => "argument",
        EdgeStyle::Implements => "implements",
        EdgeStyle::UnionMember => "union",
    }
}

/// Returns 1 when `to` lies right of `from`, -1 when it lies left of it, and
/// `tie` when the two are equal.
fn toward(from: i32, to: i32, tie: i32) -> i32 {
    match to.cmp(&from) {
        core::cmp::Ordering::Greater => 1,
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => tie,
    }
}

/// Returns the direction the curve `edge` enters its end in, positive to the
/// right, from its horizontal tangent there.
fn end_direction(edge: &SceneEdge) -> i32 {
    toward(edge.c2.x, edge.to.x, toward(edge.from.x, edge.to.x, 1))
}

/// Returns the direction the second head of the curve `edge` points in at its
/// start, back into the box it leaves.
fn start_direction(edge: &SceneEdge) -> i32 {
    toward(edge.c1.x, edge.from.x, toward(edge.to.x, edge.from.x, -1))
}

/// Returns whether `edge` is the arc of a trunk of the cluster map, whose id
/// is the cluster id of the two slugs it joins.
fn is_arc(edge: &SceneEdge) -> bool {
    matches!(edge.id, ElementId::Cluster(_))
}

/// The state of one call of [`render_typst_scene`].
struct Draw<'a> {
    /// The scene.
    scene: &'a Scene,
    /// The schema the scene was laid out from.
    schema: &'a Schema,
    /// The ids of the boxes, sorted, which tell a count chip from a stub
    /// chip.
    box_ids: Vec<&'a ElementId>,
    /// The position of the first curve of each trunk.
    first: Vec<Option<usize>>,
}

impl<'a> Draw<'a> {
    /// Returns the drawing of `scene` with its sorted box ids and the first
    /// curve of every trunk.
    fn new(scene: &'a Scene, schema: &'a Schema) -> Draw<'a> {
        let mut box_ids: Vec<&ElementId> = scene.boxes.iter().map(|b| &b.id).collect();
        box_ids.sort_unstable();
        let mut first = vec![None; scene.trunks.len()];
        for (e, edge) in scene.edges.iter().enumerate() {
            let slot = edge.trunk.and_then(|t| first.get_mut(t as usize));
            if let Some(slot) = slot {
                slot.get_or_insert(e);
            }
        }
        Draw {
            scene,
            schema,
            box_ids,
            first,
        }
    }

    /// Returns the model whose names the ids carry.
    fn model(&self) -> &'a Model {
        &self.schema.model
    }

    /// Writes the array of the boxes.
    fn boxes(&self, out: &mut String) {
        let mut lines = Lines::open(out);
        for (b, sb) in self.scene.boxes.iter().enumerate() {
            lines.next(out);
            let focus = self.scene.focus == u32::try_from(b).ok();
            self.scene_box(sb, focus, out);
            Lines::end(out);
        }
        lines.close(out);
    }

    /// Writes the dictionary of the box `sb`.
    fn scene_box(&self, sb: &SceneBox, focus: bool, out: &mut String) {
        out.push_str("(id: ");
        typst_str(&sb.id.text(self.model()), out);
        out.push_str(", kind: ");
        typst_str(kind_word(sb.kind), out);
        out.push_str(", ");
        push_rect(sb.rect, out);
        out.push_str(", name: ");
        typst_str(&sb.title, out);
        let link = element_label(self.schema, &sb.id);
        push_target(", link: ", link.as_deref(), out);
        out.push_str(", rows: ");
        let mut rows = Seq::open(out);
        for row in &sb.rows {
            rows.next(out);
            push_row(row, out);
        }
        rows.close(out);
        for (key, count) in [
            (", inherited: ", sb.inherited_folded),
            (", more: ", sb.more),
        ] {
            if count > 0 {
                out.push_str(key);
                push_number(count, out);
            }
        }
        let flags = [
            (", root: true", sb.root),
            (", focus: true", focus),
            (", unreachable: true", sb.marks.unreachable),
            (", cluster: true", matches!(sb.id, ElementId::Cluster(_))),
        ];
        for (flag, holds) in flags {
            if holds {
                out.push_str(flag);
            }
        }
        out.push(')');
    }

    /// Writes the array of the curves.
    fn edges(&self, out: &mut String) {
        let mut lines = Lines::open(out);
        for edge in &self.scene.edges {
            lines.next(out);
            self.edge(edge, out);
            Lines::end(out);
        }
        lines.close(out);
    }

    /// Writes the dictionary of the curve `edge`.
    ///
    /// A curve that merges into a trunk leaves its head to the trunk, and the
    /// arc of a trunk of the cluster map is drawn in the width of its trunk
    /// with no head, since it joins two clusters in no direction.
    fn edge(&self, edge: &SceneEdge, out: &mut String) {
        let trunk = edge.trunk.and_then(|t| self.scene.trunks.get(t as usize));
        let arc = is_arc(edge);
        for (key, p) in [
            ("(from: ", edge.from),
            (", c1: ", edge.c1),
            (", c2: ", edge.c2),
            (", to: ", edge.to),
        ] {
            out.push_str(key);
            push_point(p, out);
        }
        out.push_str(", style: ");
        let style = if arc { EdgeStyle::Field } else { edge.style };
        typst_str(style_word(style), out);
        if arc {
            if let Some(trunk) = trunk {
                out.push_str(", width: ");
                push_pt(trunk.width_cp, out);
            }
        } else {
            if trunk.is_none() {
                out.push_str(", head: ");
                push_number(end_direction(edge), out);
            }
            if edge.two_heads {
                out.push_str(", tail: ");
                push_number(start_direction(edge), out);
            }
        }
        out.push(')');
    }

    /// Writes the array of the trunks that are no arc of the cluster map,
    /// each from its meeting point to the side its curves enter.
    fn trunks(&self, out: &mut String) {
        let mut lines = Lines::open(out);
        for (t, trunk) in self.scene.trunks.iter().enumerate() {
            let Some(edge) = self.first[t].map(|e| &self.scene.edges[e]) else {
                continue;
            };
            if is_arc(edge) {
                continue;
            }
            lines.next(out);
            out.push_str("(from: ");
            push_point(trunk.at, out);
            out.push_str(", to: ");
            push_point(edge.to, out);
            out.push_str(", width: ");
            push_pt(trunk.width_cp, out);
            out.push_str(", style: ");
            typst_str(style_word(edge.style), out);
            out.push_str(", head: ");
            push_number(toward(trunk.at.x, edge.to.x, end_direction(edge)), out);
            out.push_str(", count: ");
            push_number(trunk.count, out);
            out.push(')');
            Lines::end(out);
        }
        lines.close(out);
    }

    /// Writes the array of the chips, the chips of the scene and then the
    /// count chip at the middle of every arc of the cluster map.
    fn chips(&self, out: &mut String) {
        let mut lines = Lines::open(out);
        for chip in &self.scene.chips {
            lines.next(out);
            out.push('(');
            push_rect(chip.rect, out);
            out.push_str(", text: ");
            typst_str(&chip.text, out);
            let stub = self.box_ids.binary_search(&&chip.id).is_err();
            if stub {
                out.push_str(", stub: true");
            }
            let target = chip
                .target
                .as_ref()
                .and_then(|t| element_label(self.schema, t));
            push_target(", target: ", target.as_deref(), out);
            out.push(')');
            Lines::end(out);
        }
        for edge in self.scene.edges.iter().filter(|e| is_arc(e)) {
            let Some(trunk) = edge.trunk.and_then(|t| self.scene.trunks.get(t as usize)) else {
                continue;
            };
            let label = format!("{}", trunk.count);
            let w = chip_width(&label);
            let rect = Rect {
                x: trunk.at.x.saturating_sub(w / 2),
                y: trunk.at.y.saturating_sub(CHIP_CP / 2),
                w,
                h: CHIP_CP,
            };
            lines.next(out);
            out.push('(');
            push_rect(rect, out);
            out.push_str(", text: ");
            typst_str(&label, out);
            out.push(')');
            Lines::end(out);
        }
        lines.close(out);
    }
}

/// Writes the drawn row `row` as an array of its name, its type expression,
/// the height of its port and, when it carries any, its marks.
fn push_row(row: &BoxRow, out: &mut String) {
    out.push('(');
    typst_str(&row.name, out);
    out.push_str(", ");
    typst_str(&row.type_text, out);
    out.push_str(", ");
    push_pt(row.port_y, out);
    let marks = [
        ("relay", row.marks.relay),
        ("deprecated", row.marks.deprecated),
        ("tag", row.marks.deprecated && row.marks.from_tag),
    ];
    if marks.iter().any(|&(_, holds)| holds) {
        out.push_str(", ");
        let mut seq = Seq::open(out);
        for (word, holds) in marks {
            if holds {
                seq.next(out);
                typst_str(word, out);
            }
        }
        seq.close(out);
    }
    out.push(')');
}
