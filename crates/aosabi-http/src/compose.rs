// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the chain of the middleware of a request.
 *
 * Derived from hono 4.11.9, dist/compose.js, MIT.
 */

//! `compose` and `Next`.
//!
//! The chain runs the matched handlers of a request in registration order as
//! an onion: each handler receives the continuation of the rest, a second call
//! of the continuation throws `next() called multiple times`, a truthy value a
//! handler returns becomes the response of the context when the context is not
//! finalized or the handler threw, the error handler is called only for a
//! thrown instance of `Error`, and the not found handler answers when nothing
//! finalized the context.
//!
//! A handler here returns `Result<Response, E>`. A middleware that calls
//! [`Next::run`] gets the response of the context after the rest of the chain
//! ran and returns it, changed or not, which is the value the chain already
//! holds, and a middleware that answers on its own never calls it.

use core::cell::Cell;
use std::rc::Rc;

use crate::{
    context::Context,
    fetch::{
        body::LocalBoxFuture,
        response::Response, //
    },
    web_app::{
        ErrorHandler,
        HandlerFn,
        MiddlewareFn,
        RouteError, //
    }, //
};

/// The rest of a chain, which answers the value the reference's continuation returns.
type Rest<E> = Box<dyn FnOnce(Context) -> LocalBoxFuture<'static, Result<Option<Response>, E>>>;

/// The continuation of a middleware, `next` of the reference.
pub struct Next<E> {
    /// The rest of the chain, `None` at its end with no outer continuation.
    rest: Option<Rest<E>>,
}

impl<E: 'static> Next<E> {
    /// Creates the continuation that runs `rest`, which returns nothing.
    pub fn new<F, Fut>(rest: F) -> Self
    where
        F: FnOnce(Context) -> Fut + 'static,
        Fut: Future<Output = Result<(), E>> + 'static,
    {
        Self {
            rest: Some(Box::new(move |c| {
                let rest = rest(c);
                Box::pin(async move { rest.await.map(|()| None) })
            })),
        }
    }

    /// Creates the continuation that runs `rest` and returns its response.
    ///
    /// A chain whose outer continuation returns a response takes it as the
    /// value its last step returned, as `route` of the app runs a route inside
    /// the chain of its error handler.
    pub fn answering<F, Fut>(rest: F) -> Self
    where
        F: FnOnce(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        Self {
            rest: Some(Box::new(move |c| {
                let rest = rest(c);
                Box::pin(async move { rest.await.map(Some) })
            })),
        }
    }

    /// Creates the continuation that does nothing.
    #[must_use]
    pub fn end() -> Self {
        Self { rest: None }
    }

    /// Runs the rest of the chain and returns the response of the context, `await next(); c.res`.
    ///
    /// # Errors
    ///
    /// Returns what the rest of the chain threw and no error handler took.
    pub async fn run(self, c: Context) -> Result<Response, E> {
        if let Some(rest) = self.rest {
            rest(c.clone()).await?;
        }
        Ok(c.res())
    }

    /// Runs the rest of the chain and returns the value the continuation returned.
    async fn call(self, c: Context) -> Result<Option<Response>, E> {
        match self.rest {
            Some(rest) => rest(c).await,
            None => Ok(None),
        }
    }
}

/// The state of one run of a chain.
struct Chain<E> {
    /// The handlers of the chain in order.
    middleware: Rc<[Rc<dyn MiddlewareFn<E>>]>,
    /// The error handler.
    on_error: Option<Rc<dyn ErrorHandler<E>>>,
    /// The not found handler.
    on_not_found: Option<Rc<dyn HandlerFn<E>>>,
    /// The index of the last handler started, -1 before the first.
    index: Cell<isize>,
}

/// A composed chain, which runs over a context and an outer continuation.
pub type Composed<E> =
    Rc<dyn Fn(Context, Option<Next<E>>) -> LocalBoxFuture<'static, Result<Context, E>>>;

/// Returns the chain of `middleware` with `on_error` and `on_not_found`.
///
/// The composed function runs the chain over a context and the outer
/// continuation that follows its last handler, and answers the context.
pub fn compose<E: RouteError>(
    middleware: Rc<[Rc<dyn MiddlewareFn<E>>]>,
    on_error: Option<Rc<dyn ErrorHandler<E>>>,
    on_not_found: Option<Rc<dyn HandlerFn<E>>>,
) -> Composed<E> {
    Rc::new(move |context, next| {
        let chain = Rc::new(Chain {
            middleware: Rc::clone(&middleware),
            on_error: on_error.clone(),
            on_not_found: on_not_found.clone(),
            index: Cell::new(-1),
        });
        let outer = Rc::new(core::cell::RefCell::new(next));
        dispatch(chain, outer, context, 0)
    })
}

/// Runs handler `i` of the chain.
fn dispatch<E: RouteError>(
    chain: Rc<Chain<E>>,
    outer: Rc<core::cell::RefCell<Option<Next<E>>>>,
    context: Context,
    i: usize,
) -> LocalBoxFuture<'static, Result<Context, E>> {
    Box::pin(async move {
        let i_signed = isize::try_from(i).unwrap_or(isize::MAX);
        if i_signed <= chain.index.get() {
            return Err(E::error("next() called multiple times"));
        }
        chain.index.set(i_signed);
        let mut res: Option<Response> = None;
        let mut is_error = false;
        let outer_next = if chain.middleware.get(i).is_none() && i == chain.middleware.len() {
            outer.borrow_mut().take()
        } else {
            None
        };
        let outcome = if let Some(handler) = chain.middleware.get(i) {
            context.req().set_route_index(i);
            let next_chain = Rc::clone(&chain);
            let next_outer = Rc::clone(&outer);
            let next = Next::new(move |c| async move {
                dispatch(next_chain, next_outer, c, i + 1).await.map(|_| ())
            });
            Some(handler.call(context.clone(), next).await.map(Some))
        } else if let Some(next) = outer_next {
            Some(next.call(context.clone()).await)
        } else {
            if !context.finalized()
                && let Some(on_not_found) = &chain.on_not_found
            {
                res = Some(on_not_found.call(context.clone()).await?);
            }
            None
        };
        match outcome {
            Some(Ok(response)) => res = response,
            Some(Err(err)) => match &chain.on_error {
                Some(on_error) if err.is_error() => {
                    context.set_error(Rc::new(err.clone()));
                    res = Some(on_error.call(err, context.clone()).await?);
                    is_error = true;
                }
                _ => return Err(err),
            },
            None => {}
        }
        if let Some(res) = res
            && (!context.finalized() || is_error)
        {
            context.set_res(res);
        }
        Ok(context)
    })
}
