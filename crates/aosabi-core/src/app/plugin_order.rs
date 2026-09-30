// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the order in which the plugins of a phase load.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/app/plugin-order.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `topo_sort_plugins`.
//!
//! The sort is a stable depth first sort by `dependsOn`, keyed on the identity
//! of a plugin: a plugin with no constraint keeps its place, a name two
//! plugins share resolves to the later one, a dependency on oneself is
//! dropped, a dependency on a name the phase lacks is ignored, the
//! dependencies are visited in the order of the array, and a cycle throws with
//! the names joined by U+2192. The identity of a plugin is the address of its
//! `Rc`. A plugin named with the empty string counts as unnamed where the
//! reference tests the name for truth, in the table of the names and on the
//! path of the walk, and keeps the empty string at the end of a cycle, which
//! the reference reads with `??`.
//!
//! The terminal `(anonymous)` of the cycle message stays as the reference
//! writes it and no input reaches it: the plugin the walk meets a second time
//! is always a dependency found by its name, and the table of the names holds
//! the plugins whose name is true alone.

use core::fmt;
use std::rc::Rc;

use aosabi_json::sort::sort_by_key_stable;

use crate::plugin::{
    Plugin,
    PluginSlot, //
};

/// The plugins of a phase in the order they load.
pub struct SortedOrder(pub Vec<Rc<dyn Plugin>>);

impl fmt::Debug for SortedOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|plugin| plugin.name()))
            .finish()
    }
}

/// The error of a dependency cycle.
///
/// The message is `Aosabi plugin dependency cycle: a → b → a`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginOrderError {
    /// The message.
    pub message: String,
}

impl PluginOrderError {
    /// Returns the message, `Aosabi plugin dependency cycle: ` and the path of the cycle.
    ///
    /// The path is the names on the walk and the name of the plugin met twice,
    /// joined by a space, U+2192 and a space, so the cycle of `a` and `b`
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for PluginOrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for PluginOrderError {}

/// The visiting state of a plugin in the sort.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// On the path of the walk.
    Visiting,
    /// Placed.
    Done,
}

/// The walk of one sort.
struct Walk<'a> {
    /// The plugins of the phase in their order.
    plugins: &'a [PluginSlot],
    /// The state of each plugin by identity.
    states: Vec<(*const (), State)>,
    /// The sorted plugins.
    out: Vec<Rc<dyn Plugin>>,
}

impl Walk<'_> {
    /// Returns the plugin the name `name` resolves to, the last of the name.
    ///
    /// The empty name resolves to none, because the table of the reference
    /// holds only the names that are true.
    fn by_name(&self, name: &str) -> Option<&PluginSlot> {
        if name.is_empty() {
            return None;
        }
        self.plugins
            .iter()
            .rev()
            .find(|slot| slot.plugin.name() == Some(name))
    }

    /// Returns the index of the plugin of identity `identity`, the last one.
    fn order(&self, identity: *const ()) -> usize {
        self.plugins
            .iter()
            .rposition(|slot| slot.identity() == identity)
            .unwrap_or(usize::MAX)
    }

    /// Returns the state of the plugin of identity `identity`.
    fn state(&self, identity: *const ()) -> Option<State> {
        self.states
            .iter()
            .find(|(known, _)| *known == identity)
            .map(|(_, state)| *state)
    }

    /// Sets the state of the plugin of identity `identity`.
    fn set_state(&mut self, identity: *const (), state: State) {
        match self.states.iter_mut().find(|(known, _)| *known == identity) {
            Some(entry) => entry.1 = state,
            None => self.states.push((identity, state)),
        }
    }

    /// Visits one plugin, `visit(p, stack)`.
    fn visit(&mut self, slot: &PluginSlot, stack: &[String]) -> Result<(), PluginOrderError> {
        let identity = slot.identity();
        match self.state(identity) {
            Some(State::Done) => return Ok(()),
            Some(State::Visiting) => {
                let mut path: Vec<String> = stack.to_vec();
                path.push(String::from(slot.plugin.name().unwrap_or("(anonymous)")));
                return Err(PluginOrderError {
                    message: format!(
                        "Aosabi plugin dependency cycle: {}",
                        path.join(" \u{2192} ")
                    ),
                });
            }
            None => {}
        }
        self.set_state(identity, State::Visiting);

        // Visit present dependencies first, in their original order (stability).
        let mut deps: Vec<PluginSlot> = slot
            .plugin
            .depends_on()
            .iter()
            .filter_map(|name| self.by_name(name).cloned())
            .filter(|dep| dep.identity() != identity)
            .collect();
        sort_by_key_stable(&mut deps, |dep| self.order(dep.identity()));
        let mut inner_stack = stack.to_vec();
        if let Some(name) = slot.plugin.name().filter(|name| !name.is_empty()) {
            inner_stack.push(String::from(name));
        }
        for dep in &deps {
            self.visit(dep, &inner_stack)?;
        }

        self.set_state(identity, State::Done);
        self.out.push(Rc::clone(&slot.plugin));
        Ok(())
    }
}

/// Sorts the plugins of a phase by their dependencies, `topoSortPlugins(plugins)`.
///
/// # Errors
///
/// Returns the cycle of the dependencies as [`PluginOrderError`].
pub fn topo_sort_plugins(plugins: &[PluginSlot]) -> Result<SortedOrder, PluginOrderError> {
    let mut walk = Walk {
        plugins,
        states: Vec::new(),
        out: Vec::new(),
    };
    for slot in plugins {
        walk.visit(slot, &[])?;
    }
    Ok(SortedOrder(walk.out))
}
