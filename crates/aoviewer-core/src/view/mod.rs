// SPDX-License-Identifier: MIT
/*
 * aoviewer-core view: the views, their levels of detail and their budgets.
 *
 * A view is a node set, an edge set and a level per node, reduced under the
 * budget of a page in a fixed order.
 */

//! The views.
//!
//! [`View::of`] answers a [`Route`] with the boxes and edges of its graph, a
//! [`Level`] per box and the chips that stand for what the view leaves out.
//! The views are the ego view of a type, the operation view of a root field,
//! the view of one cluster, the cluster map, the whole graph from a root and
//! the shortest path between two types. The home route answers the overview,
//! which is the whole graph when it has at most 40 boxes and 80 edges and the
//! cluster map otherwise, the whole graph of every root, since the overview of
//! a small schema shows every type. A search and the four report pages have no
//! graph.
//!
//! [`View::for_page`] answers the same route under [`Budget::PAGE`]. A view
//! over the budget is reduced in the fixed order of [`Reduction`], and a type
//! with fewer than two neighbours has no diagram.
//!
//! The ranks of a layered drawing are given by the view and not computed by
//! the layout, so every view keeps the rank of each of its boxes beside the
//! box for the layout. An ego view ranks a box by its signed distance from the
//! focus and a whole graph by its breadth first distance from the root. The
//! ranks of the other views are named in their modules.
//!
//! A [`Schema`] carries the transforms it was built under, and a view reads
//! `leaf` and `hideRoot` from them, so the flags that shaped the graph also
//! shape the view, while `ring` and `x` come from the [`Options`] of the call.
//! Under `leaf=0` a row whose type is a scalar or an enum is not drawn, so no
//! edge leaves it in any view but the operation view, which is about the one
//! row of its root field and draws the edges of that row whatever `leaf` says.
//! Under `hideRoot=1` no root is a box of a view but the focus of an ego view,
//! the centre of an operation view and an end of a path. A custom scalar is a
//! chip inside a row and a box only when it is the focus or an end of a path,
//! so it is no box of a ring, of a cluster view, of the cluster map or of the
//! whole graph, and no edge of those views enters it.
//!
//! An edge of a view is drawn once per row and target, so of the edges that
//! leave one row for one target a view keeps the first in the order of the
//! graph, and the edges of a view follow that order.
//!
//! What a box draws of its rows at each level is `row_fate`, which the
//! layout draws by and which the view counts the [`Chip::MoreFields`] of its
//! focus by, so the chip and the drawing name one count.
//!
//! [`View::of`] judges a budget by its boxes and edges, which are what
//! [`EmptyReason::OverBudget`] names. [`View::for_page`] judges the width of
//! [`Budget::PAGE`] as well, by the width of the scene the layout of the view
//! draws, since only the layout measures a box and places the columns, and its
//! reductions keep the width where they can, as its documentation says.
//!
//! Every loop runs over the nodes or edges of one graph, whose counts
//! [`ReadLimits`](crate::limits::ReadLimits) caps, over the boxes, edges and
//! chips of one view, over the rows or values of one type, which
//! [`ReadLimits`](crate::limits::ReadLimits) caps as well, over at most
//! [`MAX_EXPAND`](crate::route::MAX_EXPAND) names of `x=`, or over at most
//! [`MAX_RING`] smaller rings.

mod cluster;
mod ego;
mod operation;
mod path;
mod reduce;
mod whole;

use alloc::{
    string::String,
    vec,
    vec::Vec, //
};

use crate::{
    cluster::Clustering,
    graph::{
        Edge,
        EdgeKind,
        NodeId, //
    },
    model::{
        Kind,
        Model,
        RootKind,
        TypeId, //
    },
    route::{
        DEFAULT_RING,
        MAX_RING,
        Options,
        Route, //
    },
    schema::Schema,
    transform::Member,
};

/// The rows the focus draws before its `+N fields` row.
pub(crate) const FOCUS_ROWS: usize = 32;

/// The implementations from which an interface shows them as one chip.
const IMPLEMENTATION_CHIP: u32 = 4;

/// The fewest boxes the first reduction of a page folds into one group box.
///
/// The reduction folds implementers into one group box, and two boxes are the
/// fewest whose fold saves a box.
const PAGE_FOLD: u32 = 2;

/// The boxes up to which a whole graph draws every row.
const WHOLE_FULL: usize = 80;

/// The most referenced types a cluster box of the map names.
const MAP_TYPES: usize = 3;

/// The name of the group of the map that holds the other bucket.
const OTHER_NAME: &str = "other";

/// The mark of a node that a table of one view does not hold.
const NONE: u32 = u32::MAX;

