// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise_for_object: the result object of a selection set with promised fields.
 *
 * Derived from @graphql-tools/executor 1.5.1, cjs/execution/promiseForObject.js, MIT.
 * Derived from graphql-js 16.12.0, src/jsutils/promiseForObject.ts, MIT.
 */

//! The object whose promised members settle into it in settle order.
//!
//! The reference writes a member that is no promise into the resolved object at
//! once and a promised member inside the reaction of its own promise. The keys
//! of the resolved object are therefore the synchronous members in document
//! order first and the promised members in settle order after them, where
//! graphql-js rebuilds document order.
//!
//! The resolved object is an object of the arena of the execution, into
//! which [`execute_fields`](mod@crate::execute) appended every member that is no
//! promise before this function runs, in document order, so the function
//! takes the promised members alone and appends each in the reaction of its
//! promise.
//!
//! A promised member waits on the stack of the promised fields of the context,
//! and the slot it settles into is the object and the key of the arena, two
//! handles the reaction holds in place. So neither the members of an object nor
//! its reactions allocate, beside the promises the job model derives.

use std::rc::Rc;

use crate::{
    arena::{
        Key,
        NodeRef, //
    },
    execute::ExecutionContext,
    promise::{
        Agent,
        Promise,
        promise::Handlers, //
    },
    value::{
        Completion,
        Value, //
    }, //
};

/// A reaction of [`promise_for_object`], by its kind.
///
/// Each kind handles the fulfilment and passes a rejection through, as the
/// reactions of the reference have no rejection handler.
pub(crate) enum Pending {
    /// The reaction of a promised member, which appends its value to the
    /// object under its key, the slot of the arena it settles into.
    Member {
        /// The context whose arena holds the object.
        context: Rc<ExecutionContext>,
        /// The object.
        object: NodeRef,
        /// The key of the member.
        key: Key,
    },
    /// The reaction that answers the object once every member settled.
    Object(NodeRef),
}

impl Pending {
    /// Runs the reaction with the value the promise fulfilled with.
    pub(crate) fn run(self, resolved_value: Value) -> Completion {
        match self {
            Self::Member {
                context,
                object,
                key,
            } => {
                context.append(object, key, resolved_value);
                Completion::Normal(Value::Undefined)
            }
            Self::Object(object) => Completion::Normal(Value::from(object)),
        }
    }
}

/// Returns a promise of the object `object` becomes once every promised member has settled.
///
/// The abort signal and the `Promise.race` of line 29 have no reading, because
/// the server hands the executor no signal. The keys are walked in the order of
/// `for (const key in object)`, which for an object without a prototype is the
/// order of `OrdinaryOwnPropertyKeys`, and a response key is never an array
/// index, so the promised members of the object, the fields above `mark` on the
/// stack of the promised fields of the context, stand in document order. A
/// member that is no promise costs the reference no job, and its
/// `handleMaybePromise` appended it at once, which the caller did. One promised
/// member costs one `then`, whose reaction appends it, a single promise is
/// awaited itself and several through [`Promise::all`], and one `then` answers
/// the resolved object. A rejected member rejects the result with its reason.
/// The members leave the stack before the function returns.
pub(crate) fn promise_for_object(
    agent: &Agent,
    exe_context: &Rc<ExecutionContext>,
    object: NodeRef,
    mark: usize,
) -> Completion {
    let end = exe_context.pending.borrow().len();
    for at in mark..end {
        let member = exe_context.pending.borrow().get(at).cloned();
        let Some((key, promise)) = member else {
            break;
        };
        let slot = Pending::Member {
            context: Rc::clone(exe_context),
            object,
            key,
        };
        let value_set = promise.then_with(agent, Handlers::Object(slot));
        if let Some(member) = exe_context.pending.borrow_mut().get_mut(at) {
            member.1 = value_set;
        }
    }
    let promise_all = match end.saturating_sub(mark) {
        0 => None,
        1 => value_set_at(exe_context, mark),
        _ => Some(Promise::all_of(
            agent,
            (mark..end)
                .map(|at| value_set_at(exe_context, at).map_or(Value::Undefined, Value::Promise)),
        )),
    };
    exe_context.pending.borrow_mut().truncate(mark);
    let Some(promise_all) = promise_all else {
        return Completion::Normal(Value::from(object));
    };
    let resolved = promise_all.then_with(agent, Handlers::Object(Pending::Object(object)));
    Completion::Normal(Value::Promise(resolved))
}

/// Returns the promise the reaction of the promised member at `at` of the stack derived.
fn value_set_at(exe_context: &ExecutionContext, at: usize) -> Option<Promise> {
    exe_context
        .pending
        .borrow()
        .get(at)
        .map(|(_, value_set)| value_set.clone())
}
