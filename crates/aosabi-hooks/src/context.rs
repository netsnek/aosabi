// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: the context of one request, the object its plugins and the
 * engine extend.
 *
 * No analogue in the reference, where the context is a plain object every
 * hook reads and Object.assign extends.
 */

//! The context of a request.
//!
//! The reference extends one plain object per request with `Object.assign`,
//! and the executor hands the same object to every resolver as its context
//! value. [`HookContext`] is that object: a shared, insertion ordered map
//! from the name of a property to its value, whose clone is the same object,
//! so a later key replaces the value of an earlier one in place.

use core::{
    any::Any,
    cell::RefCell,
    fmt, //
};
use std::rc::Rc;

use aosabi_type::ObjMap;

/// The object the plugins and the engine extend per request, which is also the context value of the executor.
// An `unsafe` cell would skip the borrow flag.
#[derive(Clone, Default)]
pub struct HookContext(Rc<RefCell<ObjMap<Rc<dyn Any>>>>);

impl HookContext {
    /// Creates an empty context, `{}`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `key` to `value`, `Object.assign(context, { key: value })`.
    ///
    /// A key already present keeps its place and takes the new value.
    pub fn extend(&self, key: &str, value: Rc<dyn Any>) {
        let mut map = self.0.borrow_mut();
        match map.get_mut(key) {
            Some(slot) => *slot = value,
            None => {
                map.insert(String::from(key), value);
            }
        }
    }

    /// Returns the value of `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Rc<dyn Any>> {
        self.0.borrow().get(key).cloned()
    }

    /// Returns the value of `key` when it is a `T`.
    #[must_use]
    pub fn get_as<T: 'static>(&self, key: &str) -> Option<Rc<T>> {
        self.get(key)?.downcast::<T>().ok()
    }

    /// Returns the keys in insertion order.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        self.0.borrow().keys().cloned().collect()
    }

    /// Copies every entry of `other` into this context, `Object.assign(context, other)`.
    pub fn assign(&self, other: &HookContext) {
        if self.ptr_eq(other) {
            return;
        }
        for (key, value) in other.0.borrow().iter() {
            self.extend(key, Rc::clone(value));
        }
    }

    /// Returns a context whose entries start as a copy of this one's, `Object.create(context)` as Yoga reads it.
    ///
    /// Yoga gives each operation of a batch a context made from the server
    /// context, so a key one operation sets is not seen by another.
    #[must_use]
    pub fn derive(&self) -> Self {
        Self(Rc::new(RefCell::new(self.0.borrow().clone())))
    }

    /// Returns whether `self` and `other` are the same object.
    #[must_use]
    pub fn ptr_eq(&self, other: &HookContext) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for HookContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.keys()).finish()
    }
}
