// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the decorators.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/create-decorator.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `Decorator`, `DecoratorArgs` and `create_decorator`.
//!
//! `createDecorator(callback)` answers a decorator that replaces a method or
//! a function with
//! `async function (...args) { await callback(...args); return original.apply(this, args) }`,
//! so a decorated member is always asynchronous, the callback sees the
//! positional arguments, and stacked decorators nest one wrapper per
//! decorator with the top one outermost. [`create_decorator`] takes an async
//! closure over the arguments, whose `AsyncFn` bound accepts a closure that
//! captures an owned value, and `#[decorate(expr)]` applies the decorator at
//! its place, where the macro that reads the site hands it to the glue in
//! source order.

use core::fmt;
use std::rc::Rc;

use aosabi_execute::value::Value;
use aosabi_hooks::LocalBoxFuture;

use crate::result::Result;

/// The positional arguments of a call, the twin of `...args`.
#[derive(Clone)]
pub struct DecoratorArgs(Rc<[Value]>);

impl DecoratorArgs {
    /// Returns the arguments `args`.
    #[must_use]
    pub fn new(args: Rc<[Value]>) -> Self {
        Self(args)
    }

    /// Returns the arguments as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[Value] {
        &self.0
    }

    /// Returns the argument at `index`, `None` past the last one.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.0.get(index)
    }

    /// Returns the number of arguments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the call has no argument.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for DecoratorArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

/// The callback of a decorator, called with the arguments of every call.
type Callback = Rc<dyn Fn(DecoratorArgs) -> LocalBoxFuture<'static, Result<()>>>;

/// A decorator, the value `createDecorator` answers.
///
/// A decorator holds an `Rc` and is neither `Send` nor `Sync`, so a site
/// keeps the decorators of its `#[decorate]` in a cell of the thread, made at
/// the first call.
#[derive(Clone)]
pub struct Decorator(Callback);

impl Decorator {
    /// Runs the callback over `args`, `await callback(...args)`.
    #[must_use]
    pub fn call(&self, args: DecoratorArgs) -> LocalBoxFuture<'static, Result<()>> {
        (self.0)(args)
    }
}

impl fmt::Debug for Decorator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Decorator")
    }
}

/// Returns the decorator of `callback`, `createDecorator(callback)`.
///
/// The callback runs before the decorated member on every call, with the
/// positional arguments of the call, and an `Err` stops the call.
///
/// # Examples
///
/// ```ignore
/// pub fn audit(label: &'static str) -> Decorator {
///     create_decorator(async move |_args| {
///         get_context()?.header("x-audit", label);
///         Ok(())
///     })
/// }
/// ```
pub fn create_decorator<F>(callback: F) -> Decorator
where
    F: AsyncFn(DecoratorArgs) -> Result<()> + 'static,
{
    let callback = Rc::new(callback);
    Decorator(Rc::new(move |args| {
        let callback = Rc::clone(&callback);
        Box::pin(async move { callback(args).await })
    }))
}
