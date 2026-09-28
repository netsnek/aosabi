// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the web app, its routes, its dispatch and its handlers.
 *
 * Derived from hono 4.11.9, dist/hono-base.js, MIT.
 * Derived from hono 4.11.9, dist/hono.js, MIT.
 *
 * It carries the quirk Q57_HONO_HEAD_FROM_GET of docs/byte-equality.md section
 * 13.1.
 */

//! The web app, the translation of the app of hono.
//!
//! An app registers its routes and middleware in order into one router, and its `fetch` matches a
//! request, builds its context and runs the matched handlers: one handler alone is called
//! directly with the not found handler as its continuation, and more than one run through
//! `compose` with the error handler and the not found handler. An error that escapes is answered
//! by the error handler when it is an instance of `Error` and rejects the fetch otherwise. The
//! path is strict, so `/a/` and `/a` are two paths.
//!
//! A `HEAD` request is dispatched as `GET` and answered with no body over the
//! head of the `GET` answer, `new Response(null, getResponse)`, so the answer
//! keeps the status and the headers of the `GET` answer and loses the length
//! the adapter adds below them, and the answer of `c.text` with nothing
//! prepared, which has no init, fails, the 500 of the adapter (A9 of the
//! plan's version 2 draft, quirk `Q57_HONO_HEAD_FROM_GET`).
//!
//! A closure passed to a route method is bound by `Fn` directly, so rustc gives the closure its
//! parameter types, and [`HandlerFn`] and [`MiddlewareFn`] are the forms the app stores it in.
//!
//! A middleware tells through [`MiddlewareFn::is_csrf`] whether it is a
//! `csrf(options)`, and the dispatch marks a request that one of them matches,
//! so the default csrf of the base pipeline steps aside for the app's own,
//! divergence D70 of `docs/byte-equality.md`.

use core::{
    cell::RefCell,
    panic::Location, //
};
use std::rc::Rc;

use crate::{
    compose::{
        Next,
        compose, //
    },
    context::{
        Arg,
        Context,
        Env,
        ExecutionCtx, //
    },
    fetch::{
        body::{
            Body,
            LocalBoxFuture, //
        },
        request::Request,
        response::{
            FetchError,
            FetchErrorKind,
            Response, //
        }, //
    },
    router::{
        METHOD_NAME_ALL,
        Params,
        Router, //
    },
    utils::url::{
        get_path,
        get_path_no_strict,
        merge_path, //
    }, //
};

/// The error a handler of the app throws.
///
/// The app names no error type of a crate above it, so the type is a
/// parameter, and `aosabi-core` fills it with the thrown value of the
/// executor. The methods are the readings of a thrown value that hono and
/// the base pipeline make.
pub trait RouteError: Clone + 'static {
    /// Returns whether the value is an instance of `Error`, which decides whether the error handler takes it.
    fn is_error(&self) -> bool;

    /// Returns the response of an `HTTPException`, `err.getResponse()`, and `None` for any other value.
    fn http_exception_response(&self) -> Option<Response>;

    /// Returns `err.statusCode` when it is a number.
    fn status_code(&self) -> Option<f64>;

    /// Returns `err.message`.
    fn message(&self) -> String;

    /// Returns `err.name`, `Error` for a plain error.
    fn name(&self) -> String;

    /// Creates `new Error(message)`.
    fn error(message: &str) -> Self;

    /// Creates `new TypeError(message)`, the throw of a platform constructor.
    fn type_error(message: &str) -> Self;

    /// Creates `new RangeError(message)`, the throw of a platform constructor
    /// for a value outside its range, the status of a `Response` among them,
    /// and a `TypeError` where the error type holds no such class.
    #[must_use]
    fn range_error(message: &str) -> Self {
        Self::type_error(message)
    }

    /// Creates the error of the refusal `error` of a platform constructor,
    /// of the class its kind names.
    #[must_use]
    fn refusal(error: &FetchError) -> Self {
        match error.kind {
            FetchErrorKind::Range => Self::range_error(&error.message),
            FetchErrorKind::Type => Self::type_error(&error.message),
        }
    }
}

/// A stored route handler, `(c) => ...`, answering a response or throwing `E`.
pub trait HandlerFn<E>: 'static {
    /// Calls the handler on `c`.
    fn call(&self, c: Context) -> LocalBoxFuture<'static, Result<Response, E>>;
}

