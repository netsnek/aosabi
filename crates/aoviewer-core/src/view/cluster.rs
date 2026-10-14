// SPDX-License-Identifier: MIT
/*
 * aoviewer-core view cluster: the view of one cluster.
 *
 * A cluster route has its page and its cluster graph.
 */

//! The cluster view and the cluster map.
//!
//! **The cluster view.** `#/cluster/<slug>` draws the types of the cluster at
//! [`Level::L1`] with the edges between them, and one [`Chip::Stub`] per
//! neighbouring cluster. A neighbour is another cluster, a root, or the rest,
//! which is the other bucket and the types outside every cluster taken
//! together, and a custom scalar is none. The stub of a neighbour leaves the
//! first row of the cluster, in the order of its types and their edges, that
//! references it, or, when no type of the cluster references it, the first
//! type it references, with no row. The boxes follow the order of the cluster,
//! which is [`name_order`]. A box is ranked by its breadth first distance
//! along the edges inside the cluster from the types no other type of the
//! cluster references, and a cycle that no such type reaches starts at its
//! first type in that order. A custom scalar the clustering places in the
//! cluster is no box of the view and no edge of the view enters it, since it
//! is a chip inside the rows that use it.
//!
//! **The cluster map.** `#/clusters` draws one box per cluster, in the order
//! of the clustering, then one for the other bucket under its name when it
//! holds a type. A box carries its name, its type count and the three most
//! referenced types inside it at [`Level::L0`], most referenced meaning the
//! most edges entering the type, then [`name_order`]. One trunk joins each
//! pair of boxes that edges join, carrying their count in both directions. The
//! roots and the types outside every cluster are no box of the map, since a
//! root is a page of its own and the outside types are no cluster. A custom
//! scalar is none of the three types a box names and an edge into it counts
//! for no trunk, for the reason of the cluster view, while the type count of a
//! box is every type the clustering places in the cluster, custom scalars
//! included. The map has no focus and its ranks are 0, since it is laid out
//! once per schema and never around a focus.
//!
//! Every loop runs over the clusters of one clustering, over the nodes and
//! edges of one graph, over the types of one cluster, or over the neighbours
//! of one cluster, which are at most the clusters, the three roots and the
//! rest.

use alloc::{
    string::String,
    vec,
    vec::Vec, //
};

use super::{
    Build,
    Chip,
    Level,
    MAP_TYPES,
    MapGroup,
    MapTrunk,
    NONE,
    OTHER_NAME,
    Params,
    View,
    ViewError,
    ViewKind,
    drawn,
    find,
    id32,
    is_root,
    is_scalar,
    positions,
    rank32,
    reduce,
    type_of, //
};
use crate::{
    cluster::Clustering,
    graph::NodeId,
    schema::Schema,
    sort::name_order, //
};

/// A neighbour of a cluster, which one stub stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Neighbour {
    /// Another cluster, by its position in the clustering.
    Cluster(u32),
    /// A root.
    Root(NodeId),
    /// The other bucket and the types outside every cluster.
    Rest,
}

/// Returns the view of the cluster whose slug is `slug` under `params`.
///
/// # Errors
///
/// Returns [`ViewError::UnknownCluster`] when `clustering` holds no cluster
/// of that slug.
pub(super) fn cluster(
    schema: &Schema,
    clustering: &Clustering,
    slug: &str,
    params: Params,
) -> Result<View, ViewError> {
    let graph = &schema.graph;
    let found = clustering.clusters.iter().find(|c| c.slug == slug);
    let Some(found) = found else {
        return Err(ViewError::UnknownCluster(String::from(slug)));
    };
    let members: Vec<NodeId> = found
        .types
        .iter()
        .filter_map(|&ty| graph.node_of(ty))
        .filter(|&n| !is_scalar(schema, n))
        .collect();
    let mut b = Build::new(schema, ViewKind::Cluster, None);
    for (&n, rank) in members.iter().zip(layers(schema, &members)) {
        b.push(n, Level::L1, rank);
    }
    for &n in &members {
        for &e in graph.outbound(n) {
            let edge = &graph.edges[e as usize];
            if drawn(schema, edge) && b.contains(edge.to) {
                b.edges.push(e);
            }
        }
    }
    add_stubs(&mut b, clustering, &members);
    let mut view = b.finish();
    if let Some(at) = params.fold.at(ViewKind::Cluster) {
        reduce::fold(&mut view, schema, at);
    }
    Ok(view)
}

