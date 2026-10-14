// SPDX-License-Identifier: MIT
/*
 * aoviewer-core page cluster_page: the page of one cluster and the page of the
 * cluster map.
 *
 * A cluster route has its page and its cluster graph.
 */

//! The cluster page and the cluster map page.
//!
//! A cluster page names the source of the clustering, holds the cluster
//! diagram and lists the types of the cluster with their kind and the first
//! paragraph of their description, in the order of the clustering, which is
//! the byte order of the names. The page of the cluster map names the source,
//! holds the map and the table of the clusters, the one the home page carries
//! as well.

use alloc::{
    string::{
        String,
        ToString, //
    },
    vec,
    vec::Vec, //
};

use super::{
    Block,
    Cell,
    PageModel,
    kind_word,
    link,
    overview_crumb,
    push_diagrams,
    source_note,
    summary_cell,
    title,
    type_link, //
};
use crate::{
    cluster::Clustering,
    id::ElementId,
    route::{
        Options,
        Route, //
    },
    schema::Schema,
    view::ViewError,
};

/// The name the other bucket is shown under.
pub(super) const OTHER: &str = "other";

/// Returns the page of the cluster whose slug is `slug`, for `route`.
///
/// # Errors
///
/// Returns [`ViewError::UnknownCluster`] when `clustering` holds no cluster
/// of that slug, and the errors of
/// [`View::for_page`](crate::view::View::for_page) for the route.
pub(super) fn cluster_page(
    schema: &Schema,
    clustering: &Clustering,
    route: &Route,
    slug: &str,
    options: &Options,
) -> Result<PageModel, ViewError> {
    let model = &schema.model;
    let Some(cluster) = clustering.clusters.iter().find(|c| c.slug == slug) else {
        return Err(ViewError::UnknownCluster(String::from(slug)));
    };
    let mut blocks = vec![source_note(clustering)];
    push_diagrams(schema, clustering, route, options, &mut blocks)?;
    let rows = cluster
        .types
        .iter()
        .map(|&t| {
            let ty = model.ty(t);
            vec![
                type_link(model, t),
                Cell::Text(String::from(kind_word(ty.kind))),
                summary_cell(ty.description.as_deref()),
            ]
        })
        .collect();
    blocks.push(Block::Table {
        title: title::TYPES,
        columns: vec!["Type", "Kind", "Description"],
        rows,
    });
    blocks.push(Block::ViewerLink {
        route: route.clone(),
    });
    let mut breadcrumb = overview_crumb();
    breadcrumb.push((String::from(title::CLUSTERS), Route::Clusters));
    Ok(PageModel {
        route: route.clone(),
        title: cluster.name.clone(),
        kind: None,
        breadcrumb,
        blocks,
    })
}

/// Returns the page of the cluster map.
///
/// # Errors
///
/// Returns the errors of [`View::for_page`](crate::view::View::for_page) for
/// the route of the map.
pub(super) fn map_page(
    schema: &Schema,
    clustering: &Clustering,
    options: &Options,
) -> Result<PageModel, ViewError> {
    let route = Route::Clusters;
    let mut blocks = vec![source_note(clustering)];
    push_diagrams(schema, clustering, &route, options, &mut blocks)?;
    blocks.push(clusters_table(clustering));
    blocks.push(Block::ViewerLink {
        route: route.clone(),
    });
    Ok(PageModel {
        route,
        title: String::from(title::CLUSTERS),
        kind: None,
        breadcrumb: overview_crumb(),
        blocks,
    })
}

/// Returns the table of the clusters with their type counts, in the order of
/// the clustering, and the other bucket last when it holds a type.
pub(super) fn clusters_table(clustering: &Clustering) -> Block {
    let mut rows: Vec<Vec<Cell>> = clustering
        .clusters
        .iter()
        .map(|c| {
            vec![
                link(c.name.clone(), ElementId::Cluster(c.slug.clone())),
                Cell::Text(c.types.len().to_string()),
            ]
        })
        .collect();
    if !clustering.other.is_empty() {
        rows.push(vec![
            Cell::Text(String::from(OTHER)),
            Cell::Text(clustering.other.len().to_string()),
        ]);
    }
    Block::Table {
        title: title::CLUSTERS,
        columns: vec!["Cluster", "Types"],
        rows,
    }
}