impl<E, F, Fut> HandlerFn<E> for F
where
    F: Fn(Context) -> Fut + 'static,
    Fut: Future<Output = Result<Response, E>> + 'static,
{
    fn call(&self, c: Context) -> LocalBoxFuture<'static, Result<Response, E>> {
        Box::pin(self(c))
    }
}

/// A stored middleware, `(c, next) => ...`, answering the continuation's response or its own.
pub trait MiddlewareFn<E>: 'static {
    /// Calls the middleware on `c` with the continuation `next`.
    fn call(&self, c: Context, next: Next<E>) -> LocalBoxFuture<'static, Result<Response, E>>;

    /// Returns whether the middleware is a `csrf(options)`, which takes the
    /// place of the default csrf on every request it matches.
    fn is_csrf(&self) -> bool {
        false
    }
}

impl<E, F, Fut> MiddlewareFn<E> for F
where
    F: Fn(Context, Next<E>) -> Fut + 'static,
    Fut: Future<Output = Result<Response, E>> + 'static,
{
    fn call(&self, c: Context, next: Next<E>) -> LocalBoxFuture<'static, Result<Response, E>> {
        Box::pin(self(c, next))
    }
}

/// A stored error handler, `(err, c) => ...`.
pub trait ErrorHandler<E>: 'static {
    /// Calls the error handler on `err` and `c`.
    fn call(&self, err: E, c: Context) -> LocalBoxFuture<'static, Result<Response, E>>;
}

impl<E, F, Fut> ErrorHandler<E> for F
where
    F: Fn(E, Context) -> Fut + 'static,
    Fut: Future<Output = Result<Response, E>> + 'static,
{
    fn call(&self, err: E, c: Context) -> LocalBoxFuture<'static, Result<Response, E>> {
        Box::pin(self(err, c))
    }
}

/// A route handler stored as the middleware it is in the chain, which never calls its continuation.
struct Handler<E>(Rc<dyn HandlerFn<E>>);

impl<E: 'static> MiddlewareFn<E> for Handler<E> {
    fn call(&self, c: Context, _next: Next<E>) -> LocalBoxFuture<'static, Result<Response, E>> {
        self.0.call(c)
    }
}

/// A route of an app with an error handler of its own, mounted with
/// `app.route`, which runs inside a chain of that error handler.
///
/// Keeps the handler it wraps as `handler[COMPOSED_HANDLER]`, so the route tells what its handler
/// is.
struct Caught<E> {
    /// The handler of the route.
    inner: Rc<dyn MiddlewareFn<E>>,
    /// The error handler of the app the route comes from.
    error_handler: Rc<dyn ErrorHandler<E>>,
}

impl<E: RouteError> MiddlewareFn<E> for Caught<E> {
    fn call(&self, c: Context, next: Next<E>) -> LocalBoxFuture<'static, Result<Response, E>> {
        let inner = Rc::clone(&self.inner);
        let composed = compose(
            Rc::from(Vec::new()),
            Some(Rc::clone(&self.error_handler)),
            None,
        );
        Box::pin(async move {
            let inner_next =
                Next::answering(move |c: Context| async move { inner.call(c, next).await });
            let context = composed(c, Some(inner_next)).await?;
            Ok(context.res())
        })
    }

    fn is_csrf(&self) -> bool {
        self.inner.is_csrf()
    }
}

/// One registered route, `{ basePath, path, method, handler }` of `app.routes`.
#[derive(Clone, Debug)]
pub struct RouteRecord {
    /// The method, upper case, `ALL` for a middleware.
    pub method: String,
    /// The path, merged with the base path.
    pub path: String,
    /// The base path of the app that registered the route.
    pub base_path: String,
    /// Where the route was registered, which the authoring map reads.
    pub registered_at: &'static Location<'static>,
}

/// A route as the router stores it, its handler and its record.
#[derive(Clone)]
struct Route<E> {
    /// The handler in its chain form.
    handler: Rc<dyn MiddlewareFn<E>>,
    /// Whether the handler is a `csrf(options)`, read once when the route is added.
    csrf: bool,
    /// The record.
    record: RouteRecord,
}

