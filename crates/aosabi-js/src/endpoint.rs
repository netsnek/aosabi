// SPDX-License-Identifier: MIT
/*
 * aosabi-js endpoint: the GraphQL endpoint of the app, answered by the
 * server of graphql-over-http with no router.
 *
 * No analogue in the reference. A request of the endpoint is answered as
 * the chain of the app of the engine answers it: the first step of
 * crate::boot, the handler of aosabi-core/src/app/handler.rs, the error
 * handler of aosabi-core/src/app/application.rs, the chain of
 * aosabi-http/src/compose.rs and the dispatch of a HEAD of
 * aosabi-http/src/web_app.rs.
 */

//! The GraphQL endpoint of the app.
//!
//! The hono of the host routes every request of the app, so a request of the
//! GraphQL endpoint reaches the engine through the handler of the glue,
//! whose routing chose it, and the engine hands it to the server of
//! `graphql-over-http` the boot keeps beside the app, [`Booted`], with no
//! router and no chain of the port of hono. The answer is the one the app of
//! the engine gives such a request, whose router matches two steps for it,
//! the step the boot puts first and the handler on `*`:
//!
//! 1. The context of the request takes the place of the context the host
//!    opened the request with, and the rest runs with that context in place
//!    and under the logger of the request where the host opened it with one,
//!    as the first step runs it.
//! 2. The server answers the raw request with the environment of the
//!    context, and the answer is a copy of its answer with its status, its
//!    header fields and its substatus, `c.newResponse(res.body, res)`, as
//!    the handler answers it.
//! 3. A value the server throws, and the refusal of that copy, goes to the
//!    error handler of the app of the engine where it is an instance of
//!    `Error`, and any other value rejects. The chain calls that handler for
//!    the handler, inside the first step, and again for the first step,
//!    outside it, each time with what the call before threw, and the
//!    dispatch calls it once more, so an error meets it at most three times
//!    and its record goes through the logger of the request at the first
//!    call alone.
//! 4. A `HEAD` answers the head of that answer with an empty body, as the
//!    dispatch of the port of hono answers it.
//!
//! An answer with the status 404 is the answer of the server, where the
//! handler of the engine goes on to the routes after it and the not found
//! handler of the app. The handler of the glue goes on in the hono of the
//! host for every answer of the engine with that status, and the host
//! registers every route of a plugin of the engine and the not found handler
//! of the landing page itself, each answered by the app of the engine, so
//! the request ends with the same answer.
//!
//! The routes of the plugins of the engine and its not found handler run in
//! the app of the engine, whose chain runs the same server on `*` between
//! the routes of the two passes of the boot, [`middleware`], as the handler
//! of a Rust app runs it there.

#![forbid(unsafe_code)]

use std::rc::Rc;

use aosabi_core::{
    Thrown,
    context::with_context,
    logger::{
        Logger,
        get_logger,
        normalize_error,
        run_with_logger, //
    }, //
};
use aosabi_http::{
    Arg,
    Body,
    Context,
    MiddlewareFn,
    Next,
    Request,
    Response,
    RouteError as _,
    utils::url::get_path, //
};
use aosabi_json::{
    JsValue,
    json, //
};
use graphql_over_http::{
    HttpServer,
    ServerContext, //
};

use crate::boot::Booted;

/// The calls of the error handler an error the server throws meets outside
/// the first step at most: the call of the chain for the first step and the
/// call of the dispatch.
const OUTER_ERROR_HANDLER_CALLS: usize = 2;

