// SPDX-License-Identifier: MIT
/*
 * aoviewer-core layout: the layered layout of a view into a scene.
 *
 * Ranks are given, the order comes from six barycenter sweeps over integer
 * keys, and every loop runs under a named cap.
 */

//! The layout.
//!
//! A [`Layout`] turns a [`View`] into a [`Scene`]. The drawing is layered and
//! its ranks are the view's, the signed distance from the focus in an ego view
//! and the breadth first distance from the root in the whole graph, so what
//! remains is the order inside the columns, the vertical placement, the
//! wrapping and the routing.
//!
//! 1. **Measure.** A box is as wide as its longest row in cells times the cell
//!    width at 13 px, plus padding and 8 percent of slack. The level of a box
//!    decides its rows.
//! 2. **Order.** [`SWEEPS`] barycenter sweeps order every column. The key of a
//!    box is the position of its neighbour in the next column times 64 plus the
//!    row index of the port there, the mean of those keys is an integer
//!    division whose remainder is dropped, and ties break by the stable id. A
//!    seed breaks a tie before the id, so the boxes two scenes share keep their
//!    relative order.
//! 3. **Wrap.** The whole graph mode alone wraps a tall column into sub columns
//!    of its rank band. The wrap aims at a picture between one to two and three
//!    to one, the range the design names for the rule and step 13 of
//!    `51_viewer_core` measures. It judges the picture stacked from the top
//!    with gutters of 70 px, before the placement moves an item by its pulls
//!    and before the gutters grow, and a single item taller than the range
//!    allows cannot wrap, so the placed picture can leave the range.
//! 4. **Place.** The columns nearest the focus are placed first. Each box is
//!    pulled to the mean height of the ports it connects to, overlaps resolve
//!    in [`OVERLAP_PASSES`] forward pass, and the column shifts back by the
//!    mean displacement. Up to [`ROUTE_ITERATIONS`] further rounds pull every
//!    column to both of its neighbours, which straightens the routes, and stop
//!    once nothing moves.
//! 5. **Route.** A curve leaves the side of a row and enters the side of a
//!    header, gutters grow with the edges that cross them, and the edges into
//!    one side of one header merge into a trunk.
//!
//! Every box of a column is as wide as the widest, so every port of a column
//! sits on one of its two sides and a curve runs in a gutter from side to side,
//! where no box stands, and never through a body. A stub is an arrow into a
//! chip that names the target. Its chip hangs under the box it leaves, inside
//! the column and flush with its right side, and its arrow loops out of the
//! right side of the row into the right side of the chip, so no curve that
//! crosses a gutter meets a chip. The arrow is therefore as long as the part of
//! the box under its row and the chips above its own, not the 24 px of the
//! design.
//!
//! [`EgoLayout`] draws an edge between two boxes of adjacent ranks in full in
//! either direction and a reference of a type to itself as a loop on the right
//! of its box, and it draws every other edge, one between two boxes of one rank
//! or of ranks further apart, as a stub. Every edge of an ego view, of an
//! operation view and of a path joins adjacent ranks or a type to itself, so
//! their stubs are the ones the view names, while a cluster view, whose ranks
//! are the breadth first distance inside the cluster, has edges of the other
//! spans as well. [`WholeLayout`] draws an edge in full only when its source
//! row and its target header face each other across one gutter, and every other
//! edge is a stub. Both lay out the cluster map the same way, one column of
//! cluster boxes in the order of the clustering, which never rearranges, with
//! its trunks as arcs on the right of the column. Which layout draws which view
//! is `layout_of`.
//!
//! Everything is an `i32` in centipixels, and nothing reads a clock or a float.
//! Every loop runs over the boxes, rows, values, arguments, edges, chips,
//! items, pulls, bands or columns of one view, or under one of the caps below,
//! and counts one step per element it visits against [`MAX_STEPS`], a sort one
//! per element it orders, which is the count [`Scene::iterations`] reports and
//! `51_viewer_core` judges, never time. The loops over the characters of one
//! text and over the wrappers of one type expression are not counted, since the
//! limits of the reader cap both.