/// The four levels of `docs/aoviewer.md` section 3.6: chip, link, full and
/// focus.
///
/// A level says what a box draws, and the representation changes with it and
/// not only the scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// The chip, the kind accent and the name, for the types a cluster box
    /// of the map names.
    L0,
    /// The link, the header, the rows that link to the rest of the view and
    /// `+N fields`, for a ring box and a box of a cluster view or a path.
    L1,
    /// Full, the header and every row with its type, for an expanded
    /// neighbour and a whole graph of up to 80 boxes.
    L2,
    /// The focus, a full box with the argument counts, the Relay and
    /// deprecation marks and the row selection.
    L3,
}

/// The boxes, edges, width and ring a view may take.
///
/// A box is a type box or the group box of folded implementations, and a chip
/// that counts or names types is no box. A cluster box is the box of the
/// cluster map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// The most boxes.
    pub boxes: u16,
    /// The most edges, or the most trunks of the cluster map.
    pub edges: u16,
    /// The widest drawing in centipixels, which [`View::for_page`] judges by
    /// the scene of the view and [`View::of`] does not judge.
    pub width_cp: i32,
    /// The most boxes on each side of a ring.
    pub ring: u16,
}

impl Budget {
    /// The budget of a diagram inside a page, 40 boxes, 80 edges, 1400 px at
    /// 13 px text and the default ring of 24 boxes per side.
    pub const PAGE: Budget = Budget {
        boxes: 40,
        edges: 80,
        width_cp: 140_000,
        ring: DEFAULT_RING,
    };

    /// The budget of the whole graph and of the poster, 150 boxes and 300
    /// edges.
    ///
    /// It bounds no width, since the whole graph mode wraps tall columns
    /// rather than keeping a width, and its ring is the largest `ring=` takes,
    /// so the option decides.
    pub const WHOLE: Budget = Budget {
        boxes: 150,
        edges: 300,
        width_cp: i32::MAX,
        ring: MAX_RING,
    };
}

/// A node set, an edge set and a level per node, plus what the budget removed.
///
/// A view comes from [`View::of`] or [`View::for_page`] and never from a
/// struct literal, because it carries the ranks of its boxes and the tables
/// of the cluster map for the layout beside its public fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// The box the view centres, the focus of an ego view, the root of an
    /// operation view and the first type of a path.
    pub focus: Option<NodeId>,
    /// The boxes with their levels, the focus first where there is one.
    pub nodes: Vec<(NodeId, Level)>,
    /// The indexes of the edges of the graph the view draws, in the order of
    /// the graph.
    pub edges: Vec<u32>,
    /// The chips, each at a box of the view.
    pub chips: Vec<Chip>,
    /// The reductions that brought the view under the budget of a page, in
    /// the order they ran.
    pub reductions: Vec<Reduction>,
    /// The number of this part and the count of the parts, from 1, when a
    /// page splits the view.
    pub part: Option<(u16, u16)>,
    /// Why the view draws nothing, when it draws nothing.
    pub empty: Option<EmptyReason>,
    /// The kind of view.
    pub(crate) kind: ViewKind,
    /// The rank of every box, the column the layout draws it in, in the
    /// order of [`View::nodes`].
    pub(crate) ranks: Vec<i32>,
    /// The position of the row of the focus an operation view is about,
    /// among the rows of the focus in the graph, which the layout draws
    /// whether or not an edge leaves it.
    pub(crate) row: Option<u32>,
    /// The cluster boxes of the cluster map, in the order of the clustering.
    pub(crate) groups: Vec<MapGroup>,
    /// The trunks of the cluster map, one per pair of its boxes that edges
    /// join.
    pub(crate) trunks: Vec<MapTrunk>,
}

/// A chip of a view, a count or a name that stands for what the view leaves
/// out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chip {
    /// The `+N more` chip of a ring, at the box whose ring it is.
    More {
        /// The box whose ring leaves the types out.
        at: NodeId,
        /// The count of the types left out.
        count: u32,
        /// Whether the types reference the box, which puts the chip on the
        /// side of the referrers.
        inbound: bool,
    },
    /// The `N implementations` group box of an interface.
    Implementations {
        /// The interface.
        at: NodeId,
        /// The count of its implementations in the graph.
        count: u32,
    },
    /// The `+N fields` row of the focus, `+N values` for an enum, which the
    /// layout draws as the last row of the focus with this count.
    MoreFields {
        /// The focus.
        at: NodeId,
        /// The count the row reads, the rows past the first 32 the focus
        /// draws once its inherited fields that no edge leaves fold into one
        /// row, together with the leaf rows the second reduction of a page
        /// folds, as the rule the layout draws by decides them.
        count: u32,
    },
    /// An arrow that ends in a chip naming a box outside the view.
    Stub {
        /// The box of the view the arrow leaves.
        from: NodeId,
        /// The position of the row it leaves among the rows of `from`, for an
        /// edge that leaves a row.
        row: Option<u32>,
        /// The box outside the view the chip names.
        to: NodeId,
    },
}

