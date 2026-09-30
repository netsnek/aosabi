// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core resolvers view: the projection of a resolver as a view of
 * the selection.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/define-pylon.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The projection as a view.
//!
//! The projection of the reference is an object literal with one property
//! per selected name whose member the object holds, each member wrapped,
//! built by one loop over the selection when the object is wrapped. [`View`]
//! is that object with no map of its own. Its keys are the names of the
//! selection it shares with every object of the same selection, and its
//! properties stand in one slot per selected field, in the order of the
//! selection, so the view copies no key and hashes no key, and a read finds
//! its slot by the name, first at the slot after the one read last, where
//! the executor reads the next field of a selection in the order it collects
//! them.
//!
//! A selection names a field twice when the walk injects `__typename` for an
//! abstract type behind a `__typename` the document selects under an alias
//! alone. The loop of the reference assigns the name twice, and the second
//! assignment replaces the value of the one property in place. The view
//! keeps that property in the slot of the first field and leaves the slot of
//! the second empty, so the name has one key at the place of its first
//! field and a read answers the member of the second.
//!
//! The slots are filled when the view is built, by the loop of the
//! reference, which reads every selected member and wraps it in the order of
//! the selection. A promise among them takes its reaction there, a list wraps
//! its items there and an object is projected there, so the jobs a projection
//! enqueues and the reads it makes of the object come in the order and at
//! the time the reference makes them.

use core::cell::{
    Cell,
    RefCell, //
};
use std::rc::Rc;

use aosabi_execute::value::{
    ObjectLike,
    Value, //
};
use aosabi_json::value::array_index;

use crate::resolvers::get_selected_fields::SelectedField;

/// The projection of an object onto the fields a document selects below it.
///
/// A property a write adds stands after the properties of the selection, as
/// an assignment appends a new property to an object literal, and a write to
/// the property of a selected field replaces its value in place.
pub(crate) struct View {
    /// The selected fields, whose names are the keys of the view.
    selection: Rc<Vec<SelectedField>>,
    /// The property of each selected field in the order of the selection,
    /// `None` where the projection holds no property under its name and
    /// where the field repeats the name of an earlier one.
    slots: Box<[Option<Value>]>,
    /// The slot after the one read last, where a read looks first.
    next: Cell<usize>,
    /// Whether a write stored a property in [`View::written`].
    has_writes: Cell<bool>,
    /// The properties writes stored, in the order of their first write.
    written: RefCell<Vec<(String, Value)>>,
}

impl View {
    /// Returns the view of `selection` over `slots`, one slot per selected field.
    pub(crate) fn new(selection: Rc<Vec<SelectedField>>, slots: Box<[Option<Value>]>) -> Self {
        Self {
            selection,
            slots,
            next: Cell::new(0),
            has_writes: Cell::new(false),
            written: RefCell::new(Vec::new()),
        }
    }

    /// Returns the slot of the selected field `key`, or `None` for a name the
    /// document does not select.
    ///
    /// The search starts at the slot after the one found last and wraps
    /// around once, so a read of the next field of the selection compares one
    /// name.
    fn position(&self, key: &str) -> Option<usize> {
        let fields = self.selection.as_slice();
        let start = self.next.get().min(fields.len());
        let (before, after) = fields.split_at(start);
        let found = after
            .iter()
            .position(|field| field.name == key)
            .map(|offset| start + offset)
            .or_else(|| before.iter().position(|field| field.name == key))?;
        self.next.set(found + 1);
        Some(found)
    }

    /// Returns the value a write stored under `key`.
    fn written_value(&self, key: &str) -> Option<Value> {
        if !self.has_writes.get() {
            return None;
        }
        let written = self.written.borrow();
        written
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    }

    /// Returns the value of the property `key` of the selection.
    ///
    /// A field that repeats the name of an earlier one holds no property of
    /// its own, so a read that finds its empty slot answers the slot of the
    /// earlier field, which holds the property of the name.
    // A `get_unchecked` on the slot the search found would skip the bounds check.
    fn slot_value(&self, key: &str) -> Option<Value> {
        let index = self.position(key)?;
        if let Some(value) = self.slots.get(index)? {
            return Some(value.clone());
        }
        let first = self.selection.get(index)?.repeat_of?;
        self.slots.get(first)?.clone()
    }
}

impl ObjectLike for View {
    fn get(self: Rc<Self>, key: &str) -> Value {
        self.written_value(key)
            .or_else(|| self.slot_value(key))
            .unwrap_or(Value::Undefined)
    }

    /// Returns the keys in the order of `OrdinaryOwnPropertyKeys`, as the
    /// object literal of the reference lists them.
    ///
    /// The selected names with a property come in the order of the selection,
    /// a name the selection repeats once, at the place of its first field,
    /// whose slot alone holds the property. The properties writes added come
    /// after them in the order of their writes, and a key that is an array
    /// index moves to the front in ascending numeric order. No selected name
    /// is an array index, since a GraphQL name starts with a letter or an
    /// underscore.
    fn own_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .selection
            .iter()
            .zip(self.slots.iter())
            .filter(|(_, slot)| slot.is_some())
            .map(|(field, _)| field.name.clone())
            .collect();
        if !self.has_writes.get() {
            return keys;
        }
        for (name, _) in self.written.borrow().iter() {
            if keys.iter().any(|key| key == name) {
                continue;
            }
            match array_index(name.as_bytes()) {
                Some(index) => {
                    let at = keys
                        .iter()
                        .take_while(|key| {
                            array_index(key.as_bytes()).is_some_and(|other| other < index)
                        })
                        .count();
                    keys.insert(at, name.clone());
                }
                None => keys.push(name.clone()),
            }
        }
        keys
    }

    /// Returns whether `key` is an own property, with no list of the keys
    /// built and the slot of the next read left where it is.
    fn has_own(&self, key: &str) -> bool {
        let written =
            self.has_writes.get() && self.written.borrow().iter().any(|(name, _)| name == key);
        written
            || self
                .selection
                .iter()
                .zip(self.slots.iter())
                .any(|(field, slot)| slot.is_some() && field.name == key)
    }

    fn set(&self, key: &str, value: Value) {
        let mut written = self.written.borrow_mut();
        match written.iter_mut().find(|(name, _)| name == key) {
            Some(entry) => entry.1 = value,
            None => written.push((String::from(key), value)),
        }
        self.has_writes.set(true);
    }
}
