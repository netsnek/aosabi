// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-landing: the plugin that answers an unhandled route with the landing
 * page.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/plugins/use-unhandled-route.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The template landing.html.tmpl beside it holds the page.
 */

//! `use_unhandled_route` and the landing page.
//!
//! The plugin has no name and one `setup`, which registers the not found
//! handler of the app. The handler answers the landing page with status 404
//! through `c.html`, so the answer carries
//! `Content-Type: text/html; charset=UTF-8`. The page is the template byte for
//! byte, its malformed closing tag `</add` included, with its one
//! interpolation, the version the page names. The template names Aosabi, links
//! the documentation of Aosabi and has no link of a favicon, so the page loads
//! nothing from a host of a third party, the named divergence D60 of
//! `docs/byte-equality.md` section 3. The version is the hand maintained
//! constant [`LANDING_VERSION`].

use std::rc::Rc;

use aosabi_core::{
    App,
    IntoPlugin,
    Plugin,
    PluginObject,
    plugin::object::__wrap, //
};
use aosabi_http::{
    Arg,
    Context,
    Html,
    raw, //
};

/// The version the page names, `2.9.5` (quirk `Q04`).
pub const LANDING_VERSION: &str = "2.9.5";

/// The template of the page, the tagged template of
/// `use-unhandled-route.ts:11-175` with its one interpolation written
/// `${version}`.
pub const LANDING_HTML_TEMPLATE: &str = include_str!("landing.html.tmpl");

/// The interpolation of the template.
const VERSION_SLOT: &str = "${version}";

/// The status of the page, `c.html(..., 404)`.
const NOT_FOUND: u16 = 404;

/// Returns the landing page, the template with the version in its slot.
///
/// The version holds no character the `html` tag escapes, so the page is the
/// template with the slot replaced.
#[must_use]
pub fn render_landing() -> String {
    LANDING_HTML_TEMPLATE.replacen(VERSION_SLOT, LANDING_VERSION, 1)
}

/// The plugin of the landing page, `useUnhandledRoute()`.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnhandledRoutePlugin;

impl IntoPlugin for UnhandledRoutePlugin {
    fn into_plugin(self) -> Rc<dyn Plugin> {
        Rc::new(PluginObject {
            setup: __wrap::setup(|app: &mut App| {
                app.not_found(|c: Context| async move {
                    let page = Html::Escaped(raw(render_landing()));
                    c.html_with(page, Arg::Status(NOT_FOUND))
                });
            }),
            ..PluginObject::default()
        })
    }
}

/// Returns the plugin of the landing page, `useUnhandledRoute()`.
///
/// The object of the reference carries the key `setup` alone, and the literal
/// writes the one field, so its members are `setup`, which the boot report
/// lists for the landing plugin.
#[must_use]
pub fn use_unhandled_route() -> Rc<dyn Plugin> {
    UnhandledRoutePlugin.into_plugin()
}
