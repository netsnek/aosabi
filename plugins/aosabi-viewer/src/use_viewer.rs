// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-viewer: the plugin that holds the first slot and the route /viewer.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/plugins/use-viewer.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The slot and the route and never the page, which aoviewer owns
 * (docs/decisions/ADR-0009-aoviewer-owns-viewer.md).
 */

//! `use_viewer`.
//!
//! The plugin has no name and one `setup`, which registers `GET /viewer`
//! answering a page through `c.html`. The boot places it in the first slot of
//! the plugin list. The page is the one of
//! [`crate::page`] unless [`configure`] set another.

use core::cell::RefCell;
use std::rc::Rc;

use aosabi_core::{
    App,
    IntoPlugin,
    Plugin,
    PluginObject,
    plugin::object::__wrap, //
};
use aosabi_http::Context;

use crate::page::VIEWER_PAGE;

/// The route of the viewer, `app.get('/viewer', ...)`.
pub const VIEWER_ROUTE: &str = "/viewer";

/// The options of the viewer.
#[derive(Clone, Debug)]
pub struct ViewerOptions {
    /// The page `/viewer` answers.
    pub page: Rc<str>,
}

impl Default for ViewerOptions {
    fn default() -> Self {
        Self {
            page: Rc::from(VIEWER_PAGE),
        }
    }
}

thread_local! {
    /// The options [`configure`] set on this thread.
    static OPTIONS: RefCell<Option<ViewerOptions>> = const { RefCell::new(None) };
}

/// Sets the options of every viewer plugin made after it on this thread.
///
/// aoviewer calls it to put its page in place of the placeholder.
pub fn configure(options: ViewerOptions) {
    OPTIONS.with(|slot| *slot.borrow_mut() = Some(options));
}

/// Returns the options in force, the configured ones or the defaults.
fn options() -> ViewerOptions {
    OPTIONS
        .with(|slot| slot.borrow().clone())
        .unwrap_or_default()
}

/// The viewer plugin, `useViewer()`.
#[derive(Clone, Debug, Default)]
pub struct ViewerPlugin {
    /// The options of the plugin.
    pub options: ViewerOptions,
}

impl IntoPlugin for ViewerPlugin {
    fn into_plugin(self) -> Rc<dyn Plugin> {
        let page = self.options.page;
        Rc::new(PluginObject {
            setup: __wrap::setup(move |app: &mut App| {
                let page = Rc::clone(&page);
                app.get(VIEWER_ROUTE, move |c: Context| {
                    let page = Rc::clone(&page);
                    async move { c.html(&*page) }
                });
            }),
            ..PluginObject::default()
        })
    }
}

/// Returns the viewer plugin, `useViewer()`.
///
/// The slot and the route. The object of the reference carries the key
/// `setup` alone, and the literal writes the one field, so its members are
/// `setup`, which the slot keeps equal to the reference's while the page it
/// answers differs.
#[must_use]
pub fn use_viewer() -> Rc<dyn Plugin> {
    ViewerPlugin { options: options() }.into_plugin()
}