/// The web app, the translation of the app of hono.
pub struct WebApp<E: RouteError> {
    /// The router, shared with every app a `basePath` of this one made.
    router: Rc<RefCell<Router<Route<E>>>>,
    /// The routes in registration order, shared the same way.
    routes: Rc<RefCell<Vec<Route<E>>>>,
    /// The base path.
    base_path: String,
    /// The error handler, `None` for the default of hono.
    error_handler: Option<Rc<dyn ErrorHandler<E>>>,
    /// The not found handler, `None` for the default of hono.
    not_found_handler: Option<Rc<dyn HandlerFn<E>>>,
    /// Whether the path is strict.
    strict: bool,
}

impl<E: RouteError> Default for WebApp<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: RouteError> WebApp<E> {
    /// Creates an app with no route, `new Hono()`, whose path is strict.
    #[must_use]
    pub fn new() -> Self {
        Self {
            router: Rc::new(RefCell::new(Router::new())),
            routes: Rc::new(RefCell::new(Vec::new())),
            base_path: String::from("/"),
            error_handler: None,
            not_found_handler: None,
            strict: true,
        }
    }

    /// Creates an app whose path is not strict, `new Hono({ strict: false })`.
    #[must_use]
    pub fn new_not_strict() -> Self {
        Self {
            strict: false,
            ..Self::new()
        }
    }

