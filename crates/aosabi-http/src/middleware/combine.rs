// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the combinator except of hono, which runs a middleware unless
 * the path matches.
 *
 * The export except.
 *
 * Derived from hono 4.11.9, dist/middleware/combine/index.js, MIT.
 */

//! `except`.
//!
//! The combinator `except(paths, mw)` runs `mw` unless the path of the request
//! matches one of `paths`, in which case the continuation runs directly. The
//! paths are matched by the trie router under the method `ALL`, so `/__aosabi/*`
//! matches the path and every path below it. Aosabi wraps its request logger in
//! `except(['/__aosabi/*'], ...)`,
//! divergence D54 of `docs/byte-equality.md` section 3.

use std::rc::Rc;

use crate::{
    compose::Next,
    context::Context,
    fetch::{
        body::LocalBoxFuture,
        response::Response, //
    },
    router::{
        METHOD_NAME_ALL,
        Router, //
    }, //
    web_app::{
        MiddlewareFn,
        RouteError, //
    },
};

/// The middleware `except` makes.
struct Except<E> {
    /// The router of the paths, whose handler is `true`.
    router: Router<bool>,
    /// The middleware that runs unless a path matches.
    middleware: Rc<dyn MiddlewareFn<E>>,
}

impl<E: RouteError> MiddlewareFn<E> for Except<E> {
    fn call(&self, c: Context, next: Next<E>) -> LocalBoxFuture<'static, Result<Response, E>> {
        let matched = self
            .router
            .matches(METHOD_NAME_ALL, &c.req().path())
            .first()
            .is_some_and(|(handler, _)| *handler);
        if matched {
            return Box::pin(next.run(c));
        }
        let route_index = c.req().route_index();
        let middleware = Rc::clone(&self.middleware);
        Box::pin(async move {
            c.req().set_route_index(route_index);
            middleware.call(c, next).await
        })
    }
}

/// Returns the middleware that runs `middleware` unless the path matches one of `paths`.
pub fn except<E: RouteError>(
    paths: &[&str],
    middleware: impl MiddlewareFn<E>,
) -> impl MiddlewareFn<E> {
    let mut router = Router::new();
    for path in paths {
        router.add(METHOD_NAME_ALL, path, &true);
    }
    Except {
        router,
        middleware: Rc::new(middleware),
    }
}
