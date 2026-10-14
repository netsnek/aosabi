// SPDX-License-Identifier: MIT
/*
 * aoviewer-core render_svg: the SVG writer of a scene in the document form.
 *
 * The document form carries literal presentation attributes of one theme and
 * survives the sanitizer of a Markdown host.
 */

//! The SVG writer.
//!
//! [`render_svg`] writes a [`Scene`] as one SVG document in the literal form of
//! one theme, which is the file the generator writes and the export of the
//! viewer, byte for byte.
//!
//! The document follows the rules a Markdown host's sanitizer imposes. It uses
//! presentation attributes only, no `style` element and no baseline attribute,
//! positions every run with coordinates and a text anchor, and carries an image
//! role, a title and the stable ids, one group with the id of its element per
//! box, per drawn row and per edge. The font family names Sabi Mono and its
//! fallbacks, every run carries its measured width as `textLength`, and the 8
//! percent of slack is the layout's.
//!
//! The drawing goes from the back to the front: the canvas, the curves of the
//! edges with their heads, the trunks, the boxes with their rows and the chips
//! drawn inside them, and last the chips that stand alone. A curve runs in a
//! gutter, so a box drawn over it hides nothing of it.
//!
//! A box is a card with a 4 px accent bar on its left edge, a 30 px header in
//! the tint of its kind, a 6 px radius and a 1 px border in its accent. The
//! border carries the kind a third time, dashed for an interface, double for a
//! union, with a notch on the left edge for an input, a pill header for an enum
//! and a pill with no body for a scalar. A header holds the letter badge, the
//! name and, at a root, the root mark, and a row its name, its type in the
//! muted ink and at the focus its marks, in the cells the layout measured them
//! in. A deprecated row is struck through and hatched, and a type no root
//! reaches carries a grey corner mark. The focus has a 2 px border, since
//! weight means state. A cluster box of the map carries no letter badge, since
//! a cluster is no type.
//!
//! An edge is its cubic curve, 1.25 px in the edge neutral, solid for a field,
//! dashed 6 4 for a union member, dotted 1 4 for an implementation and dash dot
//! 6 3 1 3 for an argument, with a 6 by 5 px head that is filled for the first
//! two and hollow for the last two. A curve that merges into a trunk carries no
//! head, and the trunk is drawn over its last 24 px in its width with one head
//! for all its curves, in the style of its first curve. A head grows with the
//! stroke it ends, by twice the extra width in length and once in half width,
//! so it stays wider than a trunk. The arc of a trunk of the cluster map is
//! drawn in the width of its trunk with no head, since it joins two clusters in
//! no direction, and its count chip sits at the middle of the arc.
//!
//! A chip goes where its id stays unique. A count chip carries the id of its
//! box and is drawn inside the group of that box with no id of its own. A stub
//! chip whose id is the id of its edge is drawn inside the group of the box its
//! arrow leaves, as a group with that id that holds the arrow and the chip, so
//! the arrow carries no id of its own. Every other chip is a group of its own
//! with its id, and it holds its arrow as well when the arrow carries the id of
//! the chip.
//!
//! Every loop runs over the boxes, rows, edges, chips or trunks of one scene,
//! whose counts the steps of the layout cap, over the two label rows of one
//! box, over the characters of one string, or under a cap named here.

mod tokens;
mod write;

use alloc::{
    format,
    string::String,
    vec,
    vec::Vec, //
};

use tokens::{
    KindColours,
    Neutrals, //
};
use write::{
    Corners,
    Paint,
    PathData,
    Run,
    pt, //
};

use crate::{
    id::ElementId,
    layout::measure::{
        BADGE_CELLS,
        GAP_CELLS,
        MARK_CELLS,
        chip_width, //
    },
    model::{
        Kind,
        Model, //
    },
    scene::{
        BoxRow,
        CHIP_CP,
        CHIP_PADDING_CP,
        CHIP_TEXT_CP,
        EdgeStyle,
        HEADER_CP,
        PADDING_CP,
        Point,
        ROW_CP,
        Rect,
        Scene,
        SceneBox,
        SceneEdge,
        TEXT_CP, //
    },
    schema::Schema,
    text::{
        cells,
        run_width, //
    },
};

/// The theme a scene is written in, one file per theme.
///
/// A file cannot switch its theme, so a generated SVG carries the literal
/// colours of one theme and exists once per theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    /// The light theme, the default of paper.
    Light,
    /// The dark theme.
    Dark,
}

