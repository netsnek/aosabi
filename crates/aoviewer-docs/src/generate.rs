// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs generate: the rendering of a whole reference into memory.
 *
 * It loads the input, builds the schema, picks the clustering and renders the
 * tree with the change log, and it touches no file of the output directory.
 */

//! The generation of a tree.
//!
//! The function [`generate`] answers a [`Request`] with the whole tree in
//! memory. It loads the input through [`load`], builds the [`Schema`] under the
//! transforms of the views the options name, picks the clustering with the
//! declared map of the options, and renders every file through [`render_tree`]
//! of the core, the text change log of `--since` included. It reads the files
//! of its inputs and no other file, so the check mode compares a tree that no
//! file under `out` shaped, and [󰀇](crate::write::write) or
//! [󰀈](crate::check::check) takes the tree from here.
//!
//! Every input is read under [`ReadLimits::DEFAULT`], the limits of the
//! command and of the seam.

use aoviewer_core::{
    cluster::Clustering,
    limits::ReadLimits,
    render_svg::SvgText,
    schema::Schema,
    tree::{
        Tree,
        render_tree, //
    },
};

use crate::{
    error::DocsError,
    input::load,
    request::Request,
    since, //
};

/// Renders the whole tree of `request` into memory.
///
/// The model of the input is built into a [`Schema`] under
/// `request.options.view.transforms`, the clustering is picked with
/// `request.options.declared`, and the differences from the schema of
/// `request.since`, when it names one, become the text change log of the
/// tree. The fields `pdf` and `check` of the request are the caller's, which
/// compiles the PDF from the tree and writes or checks it, and `out` is read
/// by neither this function nor the core.
///
/// # Errors
///
/// Returns [`DocsError::Device`] for a request that names a device card and
/// [`DocsError::SvgOutlines`] for one that asks for the outlines of the SVG
/// text, both before an input is read. Returns what [`load`] returns for the
/// input, [`DocsError::Since`] around what it returns for the schema of
/// `--since`, and [`DocsError::Tree`] when the core draws no tree of the
/// model.
pub fn generate(request: &Request) -> Result<Tree, DocsError> {
    if let Some(path) = &request.device {
        return Err(DocsError::Device { path: path.clone() });
    }
    if request.options.svg_text == SvgText::Outlines {
        return Err(DocsError::SvgOutlines);
    }
    let limits = ReadLimits::DEFAULT;
    let loaded = load(&request.input, &limits)?;
    let changes = match &request.since {
        Some(old) => Some(since::changes(old, &loaded.model, &limits)?),
        None => None,
    };
    let options = &request.options;
    let schema = Schema::build(loaded.model, options.view.transforms.clone());
    let clustering = Clustering::pick(&schema, &options.declared);
    render_tree(&schema, &clustering, options, changes.as_deref()).map_err(DocsError::Tree)
}
