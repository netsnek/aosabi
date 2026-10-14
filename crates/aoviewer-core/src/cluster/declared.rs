// SPDX-License-Identifier: MIT
/*
 * aoviewer-core cluster declared: the clusters an author declares in
 * aosabi.docs.toml.
 *
 * A declared map wins over both automatic sources.
 */

//! The declared clusters.
//!
//! A declared cluster maps its name to type name globs, and a node joins the
//! first declared cluster, in the order the caller passes them, one of whose
//! globs matches the name of its type. A node no glob matches goes to the other
//! bucket, a glob that [`Glob::parse`] refuses matches nothing, and a declared
//! cluster that holds no type is no cluster. Small declared clusters stay as
//! the author wrote them, because the merge rule belongs to the automatic
//! sources.
//!
//! The loops run over the nodes of one graph and over at most
//! [`MAX_DECLARED`](super::MAX_DECLARED) clusters of at most
//! [`MAX_DECLARED_GLOBS`] globs each, and every match runs under the step cap
//! of [`Glob::matches`].

use alloc::vec::Vec;

use super::{
    Declared,
    MAX_DECLARED_GLOBS,
    OTHER,
    Placement,
    id32,
    type_of, //
};
use crate::{
    schema::Schema,
    transform::Glob, //
};

/// Returns the placement of the nodes of `schema` into the clusters of
/// `declared`, whose order the groups keep.
pub(super) fn place(schema: &Schema, declared: &[Declared]) -> Placement {
    let (model, graph) = (&schema.model, &schema.graph);
    let globs: Vec<Vec<Glob>> = declared
        .iter()
        .map(|d| {
            let read = &d.globs[..d.globs.len().min(MAX_DECLARED_GLOBS)];
            read.iter().filter_map(|g| Glob::parse(g)).collect()
        })
        .collect();
    let names = declared.iter().map(|d| d.name.clone()).collect();
    let mut placement = Placement::new(names, graph.nodes.len());
    for n in graph.node_ids() {
        if graph.nodes[n.index()].is_root {
            continue;
        }
        let name = model.name(model.ty(type_of(graph, n)).name);
        placement.at[n.index()] = globs
            .iter()
            .position(|list| list.iter().any(|g| g.matches(name)))
            .map_or(OTHER, id32);
    }
    placement
}
