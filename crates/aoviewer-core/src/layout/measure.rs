// SPDX-License-Identifier: MIT
/*
 * aoviewer-core layout measure: the sizes of the boxes of a view.
 *
 * A box is the longest row in cells times the cell width, plus padding and 8
 * percent of slack.
 */

//! The box measure.
//!
//! The level of a box decides which of its rows it draws by [`row_fate`], and
//! an enum decides its values by [`value_fate`], the rules the view counts the
//! `+N fields` row of its focus by, so the two counts agree. A box at
//! [`Level::L1`] draws the rows an edge or a stub of the view leaves, by their
//! names, and counts the rest in its last row. A box at [`Level::L2`] draws
//! every row with its type, and the focus at [`Level::L3`] draws them up to 32
//! and counts the rest, with the count of the arguments of a row after its
//! name and the Relay and deprecation marks. The row of the field of an
//! operation view carries the count of its arguments as well, because the box
//! is the signature of that field.
//!
//! At [`Level::L2`] and [`Level::L3`] the inherited fields that no edge leaves
//! fold into one row, and after the second reduction of a page the leaf rows
//! that no edge leaves fold into the last row that counts what the box leaves
//! out. Under `leaf=0` a leaf row is not drawn and not counted. An enum draws
//! its values as rows, and a union and a scalar draw their header alone.
//!
//! The header keeps room for the letter badge of the kind and, at a root, for
//! the root mark. A row is its name, two cells, and its type, and at the focus
//! two cells per mark it carries. The SVG writer sets its runs in the same
//! cells, so it reads the cell counts and the chip width from here.
//!
//! Every loop runs over the rows, the values or the arguments of one type,
//! which [`Measured::steps`] counts, or over the wrappers of one type
//! expression, whose depth the limits of the reader cap.

use alloc::{
    string::String,
    vec,
    vec::Vec, //
};

use super::NONE;
use crate::{
    graph::{
        NodeId,
        Row, //
    },
    id::ElementId,
    model::{
        Kind,
        Model,
        TypeId,
        TypeRef, //
    },
    scene::{
        BoxRow,
        CHIP_PADDING_CP,
        CHIP_TEXT_CP,
        HEADER_CP,
        Marks,
        PADDING_CP,
        ROW_CP,
        Rect,
        SceneBox,
        TEXT_CP, //
    },
    schema::Schema,
    text::{
        cells,
        cut,
        run_width_with_slack, //
    },
    transform::{
        Marked,
        Member, //
    },
    view::{
        Level,
        RowFacts,
        RowFate,
        RowRules,
        View,
        ViewKind,
        is_leaf_row,
        row_fate,
        value_fate, //
    },
};

/// The cells of a header beside its name that the letter badge of the kind and
/// its gap take, at most two letters and one space.
pub(crate) const BADGE_CELLS: u32 = 3;

/// The cells the root mark of a header takes, one glyph and one space.
const ROOT_CELLS: u32 = 2;

/// The cells between the name and the type of a row.
pub(crate) const GAP_CELLS: u32 = 2;

/// The cells one mark of a row takes, one glyph and one space.
pub(crate) const MARK_CELLS: u32 = 2;

/// The deepest type expression the measure writes, the depth the limits of the
/// reader allow, which is a `u16`.
const MAX_WRAPPERS: usize = 1 << 16;

/// A measured box, the scene box without its place, and the drawn position
/// of every row of its node.
pub(super) struct Measured {
    /// The box, placed at the origin, its ports under its top.
    pub(super) scene: SceneBox,
    /// The drawn position of every row of the node, or [`NONE`].
    pub(super) drawn: Vec<u32>,
    /// The steps the measure took, one per row of the node for its table
    /// and one for its fate, one per value, one per argument it counted and
    /// one per drawn row it sized.
    pub(super) steps: usize,
}

