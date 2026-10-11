// SPDX-License-Identifier: MIT
/*
 * aosabi-viewer: the viewer plugin in the first plugin slot.
 *
 * This file holds the module list.
 */

//! The viewer plugin.
//!
//! The plugin holds the first slot of the plugin list and the route
//! `/viewer`, and answers the route with a page of Aosabi's own, because
//! aoviewer owns the route.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `use_viewer` | `plugins/use-viewer.ts`, the slot and the route | the plugin |
//! | `page` | no analogue | the placeholder page |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebook is `tests/notebooks/30_http_tier1.ipynb`.

pub mod page;
pub mod use_viewer;

pub use page::VIEWER_PAGE;
pub use use_viewer::{
    VIEWER_ROUTE,
    ViewerOptions,
    ViewerPlugin,
    configure,
    use_viewer, //
};