/// How the text of a drawing is written.
///
/// Text stays text by default, selectable and set in the font family of the
/// drawing, and outlines convert it to paths for an exact rendering at the cost
/// of selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgText {
    /// Every run is a `<text>` element with its `textLength`.
    Text,
    /// Every run is converted to the outlines of Sabi Mono.
    Outlines,
}

/// A reason the SVG writer writes no drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgError {
    /// [`SvgText::Outlines`] was asked for, and the outlines need the face,
    /// which vendors.
    OutlinesNeedFont,
}

/// The font family of every run, Sabi Mono and its fallbacks.
const FONT_FAMILY: &str =
    "Sabi Mono, DejaVu Sans Mono, Menlo, Consolas, Liberation Mono, monospace";

/// The width of the accent bar on the left edge of a box, 4 px.
const BAR: i32 = 400;

/// The radius of a box, 6 px.
const RADIUS: i32 = 600;

/// The border of a box and of a chip, 1 px.
const BORDER: i32 = 100;

/// The border of the focus, 2 px.
const FOCUS_BORDER: i32 = 200;

/// The dash pattern of the border of an interface, in pixels.
const INTERFACE_DASH: &str = "4 3";

/// The inset of the inner border of a union, 3 px.
const DOUBLE_INSET: i32 = 300;

/// The depth of the notch of an input, 5 px, which cuts through the bar.
const NOTCH_DEPTH: i32 = 500;

/// Half the height of the notch of an input, 4 px.
const NOTCH_HALF: i32 = 400;

/// The inset of the pill header of an enum inside its header, 3 px.
const PILL_INSET: i32 = 300;

/// The legs of the grey corner mark of a type no root reaches, 10 px, inside
/// the 12 px padding. A corner whose radius is longer, the 15 px end of the
/// pill of a scalar, gives the mark legs as long as its radius, so the mark
/// lies between the curve of that corner and its chord. That curve strays at
/// most 0.03 percent of the radius beyond the quarter circle of the pill, about
/// 0.004 px at 15 px, under its border.
const CORNER_MARK: i32 = 1000;

/// The radius of a chip, 4 px, a step of the 4 px base.
const CHIP_RADIUS: i32 = 400;

/// Half the size of a mark of a row and of the root mark, 3 px, inside one cell
/// of 13 px text.
const MARK_RADIUS: i32 = 300;

/// The distance of two lines of the hatching of a deprecated row, 8 px, two
/// steps of the 4 px base.
const HATCH_STEP: i32 = 800;

/// The most lines the hatching of one row draws. The line `i` ends at the top
/// of the row `i` times [`HATCH_STEP`] right of the bar, so the last one ends
/// 4088 px right of the bar, 4092 px right of the left side of the box, and a
/// row that reaches further is hatched up to there alone.
const HATCH_LINES: u32 = 512;

/// The stroke of an edge, 1.25 px.
const EDGE_WIDTH: i32 = 125;

/// The length of a head, 6 px.
const HEAD_LENGTH: i32 = 600;

/// Half the width of a head, 2.5 px.
const HEAD_HALF: i32 = 250;

/// The tooltip of the root mark.
const ROOT_TIP: &str = "root type";

/// The tooltip of the corner mark of a type no root reaches.
const UNREACHABLE_TIP: &str = "no root reaches this type";

/// The tooltip of the Relay mark, which a drawn mark carries in the place of
/// Voyager's icon.
const RELAY_TIP: &str = "Relay connection";

/// The tooltip of the deprecation mark of a deprecated member.
const DEPRECATED_TIP: &str = "deprecated";

/// The tooltip of the hollow mark of a member its description marks deprecated.
const TAG_TIP: &str = "deprecated in its description";

/// Writes `scene` as an SVG document in the literal colours of `theme`, with
/// the title `title`, and appends it to `out`.
///
/// The ids of the boxes, rows, edges and chips are written in the text form of
/// the grammar, whose names the model of `schema` holds, the schema the scene
/// was laid out from, as the Typst writer of a scene takes it. The document is
/// the file the generator writes for a diagram and the SVG export of the
/// viewer, and it carries no timestamp, no path and no host name.
///
/// # Errors
///
/// Returns [`SvgError::OutlinesNeedFont`] and appends nothing when `text` is
/// [`SvgText::Outlines`], since the core carries no face yet.
///
/// # Panics
///
/// Panics when an id of `scene` holds a name beyond the name table of the
/// model of `schema`, which a scene laid out from another schema can do.
pub fn render_svg(
    scene: &Scene,
    schema: &Schema,
    theme: Theme,
    text: SvgText,
    title: &str,
    out: &mut String,
) -> Result<(), SvgError> {
    if text == SvgText::Outlines {
        return Err(SvgError::OutlinesNeedFont);
    }
    let draw = Draw::new(scene, &schema.model, theme);
    draw.open(title, out);
    draw.edges(out);
    draw.trunks(out);
    for b in 0..scene.boxes.len() {
        draw.scene_box(b, out);
    }
    draw.lone_chips(out);
    out.push_str("</svg>\n");
    Ok(())
}

