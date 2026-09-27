// SPDX-License-Identifier: MIT
/*
 * aosabi-execute handle_maybe_promise: the one helper that continues a value or a promise of it.
 *
 * Derived from whatwg-node 1.3.2, cjs/index.js, MIT.
 */

//! The continuation of a value that may be a promise.
//!
//! The reference builds `handleMaybePromise` out of two fake promises, one
//! that holds a value and one that holds a thrown error, and unwraps the fake
//! at the end. The fakes never queue a job. What is left of them is three
//! paths, and [`handle_maybe_promise`] is those three paths written out.
//!
//! | Input factory | The reference runs | Here |
//! | --- | --- | --- |
//! | returns a promise | `value.then(onSuccess, onError)`, `index.js:31-33` | one [`Promise::then`] with both handlers |
//! | returns any other value | `fakePromise(value).then(onSuccess)`, `index.js:45-55` | `on_success` at once |
//! | throws | `fakeRejectPromise(err).then(_, onError)`, `index.js:126-136` | `on_error` at once, or the throw |
//!
//! [`Promise::then`]: crate::promise::Promise::then

use crate::{
    promise::{
        Agent,
        Reaction, //
    },
    value::{
        Completion,
        Value, //
    }, //
};

/// A value or a promise of it, the `PromiseOrValue` of the reference.
///
/// It is [`Value`] itself, and the alias marks the positions the reference
/// types that way.
pub type MaybePromise = Value;

/// Returns whether `value` is a promise, which is `value?.then != null`.
///
/// A promise is the one thenable of the value model.
#[must_use]
pub fn is_promise(value: &Value) -> bool {
    value.is_thenable()
}

/// Continues what `input_factory` returns with `on_success`, and what it throws with `on_error`.
///
/// A promise gets one `then` with both handlers and the derived promise is
/// the result. Any other value goes to `on_success` at once. A throw of
/// `input_factory` goes to `on_error` at once, and is the result when there
/// is no `on_error`. A throw of `on_success` on the path without a promise is
/// the result and never reaches `on_error`, because the fake promise of the
/// reference takes one handler.
pub fn handle_maybe_promise(
    agent: &Agent,
    input_factory: impl FnOnce(&Agent) -> Completion,
    on_success: Reaction,
    on_error: Option<Reaction>,
) -> Completion {
    match input_factory(agent) {
        Completion::Normal(Value::Promise(promise)) => Completion::Normal(Value::Promise(
            promise.then(agent, Some(on_success), on_error),
        )),
        Completion::Normal(value) => on_success(agent, value),
        Completion::Throw(error) => match on_error {
            Some(on_error) => on_error(agent, error),
            None => Completion::Throw(error),
        },
    }
}