/// Returns the box at position `pos` of `view`, whose rows `linking`, sorted,
/// an edge or a stub of the view leaves.
///
/// # Panics
///
/// Panics when `view` was not built from the graph of `schema` or `pos` is
/// no position of its boxes.
pub(super) fn measure(schema: &Schema, view: &View, pos: usize, linking: &[u32]) -> Measured {
    let (node, level) = view.nodes[pos];
    let model = &schema.model;
    let gnode = &schema.graph.nodes[node.index()];
    let ty = model.ty(gnode.ty);
    let mut out = Measured {
        scene: SceneBox {
            id: ElementId::Type(ty.name),
            kind: ty.kind,
            level,
            rect: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            title: cut(model.name(ty.name)),
            rows: Vec::new(),
            more: 0,
            root: gnode.is_root,
            marks: Marks {
                unreachable: !schema.graph.walks().aoviewer.contains(node),
                ..Marks::default()
            },
            inherited_folded: 0,
        },
        drawn: vec![NONE; gnode.rows.len()],
        steps: gnode.rows.len(),
    };
    if ty.kind == Kind::Enum {
        values(schema, gnode.ty, level, &mut out);
    } else {
        fields(schema, view, node, linking, &mut out);
    }
    out.steps = out.steps.saturating_add(out.scene.rows.len());
    size(&mut out.scene);
    out
}

/// Draws or counts the rows of the node `node` of `view` into `out`.
fn fields(schema: &Schema, view: &View, node: NodeId, linking: &[u32], out: &mut Measured) {
    let level = out.scene.level;
    let rules = RowRules::of(schema, view);
    let operation = view.kind == ViewKind::Operation && view.focus == Some(node);
    let rows = &schema.graph.nodes[node.index()].rows;
    out.steps = out.steps.saturating_add(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let r = super::id32(i);
        let facts = RowFacts {
            links: linking.binary_search(&r).is_ok(),
            leaf: is_leaf_row(schema, node, r),
            inherited: row.inherited_from.is_some(),
        };
        let drawn = out.scene.rows.len();
        match row_fate(level, facts, drawn, rules) {
            RowFate::Draw => {
                let counted = level == Level::L3 || (operation && view.row == Some(r));
                out.drawn[i] = super::id32(drawn);
                let (drawn_row, args) = field_row(schema, row, level, counted, drawn);
                out.scene.rows.push(drawn_row);
                out.steps = out.steps.saturating_add(args);
            }
            RowFate::More => out.scene.more = out.scene.more.saturating_add(1),
            RowFate::Inherited => {
                out.scene.inherited_folded = out.scene.inherited_folded.saturating_add(1);
            }
            RowFate::Hidden => {}
        }
    }
}

/// Returns the drawn row of `row` at `level` at the position `at`, with the
/// count of its arguments after its name when `counted` holds, and the count
/// of the arguments it ran over.
fn field_row(
    schema: &Schema,
    row: &Row,
    level: Level,
    counted: bool,
    at: usize,
) -> (BoxRow, usize) {
    let (model, t) = (&schema.model, &schema.transformed);
    let field = model.field(row.field);
    let mut name = cut(model.name(field.name));
    let mut examined = 0;
    if counted {
        examined = field.args.len();
        let args = field
            .args
            .iter()
            .filter(|&&a| !t.hides(model, Member::Arg(a)))
            .count();
        if args > 0 {
            name = alloc::format!("{name}({args})");
        }
    }
    let type_ref = match t.collapse_of(row.field) {
        Some(c) => &c.node,
        None => &field.type_ref,
    };
    let mut marks = Marks::default();
    if level == Level::L3 {
        let mark = t.mark(Member::Field(row.field));
        marks.relay = t.is_relay_field(model, row.field);
        marks.deprecated = mark.is_some();
        marks.from_tag = matches!(mark, Some(Marked::Tag { .. }));
    }
    let type_text = if level == Level::L1 {
        String::new()
    } else {
        type_text(model, type_ref)
    };
    let drawn = BoxRow {
        id: ElementId::of_field(model, row.field),
        name,
        type_text,
        port_y: port(at),
        marks,
    };
    (drawn, examined)
}

