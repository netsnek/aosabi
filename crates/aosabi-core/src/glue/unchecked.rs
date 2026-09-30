// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue unchecked: a value whose declared type the run time does
 * not check.
 *
 * No analogue in the reference. It is the twin of the TypeScript cast
 * `value as unknown as T`.
 */

//! The unchecked value.
//!
//! A TypeScript resolver may return a value its declared type forbids, a
//! `null` for a `string` or a promise for a plain value, behind the cast
//! `as unknown as T`. The schema trusts the declared type and the executor
//! meets the real value, which is how a fixture provokes the error of a null
//! at a non null position. [`Unchecked`] is that cast: it is described as
//! `T` and carries any value.
//!
//! The cast leaves the type the checker sees as `T` itself, so the
//! description is a link to `T` that maps as `T`, and the inference walks
//! one node for `Unchecked<T>` and `T`. A subclass that the heritage sweep
//! finds through `T` then meets the `T` the root returns, as it does in the
//! reference, where the root of `gateway-poly-front-app` returns
//! `Promise<Profile | null>` behind a structural assignment.

use core::{
    future::Future,
    marker::PhantomData, //
};

use aosabi_execute::{
    promise::Agent,
    value::{
        Completion,
        Value, //
    }, //
};

use crate::{
    descriptor::{
        Described,
        TypeExpr,
        TypeLink, //
    },
    glue::{
        into_value::IntoValue,
        thrown::Thrown, //
    }, //
};

/// A value that the schema describes as `T` and the run time does not check.
pub struct Unchecked<T>(Value, PhantomData<fn() -> T>);

impl<T> Unchecked<T> {
    /// Returns `value` under the declared type `T`.
    pub fn value(value: impl IntoValue) -> Self {
        Self(value.into_value(), PhantomData)
    }

    /// Returns `null` under the declared type `T`.
    #[must_use]
    pub const fn null() -> Self {
        Self(Value::Null, PhantomData)
    }

    /// Returns `undefined` under the declared type `T`.
    #[must_use]
    pub const fn undefined() -> Self {
        Self(Value::Undefined, PhantomData)
    }

    /// Returns a promise under the declared type `T`, which settles the way
    /// `future` completes.
    ///
    /// The future is spawned on the agent that runs the resolver, so it is
    /// polled once before this function returns, as the executor of
    /// `new Promise` runs before the constructor returns. Outside of a
    /// running agent there is no promise to make, and the value is
    /// `undefined`.
    pub fn promise(future: impl Future<Output = Result<Value, Thrown>> + 'static) -> Self {
        let Some(agent) = Agent::current() else {
            return Self::undefined();
        };
        let promise = agent.spawn(Box::pin(async move {
            match future.await {
                Ok(value) => Completion::Normal(value),
                Err(Thrown(value)) => Completion::Throw(value),
            }
        }));
        Self(Value::Promise(promise), PhantomData)
    }
}

impl<T> IntoValue for Unchecked<T> {
    fn into_value(self) -> Value {
        self.0
    }
}

impl<T: Described> Described for Unchecked<T> {
    const TYPE: TypeExpr = TypeExpr::Boxed(TypeLink::of::<T>());
}
