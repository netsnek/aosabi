// SPDX-License-Identifier: MIT
/*
 * aoviewer-core graph: the type graph with its adjacency built once per schema
 * and transform set.
 *
 * Nodes are the named types but the leaf scalars, and edges are field,
 * argument, implements and union member.
 */

//! The type graph.
//!
//! [`Graph::build`] reads a finished [`Model`] beside its [`Transformed`] and
//! makes one [`Node`] per type the graph draws, in schema order. A type is no
//! node when the transforms hide it, when the Relay collapse found it as a
//! Relay type, removed or kept, because Voyager draws no Relay type as a node,
//! and when it is built in, which keeps the twelve scalars of a build out of every
//! box. Under `nodes=composite` the objects, interfaces and unions are the
//! nodes. Under `nodes=all` the enums and the inputs are nodes too, because
//! enums are boxes, and so is a custom scalar, because a view centres a node
//! and a custom scalar becomes a box when it is focused.
//!
//! A node of an object, an interface or an input carries one [`Row`] per
//! field the graph shows, in the order of the transforms, and a field
//! [`Transformed::hides`] leaves out has no row. The edges are
//!
//! - [`EdgeKind::Field`], from a row to the node of its named type after the
//!   Relay collapse,
//! - [`EdgeKind::Argument`], from a row to the node of the named type of an
//!   argument the graph shows, once per row and target, because an edge
//!   carries no argument and a view draws one edge per row and target,
//! - [`EdgeKind::Implements`], from an implementation to its interface, which
//!   is the direction the dotted edges run in, with the interface `Node` the
//!   collapse strips left out,
//! - [`EdgeKind::UnionMember`], from a union to a member.
//!
//! An edge exists only between two nodes. The edges of a node follow its
//! rows, a row's field edge before its argument edges, then its interfaces,
//! then its members, each in the order of the transforms, and the nodes follow
//! schema order. That is the order of the edge list, and [`Graph::outbound`]
//! and [`Graph::inbound`] answer in it.
//!
//! The graph keeps every row and every root whatever `leaf` and `hideRoot`
//! say. Voyager's `showLeafFields` changes the rows a box draws and never a
//! node or an edge, and `hideRoot` removes the root after the walk, so the
//! views and [`Graph::voyager_form`] read both flags.
//!
//! The adjacency, the three walks and the neighbour counts are computed once
//! by [`Graph::build`], which is a requirement of the core, because a pass
//! that rebuilds the adjacency per view is quadratic in the schema.
//!
//! Every loop runs over the types, fields, arguments, interfaces or members of
//! one model, whose counts [`ReadLimits`](crate::limits::ReadLimits) caps, or
//! over the nodes and edges built from them.
//!
//! [type-graph.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/graph/type-graph.ts
//! [dot.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/graph/dot.ts

mod adjacency;
mod degree;
mod voyager;
mod walk;

use alloc::{
    vec,
    vec::Vec, //
};

pub use walk::{
    ReachSet,
    Walks, //
};

use crate::{
    model::{
        FieldId,
        Kind,
        Model,
        TypeId, //
    },
    transform::{
        Member,
        NodeSet,
        Transformed,
        Transforms, //
    },
};
use adjacency::Adjacency;

/// The mark of a type that is no node in [`Graph`]'s table of nodes by type.
const NONE: u32 = u32::MAX;

/// The type graph with its adjacency built once per schema and transform set,
/// outbound and inbound over one edge index.
///
/// The fields are read only after [`Graph::build`], which fills the adjacency,
/// the walks and the neighbour counts from them.
#[derive(Clone, Debug)]
pub struct Graph {
    /// The nodes in schema order, indexed by [`NodeId`].
    pub nodes: Vec<Node>,
    /// The edges in the order of their sources, indexed by the edge indexes
    /// [`Graph::outbound`] and [`Graph::inbound`] answer.
    pub edges: Vec<Edge>,
    /// The node of every type, indexed by type, or [`NONE`].
    index: Vec<u32>,
    /// The outbound and inbound edge indexes of every node.
    adjacency: Adjacency,
    /// The three walks.
    walks: Walks,
    /// The neighbour count of every node.
    neighbours: Vec<u32>,
}

/// A node of the graph, one named type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// The type.
    pub ty: TypeId,
    /// The fields the graph shows, in the order of the transforms.
    pub rows: Vec<Row>,
    /// Whether the type is one of the three roots.
    pub is_root: bool,
}