mod ego;
mod map;
pub(crate) mod measure;
mod order;
mod place;
mod route_edges;
mod whole;

use alloc::{
    string::String,
    vec,
    vec::Vec, //
};

use crate::{
    graph::NodeId,
    id::ElementId,
    scene::{
        CHIP_CP,
        EdgeStyle,
        HEADER_CP,
        Rect,
        Scene,
        SceneBox, //
    },
    schema::Schema,
    view::{
        Chip,
        View,
        ViewKind,
        linking_rows, //
    },
};

/// A previous scene the ordering is seeded from, so shared neighbours keep
/// their relative order.
///
/// The viewer seeds the scene of a new focus from the scene on screen, and an
/// export always renders the canonical scene without a seed.
#[derive(Clone, Copy, Debug)]
pub struct Seed<'s>(pub &'s Scene);

/// A layout of a view into a scene.
pub trait Layout {
    /// Lays out `view` of `schema`, seeded from `seed` when one is given.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError::Empty`] when the view draws nothing and
    /// [`LayoutError::CapExceeded`] when the layout would take more than
    /// [`MAX_STEPS`] steps.
    ///
    /// # Panics
    ///
    /// Panics when `view` was not built from the graph of `schema`, since its
    /// boxes, ranks and edges index that graph.
    fn layout(
        &self,
        view: &View,
        schema: &Schema,
        seed: Option<Seed<'_>>,
    ) -> Result<Scene, LayoutError>;
}

/// The layout of the ego, operation, path and cluster views.
///
/// It keeps one column per rank, draws an edge between adjacent ranks in full
/// in either direction and a reference of a type to itself as a loop, and draws
/// every other edge as a stub. The cluster view takes it rather than
/// [`WholeLayout`], because the whole graph mode alone wraps a column, and
/// `layout_of` is where that choice is made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EgoLayout;

/// The layout of the whole graph, with its wrapping and its stubs.
///
/// It wraps a tall rank into sub columns and draws an edge in full only when it
/// crosses exactly one gutter forwards, and every other edge as a stub.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WholeLayout;

/// The barycenter sweeps that order the columns.
pub const SWEEPS: u32 = 6;

/// The forward passes that resolve the overlaps of a column.
pub const OVERLAP_PASSES: u32 = 1;

/// The passes that search the height a whole graph wraps its columns at.
pub const WRAP_PASSES: u32 = 8;

/// The rounds that pull every column to both of its neighbours after the
/// first placement.
pub const ROUTE_ITERATIONS: u32 = 4;

/// The most steps one layout takes over all of its loops.
pub const MAX_STEPS: u32 = 1_000_000;

/// A reason a layout answers no scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// The layout would take more than [`MAX_STEPS`] steps, and `loop_name`
    /// names the loop that crossed the cap.
    CapExceeded {
        /// The loop that crossed the cap.
        loop_name: &'static str,
    },
    /// The view draws nothing.
    Empty,
}

/// Returns the scene of `view` of `schema` drawn by the layout of its kind,
/// [`WholeLayout`] for the whole graph and [`EgoLayout`] for every other
/// view, seeded from `seed` when one is given.
///
/// # Errors
///
/// Returns the errors of [`Layout::layout`].
///
/// # Panics
///
/// Panics when `view` was not built from the graph of `schema`.
pub(crate) fn layout_of(
    view: &View,
    schema: &Schema,
    seed: Option<Seed<'_>>,
) -> Result<Scene, LayoutError> {
    match view.kind {
        ViewKind::Whole => WholeLayout.layout(view, schema, seed),
        _ => EgoLayout.layout(view, schema, seed),
    }
}

/// The margin around a drawing, 16 px, a step of the 4 px base.
const MARGIN: i32 = 1600;

/// The space between two boxes of a column, 18 px.
const GAP: i32 = 1800;

/// The mark of a position a table does not hold.
const NONE: u32 = u32::MAX;

