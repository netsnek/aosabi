// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise_reduce: the serial reduction over values that may be promises.
 *
 * Derived from @graphql-tools/utils 11.0.0, cjs/jsutils.js, MIT.
 * Derived from graphql-js 16.12.0, src/jsutils/promiseReduce.ts, MIT.
 */

//! The reduction a mutation runs its root fields with.

use std::rc::Rc;

use crate::{
    handle_maybe_promise::{
        MaybePromise,
        handle_maybe_promise, //
    },
    promise::Agent,
    value::{
        Completion,
        Value, //
    }, //
};

/// The callback of [`promise_reduce`], called with the resolved accumulator and the next value.
pub type ReduceCallback<T> = Rc<dyn Fn(&Agent, Value, T) -> Completion>;

/// Reduces `values` with `callback_fn`, waiting for the accumulator whenever it is a promise.
///
/// Each step is one [`handle_maybe_promise`] whose input is the accumulator,
/// so a step whose accumulator is a promise runs in a reaction of that
/// promise, and a step whose accumulator is a plain value runs at once. That
/// is what makes the root fields of a mutation run one after another.
///
/// # Errors
///
/// Returns what `callback_fn` throws on a step that runs at once. A throw of
/// a later step rejects the promise the function returns.
pub fn promise_reduce<T: 'static>(
    agent: &Agent,
    values: impl IntoIterator<Item = T>,
    callback_fn: &ReduceCallback<T>,
    initial_value: Value,
) -> Result<MaybePromise, Value> {
    let mut accumulator = initial_value;
    for value in values {
        let callback_fn = Rc::clone(callback_fn);
        accumulator = handle_maybe_promise(
            agent,
            |_| Completion::Normal(accumulator),
            Box::new(move |agent, resolved| callback_fn(agent, resolved, value)),
            None,
        )
        .into_result()?;
    }
    Ok(accumulator)
}