/// A reduction of a view over the budget of a page, in the fixed order of
/// `docs/documentation-generator.md` section 6.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reduction {
    /// Interface implementers folded into one group box.
    FoldedImplementers,
    /// The leaf rows of every box folded into one row counting them, which
    /// the layout draws.
    FoldedLeafRows,
    /// The ring budget of the diagram dropped.
    DroppedRing,
    /// The view split into numbered parts along cluster borders.
    Split,
}

/// Why a view draws nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyReason {
    /// The graph holds no box for the route, a type that no field reaches
    /// and that references nothing, a type or a member the graph leaves out,
    /// or a path that no edge completes.
    Unreachable,
    /// The type has fewer than two neighbours, so a sentence says what a
    /// diagram would.
    FewerThanTwoNeighbours,
    /// The view is over its budget, with the boxes and edges it would draw.
    OverBudget {
        /// The boxes.
        boxes: u32,
        /// The edges.
        edges: u32,
    },
}

/// A reason a view refuses a route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewError {
    /// The route names a type the schema does not declare.
    UnknownType(String),
    /// The route names a field, an input field or an enum value its type
    /// does not declare.
    UnknownField {
        /// The name of the type.
        ty: String,
        /// The name of the field.
        field: String,
    },
    /// The route names an argument its field does not declare.
    UnknownArg {
        /// The name of the type.
        ty: String,
        /// The name of the field.
        field: String,
        /// The name of the argument.
        arg: String,
    },
    /// The route names a root the schema does not declare.
    NoRoot(RootKind),
    /// The route names a cluster slug the clustering does not hold.
    UnknownCluster(String),
    /// The route is a search or a report page, which has no graph.
    NoGraph,
}

/// The kind of a view, which decides its ranks and how its budget counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewKind {
    /// The ego view of a type.
    Ego,
    /// The operation view of a root field.
    Operation,
    /// The view of one cluster.
    Cluster,
    /// The cluster map.
    ClusterMap,
    /// The whole graph from a root or from every root.
    Whole,
    /// The shortest path between two types.
    Path,
}

/// A cluster box of the cluster map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MapGroup {
    /// The name of the cluster, or `other` for the other bucket.
    pub(crate) name: String,
    /// The slug of the cluster, which the other bucket has not.
    pub(crate) slug: Option<String>,
    /// The count of the types of the cluster.
    pub(crate) types: u32,
    /// The position in [`View::nodes`] of the first type the box names.
    pub(crate) first: u32,
    /// The count of the types the box names, at most three.
    pub(crate) shown: u32,
}

/// A trunk of the cluster map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MapTrunk {
    /// The first of the two cluster boxes, the lower position.
    pub(crate) a: u32,
    /// The second of the two cluster boxes.
    pub(crate) b: u32,
    /// The count of the edges between the two clusters, in either direction.
    pub(crate) count: u32,
}

/// Which rule folds the implementations of an interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fold {
    /// The rule of the viewer, the chip from four implementations in an ego
    /// view and no fold elsewhere.
    Viewer,
    /// The first reduction of a page.
    Page,
}

impl Fold {
    /// Returns the count of free implementations from which a whole graph or
    /// a cluster view of `kind` folds them, or `None` when it does not.
    ///
    /// The viewer folds none there and a page folds a group of two. An ego
    /// view folds by [`Fold::folds_ring`] instead. A path and an operation
    /// view never fold, because a path is a chain and an operation view draws
    /// no two implementations of one interface with their implements edges,
    /// as the only implements edges it draws leave the return type, and the
    /// cluster map draws no type box to fold.
    fn at(self, kind: ViewKind) -> Option<u32> {
        match (self, kind) {
            (Fold::Page, ViewKind::Whole | ViewKind::Cluster) => Some(PAGE_FOLD),
            _ => None,
        }
    }

    /// Returns whether the ring of an interface with `count` implementations
    /// in the graph folds them into one [`Chip::Implementations`], where the
    /// fold takes `freed` boxes out of the ring.
    ///
    /// The viewer folds from four implementations. A page folds as well when
    /// the fold takes at least two boxes out of the ring, so the one group box
    /// stands for at least two.
    fn folds_ring(self, count: u32, freed: usize) -> bool {
        count >= IMPLEMENTATION_CHIP || (self == Fold::Page && id32(freed) >= PAGE_FOLD)
    }
}

/// The parameters a view is built under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Params {
    /// The ring per side.
    ring: u16,
    /// The rule that folds implementations.
    fold: Fold,
    /// The boxes the view may hold, which bounds what an expansion adds.
    boxes: u16,
}