    /// Registers `handler` for `GET` requests of `path`, `app.get(path, handler)`.
    #[track_caller]
    pub fn get<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        self.on("GET", path, handler)
    }

    /// Registers `handler` for `POST` requests of `path`, `app.post(path, handler)`.
    #[track_caller]
    pub fn post<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        self.on("POST", path, handler)
    }

    /// Registers `handler` for `PUT` requests of `path`, `app.put(path, handler)`.
    #[track_caller]
    pub fn put<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        self.on("PUT", path, handler)
    }

    /// Registers `handler` for `DELETE` requests of `path`, `app.delete(path, handler)`.
    #[track_caller]
    pub fn delete<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        self.on("DELETE", path, handler)
    }

    /// Registers `handler` for `OPTIONS` requests of `path`, `app.options(path, handler)`.
    #[track_caller]
    pub fn options<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        self.on("OPTIONS", path, handler)
    }

    /// Registers `handler` for `PATCH` requests of `path`, `app.patch(path, handler)`.
    #[track_caller]
    pub fn patch<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        self.on("PATCH", path, handler)
    }

    /// Registers `handler` for every method of `path`, `app.all(path, handler)`.
    #[track_caller]
    pub fn all<F, Fut>(&mut self, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        self.on(METHOD_NAME_ALL, path, handler)
    }

    /// Registers `handler` for `method` on `path`, `app.on(method, path, handler)`.
    ///
    /// The method is written in upper case, as the reference writes it.
    #[track_caller]
    pub fn on<F, Fut>(&mut self, method: &str, path: &str, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        let location = Location::caller();
        let handler: Rc<dyn HandlerFn<E>> = Rc::new(handler);
        // The upper case reaches the record of the route, which `app.routes`
        // shows, so it keeps every mapping of `toUpperCase`.
        self.add_route(
            &upper_case(method),
            path,
            Rc::new(Handler(handler)),
            location,
        );
        self
    }

    /// Registers `middleware` for every method of `path` and below it, `app.use(path, middleware)`.
    #[track_caller]
    pub fn r#use<F, Fut>(&mut self, path: &str, middleware: F) -> &mut Self
    where
        F: Fn(Context, Next<E>) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        let location = Location::caller();
        self.add_route(METHOD_NAME_ALL, path, Rc::new(middleware), location);
        self
    }

    /// Registers a middleware a crate already holds as a trait object, `app.use(path, middleware)`.
    #[track_caller]
    pub fn use_boxed(&mut self, path: &str, middleware: Rc<dyn MiddlewareFn<E>>) -> &mut Self {
        let location = Location::caller();
        self.add_route(METHOD_NAME_ALL, path, middleware, location);
        self
    }

    /// Mounts the routes of `app` under `path`, `app.route(path, app)`.
    ///
    /// Each route of `app` is added under the base path joined with `path`, and when `app` has an
    /// error handler of its own the route runs inside a chain of that handler.
    pub fn route(&mut self, path: &str, app: &WebApp<E>) -> &mut Self {
        let sub = self.base_path(path);
        let routes: Vec<Route<E>> = app.routes.borrow().clone();
        for route in routes {
            let handler: Rc<dyn MiddlewareFn<E>> = match &app.error_handler {
                None => Rc::clone(&route.handler),
                Some(error_handler) => Rc::new(Caught {
                    inner: Rc::clone(&route.handler),
                    error_handler: Rc::clone(error_handler),
                }),
            };
            sub.add_route(
                &route.record.method,
                &route.record.path,
                handler,
                route.record.registered_at,
            );
        }
        self
    }

    /// Returns an app that shares the routes of this one under the base path `path`, `app.basePath(path)`.
    ///
    /// The new app shares the router and the list of routes and keeps the error and not found
    /// handlers.
    #[must_use]
    pub fn base_path(&self, path: &str) -> Self {
        Self {
            router: Rc::clone(&self.router),
            routes: Rc::clone(&self.routes),
            base_path: merge_path(&self.base_path, path),
            error_handler: self.error_handler.clone(),
            not_found_handler: self.not_found_handler.clone(),
            strict: self.strict,
        }
    }

    /// Sets the error handler, `app.onError(handler)`.
    pub fn on_error<F, Fut>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(E, Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        self.error_handler = Some(Rc::new(handler));
        self
    }

    /// Sets the not found handler, `app.notFound(handler)`.
    pub fn not_found<F, Fut>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(Context) -> Fut + 'static,
        Fut: Future<Output = Result<Response, E>> + 'static,
    {
        self.not_found_handler = Some(Rc::new(handler));
        self
    }

    /// Returns the registered routes in order, `app.routes`.
    #[must_use]
    pub fn routes(&self) -> Vec<RouteRecord> {
        self.routes
            .borrow()
            .iter()
            .map(|route| route.record.clone())
            .collect()
    }

    /// Runs the first `count` routes after every later route, while `app.routes` keeps the order
    /// of registration.
    ///
    /// The router matches in the order routes enter it, so it is built again with the first
    /// `count` routes at its end. The base pipeline of a Rust service calls it once, at the boot,
    /// with the routes the app registered before the boot, so they run behind the pipeline,
    /// divergence D75 of `docs/byte-equality.md`. Under the feature `host-platform` the order
    /// stays as registered, since the TypeScript option keeps the order of a TypeScript app, whose routes registered
    /// before the boot run before its pipeline.
    pub fn put_behind(&mut self, count: usize) {
        if cfg!(feature = "host-platform") {
            return;
        }
        let registered = self.routes.borrow();
        let count = count.min(registered.len());
        let mut router = Router::new();
        for route in registered[count..].iter().chain(&registered[..count]) {
            router.add(&route.record.method, &route.record.path, route);
        }
        *self.router.borrow_mut() = router;
    }

    /// Answers `request`, `app.fetch(request, env, executionCtx)`.
    ///
    /// # Errors
    ///
    /// The future answers the value the chain threw when it is no instance of
    /// `Error` or the error handler threw, and the `TypeError` of a `HEAD`
    /// answer over a `GET` answer that has no init, the rejections of the
    /// reference's `fetch`.
    pub fn fetch(
        &self,
        request: Rc<Request>,
        env: Env,
        execution_ctx: Option<ExecutionCtx>,
    ) -> LocalBoxFuture<'static, Result<Response, E>> {
        let method = String::from(&*request.method);
        self.dispatch(request, execution_ctx, env, &method)
    }

    /// Adds one route.
    fn add_route(
        &self,
        method: &str,
        path: &str,
        handler: Rc<dyn MiddlewareFn<E>>,
        location: &'static Location<'static>,
    ) {
        // The record carries the method as `app.routes` shows it, so the
        // upper case keeps every mapping of `toUpperCase`.
        let method = upper_case(method);
        let path = merge_path(&self.base_path, path);
        let route = Route {
            csrf: handler.is_csrf(),
            handler,
            record: RouteRecord {
                method: method.clone(),
                path: path.clone(),
                base_path: self.base_path.clone(),
                registered_at: location,
            },
        };
        self.router.borrow_mut().add(&method, &path, &route);
        self.routes.borrow_mut().push(route);
    }

    /// Dispatches `request` as `method`.
    fn dispatch(
        &self,
        request: Rc<Request>,
        execution_ctx: Option<ExecutionCtx>,
        env: Env,
        method: &str,
    ) -> LocalBoxFuture<'static, Result<Response, E>> {
        if method == "HEAD" {
            let get = self.dispatch(request, execution_ctx, env, "GET");
            return Box::pin(async move {
                let get_response = get.await?;
                Response::from_response(Body::Empty, &get_response)
                    .map_err(|error| E::refusal(&error))
            });
        }
        let path = if self.strict {
            get_path(&request.url)
        } else {
            get_path_no_strict(&request.url)
        };
        let matched: Vec<(Route<E>, Params)> = self.router.borrow().matches(method, &path);
        let records: Rc<[(RouteRecord, Params)]> = matched
            .iter()
            .map(|(route, params)| (route.record.clone(), params.clone()))
            .collect();
        let c = Context::new(request, path, records, env, execution_ctx);
        if matched.iter().any(|(route, _)| route.csrf) {
            c.set_extension(Rc::new(crate::middleware::csrf::AppCsrf));
        }
        let error_handler = self.error_handler.clone();
        let not_found = self.not_found_handler.clone();
        if matched.len() == 1 {
            let handler = Rc::clone(&matched[0].0.handler);
            let not_found_next = not_found.clone();
            let next = Next::new(move |c: Context| async move {
                let res = run_not_found(not_found_next.as_ref(), c.clone()).await?;
                c.set_res(res);
                Ok(())
            });
            return Box::pin(async move {
                match handler.call(c.clone(), next).await {
                    Ok(res) => Ok(res),
                    Err(err) => handle_error(err, c, error_handler.as_ref()).await,
                }
            });
        }
        let middleware: Rc<[Rc<dyn MiddlewareFn<E>>]> = matched
            .into_iter()
            .map(|(route, _)| route.handler)
            .collect();
        let on_error: Rc<dyn ErrorHandler<E>> = match &error_handler {
            Some(handler) => Rc::clone(handler),
            None => Rc::new(default_error_handler::<E>),
        };
        let on_not_found: Rc<dyn HandlerFn<E>> = match &not_found {
            Some(handler) => Rc::clone(handler),
            None => Rc::new(default_not_found::<E>),
        };
        let composed = compose(middleware, Some(on_error), Some(on_not_found));
        Box::pin(async move {
            match composed(c.clone(), None).await {
                Ok(context) if context.finalized() => Ok(context.res()),
                Ok(_) => {
                    let err = E::error(
                        "Context is not finalized. Did you forget to return a Response object or `await next()`?",
                    );
                    handle_error(err, c, error_handler.as_ref()).await
                }
                Err(err) => handle_error(err, c, error_handler.as_ref()).await,
            }
        })
    }
}

