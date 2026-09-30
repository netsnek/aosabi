// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core: the member middleware of a plugin and the value it hands up.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/index.ts and
 * packages/pylon/src/app/index.ts, Apache-2.0. Changed by the Aosabi authors.
 */

//! `PluginMiddleware`, `ValueMiddlewareFn` and `IntoMiddlewareValue`.
//!
//! The member `middleware` is a middleware of Hono, which answers a response
//! or nothing. The loader runs the members through a dispatch of its own,
//! which hands each one `() => dispatch(i + 1)` and hands Hono the value the
//! first one answers, so the value of a middleware reaches Hono only through
//! every middleware before it. A middleware that answers `return next()` hands
//! the value of its continuation up, and one that runs `await next()` and
//! returns nothing answers nothing, so whatever its continuation answered is
//! lost.
//!
//! [`PluginMiddleware`] holds the two Rust forms of the member, and the
//! helper `middleware` of `plugin!` picks one by the output of the closure
//! through [`IntoMiddlewareValue`]:
//!
//! | Output of the closure | Form | The reference |
//! | --- | --- | --- |
//! | `Result<Response>` | [`PluginMiddleware::Response`] | `return next()`, or a response of its own |
//! | `Result<()>` | [`PluginMiddleware::Value`] | `await next()` with nothing returned |
//! | `Result<Option<Response>>` | [`PluginMiddleware::Value`] | a response of its own on one branch and nothing on another |
//!
//! The first form is the one of `docs/authoring.md` section 5: its
//! continuation sets the value of the rest of the chain on the context, so
//! `next.run(c)` answers it. The second keeps the value of the middleware
//! apart from the context, so its continuation drops what the rest of the
//! chain answered, as the reference does.

use core::future::Future;
use std::rc::Rc;

use aosabi_hooks::LocalBoxFuture;
use aosabi_http::{
    Context,
    MiddlewareFn,
    Response, //
};

use crate::{
    app::application::Next,
    glue::Thrown,
    result::Result, //
};

/// A middleware of the value form, which answers a response of its own or nothing.
pub trait ValueMiddlewareFn: 'static {
    /// Calls the middleware on `c` with the continuation `next` and answers
    /// its value, `None` for nothing.
    fn call(&self, c: Context, next: Next) -> LocalBoxFuture<'static, Result<Option<Response>>>;
}

impl<F, Fut> ValueMiddlewareFn for F
where
    F: Fn(Context, Next) -> Fut + 'static,
    Fut: Future<Output = Result<Option<Response>>> + 'static,
{
    fn call(&self, c: Context, next: Next) -> LocalBoxFuture<'static, Result<Option<Response>>> {
        Box::pin(self(c, next))
    }
}

/// The member `middleware` of a plugin in one of its two forms.
#[derive(Clone)]
pub enum PluginMiddleware {
    /// A middleware that answers the response of its continuation or one of
    /// its own, `return next()` or `return c.text(...)`.
    Response(Rc<dyn MiddlewareFn<Thrown>>),
    /// A middleware that answers a value of its own, a response or nothing,
    /// and whose continuation drops the value of the rest of the chain,
    /// `await next()` with nothing returned.
    Value(Rc<dyn ValueMiddlewareFn>),
}

impl From<Rc<dyn MiddlewareFn<Thrown>>> for PluginMiddleware {
    fn from(middleware: Rc<dyn MiddlewareFn<Thrown>>) -> Self {
        Self::Response(middleware)
    }
}

/// What the closure of a `middleware` may answer, which picks the form of the member.
pub trait IntoMiddlewareValue: Sized + 'static {
    /// Returns the member the closure `f` is.
    fn plugin_middleware<F, Fut>(f: F) -> PluginMiddleware
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future<Output = Self> + 'static;

    /// Returns the member the asynchronous closure `f` is.
    fn plugin_middleware_async<F>(f: F) -> PluginMiddleware
    where
        F: AsyncFn(Context, Next) -> Self + 'static;
}

impl IntoMiddlewareValue for Result<Response> {
    fn plugin_middleware<F, Fut>(f: F) -> PluginMiddleware
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future<Output = Self> + 'static,
    {
        PluginMiddleware::Response(Rc::new(f))
    }

    fn plugin_middleware_async<F>(f: F) -> PluginMiddleware
    where
        F: AsyncFn(Context, Next) -> Self + 'static,
    {
        // Each call owns the closure its future borrows.
        let f = Rc::new(f);
        PluginMiddleware::Response(Rc::new(move |c: Context, next: Next| {
            let f = Rc::clone(&f);
            async move { f(c, next).await }
        }))
    }
}

impl IntoMiddlewareValue for Result<()> {
    fn plugin_middleware<F, Fut>(f: F) -> PluginMiddleware
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future<Output = Self> + 'static,
    {
        PluginMiddleware::Value(Rc::new(move |c: Context, next: Next| {
            let answer = f(c, next);
            async move { answer.await.map(|()| None) }
        }))
    }

    fn plugin_middleware_async<F>(f: F) -> PluginMiddleware
    where
        F: AsyncFn(Context, Next) -> Self + 'static,
    {
        let f = Rc::new(f);
        PluginMiddleware::Value(Rc::new(move |c: Context, next: Next| {
            let f = Rc::clone(&f);
            async move { f(c, next).await.map(|()| None) }
        }))
    }
}

impl IntoMiddlewareValue for Result<Option<Response>> {
    fn plugin_middleware<F, Fut>(f: F) -> PluginMiddleware
    where
        F: Fn(Context, Next) -> Fut + 'static,
        Fut: Future<Output = Self> + 'static,
    {
        PluginMiddleware::Value(Rc::new(f))
    }

    fn plugin_middleware_async<F>(f: F) -> PluginMiddleware
    where
        F: AsyncFn(Context, Next) -> Self + 'static,
    {
        let f = Rc::new(f);
        PluginMiddleware::Value(Rc::new(move |c: Context, next: Next| {
            let f = Rc::clone(&f);
            async move { f(c, next).await }
        }))
    }
}