/// A row of a node, one field or input field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The field.
    pub field: FieldId,
    /// The named type the field returns or takes after the Relay collapse,
    /// when it is a node.
    pub target: Option<TypeId>,
    /// The position of the field's collapse in
    /// [`Transformed::collapsed`], when the Relay collapse collapsed it.
    pub collapsed: Option<u32>,
    /// The interface the field comes from, when it is inherited.
    pub inherited_from: Option<TypeId>,
}

/// An edge of the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    /// The node the edge leaves.
    pub from: NodeId,
    /// The position of the row the edge leaves among the rows of `from`, for
    /// a field and an argument edge.
    pub row: Option<u32>,
    /// The node the edge enters.
    pub to: NodeId,
    /// The kind.
    pub kind: EdgeKind,
}

/// The id of a node, its position in [`Graph::nodes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(pub(crate) u32);

impl NodeId {
    /// Returns the position of the node in [`Graph::nodes`], which a caller
    /// uses to index a table of its own beside the graph.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The kind of an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// A field or an input field names the target.
    Field,
    /// An argument of the field names the target.
    Argument,
    /// The source implements the target interface.
    Implements,
    /// The target is a member of the source union.
    UnionMember,
}

/// Returns `i` as a `u32`, saturated.
///
/// Every count the graph numbers is capped far below `u32::MAX` by
/// [`ReadLimits`](crate::limits::ReadLimits), so the saturation never
/// happens.
fn id32(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

impl Graph {
    /// Builds the type graph of `model` under `transforms` and their result
    /// `transformed`, with its adjacency, its walks and its neighbour counts.
    ///
    /// The graph reads the node set of `transforms` and everything else from
    /// `transformed`, which [`apply`](crate::transform::apply) made from the
    /// same model and transforms.
    ///
    /// # Panics
    ///
    /// Panics when `transformed` comes from another model.
    #[must_use]
    pub fn build(model: &Model, transforms: &Transforms, transformed: &Transformed) -> Graph {
        let types: Vec<TypeId> = model
            .types()
            .iter()
            .copied()
            .filter(|&t| is_node(model, transforms.nodes, transformed, t))
            .collect();
        let mut index = vec![NONE; model.types().len()];
        for (i, t) in types.iter().enumerate() {
            index[t.index()] = id32(i);
        }
        let make = Make {
            model,
            transformed,
            index: &index,
        };
        let mut nodes = Vec::with_capacity(types.len());
        let mut edges = Vec::new();
        for (i, &ty) in types.iter().enumerate() {
            let node = make.node(ty);
            make.edges(NodeId(id32(i)), &node, &mut edges);
            nodes.push(node);
        }
        let adjacency = Adjacency::build(nodes.len(), &edges);
        let mut graph = Graph {
            nodes,
            edges,
            index,
            adjacency,
            walks: Walks::default(),
            neighbours: Vec::new(),
        };
        graph.walks = Walks::compute(&graph, model);
        graph.neighbours = degree::neighbours(&graph, model);
        graph
    }

    /// Returns the node of the type `ty`, if the type is one.
    #[must_use]
    pub fn node_of(&self, ty: TypeId) -> Option<NodeId> {
        self.index
            .get(ty.index())
            .copied()
            .filter(|&n| n != NONE)
            .map(NodeId)
    }

    /// Returns every node id in order.
    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> + use<> {
        (0..self.nodes.len()).map(|i| NodeId(id32(i)))
    }

    /// Returns the indexes of the edges that leave `node`, in the order of the
    /// edge list.
    ///
    /// # Panics
    ///
    /// Panics when `node` comes from another graph.
    #[must_use]
    pub fn outbound(&self, node: NodeId) -> &[u32] {
        self.adjacency.outbound(node)
    }

    /// Returns the indexes of the edges that enter `node`, in the order of the
    /// edge list.
    ///
    /// # Panics
    ///
    /// Panics when `node` comes from another graph.
    #[must_use]
    pub fn inbound(&self, node: NodeId) -> &[u32] {
        self.adjacency.inbound(node)
    }

    /// Returns the three walks of reachability.
    #[must_use]
    pub fn walks(&self) -> &Walks {
        &self.walks
    }

    /// Returns the count of the nodes a view of `node` draws as its
    /// neighbours.
    ///
    /// A neighbour is another node joined to `node` by an edge of any kind in
    /// either direction, counted once. A custom scalar is never counted as the
    /// neighbour of another node, and a custom scalar's own count is the count
    /// of the nodes that use it. A type with fewer than two neighbours has no
    /// neighbourhood diagram.
    ///
    /// # Panics
    ///
    /// Panics when `node` comes from another graph.
    #[must_use]
    pub fn neighbours(&self, node: NodeId) -> u32 {
        self.neighbours[node.index()]
    }

    /// Returns the edge at `index` of an adjacency list.
    fn edge(&self, index: u32) -> &Edge {
        &self.edges[index as usize]
    }
}

/// Returns whether the type `t` is a node under `nodes` and `transformed`.
fn is_node(model: &Model, nodes: NodeSet, transformed: &Transformed, t: TypeId) -> bool {
    if transformed.is_hidden(t) || transformed.is_relay_type(t) || transformed.is_builtin(t) {
        return false;
    }
    match nodes {
        NodeSet::Composite => is_composite(model.ty(t).kind),
        NodeSet::All => true,
    }
}

/// Returns whether `kind` is an object, an interface or a union, the kinds
/// Voyager draws.
///
/// [type-graph.ts]: https://github.com/APIs-guru/graphql-voyager/blob/656b8ddb4e7bec958d7300e2d737f632c7b5ba30/src/graph/type-graph.ts
fn is_composite(kind: Kind) -> bool {
    matches!(kind, Kind::Object | Kind::Interface | Kind::Union)
}

/// The inputs of the build of the nodes and edges.
struct Make<'a> {
    model: &'a Model,
    transformed: &'a Transformed,
    /// The node of every type, indexed by type, or [`NONE`].
    index: &'a [u32],
}