/// Where a chip is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    /// Inside the group of the box whose id it carries, with no id of its
    /// own.
    Count,
    /// Inside the group of the box its arrow leaves, as a group with its id
    /// that holds the arrow at the position `arrow` of the curves.
    Stub {
        /// The arrow.
        arrow: usize,
    },
    /// As a group of its own with its id, which holds the arrow at the
    /// position `arrow` of the curves when the arrow carries the same id.
    Lone {
        /// The arrow, when it carries the id of the chip.
        arrow: Option<usize>,
    },
}

/// The state of one drawing.
struct Draw<'a> {
    /// The scene.
    scene: &'a Scene,
    /// The model whose names the ids carry.
    model: &'a Model,
    /// The theme.
    theme: Theme,
    /// The neutral colours of the theme.
    neutral: Neutrals,
    /// Where each chip is drawn.
    places: Vec<Place>,
    /// The positions of the chips each box holds, in the order of the chips.
    held: Vec<Vec<usize>>,
    /// Whether each curve is drawn inside the group of a chip.
    in_chip: Vec<bool>,
    /// The position of the first curve of each trunk.
    first: Vec<Option<usize>>,
}

impl<'a> Draw<'a> {
    /// Returns the drawing of `scene` in `theme`, with the place of every
    /// chip and the first curve of every trunk.
    fn new(scene: &'a Scene, model: &'a Model, theme: Theme) -> Draw<'a> {
        let mut draw = Draw {
            scene,
            model,
            theme,
            neutral: tokens::neutrals(theme),
            places: Vec::with_capacity(scene.chips.len()),
            held: vec![Vec::new(); scene.boxes.len()],
            in_chip: vec![false; scene.edges.len()],
            first: vec![None; scene.trunks.len()],
        };
        draw.place_chips();
        for (e, edge) in scene.edges.iter().enumerate() {
            let slot = edge.trunk.and_then(|t| draw.first.get_mut(t as usize));
            if let Some(slot) = slot {
                slot.get_or_insert(e);
            }
        }
        draw
    }

    /// Decides where every chip is drawn and which curves the chips hold.
    ///
    /// A chip whose id is the id of a box is a count chip. A chip whose id is
    /// the id of a curve holds the first curve with that id when no chip before
    /// it holds that curve, and goes into the box the curve leaves from the
    /// right side of a row or of a header when its id is an edge id.
    fn place_chips(&mut self) {
        let scene = self.scene;
        let boxes = sorted_ids(scene.boxes.iter().map(|b| &b.id));
        let curves = sorted_ids(scene.edges.iter().map(|e| &e.id));
        for (k, chip) in scene.chips.iter().enumerate() {
            if let Some(b) = find_id(&boxes, &chip.id) {
                self.held[b].push(k);
                self.places.push(Place::Count);
                continue;
            }
            let arrow = find_id(&curves, &chip.id).filter(|&e| !self.in_chip[e]);
            if let Some(e) = arrow {
                self.in_chip[e] = true;
            }
            let owner = match (arrow, &chip.id) {
                (Some(e), ElementId::Edge(..)) => leaving(scene, &scene.edges[e]).map(|b| (b, e)),
                _ => None,
            };
            let place = match owner {
                Some((b, e)) => {
                    self.held[b].push(k);
                    Place::Stub { arrow: e }
                }
                None => Place::Lone { arrow },
            };
            self.places.push(place);
        }
    }

    /// Appends the root element, the title and the canvas.
    ///
    /// The root carries the font family and the size of the text of a box,
    /// which every run inherits and a chip run overrides.
    fn open(&self, title: &str, out: &mut String) {
        let (w, h) = (self.scene.width, self.scene.height);
        out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\"");
        write::len_attr(out, "width", w);
        write::len_attr(out, "height", h);
        out.push_str(" viewBox=\"0 0 ");
        write::num(out, w);
        out.push(' ');
        write::num(out, h);
        out.push('"');
        write::lit_attr(out, "role", "img");
        write::lit_attr(out, "font-family", FONT_FAMILY);
        write::len_attr(out, "font-size", TEXT_CP);
        out.push_str(">\n<title>");
        write::text(out, title);
        out.push_str("</title>\n");
        let canvas = Rect { x: 0, y: 0, w, h };
        write::rect(out, canvas, 0, Paint::fill(self.neutral.canvas));
    }

    /// Appends every curve no chip holds, each as a group with its id.
    fn edges(&self, out: &mut String) {
        for (e, edge) in self.scene.edges.iter().enumerate() {
            if self.in_chip[e] {
                continue;
            }
            write::open_group(out, Some(&edge.id.text(self.model)));
            if is_arc(edge) {
                self.arc(edge, out);
            } else {
                self.curve(edge, out);
            }
            write::close_group(out);
        }
    }

    /// Appends the curve of `edge` with its heads.
    ///
    /// A curve that merges into a trunk leaves its head to the trunk.
    fn curve(&self, edge: &SceneEdge, out: &mut String) {
        let mut d = PathData::default();
        d.move_to(edge.from);
        d.cubic_to(edge.c1, edge.c2, edge.to);
        let paint = Paint {
            dash: dash(edge.style),
            ..Paint::stroke(self.neutral.edge, EDGE_WIDTH)
        };
        write::path(out, &d.d, paint, None);
        let trunked = edge
            .trunk
            .is_some_and(|t| self.scene.trunks.get(t as usize).is_some());
        if !trunked {
            self.head(edge.to, end_direction(edge), edge.style, EDGE_WIDTH, out);
        }
        if edge.two_heads {
            self.head(
                edge.from,
                start_direction(edge),
                edge.style,
                EDGE_WIDTH,
                out,
            );
        }
    }

    /// Appends the arc of a trunk of the cluster map in the width of its
    /// trunk, and the count chip at the middle of the arc.
    fn arc(&self, edge: &SceneEdge, out: &mut String) {
        let trunk = edge.trunk.and_then(|t| self.scene.trunks.get(t as usize));
        let width = trunk.map_or(EDGE_WIDTH, |t| t.width_cp);
        let mut d = PathData::default();
        d.move_to(edge.from);
        d.cubic_to(edge.c1, edge.c2, edge.to);
        write::path(out, &d.d, Paint::stroke(self.neutral.edge, width), None);
        let Some(trunk) = trunk else {
            return;
        };
        let label = format!("{}", trunk.count);
        let w = chip_width(&label);
        let chip = Rect {
            x: trunk.at.x.saturating_sub(w / 2),
            y: trunk.at.y.saturating_sub(CHIP_CP / 2),
            w,
            h: CHIP_CP,
        };
        self.chip(chip, &label, out);
    }

    /// Appends every trunk that is no arc of the cluster map, its last 24 px
    /// in its width from its meeting point to the side its curves enter, and
    /// its head.
    fn trunks(&self, out: &mut String) {
        for (t, trunk) in self.scene.trunks.iter().enumerate() {
            let Some(edge) = self.first[t].map(|e| &self.scene.edges[e]) else {
                continue;
            };
            if is_arc(edge) {
                continue;
            }
            let mut d = PathData::default();
            d.move_to(trunk.at);
            d.line_to(edge.to);
            write::path(
                out,
                &d.d,
                Paint::stroke(self.neutral.edge, trunk.width_cp),
                None,
            );
            let direction = toward(trunk.at.x, edge.to.x, end_direction(edge));
            self.head(edge.to, direction, edge.style, trunk.width_cp, out);
        }
    }

    /// Appends the head of a stroke of the width `width` in the style
    /// `style`, whose tip is `tip` and which points right when `direction` is
    /// positive and left otherwise.
    fn head(&self, tip: Point, direction: i32, style: EdgeStyle, width: i32, out: &mut String) {
        let extra = width.saturating_sub(EDGE_WIDTH).max(0);
        let length = HEAD_LENGTH.saturating_add(extra.saturating_mul(2));
        let half = HEAD_HALF.saturating_add(extra);
        let base = if direction > 0 {
            tip.x.saturating_sub(length)
        } else {
            tip.x.saturating_add(length)
        };
        let mut d = PathData::default();
        d.move_to(tip);
        d.line_to(pt(base, tip.y.saturating_sub(half)));
        d.line_to(pt(base, tip.y.saturating_add(half)));
        d.close();
        let n = self.neutral;
        let paint = match style {
            EdgeStyle::Field | EdgeStyle::UnionMember => Paint::fill(n.edge),
            EdgeStyle::Implements | EdgeStyle::Argument => Paint {
                fill: n.canvas,
                ..Paint::stroke(n.edge, EDGE_WIDTH)
            },
        };
        write::path(out, &d.d, paint, None);
    }

    /// Appends the box at position `b` as a group with its id, its frame,
    /// its header, its rows and the chips it holds.
    fn scene_box(&self, b: usize, out: &mut String) {
        let sb = &self.scene.boxes[b];
        let colours = tokens::kind(self.theme, sb.kind);
        let focus = self.scene.focus == u32::try_from(b).ok();
        write::open_group(out, Some(&sb.id.text(self.model)));
        if sb.kind == Kind::Scalar {
            self.pill(sb, colours, focus, out);
        } else {
            self.frame(sb, colours, focus, out);
        }
        self.header(sb, colours, out);
        for row in &sb.rows {
            self.row(sb, row, out);
        }
        self.labels(sb, out);
        for &k in &self.held[b] {
            self.held_chip(k, out);
        }
        write::close_group(out);
    }

    /// Appends the frame of the box `sb` of a kind other than a scalar.
    ///
    /// The first rectangle is the outline in the accent, and the card over
    /// all of it but the bar leaves the bar in the accent with the corners of
    /// the outline. The border is drawn last, over the tint and the corner
    /// mark.
    fn frame(&self, sb: &SceneBox, colours: KindColours, focus: bool, out: &mut String) {
        let r = sb.rect;
        write::rect(out, r, RADIUS, Paint::fill(colours.accent));
        let body = Rect {
            x: r.x.saturating_add(BAR),
            w: r.w.saturating_sub(BAR),
            ..r
        };
        let right = Corners {
            tl: 0,
            tr: RADIUS,
            br: RADIUS,
            bl: 0,
        };
        let card = write::rounded(body, right);
        write::path(out, &card, Paint::fill(self.neutral.card), None);
        if sb.kind == Kind::Enum {
            let pill = Rect {
                x: body.x.saturating_add(PILL_INSET),
                y: r.y.saturating_add(PILL_INSET),
                w: body.w.saturating_sub(2 * PILL_INSET),
                h: HEADER_CP - 2 * PILL_INSET,
            };
            write::rect(out, pill, pill.h / 2, Paint::fill(colours.tint));
        } else {
            let header = Rect {
                h: HEADER_CP.min(r.h),
                ..body
            };
            let br = if r.h <= HEADER_CP { RADIUS } else { 0 };
            let tint = write::rounded(header, Corners { br, ..right });
            write::path(out, &tint, Paint::fill(colours.tint), None);
        }
        self.corner(sb, RADIUS, out);
        let border = Paint {
            dash: (sb.kind == Kind::Interface).then_some(INTERFACE_DASH),
            ..Paint::stroke(colours.accent, if focus { FOCUS_BORDER } else { BORDER })
        };
        write::rect(out, r, RADIUS, border);
        match sb.kind {
            Kind::Union => {
                let inner = Rect {
                    x: r.x.saturating_add(DOUBLE_INSET),
                    y: r.y.saturating_add(DOUBLE_INSET),
                    w: r.w.saturating_sub(2 * DOUBLE_INSET),
                    h: r.h.saturating_sub(2 * DOUBLE_INSET),
                };
                let paint = Paint::stroke(colours.accent, BORDER);
                write::rect(out, inner, RADIUS - DOUBLE_INSET, paint);
            }
            Kind::Input => self.notch(r, colours, out),
            _ => {}
        }
    }

    /// Appends the notch of an input on the left edge of `r`, at the middle
    /// of its header, an open path whose fill cuts the bar and the border and
    /// whose two legs are stroked in the accent.
    fn notch(&self, r: Rect, colours: KindColours, out: &mut String) {
        let mid = r.y.saturating_add(HEADER_CP / 2);
        let mut d = PathData::default();
        d.move_to(pt(r.x, mid.saturating_sub(NOTCH_HALF)));
        d.line_to(pt(r.x.saturating_add(NOTCH_DEPTH), mid));
        d.line_to(pt(r.x, mid.saturating_add(NOTCH_HALF)));
        let paint = Paint {
            fill: self.neutral.canvas,
            ..Paint::stroke(colours.accent, BORDER)
        };
        write::path(out, &d.d, paint, None);
    }

    /// Appends the frame of the box `sb` of a scalar, a pill in its tint with
    /// no body.
    fn pill(&self, sb: &SceneBox, colours: KindColours, focus: bool, out: &mut String) {
        let r = sb.rect;
        let radius = r.h.min(HEADER_CP) / 2;
        write::rect(out, r, radius, Paint::fill(colours.tint));
        self.corner(sb, radius, out);
        let width = if focus { FOCUS_BORDER } else { BORDER };
        write::rect(out, r, radius, Paint::stroke(colours.accent, width));
    }

    /// Appends the grey corner mark of `sb` when no root reaches its type,
    /// in its top right corner, whose radius is `radius`.
    fn corner(&self, sb: &SceneBox, radius: i32, out: &mut String) {
        if !sb.marks.unreachable {
            return;
        }
        let d = write::corner_wedge(sb.rect, CORNER_MARK, radius);
        let paint = Paint::fill(self.neutral.border);
        write::path(out, &d, paint, Some(UNREACHABLE_TIP));
    }

    /// Appends the header text of `sb`, the letter badge, the name and, at a
    /// root, the root mark, in the cells the layout measured them in.
    ///
    /// A cluster box of the map draws no badge and starts its name where the
    /// badge would stand.
    fn header(&self, sb: &SceneBox, colours: KindColours, out: &mut String) {
        let r = sb.rect;
        let x = r.x.saturating_add(PADDING_CP);
        let mid = r.y.saturating_add(HEADER_CP / 2);
        let bold = |fill| Run {
            x,
            mid,
            size: TEXT_CP,
            bold: true,
            fill,
        };
        let badge = if matches!(sb.id, ElementId::Cluster(_)) {
            0
        } else {
            write::run(out, bold(colours.accent), tokens::letter(sb.kind));
            BADGE_CELLS
        };
        let name = Run {
            x: x.saturating_add(run_width(badge, TEXT_CP)),
            ..bold(self.neutral.text)
        };
        write::run(out, name, &sb.title);
        if sb.root {
            let cell = badge.saturating_add(cells(&sb.title)).saturating_add(1);
            self.root_mark(pt(cell_middle(x, cell), mid), out);
        }
    }

    /// Appends the root mark centred on `c`, a triangle in the text ink that
    /// points right.
    fn root_mark(&self, c: Point, out: &mut String) {
        let mut d = PathData::default();
        d.move_to(pt(
            c.x.saturating_sub(MARK_RADIUS),
            c.y.saturating_sub(MARK_RADIUS),
        ));
        d.line_to(pt(c.x.saturating_add(MARK_RADIUS), c.y));
        d.line_to(pt(
            c.x.saturating_sub(MARK_RADIUS),
            c.y.saturating_add(MARK_RADIUS),
        ));
        d.close();
        write::path(out, &d.d, Paint::fill(self.neutral.text), Some(ROOT_TIP));
    }

    /// Appends the drawn row `row` of `sb` as a group with its id, its
    /// hatching when it is deprecated, its name, its type and its marks.
    fn row(&self, sb: &SceneBox, row: &BoxRow, out: &mut String) {
        let x = sb.rect.x.saturating_add(PADDING_CP);
        let mid = row.port_y;
        write::open_group(out, Some(&row.id.text(self.model)));
        if row.marks.deprecated {
            self.hatch(sb.rect, mid.saturating_sub(ROW_CP / 2), out);
        }
        let run = Run {
            x,
            mid,
            size: TEXT_CP,
            bold: false,
            fill: self.neutral.text,
        };
        write::run(out, run, &row.name);
        let mut cell = cells(&row.name);
        if row.marks.deprecated {
            self.strike(run, cell, out);
        }
        if !row.type_text.is_empty() {
            let at = x.saturating_add(run_width(cell.saturating_add(GAP_CELLS), TEXT_CP));
            let muted = Run {
                x: at,
                fill: self.neutral.muted,
                ..run
            };
            write::run(out, muted, &row.type_text);
            cell = cell
                .saturating_add(GAP_CELLS)
                .saturating_add(cells(&row.type_text));
        }
        self.row_marks(row, x, cell, out);
        write::close_group(out);
    }

    /// Appends the line that strikes through the first `cells` cells of the
    /// run `run`.
    fn strike(&self, run: Run<'_>, cells: u32, out: &mut String) {
        let y = write::strike(write::baseline(run.mid, run.size), run.size);
        let mut d = PathData::default();
        d.move_to(pt(run.x, y));
        d.line_to(pt(run.x.saturating_add(run_width(cells, run.size)), y));
        write::path(out, &d.d, Paint::stroke(self.neutral.text, BORDER), None);
    }

    /// Appends the marks of `row`, whose text starts at `x` and takes `cell`
    /// cells, the Relay mark and then the deprecation mark, each in the
    /// second of its two cells.
    fn row_marks(&self, row: &BoxRow, x: i32, cell: u32, out: &mut String) {
        let n = self.neutral;
        let mut cell = cell;
        if row.marks.relay {
            self.relay_mark(pt(cell_middle(x, cell.saturating_add(1)), row.port_y), out);
            cell = cell.saturating_add(MARK_CELLS);
        }
        if !row.marks.deprecated {
            return;
        }
        let c = pt(cell_middle(x, cell.saturating_add(1)), row.port_y);
        let (paint, tip) = if row.marks.from_tag {
            let hollow = Paint {
                fill: n.card,
                ..Paint::stroke(n.text, BORDER)
            };
            (hollow, TAG_TIP)
        } else {
            (Paint::fill(n.text), DEPRECATED_TIP)
        };
        write::circle(out, c, MARK_RADIUS, paint, Some(tip));
    }

    /// Appends the Relay mark centred on `c`, a diamond in the text ink.
    fn relay_mark(&self, c: Point, out: &mut String) {
        let mut d = PathData::default();
        d.move_to(pt(c.x, c.y.saturating_sub(MARK_RADIUS)));
        d.line_to(pt(c.x.saturating_add(MARK_RADIUS), c.y));
        d.line_to(pt(c.x, c.y.saturating_add(MARK_RADIUS)));
        d.line_to(pt(c.x.saturating_sub(MARK_RADIUS), c.y));
        d.close();
        write::path(out, &d.d, Paint::fill(self.neutral.text), Some(RELAY_TIP));
    }

    /// Appends the hatching of the row band of the box `r` whose top is
    /// `top`, lines at 45 degrees [`HATCH_STEP`] apart between the bar and
    /// the right side, at most [`HATCH_LINES`] of them.
    fn hatch(&self, r: Rect, top: i32, out: &mut String) {
        let (left, right) = (r.x.saturating_add(BAR), r.x.saturating_add(r.w));
        let bottom = top.saturating_add(ROW_CP);
        let mut d = PathData::default();
        let mut start = left.saturating_sub(ROW_CP);
        for _ in 0..HATCH_LINES {
            if start >= right {
                break;
            }
            let end = start.saturating_add(ROW_CP);
            let from = if start < left {
                pt(left, bottom.saturating_sub(left.saturating_sub(start)))
            } else {
                pt(start, bottom)
            };
            let to = if end > right {
                pt(right, top.saturating_add(end.saturating_sub(right)))
            } else {
                pt(end, top)
            };
            if from.x < to.x {
                d.move_to(from);
                d.line_to(to);
            }
            start = start.saturating_add(HATCH_STEP);
        }
        if !d.d.is_empty() {
            write::path(out, &d.d, Paint::stroke(self.neutral.border, BORDER), None);
        }
    }

    /// Appends the row of the folded inherited fields of `sb` and the row that
    /// counts what it leaves out, in that order under its drawn rows.
    fn labels(&self, sb: &SceneBox, out: &mut String) {
        let labels = [sb.inherited_label(), sb.more_label()];
        let x = sb.rect.x.saturating_add(PADDING_CP);
        let mut band = i32::try_from(sb.rows.len()).unwrap_or(i32::MAX);
        for label in labels.iter().flatten() {
            let mid = sb
                .rect
                .y
                .saturating_add(HEADER_CP)
                .saturating_add(band.saturating_mul(ROW_CP))
                .saturating_add(ROW_CP / 2);
            let run = Run {
                x,
                mid,
                size: TEXT_CP,
                bold: false,
                fill: self.neutral.muted,
            };
            write::run(out, run, label);
            band = band.saturating_add(1);
        }
    }

    /// Appends the chip at position `k`, which a box holds, a count chip as
    /// it is and a stub chip as a group with its id that holds its arrow.
    fn held_chip(&self, k: usize, out: &mut String) {
        let chip = &self.scene.chips[k];
        match self.places[k] {
            Place::Count => self.chip(chip.rect, &chip.text, out),
            Place::Stub { arrow } => {
                write::open_group(out, Some(&chip.id.text(self.model)));
                self.curve(&self.scene.edges[arrow], out);
                self.chip(chip.rect, &chip.text, out);
                write::close_group(out);
            }
            Place::Lone { .. } => {}
        }
    }

    /// Appends every chip no box holds, as a group with its id that holds its
    /// arrow when the arrow carries the same id.
    fn lone_chips(&self, out: &mut String) {
        for (k, chip) in self.scene.chips.iter().enumerate() {
            let Place::Lone { arrow } = self.places[k] else {
                continue;
            };
            write::open_group(out, Some(&chip.id.text(self.model)));
            if let Some(e) = arrow {
                self.curve(&self.scene.edges[e], out);
            }
            self.chip(chip.rect, &chip.text, out);
            write::close_group(out);
        }
    }

    /// Appends a chip over `r` whose text is `label`, a card with a border and
    /// its text at the size of a chip after its padding.
    fn chip(&self, r: Rect, label: &str, out: &mut String) {
        let n = self.neutral;
        let paint = Paint {
            fill: n.card,
            ..Paint::stroke(n.border, BORDER)
        };
        write::rect(out, r, CHIP_RADIUS, paint);
        let run = Run {
            x: r.x.saturating_add(CHIP_PADDING_CP),
            mid: r.y.saturating_add(r.h / 2),
            size: CHIP_TEXT_CP,
            bold: false,
            fill: n.text,
        };
        write::run(out, run, label);
    }
}

