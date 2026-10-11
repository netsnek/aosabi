// SPDX-License-Identifier: MIT
/*
 * aosabi-landing: the landing page plugin of Aosabi.
 *
 * This file holds the module list. The template landing.html.tmpl beside it
 * holds the page.
 */

//! The landing page plugin of Aosabi.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `use_unhandled_route` | `plugins/use-unhandled-route.ts` | the plugin, the landing page and the version it names |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebook is `tests/notebooks/30_http_tier1.ipynb`.

pub mod use_unhandled_route;

pub use use_unhandled_route::{
    LANDING_HTML_TEMPLATE,
    LANDING_VERSION,
    UnhandledRoutePlugin,
    render_landing,
    use_unhandled_route, //
};