/// Draws or counts the values of the enum `ty` at `level` into `out`.
fn values(schema: &Schema, ty: TypeId, level: Level, out: &mut Measured) {
    let (model, t) = (&schema.model, &schema.transformed);
    let values = t.values(model, ty);
    out.steps = out.steps.saturating_add(values.len());
    for v in values {
        if t.hides(model, Member::Value(v)) {
            continue;
        }
        let b = &mut out.scene;
        let drawn = b.rows.len();
        match value_fate(level, drawn) {
            RowFate::Draw => {}
            RowFate::More => {
                b.more = b.more.saturating_add(1);
                continue;
            }
            RowFate::Inherited | RowFate::Hidden => continue,
        }
        let mut marks = Marks::default();
        if level == Level::L3 {
            let mark = t.mark(Member::Value(v));
            marks.deprecated = mark.is_some();
            marks.from_tag = matches!(mark, Some(Marked::Tag { .. }));
        }
        b.rows.push(BoxRow {
            id: ElementId::of_value(model, v),
            name: cut(model.name(model.value(v).name)),
            type_text: String::new(),
            port_y: port(drawn),
            marks,
        });
    }
}

/// Returns the height of the port of the drawn row at position `at` under the
/// top of its box, the middle of its band.
pub(super) fn port(at: usize) -> i32 {
    let at = i32::try_from(at).unwrap_or(i32::MAX);
    HEADER_CP
        .saturating_add(at.saturating_mul(ROW_CP))
        .saturating_add(ROW_CP / 2)
}

/// Sets the width and the height of `b` from its header and its rows.
///
/// The height is the header, one band per drawn row, and one band each for
/// the row of the folded inherited fields and for the row that counts what
/// the box leaves out.
pub(super) fn size(b: &mut SceneBox) {
    let mut widest = cells(&b.title).saturating_add(BADGE_CELLS);
    if b.root {
        widest = widest.saturating_add(ROOT_CELLS);
    }
    for row in &b.rows {
        widest = widest.max(row_cells(row));
    }
    let mut bands = b.rows.len();
    if let Some(label) = b.inherited_label() {
        widest = widest.max(cells(&label));
        bands += 1;
    }
    if let Some(label) = b.more_label() {
        widest = widest.max(cells(&label));
        bands += 1;
    }
    let bands = i32::try_from(bands).unwrap_or(i32::MAX);
    b.rect.w = run_width_with_slack(widest, TEXT_CP).saturating_add(2 * PADDING_CP);
    b.rect.h = HEADER_CP.saturating_add(bands.saturating_mul(ROW_CP));
}

/// Returns the cells a drawn row takes, its name, the gap, its type and its
/// marks.
fn row_cells(row: &BoxRow) -> u32 {
    let marks = [row.marks.relay, row.marks.deprecated]
        .iter()
        .filter(|&&m| m)
        .count();
    let mut n = cells(&row.name);
    if !row.type_text.is_empty() {
        n = n
            .saturating_add(GAP_CELLS)
            .saturating_add(cells(&row.type_text));
    }
    n.saturating_add(MARK_CELLS.saturating_mul(super::id32(marks)))
}

/// Returns the width of a chip whose text is `text`, its run with slack at
/// the size of a chip and its padding on both sides.
pub(crate) fn chip_width(text: &str) -> i32 {
    run_width_with_slack(cells(text), CHIP_TEXT_CP).saturating_add(2 * CHIP_PADDING_CP)
}

/// Returns the width of a box that holds the text of the chip whose text is
/// `text` between its paddings, which a chip drawn inside the element of a
/// box needs.
pub(super) fn chip_room(text: &str) -> i32 {
    run_width_with_slack(cells(text), CHIP_TEXT_CP).saturating_add(2 * PADDING_CP)
}

/// Returns the type expression `type_ref` as GraphQL writes it, such as
/// `[Project!]!`, with its name cut after 48 cells.
fn type_text(model: &Model, type_ref: &TypeRef) -> String {
    let mut open = String::new();
    let mut close: Vec<char> = Vec::new();
    let mut cur = type_ref;
    let mut named = None;
    for _ in 0..MAX_WRAPPERS {
        match cur {
            TypeRef::Named(id) => {
                named = Some(*id);
                break;
            }
            TypeRef::List(inner) => {
                open.push('[');
                close.push(']');
                cur = inner;
            }
            TypeRef::NonNull(inner) => {
                close.push('!');
                cur = inner;
            }
        }
    }
    let mut out = open;
    if let Some(id) = named {
        out.push_str(&cut(model.name(model.ty(id).name)));
    }
    out.extend(close.iter().rev());
    out
}