/// Answers an error that escaped the chain.
///
/// An instance of `Error` goes to the error handler, and any other value rejects.
async fn handle_error<E: RouteError>(
    err: E,
    c: Context,
    error_handler: Option<&Rc<dyn ErrorHandler<E>>>,
) -> Result<Response, E> {
    if !err.is_error() {
        return Err(err);
    }
    match error_handler {
        Some(handler) => handler.call(err, c).await,
        None => default_error_handler(err, c).await,
    }
}

/// Runs the not found handler of the app, or hono's default.
async fn run_not_found<E: RouteError>(
    handler: Option<&Rc<dyn HandlerFn<E>>>,
    c: Context,
) -> Result<Response, E> {
    match handler {
        Some(handler) => handler.call(c).await,
        None => default_not_found(c).await,
    }
}

/// The default not found handler, `c.text("404 Not Found", 404)`.
async fn default_not_found<E: RouteError>(c: Context) -> Result<Response, E> {
    c.text_with("404 Not Found", Arg::Status(404))
}

/// The default error handler.
///
/// An error with a response of its own is answered by `c.newResponse(res.body, res)`, and any
/// other error is written to standard error as `console.error` writes it and answered
/// `Internal Server Error` with 500.
async fn default_error_handler<E: RouteError>(err: E, c: Context) -> Result<Response, E> {
    if let Some(res) = err.http_exception_response() {
        return c.new_response(res.body(), &res);
    }
    eprintln!("{}: {}", err.name(), err.message());
    c.text_with("Internal Server Error", Arg::Status(500))
}

/// Returns `text` in upper case, as `toUpperCase` writes it, through the
/// form of ASCII where `text` is ASCII, as a method of HTTP is, which writes
/// the same.
fn upper_case(text: &str) -> String {
    if text.is_ascii() {
        return text.to_ascii_uppercase();
    }
    upper_case_unicode(text)
}

/// Returns `text` in upper case by the case mapping of Unicode.
// Out of line, so an app compiles the tables of the mapping where a method needs them. A host
// `toUpperCase` would drop the tables.
#[cold]
#[inline(never)]
fn upper_case_unicode(text: &str) -> String {
    text.to_uppercase()
}
