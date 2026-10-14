// SPDX-License-Identifier: MIT
/*
 * aoviewer-core schema: the bundle of a model, its transforms, its graph and
 * its anchors.
 *
 * The bundle is built once per model and transform set, and every view, page
 * and writer reads it.
 */

//! The schema bundle.

use crate::{
    graph::Graph,
    id::Anchors,
    model::Model,
    transform::{
        self,
        Transformed,
        Transforms, //
    },
};

/// The bundle built once per model and transform set, which every view, page
/// and writer reads.
///
/// The adjacency of the graph is built once here and never per view, which
/// keeps a pass over every page linear in the schema.
#[derive(Clone, Debug)]
pub struct Schema {
    /// The model, which the transforms never change.
    pub model: Model,
    /// The transforms the bundle was built under.
    pub transforms: Transforms,
    /// The tables of the transforms.
    pub transformed: Transformed,
    /// The type graph with its adjacency, its walks and its neighbour counts.
    pub graph: Graph,
    /// The anchors of the documents.
    pub anchors: Anchors,
}

impl Schema {
    /// Builds the bundle of `model` under `transforms`.
    ///
    /// The transforms run once, the graph is built from their tables, and
    /// the anchors from the model, which the transforms leave whole so the
    /// documents list every member the graph hides.
    #[must_use]
    pub fn build(model: Model, transforms: Transforms) -> Schema {
        let transformed = transform::apply(&model, &transforms);
        let graph = Graph::build(&model, &transforms, &transformed);
        let anchors = Anchors::build(&model);
        Schema {
            model,
            transforms,
            transformed,
            graph,
            anchors,
        }
    }
}
