// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue completion: the return of a Rust method as the completion
 * of a JavaScript call.
 *
 * No analogue in the reference. The completion is the completion record of
 * ECMA-262 section 6.2.4.
 */

//! The completion of a resolver method.
//!
//! A method returns a value, which is a normal completion, or a `Result`,
//! whose `Err` is a throw completion. An `async fn` returns a future of
//! either, which becomes a promise on the agent of the request. The
//! generated function of a method ends in [`complete`] or in
//! [`complete_future`], and a return type that is neither refuses with a
//! trait bound error.

use core::future::Future;

use aosabi_execute::{
    promise::Agent,
    value::{
        Completion,
        ErrorObject,
        Function,
        Value, //
    }, //
};
use aosabi_lang::GraphQLError;

use crate::{
    glue::{
        into_value::IntoValue,
        shared::Shared,
        thrown::Thrown, //
    }, //
    resolvers::ServiceError,
};

/// An error type of a resolver as the value a JavaScript `throw` carries.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be thrown by a resolver",
    note = "return `aosabi::Result<T>`, whose error is `aosabi::Error`, or `Result<T, aosabi::ServiceError>`"
)]
pub trait IntoThrown {
    /// Returns the thrown value.
    fn into_thrown(self) -> Value;
}

impl IntoThrown for Thrown {
    fn into_thrown(self) -> Value {
        self.0
    }
}

impl IntoThrown for ServiceError {
    fn into_thrown(self) -> Value {
        Thrown::from(self).0
    }
}

impl IntoThrown for GraphQLError {
    fn into_thrown(self) -> Value {
        Value::from(ErrorObject::GraphQL(self))
    }
}

impl IntoThrown for Value {
    fn into_thrown(self) -> Value {
        self
    }
}

/// The return of a resolver method as a completion.
#[diagnostic::on_unimplemented(
    message = "a resolver cannot return `{Self}`",
    note = "a resolver returns a type with a GraphQL description, or a `Result` of one whose error can be thrown"
)]
pub trait IntoCompletion {
    /// Returns the completion.
    fn into_completion(self) -> Completion;
}

impl<T: IntoValue> IntoCompletion for T {
    fn into_completion(self) -> Completion {
        Completion::Normal(self.into_value())
    }
}

impl<T: IntoValue, E: IntoThrown> IntoCompletion for Result<T, E> {
    fn into_completion(self) -> Completion {
        match self {
            Ok(value) => Completion::Normal(value.into_value()),
            Err(error) => Completion::Throw(error.into_thrown()),
        }
    }
}

/// The methods of a `#[aosabi::resolvers]` block as functions of the executor.
///
/// The derive implements the trait over the method table of the block of its
/// type, which is empty where the type has no block.
pub trait ResolverMethods: Sized + 'static {
    /// The names of the methods, in declaration order.
    const NAMES: &'static [&'static str];

    /// Returns the method `key` bound to `this`, the whole object or a part
    /// of another, `None` for a name the block does not declare.
    fn method(this: &Shared<Self>, key: &str) -> Option<Function>;
}

/// Returns the completion of a method that returned `result`.
pub fn complete<R: IntoCompletion>(result: R) -> Completion {
    result.into_completion()
}

/// Returns the completion of an `async fn`, a promise that settles the way
/// `future` completes.
///
/// The future is spawned on `agent`, which polls it once before this
/// function returns, because the body of an async function runs
/// synchronously up to its first await.
pub fn complete_future<R: IntoCompletion + 'static>(
    agent: &Agent,
    future: impl Future<Output = R> + 'static,
) -> Completion {
    let promise = agent.spawn(Box::pin(async move { future.await.into_completion() }));
    Completion::Normal(Value::Promise(promise))
}