/// The mode a layout runs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// One column per rank, edges between adjacent ranks in full.
    Ego,
    /// Wrapped rank bands, edges that face each other across one gutter in
    /// full and stubs for the rest.
    Whole,
}

/// The steps a layout took, counted against [`MAX_STEPS`].
#[derive(Clone, Copy, Debug, Default)]
struct Steps {
    /// The steps so far.
    used: u32,
}

impl Steps {
    /// Counts `count` steps of the loop `loop_name`.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError::CapExceeded`] naming `loop_name` once the steps
    /// exceed [`MAX_STEPS`].
    fn take(&mut self, count: usize, loop_name: &'static str) -> Result<(), LayoutError> {
        self.used = self.used.saturating_add(id32(count));
        if self.used > MAX_STEPS {
            return Err(LayoutError::CapExceeded { loop_name });
        }
        Ok(())
    }
}

/// What an item of a column is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Piece {
    /// A box, by its position among the boxes of the view.
    Box(usize),
    /// A count chip, by its position in [`Plan::chips`].
    Chip(usize),
}

/// An item of a column, a box or a count chip, with its rank, its size and
/// its height.
#[derive(Clone, Copy, Debug)]
struct Item {
    /// The box or the chip.
    piece: Piece,
    /// The rank, the column band it stands in.
    rank: i32,
    /// The width, which a box takes from its column.
    w: i32,
    /// The height.
    h: i32,
    /// The top side.
    y: i32,
    /// The position in the order that breaks a tie between two keys.
    tie: u32,
    /// The column that holds the item.
    column: usize,
}

/// A pull of an item towards another of an adjacent rank, one edge between
/// two boxes or a count chip towards its box.
#[derive(Clone, Copy, Debug)]
struct Pull {
    /// The other item.
    other: usize,
    /// The index of the port on the other item, 0 for its header and one
    /// more than the position of its row otherwise, at most 63.
    port: i64,
    /// The height of that port under the top of the other item.
    other_off: i32,
    /// The height of the own port under the top of the item.
    own_off: i32,
}

/// A column, one rank or one sub column of a rank band.
#[derive(Clone, Debug)]
struct Column {
    /// The rank.
    rank: i32,
    /// The items from the top down.
    items: Vec<usize>,
    /// The left side.
    x: i32,
    /// The width every box of the column takes.
    w: i32,
    /// Whether it is the first sub column of its band.
    first: bool,
    /// Whether it is the last sub column of its band.
    last: bool,
}

/// How a link is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// In full from the right side of the source to the left side of the
    /// target header, one rank further.
    Forward,
    /// In full from the left side of the source to the right side of the
    /// target header, one rank back.
    Backward,
    /// A loop on the right of a box from a row to its own header.
    Loop,
    /// An arrow into a chip that names the target.
    Stub,
    /// Merged into the opposite forward link, which carries two heads.
    Merged,
}

/// One edge of the view, or one stub of the view, as the layout draws it.
#[derive(Clone, Debug)]
struct Link {
    /// The id, the source id and the name of the target type, or the id of
    /// its chip for a bare stub once the chip is made.
    id: ElementId,
    /// The style.
    style: EdgeStyle,
    /// The box it leaves.
    from: usize,
    /// The row of the node of `from` it leaves, by its position in the
    /// graph, or `None` for the header.
    row: Option<u32>,
    /// The box it enters, or `None` for a type the view does not draw.
    to: Option<usize>,
    /// The node it points at.
    target: NodeId,
    /// How it is drawn.
    shape: Shape,
    /// Whether it stands for a pair of opposite references.
    two_heads: bool,
    /// The position in [`Plan::chips`] of the chip of a stub.
    chip: Option<usize>,
    /// Whether it is a stub whose arrow the grammar names no edge for, one
    /// that leaves the header of its box for a type the model relates to it
    /// in no way, so its arrow carries the id of its chip.
    bare: bool,
}

