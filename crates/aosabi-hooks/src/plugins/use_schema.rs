// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the schema plugin, which sets the schema of every request.
 */

//! `use_schema`.
//!
//! The schema plugin sets its schema in `on_plugin_init`, so the orchestrator
//! hands it to every plugin in its first `on_schema_change`.

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_type::Schema;

use crate::plugin::{
    HookPlugin,
    OnPluginInit, //
};

/// The schema plugin.
struct UseSchema(Arc<Schema>);

impl HookPlugin for UseSchema {
    fn on_plugin_init(&self, p: &mut OnPluginInit<'_>) {
        p.set_schema(Arc::clone(&self.0));
    }
}

/// Returns the plugin that sets `schema`, `useSchema(schema)`.
#[must_use]
pub fn use_schema(schema: Arc<Schema>) -> Rc<dyn HookPlugin> {
    Rc::new(UseSchema(schema))
}