impl View {
    /// Returns the view of `route` under `options` and `budget`.
    ///
    /// The ring of an ego or an operation view is the smaller of the ring of
    /// `options` and of `budget`, and an expansion adds boxes only while the
    /// view holds fewer than the boxes of `budget`. A view over the boxes or
    /// the edges of `budget` is answered empty with
    /// [`EmptyReason::OverBudget`], which is how the whole graph mode says why
    /// it is not offered. A type or a member the graph leaves out is answered
    /// empty with [`EmptyReason::Unreachable`].
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::UnknownType`], [`ViewError::UnknownField`],
    /// [`ViewError::UnknownArg`], [`ViewError::NoRoot`] or
    /// [`ViewError::UnknownCluster`] when `route` names what the schema or
    /// `clustering` does not hold, and [`ViewError::NoGraph`] for a search
    /// and a report page.
    ///
    /// # Panics
    ///
    /// Panics when the graph of `schema` was not built from its model.
    pub fn of(
        schema: &Schema,
        clustering: &Clustering,
        route: &Route,
        options: &Options,
        budget: &Budget,
    ) -> Result<View, ViewError> {
        let params = Params {
            ring: options.ring.min(budget.ring),
            fold: Fold::Viewer,
            boxes: budget.boxes,
        };
        let view = build(schema, clustering, route, options, params)?;
        if view.empty.is_some() || view.fits(budget) {
            return Ok(view);
        }
        let (boxes, edges) = view.size();
        Ok(View::empty(
            view.kind,
            EmptyReason::OverBudget { boxes, edges },
        ))
    }

    /// Returns the same view under the page budget, reduced in the fixed
    /// order of docs/documentation-generator.md section 6.2, split into
    /// numbered parts last, or empty with its reason
    /// (docs/documentation-generator.md section 6.1).
    ///
    /// A type with fewer than two neighbours, counted over the edges `leaf`
    /// keeps and the boxes `hideRoot` keeps, answers one empty view with
    /// [`EmptyReason::FewerThanTwoNeighbours`], or with
    /// [`EmptyReason::Unreachable`] when the graph joins it to none and it is
    /// no root, and a view that fits [`Budget::PAGE`] answers itself. A view
    /// fits when it holds at most the boxes and edges of the budget and the
    /// scene of its layout is at most as wide as the budget. A view over the
    /// budget runs the four reductions of [`Reduction`] in order until it
    /// fits. The first folds the implementations of an interface into its
    /// group box wherever that takes at least two boxes away, the second
    /// records that the layout folds the leaf rows, the third drops the ring
    /// one box at a time down to none, and the fourth splits what is left into
    /// parts along cluster borders, and every part carries the reductions that
    /// shaped it. The split packs by boxes and edges, so a view within them
    /// that the first three steps leave wider than the budget is answered
    /// whole and wider. A view no split brings under the boxes and edges of
    /// the budget is answered empty with [`EmptyReason::OverBudget`].
    ///
    /// # Errors
    ///
    /// Returns the errors of [`View::of`].
    ///
    /// # Panics
    ///
    /// Panics when the graph of `schema` was not built from its model.
    pub fn for_page(
        schema: &Schema,
        clustering: &Clustering,
        route: &Route,
        options: &Options,
    ) -> Result<Vec<View>, ViewError> {
        reduce::for_page(schema, clustering, route, options)
    }

    /// Returns whether the view draws at most the boxes, the edges and, by
    /// the width of its scene, the width of `budget`.
    ///
    /// The scene is the one [`crate::layout::layout_of`] draws, and a view
    /// whose layout crosses its cap is taken as too wide, so a reduction
    /// shrinks it further.
    pub(crate) fn fits_page(&self, schema: &Schema, budget: &Budget) -> bool {
        if !self.fits(budget) {
            return false;
        }
        match crate::layout::layout_of(self, schema, None) {
            Ok(scene) => scene.width <= budget.width_cp,
            Err(crate::layout::LayoutError::Empty) => true,
            Err(crate::layout::LayoutError::CapExceeded { .. }) => false,
        }
    }

    /// Returns a view of `kind` that draws nothing, for `reason`.
    fn empty(kind: ViewKind, reason: EmptyReason) -> View {
        View {
            empty: Some(reason),
            ..View::blank(kind, None)
        }
    }

    /// Returns a view of `kind` with the focus `focus` and nothing else.
    fn blank(kind: ViewKind, focus: Option<NodeId>) -> View {
        View {
            focus,
            nodes: Vec::new(),
            edges: Vec::new(),
            chips: Vec::new(),
            reductions: Vec::new(),
            part: None,
            empty: None,
            kind,
            ranks: Vec::new(),
            row: None,
            groups: Vec::new(),
            trunks: Vec::new(),
        }
    }

    /// Returns the boxes and the edges the view draws.
    ///
    /// The boxes of the cluster map are its cluster boxes and its edges are
    /// its trunks. Every other view counts its type boxes and its group boxes
    /// of implementations.
    fn size(&self) -> (u32, u32) {
        if self.kind == ViewKind::ClusterMap {
            return (id32(self.groups.len()), id32(self.trunks.len()));
        }
        let groups = self
            .chips
            .iter()
            .filter(|c| matches!(c, Chip::Implementations { .. }))
            .count();
        (id32(self.nodes.len() + groups), id32(self.edges.len()))
    }

    /// Returns whether the view draws at most the boxes and the edges of
    /// `budget`.
    fn fits(&self, budget: &Budget) -> bool {
        let (boxes, edges) = self.size();
        boxes <= u32::from(budget.boxes) && edges <= u32::from(budget.edges)
    }
}

/// Returns the view of `route` under `params`, before any budget.
///
/// A view whose route leaves it with no box answers empty with
/// [`EmptyReason::Unreachable`], and the focus of an ego view carries its
/// [`Chip::MoreFields`] as [`more_fields`] counts it.
fn build(
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    options: &Options,
    params: Params,
) -> Result<View, ViewError> {
    let model = &schema.model;
    let mut view = match route {
        Route::Home => overview(schema, clustering),
        Route::Type { name, field, arg } => {
            let ty = type_named(model, name)?;
            check_member(model, ty, field.as_deref(), arg.as_deref())?;
            match schema.graph.node_of(ty) {
                Some(focus) => ego::ego(schema, clustering, focus, options, params),
                None => View::empty(ViewKind::Ego, EmptyReason::Unreachable),
            }
        }
        Route::Op { root, field } => operation::operation(schema, *root, field, params)?,
        Route::Cluster { slug } => cluster::cluster(schema, clustering, slug, params)?,
        Route::Clusters => cluster::map(schema, clustering),
        Route::Whole { root } => whole::whole(schema, root, params)?,
        Route::Path { from, to } => path::path(schema, from, to)?,
        Route::Search { .. }
        | Route::Unreachable
        | Route::Deprecated
        | Route::Scalars
        | Route::Relay => return Err(ViewError::NoGraph),
    };
    if view.empty.is_none() && view.nodes.is_empty() {
        return Ok(View::empty(view.kind, EmptyReason::Unreachable));
    }
    more_fields(schema, &mut view);
    Ok(view)
}

/// Returns the overview, the whole graph of every root when it fits
/// [`Budget::PAGE`] and the cluster map otherwise.
///
/// The whole graph is judged as the viewer draws it, before any reduction, and
/// it is the graph of every root, since the overview of a small schema shows
/// every type.
fn overview(schema: &Schema, clustering: &Clustering) -> View {
    let params = Params {
        ring: Budget::PAGE.ring,
        fold: Fold::Viewer,
        boxes: Budget::PAGE.boxes,
    };
    let all = whole::every_root(schema, params);
    if all.fits(&Budget::PAGE) {
        all
    } else {
        cluster::map(schema, clustering)
    }
}

/// Returns the type called `name`.
fn type_named(model: &Model, name: &str) -> Result<TypeId, ViewError> {
    model
        .type_by_name(name)
        .ok_or_else(|| ViewError::UnknownType(String::from(name)))
}

/// Checks that the type `ty` declares the field `field` and the field the
/// argument `arg`.
///
/// The field of an enum is one of its values, so the route of a type selects
/// an enum value the way it selects a field.
fn check_member(
    model: &Model,
    ty: TypeId,
    field: Option<&str>,
    arg: Option<&str>,
) -> Result<(), ViewError> {
    let Some(field) = field else {
        return Ok(());
    };
    let t = model.ty(ty);
    let unknown = || ViewError::UnknownField {
        ty: String::from(model.name(t.name)),
        field: String::from(field),
    };
    let found = t
        .fields
        .iter()
        .copied()
        .find(|&f| model.name(model.field(f).name) == field);
    let Some(f) = found else {
        let value = t
            .values
            .iter()
            .any(|&v| model.name(model.value(v).name) == field);
        return match (value, arg) {
            (true, None) => Ok(()),
            _ => Err(unknown()),
        };
    };
    let Some(arg) = arg else {
        return Ok(());
    };
    if model
        .field(f)
        .args
        .iter()
        .any(|&a| model.name(model.arg(a).name) == arg)
    {
        Ok(())
    } else {
        Err(ViewError::UnknownArg {
            ty: String::from(model.name(t.name)),
            field: String::from(field),
            arg: String::from(arg),
        })
    }
}

/// A view under construction.
struct Build<'s> {
    /// The schema whose graph the view draws.
    schema: &'s Schema,
    /// The kind of view.
    kind: ViewKind,
    /// The focus.
    focus: Option<NodeId>,
    /// The boxes with their levels.
    nodes: Vec<(NodeId, Level)>,
    /// The rank of every box.
    ranks: Vec<i32>,
    /// The boxes as a set.
    members: Bits,
    /// The edges, in any order and repeated, which [`Build::finish`] sorts.
    edges: Vec<u32>,
    /// The chips.
    chips: Vec<Chip>,
}

impl<'s> Build<'s> {
    /// Returns an empty view of `kind` over the graph of `schema`.
    fn new(schema: &'s Schema, kind: ViewKind, focus: Option<NodeId>) -> Build<'s> {
        Build {
            schema,
            kind,
            focus,
            nodes: Vec::new(),
            ranks: Vec::new(),
            members: Bits::new(schema.graph.nodes.len()),
            edges: Vec::new(),
            chips: Vec::new(),
        }
    }

    /// Returns whether the view holds `node`.
    fn contains(&self, node: NodeId) -> bool {
        self.members.contains(node)
    }

    /// Adds `node` at `level` and `rank`, unless the view holds it already,
    /// which keeps the level and the rank it has.
    fn push(&mut self, node: NodeId, level: Level, rank: i32) {
        if self.members.insert(node) {
            self.nodes.push((node, level));
            self.ranks.push(rank);
        }
    }

    /// Returns the position of `node` among the boxes.
    fn position(&self, node: NodeId) -> Option<usize> {
        self.nodes.iter().position(|&(n, _)| n == node)
    }

    /// Returns the boxes the view holds, its type boxes and its group boxes
    /// of implementations.
    fn boxes(&self) -> usize {
        let groups = self
            .chips
            .iter()
            .filter(|c| matches!(c, Chip::Implementations { .. }))
            .count();
        self.nodes.len() + groups
    }

    /// Returns the view, its edges sorted and drawn once per row and target.
    ///
    /// Of the edges that share a source, a row and a target the first in the
    /// order of the graph stays.
    fn finish(self) -> View {
        let graph = &self.schema.graph;
        let mut keyed: Vec<(EdgeKey, u32)> = self
            .edges
            .iter()
            .map(|&e| {
                let edge = &graph.edges[e as usize];
                ((edge.from, edge.row, edge.to), e)
            })
            .collect();
        keyed.sort_unstable();
        keyed.dedup_by_key(|&mut (key, _)| key);
        let mut edges: Vec<u32> = keyed.into_iter().map(|(_, e)| e).collect();
        edges.sort_unstable();
        View {
            nodes: self.nodes,
            edges,
            chips: self.chips,
            ranks: self.ranks,
            ..View::blank(self.kind, self.focus)
        }
    }
}

/// The source, the row and the target of an edge, which a view draws once.
type EdgeKey = (NodeId, Option<u32>, NodeId);

/// A set of the nodes of one graph as a bit set.
#[derive(Clone, Debug)]
struct Bits {
    /// One bit per node, the node of id `i` at bit `i % 64` of word `i / 64`.
    words: Vec<u64>,
}

impl Bits {
    /// Returns the empty set over `len` nodes.
    fn new(len: usize) -> Bits {
        Bits {
            words: vec![0; len.div_ceil(64)],
        }
    }

    /// Adds `node` and returns whether it was absent.
    fn insert(&mut self, node: NodeId) -> bool {
        let i = node.index();
        let (word, bit) = (&mut self.words[i / 64], 1u64 << (i % 64));
        let absent = *word & bit == 0;
        *word |= bit;
        absent
    }

    /// Returns whether the set holds `node`.
    fn contains(&self, node: NodeId) -> bool {
        let i = node.index();
        self.words
            .get(i / 64)
            .is_some_and(|word| word & (1u64 << (i % 64)) != 0)
    }
}

/// Returns every node of `nodes` with its position, sorted by node.
fn positions(nodes: impl Iterator<Item = NodeId>) -> Vec<(NodeId, usize)> {
    let mut out: Vec<(NodeId, usize)> = nodes.enumerate().map(|(i, n)| (n, i)).collect();
    out.sort_unstable();
    out
}

/// Returns the position of `node` in a table of [`positions`].
fn find(positions: &[(NodeId, usize)], node: NodeId) -> Option<usize> {
    positions
        .binary_search_by_key(&node, |&(n, _)| n)
        .ok()
        .map(|i| positions[i].1)
}

/// Returns the box a chip hangs at, the box the arrow of a stub leaves.
fn chip_node(chip: &Chip) -> NodeId {
    match *chip {
        Chip::More { at, .. } | Chip::Implementations { at, .. } | Chip::MoreFields { at, .. } => {
            at
        }
        Chip::Stub { from, .. } => from,
    }
}

/// Returns the type of the node `node`.
fn type_of(schema: &Schema, node: NodeId) -> TypeId {
    schema.graph.nodes[node.index()].ty
}

/// Returns the name of the type `ty`.
fn type_name(model: &Model, ty: TypeId) -> &str {
    model.name(model.ty(ty).name)
}

/// Returns whether `node` is a custom scalar, which is a chip inside a row
/// and a box only as the focus or an end of a path.
fn is_scalar(schema: &Schema, node: NodeId) -> bool {
    schema.model.ty(type_of(schema, node)).kind == Kind::Scalar
}

/// Returns whether `node` enters a view as a box other than its focus and
/// the ends of a path, which a custom scalar never does and a root does not
/// under `hideRoot=1`.
fn is_box(schema: &Schema, node: NodeId) -> bool {
    !(is_scalar(schema, node) || (schema.transforms.hide_root && is_root(schema, node)))
}

/// Returns whether `node` is one of the three roots.
fn is_root(schema: &Schema, node: NodeId) -> bool {
    schema.graph.nodes[node.index()].is_root
}

/// Returns whether the view draws `edge`, which it does unless the edge
/// leaves a row that `leaf=0` hides.
pub(crate) fn drawn(schema: &Schema, edge: &Edge) -> bool {
    schema.transforms.leaf
        || edge
            .row
            .is_none_or(|row| !is_leaf_row(schema, edge.from, row))
}

/// Returns whether the row at position `row` of `node` is a leaf row, a row
/// whose type after the Relay collapse is a scalar or an enum.
pub(crate) fn is_leaf_row(schema: &Schema, node: NodeId, row: u32) -> bool {
    let row = &schema.graph.nodes[node.index()].rows[row as usize];
    let named = match row.collapsed {
        Some(i) => schema.transformed.collapsed[i as usize].node.named(),
        None => schema.model.field(row.field).type_ref.named(),
    };
    matches!(schema.model.ty(named).kind, Kind::Scalar | Kind::Enum)
}

/// What becomes of one row of a box, which the layout draws by and the view
/// counts the `+N fields` row of its focus by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowFate {
    /// The box draws it.
    Draw,
    /// The last row of the box, the one that counts what the box leaves out,
    /// counts it.
    More,
    /// The row of the folded inherited fields counts it.
    Inherited,
    /// The box neither draws nor counts it.
    Hidden,
}

/// What the fate of one field row reads of the row.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowFacts {
    /// An edge or a stub of the view leaves the row.
    pub(crate) links: bool,
    /// The row names a scalar or an enum, [`is_leaf_row`].
    pub(crate) leaf: bool,
    /// The row is a field inherited from an interface.
    pub(crate) inherited: bool,
}

/// What the fate of one field row reads of the view and its options.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowRules {
    /// The option `leaf`, which draws the leaf rows.
    pub(crate) leaf: bool,
    /// The second reduction of a page, [`Reduction::FoldedLeafRows`], which
    /// folds the leaf rows.
    pub(crate) folds_leaves: bool,
}

impl RowRules {
    /// Returns the rules of `view` of `schema`.
    pub(crate) fn of(schema: &Schema, view: &View) -> RowRules {
        RowRules {
            leaf: schema.transforms.leaf,
            folds_leaves: view.reductions.contains(&Reduction::FoldedLeafRows),
        }
    }
}

/// Returns what becomes of a field row with `facts` at `level`, after
/// `drawn` drawn rows of its box, under `rules`.
///
/// Under `leaf=0` a leaf row that no edge leaves is neither drawn nor counted.
/// A box at [`Level::L0`] draws its header alone. At every other level a row
/// an edge or a stub leaves is drawn, since a curve leaves a drawn row, and
/// [`Level::L1`] counts the rest in its last row. At [`Level::L2`] and
/// [`Level::L3`] the inherited fields fold into one row, after the second
/// reduction of a page the leaf rows fold into the last row, and the focus
/// draws up to [`FOCUS_ROWS`] and counts the rest.
pub(crate) fn row_fate(level: Level, facts: RowFacts, drawn: usize, rules: RowRules) -> RowFate {
    if facts.leaf && !rules.leaf && !facts.links {
        return RowFate::Hidden;
    }
    match level {
        Level::L0 => RowFate::Hidden,
        _ if facts.links => RowFate::Draw,
        Level::L1 => RowFate::More,
        _ if facts.inherited => RowFate::Inherited,
        _ if facts.leaf && rules.folds_leaves => RowFate::More,
        Level::L3 if drawn >= FOCUS_ROWS => RowFate::More,
        Level::L2 | Level::L3 => RowFate::Draw,
    }
}

/// Returns what becomes of a value an enum box at `level` shows, after
/// `drawn` drawn values.
///
/// An enum draws its values as rows, none at [`Level::L0`], the count alone at
/// [`Level::L1`], every value at [`Level::L2`] and up to [`FOCUS_ROWS`] at the
/// focus.
pub(crate) fn value_fate(level: Level, drawn: usize) -> RowFate {
    match level {
        Level::L0 => RowFate::Hidden,
        Level::L1 => RowFate::More,
        Level::L3 if drawn >= FOCUS_ROWS => RowFate::More,
        Level::L2 | Level::L3 => RowFate::Draw,
    }
}

/// Returns, for every box of `view` by its position, the rows of its node
/// that an edge or a stub of the view leaves, and at the focus of an
/// operation view the row of its field, each list sorted and without repeats.
///
/// # Panics
///
/// Panics when `view` was not built from the graph of `schema`.
pub(crate) fn linking_rows(schema: &Schema, view: &View) -> Vec<Vec<u32>> {
    let graph = &schema.graph;
    let pos = positions(view.nodes.iter().map(|&(n, _)| n));
    let mut out: Vec<Vec<u32>> = vec![Vec::new(); view.nodes.len()];
    let mut add = |node: NodeId, row: Option<u32>| {
        if let (Some(p), Some(r)) = (find(&pos, node), row) {
            out[p].push(r);
        }
    };
    for &e in &view.edges {
        let edge = &graph.edges[e as usize];
        add(edge.from, edge.row);
    }
    for chip in &view.chips {
        if let Chip::Stub { from, row, .. } = *chip {
            add(from, row);
        }
    }
    if view.kind == ViewKind::Operation
        && let Some(focus) = view.focus
    {
        add(focus, view.row);
    }
    for rows in &mut out {
        rows.sort_unstable();
        rows.dedup();
    }
    out
}

/// Sets the [`Chip::MoreFields`] of the focus of an ego view to the count its
/// `+N fields` row reads, and leaves it out when the row counts nothing.
///
/// The count is what [`row_fate`] or, for an enum, [`value_fate`] counts at
/// the level of the focus, with the rows of [`linking_rows`] and the rules of
/// [`RowRules::of`], which is what the layout draws the focus by. The view
/// sets it once it is built and again once the reductions of a page are
/// recorded, since the leaf fold changes the count.
fn more_fields(schema: &Schema, view: &mut View) {
    view.chips.retain(|c| !matches!(c, Chip::MoreFields { .. }));
    if view.kind != ViewKind::Ego {
        return;
    }
    let Some(focus) = view.focus else {
        return;
    };
    let Some(p) = view.nodes.iter().position(|&(n, _)| n == focus) else {
        return;
    };
    let level = view.nodes[p].1;
    let (model, t) = (&schema.model, &schema.transformed);
    let node = &schema.graph.nodes[focus.index()];
    let mut drawn = 0;
    let mut count: u32 = 0;
    let mut tally = |fate: RowFate, drawn: &mut usize| match fate {
        RowFate::Draw => *drawn += 1,
        RowFate::More => count = count.saturating_add(1),
        RowFate::Inherited | RowFate::Hidden => {}
    };
    if model.ty(node.ty).kind == Kind::Enum {
        for v in t.values(model, node.ty) {
            if !t.hides(model, Member::Value(v)) {
                tally(value_fate(level, drawn), &mut drawn);
            }
        }
    } else {
        let linking = &linking_rows(schema, view)[p];
        let rules = RowRules::of(schema, view);
        for (i, row) in node.rows.iter().enumerate() {
            let r = id32(i);
            let facts = RowFacts {
                links: linking.binary_search(&r).is_ok(),
                leaf: is_leaf_row(schema, focus, r),
                inherited: row.inherited_from.is_some(),
            };
            tally(row_fate(level, facts, drawn, rules), &mut drawn);
        }
    }
    if count > 0 {
        view.chips.push(Chip::MoreFields { at: focus, count });
    }
}

/// Returns the count of the implementations of the interface `node` in the
/// graph, the types with an implements edge into it.
///
/// A model names an interface once among the interfaces of a type, so each
/// edge is one implementation.
fn implementers(schema: &Schema, node: NodeId) -> u32 {
    let graph = &schema.graph;
    id32(
        graph
            .inbound(node)
            .iter()
            .filter(|&&e| graph.edges[e as usize].kind == EdgeKind::Implements)
            .count(),
    )
}

/// Returns `i` as a `u32`, saturated.
///
/// Every count a view numbers is capped far below `u32::MAX` by
/// [`ReadLimits`](crate::limits::ReadLimits), so the saturation never
/// happens.
fn id32(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// Returns `i` as a rank, saturated.
fn rank32(i: u32) -> i32 {
    i32::try_from(i).unwrap_or(i32::MAX)
}