/// The kind of a chip the layout places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChipKind {
    /// A `+N more` chip, on the side of the referrers when `inbound` holds.
    More {
        /// Whether the types it counts reference its box.
        inbound: bool,
    },
    /// An `N implementations` chip.
    Implementations,
    /// The chip a stub ends in.
    Stub,
}

/// A chip under layout.
#[derive(Clone, Debug)]
struct ChipPlan {
    /// The id.
    id: ElementId,
    /// The text.
    text: String,
    /// The element a click opens.
    target: Option<ElementId>,
    /// The place and the size.
    rect: Rect,
    /// The box it hangs at, or the box its stub leaves.
    at: usize,
    /// The kind.
    kind: ChipKind,
    /// The position of the chip of a stub among the stub chips under its
    /// box, from the top.
    slot: i32,
}

/// The state of one layout between its stages.
struct Plan<'a> {
    /// The view.
    view: &'a View,
    /// The schema whose graph the view draws.
    schema: &'a Schema,
    /// The mode.
    mode: Mode,
    /// The position of every box of the view, sorted by node.
    at: Vec<(NodeId, usize)>,
    /// The boxes, measured and not yet placed.
    boxes: Vec<SceneBox>,
    /// The drawn position of every row of the node of every box, or [`NONE`].
    drawn: Vec<Vec<u32>>,
    /// The links.
    links: Vec<Link>,
    /// The count chips of the view, then the chips of the stubs.
    chips: Vec<ChipPlan>,
    /// The boxes, then the count chips, as items of the columns.
    items: Vec<Item>,
    /// The pulls of every item.
    pulls: Vec<Vec<Pull>>,
    /// The columns from the left to the right.
    columns: Vec<Column>,
    /// The steps so far.
    steps: Steps,
}

/// Lays out `view` of `schema` in `mode`, seeded from `seed`.
///
/// # Errors
///
/// Returns [`LayoutError::Empty`] for a view that draws nothing and
/// [`LayoutError::CapExceeded`] for a layout over [`MAX_STEPS`].
fn run(
    view: &View,
    schema: &Schema,
    seed: Option<Seed<'_>>,
    mode: Mode,
) -> Result<Scene, LayoutError> {
    if view.empty.is_some() {
        return Err(LayoutError::Empty);
    }
    if view.kind == ViewKind::ClusterMap {
        return map::layout(view, schema);
    }
    if view.nodes.is_empty() {
        return Err(LayoutError::Empty);
    }
    let mut plan = Plan::new(view, schema, mode)?;
    let bands = order::bands(&mut plan, seed)?;
    let columns = match mode {
        Mode::Ego => {
            plan.steps.take(bands.len(), "columns")?;
            ego::columns(bands)
        }
        Mode::Whole => whole::wrap(&mut plan, &bands)?,
    };
    plan.set_columns(columns)?;
    route_edges::classify(&mut plan)?;
    place::widths(&mut plan)?;
    place::vertical(&mut plan)?;
    place::horizontal(&mut plan)?;
    place::normalize(&mut plan)?;
    route_edges::finish(plan)
}

