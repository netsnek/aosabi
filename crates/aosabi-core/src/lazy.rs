// SPDX-License-Identifier: MIT
/*
 * aosabi-core: a member whose value is computed when it is selected.
 *
 * No analogue on the reference side, where such a member is a property whose
 * value is an arrow function with no parameter, => value, which the executor
 * calls when the member is selected.
 */

//! A member whose value is computed each time it is selected, the twin of a
//! property `() => value`.
//!
//! A patch of the reference's federation hands a member to the executor as an
//! arrow, `org: () => api.orgs.delegate('Query.org', ...)`, and the default
//! field resolver of the executor calls a property that holds a function with
//! the arguments, the context and the info of the field. The arrow ignores
//! them, so the member costs nothing until a selection reaches it. [`Lazy`] is
//! that arrow: the schema describes it as a callable with no parameter that
//! returns `T`, and the executor meets a function that answers a promise of
//! `T`.
//!
//! The computation starts each time the function is called, as an arrow runs
//! each time it is called, so two selections of one lazy member under two
//! aliases run it twice. The function is anonymous, where the arrow of an
//! object literal takes the name of its property, because a value does not
//! know the member it is stored in. Only `inspect` prints that name, and no
//! answer of the executor passes a lazy member to it.
//!
//! A lazy member stands as a public field of a described struct, a patch
//! struct above all, whose object reads the field through a shared reference
//! and hands the executor the same function as [`IntoValue`] does, so every
//! read of the property is the one arrow the object literal holds.
//!
//! A delegation hands back the object its patch made, whose lazy member is
//! that function, and [`FromDelegated`] reads it back into a `Lazy` that
//! calls it, so a struct that holds the member reads as the patch wrote it.

use alloc::{
    boxed::Box,
    rc::Rc, //
};
use core::{
    fmt,
    future::IntoFuture, //
};

use aosabi_execute::{
    promise::Agent,
    value::{
        Completion,
        Function,
        Value, //
    }, //
};
use aosabi_hooks::LocalBoxFuture;

use crate::{
    descriptor::{
        Described,
        TypeExpr,
        TypeLink,
        callable::CallableInfo, //
    },
    glue::{
        Thrown,
        completion::complete_future,
        into_value::IntoValue,
        shared::{
            FieldValue,
            Shared, //
        }, //
    },
    remote::FromDelegated, //
};

/// The computation of a lazy member, which starts a new future each call.
type Compute<T> = dyn Fn() -> LocalBoxFuture<'static, Result<T, Thrown>>;

/// A member whose value is computed when it is selected, the twin of a
/// property `() => value`.
///
/// A clone is the same member, because the computation sits behind an `Rc`.
///
/// # Examples
///
/// ```ignore
/// let org = Lazy::new(move || {
///     let api = api.clone();
///     async move { api.orgs.delegate("Query.org", args).await }
/// });
/// ```
pub struct Lazy<T>(Rc<Compute<T>>);

impl<T: 'static> Lazy<T> {
    /// Returns the member that runs `compute` each time the member is
    /// resolved.
    ///
    /// `compute` answers anything that turns into a future of the value or of
    /// the thrown value, an `async` block among them. The body of an `async`
    /// block runs up to its first await when the executor calls the member, as
    /// the body of an async arrow runs when it is called.
    pub fn new<F, Fut>(compute: F) -> Self
    where
        F: Fn() -> Fut + 'static,
        Fut: IntoFuture<Output = Result<T, Thrown>> + 'static,
        Fut::IntoFuture: 'static,
    {
        Self(Rc::new(move || Box::pin(compute().into_future())))
    }

    /// Starts the computation and returns its future.
    ///
    /// This is a call of the arrow, so every call starts a computation of its
    /// own.
    #[must_use]
    pub fn compute(&self) -> LocalBoxFuture<'static, Result<T, Thrown>> {
        (self.0)()
    }
}

impl<T> Clone for Lazy<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T> fmt::Debug for Lazy<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Lazy(..)")
    }
}

impl<T: Described> Described for Lazy<T> {
    const TYPE: TypeExpr = TypeExpr::Callable(&CallableInfo {
        params: &[],
        returns: TypeLink::of::<T>(),
    });
}

impl<T: IntoValue + 'static> IntoValue for Lazy<T> {
    /// Returns the function the executor calls, which answers a promise that
    /// settles the way one run of the computation completes.
    ///
    /// The executor calls it with the arguments, the context and the info of
    /// the field, which an arrow with no parameter ignores. The run is spawned
    /// on the agent of the request, which polls it once before the call
    /// returns.
    fn into_value(self) -> Value {
        Value::Function(Function::new("", move |agent, _arguments| {
            complete_future(agent, (self.0)())
        }))
    }
}

impl<T: IntoValue + 'static> FieldValue for Lazy<T> {
    /// Returns the function of [`IntoValue`] over a clone of the member,
    /// which shares the computation, so the struct that holds the member
    /// keeps it.
    fn field_value(&self, _: &dyn Fn() -> Shared<Self>) -> Value {
        IntoValue::into_value(self.clone())
    }
}

impl<T: FromDelegated<T> + 'static, R> FromDelegated<R> for Lazy<T> {
    /// Returns the member that calls the function the delegated value holds
    /// each time it is resolved, and reads what the call settles to as `T`.
    ///
    /// The executor calls a function valued member with the arguments, the
    /// context and the info of the field, which the arrow of a patch ignores,
    /// so the call here passes none. A value that is no function is the value
    /// of every resolution, as a member that holds a value and no arrow is.
    fn from_delegated(value: Value) -> Result<Self, Thrown> {
        Ok(Self::new(move || {
            let value = value.clone();
            async move {
                let Value::Function(function) = value else {
                    return T::from_delegated(value);
                };
                let Some(agent) = Agent::current() else {
                    return Err(Thrown::type_error(
                        "A lazy member is called outside of an agent.",
                    ));
                };
                let settled = match function.call(&agent, &[]) {
                    Completion::Normal(Value::Promise(promise)) => promise.await.map_err(Thrown)?,
                    Completion::Normal(settled) => settled,
                    Completion::Throw(thrown) => return Err(Thrown(thrown)),
                };
                T::from_delegated(settled)
            }
        }))
    }
}
