// SPDX-License-Identifier: MIT
/*
 * aoviewer-core view operation: the view of one root field and its first ring.
 *
 * The field signature sits in the centre, the input types of its arguments on
 * the left and the return type with its first ring on the right.
 */

//! The operation view.
//!
//! The root that declares the field is the focus at rank 0, at [`Level::L1`],
//! and the view keeps the row of the field, so the box draws its header and
//! the signature of that one field. The types of the arguments sit at rank -1
//! and the return type at rank 1, both at [`Level::L2`] with all their rows.
//! The first ring of the return type is the types it references, at rank 2 at
//! [`Level::L1`], at most the ring of them in the order of its rows, the rest
//! one [`Chip::More`]. A type that is both an argument type and the return
//! type is drawn once, as the return type.
//!
//! The edges are those that leave the row of the field and those that join
//! the return type to its ring. The row of the field draws its edges whatever
//! `leaf` says, because the view is about that row. A root field that the
//! graph leaves out, because the Relay collapse deletes it, because it is
//! deprecated under `deprecated=0` or because `hide=` hides its root, answers
//! an empty view.
//!
//! Every loop runs over the fields of one root or over the edges that leave
//! one node of one graph.

use alloc::{
    string::String,
    vec::Vec, //
};

use super::{
    Bits,
    Build,
    Chip,
    EmptyReason,
    Level,
    Params,
    View,
    ViewError,
    ViewKind,
    drawn,
    id32,
    is_box,
    type_name, //
};
use crate::{
    graph::{
        EdgeKind,
        NodeId, //
    },
    model::RootKind,
    schema::Schema,
};

/// Returns the operation view of the field `field` of the root `root` under
/// `params`.
///
/// # Errors
///
/// Returns [`ViewError::NoRoot`] when the schema declares no root `root` and
/// [`ViewError::UnknownField`] when the root declares no field `field`.
pub(super) fn operation(
    schema: &Schema,
    root: RootKind,
    field: &str,
    params: Params,
) -> Result<View, ViewError> {
    let (model, graph) = (&schema.model, &schema.graph);
    let root_ty = model.roots().get(root).ok_or(ViewError::NoRoot(root))?;
    let declared = model
        .ty(root_ty)
        .fields
        .iter()
        .copied()
        .find(|&f| model.name(model.field(f).name) == field);
    let Some(declared) = declared else {
        return Err(ViewError::UnknownField {
            ty: String::from(type_name(model, root_ty)),
            field: String::from(field),
        });
    };
    let empty = View::empty(ViewKind::Operation, EmptyReason::Unreachable);
    let Some(centre) = graph.node_of(root_ty) else {
        return Ok(empty);
    };
    let Some(row) = graph.nodes[centre.index()]
        .rows
        .iter()
        .position(|r| r.field == declared)
    else {
        return Ok(empty);
    };
    let row = Some(id32(row));
    let from_row: Vec<u32> = graph
        .outbound(centre)
        .iter()
        .copied()
        .filter(|&e| graph.edges[e as usize].row == row)
        .collect();
    let mut b = Build::new(schema, ViewKind::Operation, Some(centre));
    b.push(centre, Level::L1, 0);
    let ret = from_row
        .iter()
        .map(|&e| &graph.edges[e as usize])
        .find(|e| e.kind == EdgeKind::Field && e.to != centre && is_box(schema, e.to))
        .map(|e| e.to);
    if let Some(ret) = ret {
        b.push(ret, Level::L2, 1);
    }
    for &e in &from_row {
        let edge = &graph.edges[e as usize];
        if edge.kind == EdgeKind::Argument && edge.to != centre && is_box(schema, edge.to) {
            b.push(edge.to, Level::L2, -1);
        }
    }
    for &e in &from_row {
        if b.contains(graph.edges[e as usize].to) {
            b.edges.push(e);
        }
    }
    if let Some(ret) = ret {
        add_return_ring(&mut b, ret, usize::from(params.ring));
    }
    let mut view = b.finish();
    view.row = row;
    Ok(view)
}

/// Adds the first ring of the return type `ret`, at most `ring` of the types
/// it references, and the edges that join it to them.
fn add_return_ring(b: &mut Build<'_>, ret: NodeId, ring: usize) {
    let schema = b.schema;
    let graph = &schema.graph;
    let mut seen = Bits::new(graph.nodes.len());
    let mut outs = Vec::new();
    for &e in graph.outbound(ret) {
        let edge = &graph.edges[e as usize];
        let to = edge.to;
        if to != ret
            && drawn(schema, edge)
            && !b.contains(to)
            && is_box(schema, to)
            && seen.insert(to)
        {
            outs.push(to);
        }
    }
    let taken = outs.len().min(ring);
    let mut added = Bits::new(graph.nodes.len());
    for &n in &outs[..taken] {
        b.push(n, Level::L1, 2);
        added.insert(n);
    }
    for &e in graph.outbound(ret) {
        let edge = &graph.edges[e as usize];
        if drawn(schema, edge) && (edge.to == ret || added.contains(edge.to)) {
            b.edges.push(e);
        }
    }
    if outs.len() > taken {
        b.chips.push(Chip::More {
            at: ret,
            count: id32(outs.len() - taken),
            inbound: false,
        });
    }
}
