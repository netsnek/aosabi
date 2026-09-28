// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the plugin that extends the context of every request.
 */

//! `use_extend_context`.
//!
//! The plugin runs its factory over the context in `on_context_building` and
//! extends the context with what the factory answers. A factory answers a
//! future, so an asynchronous one fits beside a synchronous one.

use core::any::Any;
use std::rc::Rc;

use aosabi_execute::Value;

use crate::{
    context::HookContext,
    plugin::{
        ContextBuildingHook,
        HookPlugin,
        LocalBoxFuture,
        OnContextBuilding, //
    }, //
};

/// The factory of the extension, which answers the entries to set on the context.
pub type ContextFactory =
    Rc<dyn Fn(&HookContext) -> LocalBoxFuture<'static, Result<Vec<(String, Rc<dyn Any>)>, Value>>>;

/// The plugin that extends the context.
struct UseExtendContext(ContextFactory);

impl HookPlugin for UseExtendContext {
    fn on_context_building(&self, p: &mut OnContextBuilding<'_>) -> Option<ContextBuildingHook> {
        let factory = (self.0)(p.context);
        let context = p.context.clone();
        let wait: LocalBoxFuture<'static, Result<(), Value>> = Box::pin(async move {
            for (key, value) in factory.await? {
                context.extend(&key, value);
            }
            Ok(())
        });
        Some(ContextBuildingHook {
            wait: Some(wait),
            done: None,
        })
    }
}

/// Returns the plugin that extends the context with what `factory` answers, `useExtendContext(factory)`.
#[must_use]
pub fn use_extend_context(factory: ContextFactory) -> Rc<dyn HookPlugin> {
    Rc::new(UseExtendContext(factory))
}
