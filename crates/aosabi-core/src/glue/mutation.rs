// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core glue mutation: the run time of an entry under mutation(...)
 * and the payload a method of the native layer answers.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/mutation.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! `mutation_wrapper` and [`MutationPayload`].
//!
//! The wrapper is one async function: it calls the entry inside a `try`, awaits
//! what the entry answered, and answers `{...result, userErrors: []}`, or
//! `{userErrors}` for a throw that `toUserErrors` maps, and rethrows any other
//! throw.
//!
//! An entry of `graphql!` stands under `mutation(...)` on the derive path. A
//! plain crate of the native layer holds no macro, so a method answers
//! [`MutationPayload`] in its return position, and the glue the builder writes
//! for the crate runs the method under the same wrapper.

use aosabi_execute::{
    promise::{
        Agent,
        Promise, //
    },
    value::{
        Completion,
        Value, //
    }, //
};

use crate::{
    glue::{
        IntoValue,
        Thrown, //
    },
    mutation::{
        failure_payload,
        success_payload,
        to_user_errors, //
    },
};

/// The throw of a mutation polled outside an agent, which no executor does.
const OUTSIDE_AGENT: &str = "a mutation ran outside the agent of its request";

/// The answer of a method of the native layer whose field is the payload of
/// `mutation(...)`, as in
/// `rename: mutation(async (name: string) => ({renamed: name}))`.
///
/// A method of a plain crate answers `MutationPayload<T>`, or a `Result` of
/// one, from a plain `fn` or an `async fn`, and the builder reads the type
/// from the signature, so the application writes no attribute:
///
/// ```ignore
/// use aosabi::prelude::*;
///
/// pub struct Rename {
///     pub renamed: String,
/// }
///
/// pub struct Mutation;
///
/// impl Mutation {
///     pub async fn rename(&self, name: String) -> MutationPayload<Rename> {
///         MutationPayload(Rename { renamed: name })
///     }
/// }
/// ```
///
/// The field answers the payload type of the reference's return type, the
/// public fields of the struct `T` in their order, each nullable, then
/// `userErrors: [UserError!]!`, the intersection of two objects with no name of
/// their own that the inference names after its field, `Rename` for `rename`.
/// Every field that answers the payload of one struct answers that one type. A
/// later payload of the same field name over another struct skips the names
/// that the members of the first payload of the schema took, for example
/// `Rename_2` when the payload of `rename` over a struct with a field is the
/// first payload of the schema. The glue runs the method inside the wrapper of
/// this module, so a value answers its fields beside `userErrors: []`, an `Err`
/// of a `ServiceError` answers `{userErrors}` with the `field` of its details,
/// its message and its code, and every other error is thrown again.
///
/// The name is neither `Mutation`, the root an application declares and
/// registers with `.mutation(Mutation)`, nor `Payload`, the payload of a pub
/// sub at the root of the facade, so a glob of the prelude names it beside
/// both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MutationPayload<T>(pub T);

impl<T> From<T> for MutationPayload<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

/// The value the wrapper spreads into the payload, the value of the struct.
impl<T: IntoValue> IntoValue for MutationPayload<T> {
    fn into_value(self) -> Value {
        self.0.into_value()
    }
}

/// Calls the entry `call` inside the mutation wrapper and answers the promise
/// of the wrapper.
#[must_use]
pub fn mutation_wrapper(
    agent: &Agent,
    call: impl FnOnce(&Agent) -> Completion + 'static,
) -> Completion {
    let body = async move {
        let Some(agent) = Agent::current() else {
            return Completion::Throw(Thrown::error(OUTSIDE_AGENT).0);
        };
        // `const result = await fn(...args)`, a throw of the call caught as a
        // rejection of the await.
        let awaited = match call(&agent) {
            Completion::Normal(value) => Promise::resolve(&agent, value).await,
            Completion::Throw(thrown) => Err(thrown),
        };
        match awaited {
            Ok(result) => Completion::Normal(success_payload(&result)),
            Err(err) => {
                let thrown = Thrown(err);
                match to_user_errors(&thrown) {
                    Some(user_errors) => Completion::Normal(failure_payload(user_errors)),
                    None => Completion::Throw(thrown.0),
                }
            }
        }
    };
    Completion::Normal(Value::Promise(agent.spawn(Box::pin(body))))
}