/// Answers `request`, a request of the GraphQL endpoint the host hands in as
/// `number`, on the server of `booted`.
///
/// # Errors
///
/// The future answers what the server throws when it is no instance of
/// `Error`, and what the error handler of the app throws at its last call,
/// which the module answers as the throw of the app.
pub async fn answer(
    booted: Rc<Booted>,
    request: Rc<Request>,
    number: u32,
) -> Result<Response, Thrown> {
    let head = request.method == "HEAD";
    let path = get_path(&request.url);
    let c = Context::new(request, path, Rc::from([]), crate::boot::env(), None);
    crate::context::replace_context(number, &c);
    let served = serve(Rc::clone(&booted.server), c.clone());
    let served = match crate::request::logger_of(number) {
        Some(raw) => with_context(c.clone(), run_with_logger(Logger::from_raw(raw), served)).await,
        None => with_context(c.clone(), served).await,
    };
    let answer = match served {
        Ok(answer) => answer,
        Err(err) => answer_error(err, &c, OUTER_ERROR_HANDLER_CALLS)?,
    };
    if !head {
        return Ok(answer);
    }
    Response::from_response(Body::Empty, &answer).map_err(|error| Thrown::refusal(&error))
}

/// Answers the request of `c` on `server`, and what the server throws
/// through one call of the error handler of the app of the engine.
async fn serve(server: Rc<HttpServer>, c: Context) -> Result<Response, Thrown> {
    let context = ServerContext {
        env: c.env(),
        execution_ctx: c.execution_ctx(),
    };
    let answered =
        match graphql_over_http::answer(&server, c.req().raw(), context.into_context()).await {
            Ok(response) => c.new_response(response.body(), &response),
            Err(thrown) => Err(Thrown(thrown)),
        };
    match answered {
        Ok(response) => Ok(response),
        Err(err) => answer_error(err, &c, 1),
    }
}

/// Answers `err` through the error handler of the app of the engine, at
/// most `calls` times, each time with what the call before threw, and
/// rejects with a value that is no instance of `Error`.
fn answer_error(mut err: Thrown, c: &Context, calls: usize) -> Result<Response, Thrown> {
    for _ in 0..calls {
        if !err.is_error() {
            return Err(err);
        }
        match error_response(&err, c) {
            Ok(response) => return Ok(response),
            Err(thrown) => err = thrown,
        }
    }
    Err(err)
}

/// Returns the answer of the error handler of the app of the engine to
/// `err`: the response of an `HTTPException`, the JSON `{ error }` with the
/// message and a numeric `statusCode` as the status, or the record
/// `unhandled route error` and the JSON `{ error: "Internal Server Error" }`
/// with the status 500.
///
/// The error handler of an app with no base pipeline, which the app of the
/// engine is.
fn error_response(err: &Thrown, c: &Context) -> Result<Response, Thrown> {
    if let Some(response) = err.http_exception_response() {
        return Ok(response);
    }
    if let Some(status) = err.status_code() {
        let body = JsValue::Object(json!({ error: err.message() }));
        return c.json_with(&body, Arg::Status(status_u16(status)));
    }
    get_logger().error(
        "unhandled route error",
        json!({ err: normalize_error(&err.0) }),
    );
    let body = JsValue::Object(json!({ error: "Internal Server Error" }));
    c.json_with(&body, Arg::Status(500))
}

/// Returns a numeric `statusCode` as the status of a response, and 0, which
/// the platform refuses, for a number outside the range of a status.
///
/// hono hands the number to the `Response` constructor, which takes an
/// integer in its range and refuses any other with a `RangeError`.
fn status_u16(status: f64) -> u16 {
    if (0.0..=f64::from(u16::MAX)).contains(&status) {
        // The range test leaves a number the target holds.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the value lies in the range of the target"
        )]
        let status = status as u16;
        status
    } else {
        0
    }
}

/// Returns the step of the app of the engine that answers a request on
/// `server`, the handler of a Rust app over the server the boot keeps, which
/// goes on to the rest of the chain where the server answers 404.
pub fn middleware(server: Rc<HttpServer>) -> Rc<dyn MiddlewareFn<Thrown>> {
    Rc::new(move |c: Context, next: Next<Thrown>| {
        let server = Rc::clone(&server);
        async move {
            let context = ServerContext {
                env: c.env(),
                execution_ctx: c.execution_ctx(),
            };
            let response =
                graphql_over_http::answer(&server, c.req().raw(), context.into_context())
                    .await
                    .map_err(Thrown)?;
            if response.status() == 404 {
                return next.run(c).await;
            }
            c.new_response(response.body(), &response)
        }
    })
}
