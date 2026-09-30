// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the plugin that reads `@inContext` off the executing operation.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/plugins/use-in-context.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `use_in_context`.
//!
//! The plugin is named `in-context`. Its hook `onExecute` runs after the parse
//! and the validation, finds the operation the request runs, reads the
//! arguments `locale` and `context` of its `@inContext` from the variables of
//! the request or from string literals, and binds the result to the context of
//! the request only when something resolved. A resolver reads it through
//! `get_locale()` and `get_in_context()`. The reading is the one of
//! `in_context::in_context_of`, which the service below HTTP runs where this
//! hook would run.

use std::rc::Rc;

use crate::{
    context::get_context,
    in_context::{
        IN_CONTEXT,
        in_context_of, //
    },
    plugin::{
        Plugin,
        PluginObject,
        object::__wrap, //
    },
};

/// Returns the plugin that reads `@inContext`, `useInContext()`.
///
/// Its members are `name` and `onExecute`, the two keys of the literal at
/// `use-in-context.ts:17-19`.
#[must_use]
pub fn use_in_context() -> Rc<dyn Plugin> {
    Rc::new(PluginObject {
        name: __wrap::name("in-context"),
        on_execute: __wrap::on_execute(|p| {
            let in_context = in_context_of(
                &p.args.document,
                p.args.operation_name.as_deref(),
                p.args.variables(),
            );
            if let Some(in_context) = in_context {
                // No request context, an in process execution outside a
                // request, has nothing to bind to.
                if let Ok(context) = get_context() {
                    context.set(IN_CONTEXT, in_context);
                }
            }
            Ok(None)
        }),
        ..PluginObject::default()
    })
}