impl<'a> Plan<'a> {
    /// Returns the plan of `view` of `schema` in `mode`, with its links, its
    /// measured boxes, its count chips, its items and their pulls.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError::CapExceeded`] when the steps exceed
    /// [`MAX_STEPS`].
    fn new(view: &'a View, schema: &'a Schema, mode: Mode) -> Result<Plan<'a>, LayoutError> {
        let mut steps = Steps::default();
        steps.take(2 * view.nodes.len(), "measure")?;
        let mut at: Vec<(NodeId, usize)> = view
            .nodes
            .iter()
            .enumerate()
            .map(|(i, &(n, _))| (n, i))
            .collect();
        at.sort_unstable();
        let links = route_edges::links(view, schema, &at, &mut steps)?;
        let mut plan = Plan {
            view,
            schema,
            mode,
            at,
            boxes: Vec::with_capacity(view.nodes.len()),
            drawn: Vec::with_capacity(view.nodes.len()),
            links,
            chips: Vec::new(),
            items: Vec::new(),
            pulls: Vec::new(),
            columns: Vec::new(),
            steps,
        };
        plan.measure()?;
        plan.count_chips()?;
        plan.make_items()?;
        plan.make_pulls()?;
        Ok(plan)
    }

    /// Measures every box, with the rows the edges and the stubs of the view
    /// leave, which are [`linking_rows`].
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError::CapExceeded`] when the steps exceed
    /// [`MAX_STEPS`].
    fn measure(&mut self) -> Result<(), LayoutError> {
        let view = self.view;
        let listed = view.nodes.len() + view.edges.len() + view.chips.len() + 1;
        self.steps.take(3 * listed, "measure")?;
        let linking = linking_rows(self.schema, view);
        for (pos, rows) in linking.iter().enumerate() {
            let measured = measure::measure(self.schema, view, pos, rows);
            self.steps.take(measured.steps + 1, "measure")?;
            self.boxes.push(measured.scene);
            self.drawn.push(measured.drawn);
        }
        Ok(())
    }

    /// Adds the `+N more` and `N implementations` chips of the view, and
    /// widens the box a chip hangs at to the text of the chip.
    ///
    /// The `+N fields` chip of the focus is the last row of its box, which the
    /// box counts itself by the rule the view counts the chip by. A count chip
    /// carries the id of its box, so a writer draws it inside the element of
    /// the box, and the box keeps room for its text between its paddings like
    /// for any of its runs.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError::CapExceeded`] when the steps exceed
    /// [`MAX_STEPS`].
    fn count_chips(&mut self) -> Result<(), LayoutError> {
        let model = &self.schema.model;
        for chip in &self.view.chips {
            self.steps.take(1, "chips")?;
            let (node, kind, text) = match *chip {
                Chip::More { at, count, inbound } => (
                    at,
                    ChipKind::More { inbound },
                    alloc::format!("+{count} more"),
                ),
                Chip::Implementations { at, count } => (
                    at,
                    ChipKind::Implementations,
                    alloc::format!("{count} implementations"),
                ),
                Chip::MoreFields { .. } | Chip::Stub { .. } => continue,
            };
            let Some(at) = find(&self.at, node) else {
                continue;
            };
            let ty = self.schema.graph.nodes[node.index()].ty;
            let id = ElementId::of_type(model, ty);
            let room = measure::chip_room(&text);
            let rect = &mut self.boxes[at].rect;
            rect.w = rect.w.max(room);
            self.chips.push(ChipPlan {
                id: id.clone(),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: measure::chip_width(&text),
                    h: CHIP_CP,
                },
                text,
                target: Some(id),
                at,
                kind,
                slot: 0,
            });
        }
        Ok(())
    }

    /// Makes one item per box and one per count chip.
    ///
    /// A `+N more` chip stands in the column of the side it counts, the
    /// referrers on the left of its box and the referenced types on the right,
    /// and an `N implementations` chip on the left of its interface, where the
    /// implementations it stands for would stand.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError::CapExceeded`] when the steps exceed
    /// [`MAX_STEPS`].
    fn make_items(&mut self) -> Result<(), LayoutError> {
        self.steps
            .take(self.boxes.len() + self.chips.len(), "items")?;
        for (i, b) in self.boxes.iter().enumerate() {
            self.items.push(Item {
                piece: Piece::Box(i),
                rank: self.view.ranks[i],
                w: b.rect.w,
                h: b.rect.h,
                y: 0,
                tie: 0,
                column: 0,
            });
        }
        for (k, chip) in self.chips.iter().enumerate() {
            let rank = self.view.ranks[chip.at];
            let rank = match chip.kind {
                ChipKind::More { inbound: false } => rank.saturating_add(1),
                _ => rank.saturating_sub(1),
            };
            self.items.push(Item {
                piece: Piece::Chip(k),
                rank,
                w: chip.rect.w,
                h: chip.rect.h,
                y: 0,
                tie: 0,
                column: 0,
            });
        }
        Ok(())
    }

    /// Makes the pulls of every item, one per link between two boxes of
    /// adjacent ranks in both directions and one per count chip towards its
    /// box.
    ///
    /// A link one rank back that a link one rank forward between the same two
    /// boxes answers pulls nothing, because the two are drawn as one curve
    /// that leaves the row of the forward one, and a second pull would move
    /// the key of the box away from the row it hangs at.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError::CapExceeded`] when the steps exceed
    /// [`MAX_STEPS`].
    fn make_pulls(&mut self) -> Result<(), LayoutError> {
        self.steps.take(
            self.items.len() + 3 * self.links.len() + self.chips.len(),
            "pulls",
        )?;
        let mut pulls: Vec<Vec<Pull>> = vec![Vec::new(); self.items.len()];
        let rank = |b: usize| i64::from(self.items[b].rank);
        let mut forward: Vec<(usize, usize)> = self
            .links
            .iter()
            .filter_map(|l| l.to.map(|t| (l.from, t)))
            .filter(|&(a, b)| rank(b) - rank(a) == 1)
            .collect();
        forward.sort_unstable();
        for link in &self.links {
            let Some(to) = link.to else {
                continue;
            };
            let (a, b) = (link.from, to);
            let step = rank(b) - rank(a);
            let answered = step == -1 && forward.binary_search(&(b, a)).is_ok();
            if step.abs() != 1 || answered {
                continue;
            }
            let (port, off) = self.port(a, link.row);
            pulls[a].push(Pull {
                other: b,
                port: 0,
                other_off: HEADER_CP / 2,
                own_off: off,
            });
            pulls[b].push(Pull {
                other: a,
                port,
                other_off: off,
                own_off: HEADER_CP / 2,
            });
        }
        let first = self.boxes.len();
        for (k, chip) in self.chips.iter().enumerate() {
            let pull = match chip.kind {
                ChipKind::More { .. } => Pull {
                    other: chip.at,
                    port: 63,
                    other_off: self.items[chip.at].h,
                    own_off: CHIP_CP,
                },
                ChipKind::Implementations | ChipKind::Stub => Pull {
                    other: chip.at,
                    port: 0,
                    other_off: HEADER_CP / 2,
                    own_off: CHIP_CP / 2,
                },
            };
            pulls[first + k].push(pull);
        }
        self.pulls = pulls;
        Ok(())
    }

    /// Takes `columns` as the columns of the plan and marks the column of
    /// every item they hold.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError::CapExceeded`] when the steps exceed
    /// [`MAX_STEPS`].
    fn set_columns(&mut self, columns: Vec<Column>) -> Result<(), LayoutError> {
        self.steps
            .take(columns.len() + self.items.len(), "columns")?;
        for (c, column) in columns.iter().enumerate() {
            for &i in &column.items {
                self.items[i].column = c;
            }
        }
        self.columns = columns;
        Ok(())
    }

    /// Returns the index of the port of the row `row` of the box `b` as a
    /// key counts it, and the height of that port under the top of the box.
    ///
    /// The header is port 0 and a drawn row one more than its position, at most
    /// 63, so a key of 64 per position keeps the positions apart. A row the box
    /// does not draw is its header.
    fn port(&self, b: usize, row: Option<u32>) -> (i64, i32) {
        let drawn = row
            .and_then(|r| self.drawn[b].get(r as usize).copied())
            .filter(|&p| p != NONE);
        match drawn {
            Some(p) => (
                i64::from(p.saturating_add(1).min(63)),
                self.boxes[b].rows[p as usize].port_y,
            ),
            None => (0, HEADER_CP / 2),
        }
    }
}

/// Returns the position of `node` in a table of positions sorted by node.
fn find(at: &[(NodeId, usize)], node: NodeId) -> Option<usize> {
    at.binary_search_by_key(&node, |&(n, _)| n)
        .ok()
        .map(|i| at[i].1)
}

/// Returns `i` as a `u32`, saturated.
///
/// Every count a layout numbers is capped far below `u32::MAX` by the budget
/// of its view and by [`MAX_STEPS`], so the saturation never happens.
fn id32(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}