impl Make<'_> {
    /// Returns the node of the type `t`, if the type is one.
    fn node_of(&self, t: TypeId) -> Option<NodeId> {
        Some(self.index[t.index()])
            .filter(|&n| n != NONE)
            .map(NodeId)
    }

    /// Returns the node of the type `ty` with its rows.
    fn node(&self, ty: TypeId) -> Node {
        let (model, t) = (self.model, self.transformed);
        let rows = t
            .fields(model, ty)
            .into_iter()
            .filter(|&f| !t.hides(model, Member::Field(f)))
            .map(|f| self.row(f))
            .collect();
        Node {
            ty,
            rows,
            is_root: model.roots().kind_of(ty).is_some(),
        }
    }

    /// Returns the row of the field `f`.
    fn row(&self, f: FieldId) -> Row {
        let t = self.transformed;
        let collapsed = t.collapsed.binary_search_by_key(&f, |c| c.field).ok();
        let named = match collapsed {
            Some(i) => t.collapsed[i].node.named(),
            None => self.model.field(f).type_ref.named(),
        };
        Row {
            field: f,
            target: self.node_of(named).map(|_| named),
            collapsed: collapsed.map(id32),
            inherited_from: t.inherited_from(f),
        }
    }

    /// Appends the edges that leave `node`, whose id is `from`, to `out`.
    fn edges(&self, from: NodeId, node: &Node, out: &mut Vec<Edge>) {
        let (model, t) = (self.model, self.transformed);
        for (i, row) in node.rows.iter().enumerate() {
            let row_at = Some(id32(i));
            if let Some(to) = row.target.and_then(|ty| self.node_of(ty)) {
                out.push(Edge {
                    from,
                    row: row_at,
                    to,
                    kind: EdgeKind::Field,
                });
            }
            self.argument_edges(from, row_at, row.field, out);
        }
        let stripped = t.node_interface();
        for i in t.implements(model, node.ty) {
            if let Some(to) = self.node_of(i).filter(|_| Some(i) != stripped) {
                out.push(Edge {
                    from,
                    row: None,
                    to,
                    kind: EdgeKind::Implements,
                });
            }
        }
        for m in t.members(model, node.ty) {
            if let Some(to) = self.node_of(m) {
                out.push(Edge {
                    from,
                    row: None,
                    to,
                    kind: EdgeKind::UnionMember,
                });
            }
        }
    }

    /// Appends one argument edge per target node of the arguments of the
    /// field `field` the graph shows, leaving the row `row` of `from`.
    fn argument_edges(&self, from: NodeId, row: Option<u32>, field: FieldId, out: &mut Vec<Edge>) {
        let (model, t) = (self.model, self.transformed);
        let first = out.len();
        for a in t.args(model, field) {
            if t.hides(model, Member::Arg(a)) {
                continue;
            }
            let Some(to) = self.node_of(model.arg(a).type_ref.named()) else {
                continue;
            };
            if out[first..].iter().all(|e| e.to != to) {
                out.push(Edge {
                    from,
                    row,
                    to,
                    kind: EdgeKind::Argument,
                });
            }
        }
    }
}
