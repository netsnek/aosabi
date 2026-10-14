// SPDX-License-Identifier: MIT
/*
 * aoviewer-core: the shared core of aoviewer and of the documentation
 * generator.
 *
 * No file of the reference is translated here. aoviewer replaces the Voyager
 * page of the /viewer route.
 * The core answers what a route looks like under a set of options, as a page
 * model and a graph scene, and the viewer and the documents call the same
 * function.
 */

//! The shared core of aoviewer and of the documentation generator.
//!
//! The core reads a schema into one model, transforms it, builds the type
//! graph with its walks and its clusters, and answers a route with a view, a
//! layout and a scene. The scene and the page model are written as SVG,
//! Markdown and Typst, and the tree of a reference is planned, indexed and
//! hashed here as well, so the native build and the wasm module write the same
//! bytes.
//!
//! The crate builds with no standard library and needs an allocator. It
//! carries no float, no hash container, no clock, no thread and no `unsafe`,
//! and every loop runs under a cap named as a constant.
//!
//! The judging notebooks are `tests/notebooks/51_viewer_core.ipynb`,
//! `tests/notebooks/75_docgen.ipynb` and `tests/notebooks/76_docs_pdf.ipynb`,
//! which drive `aosabi-devtool viewer`. There is no test in this crate, which
//! is the rule of CONTRIBUTING.md section 3.

#![no_std]

// The model, the views and every writer of the core allocate through this
// crate. The lint finds the line unused as long as no module of the crate
// holds code, and it is allowed rather than expected, so the first module that
// allocates needs no change here.
#[allow(unused_extern_crates)]
extern crate alloc;

pub mod cluster;
#[cfg(any(feature = "markdown", feature = "typst"))]
pub mod commonmark;
pub mod diff;
pub mod escape;
pub mod graph;
pub mod id;
pub mod json;
pub mod layout;
pub mod limits;
pub mod model;
pub mod page;
pub mod random;
pub mod read;
#[cfg(feature = "markdown")]
pub mod render_markdown;
pub mod render_svg;
#[cfg(feature = "typst")]
pub mod render_typst;
pub mod route;
pub mod scene;
pub mod schema;
pub mod sha256;
pub mod sort;
pub mod text;
pub mod transform;
pub mod tree;
pub mod view;

/// The version of the layout and of every writer.
///
/// It is written into `manifest.json` and bumped by a change that moves an
/// output byte on purpose, which is a deliberate golden refresh.
pub const LAYOUT_VERSION: u16 = 1;