/// Returns the ids `ids` with their positions, sorted by id and then by
/// position.
fn sorted_ids<'s>(ids: impl Iterator<Item = &'s ElementId>) -> Vec<(&'s ElementId, usize)> {
    let mut sorted: Vec<(&ElementId, usize)> = ids.enumerate().map(|(i, id)| (id, i)).collect();
    sorted.sort_unstable();
    sorted
}

/// Returns the first position whose id is `id` in `sorted`, a table that
/// [`sorted_ids`] built.
fn find_id(sorted: &[(&ElementId, usize)], id: &ElementId) -> Option<usize> {
    let i = sorted.partition_point(|&(x, _)| x < id);
    sorted.get(i).filter(|&&(x, _)| x == id).map(|&(_, p)| p)
}

/// Returns the position of the box whose right side the curve `edge` leaves,
/// or `None` when it leaves no right side of a box.
fn leaving(scene: &Scene, edge: &SceneEdge) -> Option<usize> {
    scene.boxes.iter().position(|b| {
        let r = b.rect;
        let bottom = r.y.saturating_add(r.h);
        r.x.saturating_add(r.w) == edge.from.x && (r.y..=bottom).contains(&edge.from.y)
    })
}

/// Returns whether `edge` is the arc of a trunk of the cluster map, whose id is
/// the cluster id of the two slugs it joins.
fn is_arc(edge: &SceneEdge) -> bool {
    matches!(edge.id, ElementId::Cluster(_))
}

/// Returns the dash pattern of a curve in the style `style`, in pixels.
fn dash(style: EdgeStyle) -> Option<&'static str> {
    match style {
        EdgeStyle::Field => None,
        EdgeStyle::UnionMember => Some("6 4"),
        EdgeStyle::Implements => Some("1 4"),
        EdgeStyle::Argument => Some("6 3 1 3"),
    }
}

/// Returns the direction the curve `edge` enters its end in, positive to the
/// right, from its horizontal tangent there.
fn end_direction(edge: &SceneEdge) -> i32 {
    toward(edge.c2.x, edge.to.x, toward(edge.from.x, edge.to.x, 1))
}

/// Returns the direction the second head of the curve `edge` points in at
/// its start, back into the box it leaves.
fn start_direction(edge: &SceneEdge) -> i32 {
    toward(edge.c1.x, edge.from.x, toward(edge.to.x, edge.from.x, -1))
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

/// Returns the middle of the cell `cell` of a run of 13 px text that starts
/// at `x`.
fn cell_middle(x: i32, cell: u32) -> i32 {
    let left = run_width(cell, TEXT_CP);
    let right = run_width(cell.saturating_add(1), TEXT_CP);
    x.saturating_add(left / 2).saturating_add(right / 2)
}
