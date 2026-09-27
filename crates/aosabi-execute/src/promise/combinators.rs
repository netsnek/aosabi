// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise combinators: Promise.all.
 *
 * Translation of ECMA-262 section 27.2.4.1, Promise.all, with
 * PerformPromiseAll of section 27.2.4.1.2 and the resolve element functions
 * of section 27.2.4.1.3, read at https://tc39.es/ecma262/#sec-promise.all.
 */

//! The one combinator the reference executor uses.
//!
//! The executor waits for the items of a list, for the promised members of an
//! object and for the `isTypeOf` answers of an abstract type with
//! `Promise.all` and with no other combinator.

use core::cell::{
    Cell,
    RefCell, //
};
use std::rc::Rc;

use crate::{
    promise::{
        Agent,
        Promise,
        ResolvingFunctions,
        promise::{
            Handlers,
            perform_promise_then, //
        }, //
    },
    value::Value, //
};

/// The record the resolve element functions of one `Promise.all` call share.
struct Elements {
    /// `values`, one slot per element, `Value::Undefined` until it fulfils.
    values: RefCell<Vec<Value>>,
    /// `remainingElementsCount`, which starts at 1 so that the aggregate
    /// cannot fulfil before every element is registered.
    remaining: Cell<usize>,
    /// `resultCapability`, the resolving functions of the aggregate promise.
    capability: ResolvingFunctions,
}

/// The resolve element function and the reject function of one element of `Promise.all`.
///
/// The two are the handlers of the one reaction `PerformPromiseAll`
/// registers on the element, and they share the record of the call, so the
/// reaction holds its state in place.
pub(crate) struct Element {
    /// The record of the call.
    elements: Rc<Elements>,
    /// The index of the element.
    index: usize,
}

impl Element {
    /// Stores the value of the element and fulfils the aggregate once no element remains.
    ///
    /// Translation of the `Promise.all` Resolve Element Functions of ECMA-262
    /// section 27.2.4.1.3. The `alreadyCalled` flag of the clause has no
    /// reading, because a reaction runs once by its type.
    pub(crate) fn resolve(self, agent: &Agent, value: Value) {
        if let Some(slot) = self.elements.values.borrow_mut().get_mut(self.index) {
            *slot = value;
        }
        settle_when_complete(agent, &self.elements);
    }

    /// Rejects the aggregate with `reason`, `resultCapability.[[Reject]]` of the clause.
    pub(crate) fn reject(self, agent: &Agent, reason: Value) {
        self.elements.capability.reject(agent, reason);
    }
}

impl Promise {
    /// Returns a promise of the list of the values `values` fulfil with.
    ///
    /// Translation of `Promise.all` and `PerformPromiseAll` of ECMA-262
    /// sections 27.2.4.1 and 27.2.4.1.2. Every element goes through
    /// [`Promise::resolve`] and then through one `then`, in index order, so
    /// an element that is no promise still costs one job. The aggregate
    /// fulfils with a `Value::List` inside the job of the last element that
    /// fulfils, it rejects inside the job of the first element that rejects,
    /// and an empty list fulfils it at once.
    ///
    /// The promise each `then` derives is never read, since the call drops
    /// it and its reactions are none, and its settlement enqueues no job. So
    /// the reaction of an element carries no capability and derives no
    /// promise, and the jobs and their order are those of the clause.
    #[must_use]
    pub fn all(agent: &Agent, values: Vec<Value>) -> Self {
        Self::all_of(agent, values.into_iter())
    }

    /// Returns a promise of the list of the values `values` yields fulfil with.
    ///
    /// This is [`Promise::all`] over an iterator, so a caller that holds the
    /// elements in place builds no list of them.
    #[must_use]
    pub(crate) fn all_of(agent: &Agent, values: impl ExactSizeIterator<Item = Value>) -> Self {
        let (promise, result_capability) = Self::with_resolvers(agent);
        let elements = Rc::new(Elements {
            values: RefCell::new(Vec::with_capacity(values.len())),
            remaining: Cell::new(1),
            capability: result_capability,
        });

        for (index, next_value) in values.enumerate() {
            elements.values.borrow_mut().push(Value::Undefined);
            let next_promise = Self::resolve(agent, next_value);
            elements.remaining.set(elements.remaining.get() + 1);
            let element = Element {
                elements: Rc::clone(&elements),
                index,
            };
            perform_promise_then(agent, &next_promise, Handlers::Element(element), None);
        }

        settle_when_complete(agent, &elements);
        promise
    }
}

/// Counts one element down and fulfils the aggregate when none remains.
///
/// This is the decrement both the loop end of `PerformPromiseAll` and every
/// resolve element function perform.
fn settle_when_complete(agent: &Agent, elements: &Elements) {
    let remaining = elements.remaining.get().saturating_sub(1);
    elements.remaining.set(remaining);
    if remaining == 0 {
        let values_array = elements.values.take();
        elements
            .capability
            .resolve(agent, Value::List(Rc::new(values_array)));
    }
}