/// Returns the rank of every node of `members`, in their order, by breadth
/// first distance along the drawn edges between them.
fn layers(schema: &Schema, members: &[NodeId]) -> Vec<i32> {
    let graph = &schema.graph;
    let at = positions(members.iter().copied());
    let inside = |node: NodeId| find(&at, node);
    let count = members.len();
    let mut referenced = vec![false; count];
    for &n in members {
        for &e in graph.outbound(n) {
            let edge = &graph.edges[e as usize];
            if edge.to != n
                && drawn(schema, edge)
                && let Some(j) = inside(edge.to)
            {
                referenced[j] = true;
            }
        }
    }
    let mut rank = vec![NONE; count];
    let mut queue: Vec<usize> = (0..count).filter(|&i| !referenced[i]).collect();
    for &i in &queue {
        rank[i] = 0;
    }
    let mut head = 0;
    for _ in 0..=count {
        for _ in 0..count {
            let Some(&i) = queue.get(head) else {
                break;
            };
            head += 1;
            for &e in graph.outbound(members[i]) {
                let edge = &graph.edges[e as usize];
                let Some(j) = inside(edge.to).filter(|&j| rank[j] == NONE) else {
                    continue;
                };
                if drawn(schema, edge) {
                    rank[j] = rank[i] + 1;
                    queue.push(j);
                }
            }
        }
        let Some(i) = (0..count).find(|&i| rank[i] == NONE) else {
            break;
        };
        rank[i] = 0;
        queue.push(i);
    }
    rank.into_iter().map(rank32).collect()
}

/// Adds one stub per neighbour of the cluster whose nodes are `members`.
fn add_stubs(b: &mut Build<'_>, clustering: &Clustering, members: &[NodeId]) {
    let graph = &b.schema.graph;
    let mut done: Vec<Neighbour> = Vec::new();
    for &n in members {
        for &e in graph.outbound(n) {
            let edge = &graph.edges[e as usize];
            if !drawn(b.schema, edge) {
                continue;
            }
            if let Some(key) = neighbour(b, clustering, edge.to).filter(|k| !done.contains(k)) {
                done.push(key);
                b.chips.push(Chip::Stub {
                    from: n,
                    row: edge.row,
                    to: edge.to,
                });
            }
        }
    }
    for &n in members {
        for &e in graph.inbound(n) {
            let edge = &graph.edges[e as usize];
            if !drawn(b.schema, edge) {
                continue;
            }
            if let Some(key) = neighbour(b, clustering, edge.from).filter(|k| !done.contains(k)) {
                done.push(key);
                b.chips.push(Chip::Stub {
                    from: n,
                    row: None,
                    to: edge.from,
                });
            }
        }
    }
}

/// Returns the neighbour `node` belongs to, when it is outside the view and
/// can be a stub.
fn neighbour(b: &Build<'_>, clustering: &Clustering, node: NodeId) -> Option<Neighbour> {
    let schema = b.schema;
    if b.contains(node) || is_scalar(schema, node) {
        return None;
    }
    if is_root(schema, node) {
        return (!schema.transforms.hide_root).then_some(Neighbour::Root(node));
    }
    Some(match clustering.of(type_of(schema, node)) {
        Some(c) => Neighbour::Cluster(c),
        None => Neighbour::Rest,
    })
}

/// Returns the cluster map of `clustering`.
pub(super) fn map(schema: &Schema, clustering: &Clustering) -> View {
    let graph = &schema.graph;
    let mut group = vec![NONE; graph.nodes.len()];
    let mut lists: Vec<(String, Option<String>, Vec<NodeId>)> = Vec::new();
    for c in &clustering.clusters {
        lists.push((
            c.name.clone(),
            Some(c.slug.clone()),
            c.types.iter().filter_map(|&ty| graph.node_of(ty)).collect(),
        ));
    }
    if !clustering.other.is_empty() {
        lists.push((
            String::from(OTHER_NAME),
            None,
            clustering
                .other
                .iter()
                .filter_map(|&ty| graph.node_of(ty))
                .collect(),
        ));
    }
    let mut view = View::blank(ViewKind::ClusterMap, None);
    for (g, (name, slug, nodes)) in lists.into_iter().enumerate() {
        let mut shown: Vec<NodeId> = nodes
            .iter()
            .copied()
            .filter(|&n| !is_scalar(schema, n))
            .collect();
        for &n in &shown {
            group[n.index()] = id32(g);
        }
        shown.sort_by(|&x, &y| {
            graph
                .inbound(y)
                .len()
                .cmp(&graph.inbound(x).len())
                .then_with(|| name_order(&schema.model, type_of(schema, x), type_of(schema, y)))
        });
        shown.truncate(MAP_TYPES);
        view.groups.push(MapGroup {
            name,
            slug,
            types: id32(nodes.len()),
            first: id32(view.nodes.len()),
            shown: id32(shown.len()),
        });
        for n in shown {
            view.nodes.push((n, Level::L0));
            view.ranks.push(0);
        }
    }
    let mut pairs: Vec<(u32, u32)> = graph
        .edges
        .iter()
        .filter(|e| drawn(schema, e))
        .map(|e| (group[e.from.index()], group[e.to.index()]))
        .filter(|&(x, y)| x != NONE && y != NONE && x != y)
        .map(|(x, y)| (x.min(y), x.max(y)))
        .collect();
    pairs.sort_unstable();
    for run in pairs.chunk_by(|x, y| x == y) {
        view.trunks.push(MapTrunk {
            a: run[0].0,
            b: run[0].1,
            count: id32(run.len()),
        });
    }
    view
}
