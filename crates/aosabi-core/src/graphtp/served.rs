// SPDX-License-Identifier: MIT
/*
 * aosabi-core graphtp served: the fetch a served build and a carrier hand the
 * GraphTP listener, straight through the hooks and the engine of the booted
 * app.
 *
 * No analogue in the reference. A call runs the steps of the HTTP layer after
 * its request parse on the GraphQL server the boot mounted, and a call of an
 * app whose router holds more than the core takes the POST of steps 2 and 3
 * of docs/graphtp-in-aosabi.md section 3 through the fetch of the app. The
 * answer carries what step 4 of that section reads, with the substatus of
 * section 4 copied from the field of the Response the HTTP layer sets.
 */

//! The fetch of a served build and of a carrier.
//!
//! [`fetch`] is the fetch function a served build hands its listeners, and
//! [`carrier_fetch`] the one the device carriers Serial Line, USB and
//! Bluetooth and the carrier Network hand theirs. Both take a call straight
//! to the hooks and the engine of the GraphQL server the boot mounted,
//! `BootedApp::graphql_server`, with no request through
//! [`BootedApp::fetch`]. The road calls the plain functions of
//! `graphql-over-http` in the order its request handler calls them after the
//! request parse:
//!
//! 1. `parse_json_content` reads the parameters of the content with the
//!    refusals of a JSON body, and `single_operation` refuses a batch.
//! 2. `HttpServer::get_result_for_params` runs the hooks `onParams` with the
//!    check of the parameters, the hook layer of `aosabi-hooks` with the
//!    masking of the errors, and the hooks `onExecutionResult`.
//! 3. `HttpServer::process_result` runs the hooks `onResultProcess` and the
//!    processor the accepted types choose, which decides the status and the
//!    substatus.
//! 4. A throw of these steps goes through `handle_error` into a result that
//!    `process_result` answers in turn.
//! 5. An answer of status 404 falls through to the rest of the app, as the
//!    GraphQL middleware lets it fall through, and the rest of an app the
//!    road serves is the not found handler of its router. The road answers
//!    the not found answer of hono, since neither the router nor the booted
//!    app hands out a handler an app sets, the landing page among them, and
//!    a client of `GraphTP` reads either as the status 404 with an empty body.
//!
//! The call runs inside a context of its own, bound as the base pipeline binds
//! the context of a request, so `getContext()` answers in every hook and
//! resolver. Its request carries the fields of the call, the credential as
//! `authorization` among them, and the context of the hook layer holds it
//! under `request`, so an app reads the credential on `GraphTP` as it reads it
//! over HTTP. The context holds the routes the router matches for the call, at
//! the index of the GraphQL middleware, as the router hands them to the
//! context of a request. The agents of its operations are collected and, on a
//! host with a spawner, drained after its answer as the GraphQL middleware
//! drains the agents of a request, so a timer a resolver sets fires after the
//! answer on `GraphTP` as it fires over HTTP.
//!
//! The road serves an app a call would pass through the core alone, which
//! `straight_server` decides once per app from what the boot built and never
//! from what a plugin declares. A plugin of the configuration of the app in
//! any form, `use_identity` above all, a plugin a host hands the boot, the
//! viewer and the landing page among them, a plugin of the core with a member
//! the road does not run, a middleware on the plugin chain, a route or
//! middleware of the app on the path of a call, another endpoint or a health
//! check on the path of a call keep the app on its fetch, the
//! POST of `docs/graphtp-in-aosabi.md` section 3, so every check the app
//! holds runs on every call as it runs over HTTP.
//!
//! A body the app writes whole reaches the listener whole, and a body the app
//! writes as a stream reaches it as its [`Chunks`], which the listener reads
//! whole or, under `text/event-stream`, as the events of a stream. Each fetch
//! keeps its answer of a rejection: a carrier answers a field the platform
//! refuses with 400 and a rejection with 500 whatever its name, and a served
//! build answers a rejection named `TimeoutError` with 504 and any other with
//! 500, as the adapter of Node answers them. The module builds with the
//! feature `runtime` beside `homa`, since the Fetch types and the booted app
//! are of the general stack and the module sits in the listener's module,
//! while the listener beside it needs neither.

use alloc::{
    boxed::Box,
    rc::Rc,
    string::String,
    vec::Vec, //
};
use core::task::{
    Context,
    Poll, //
};

use aosabi_execute::Value;
use aosabi_hooks::HookContext;
use aosabi_http::{
    Arg,
    BodyStream,
    Env,
    Headers,
    LocalBoxFuture,
    Response,
    RouteError,
    RouteRecord,
    router::Params, //
};
use graphql_over_http::{
    HttpResult,
    HttpServer,
    ServerContext,
    checks::single_operation,
    error::{
        errors_result,
        handle_error, //
    },
    request_parser::post_json::parse_json_content, //
};

use crate::{
    app::{
        BootedApp,
        execute_config::{
            Builtin,
            PluginLabel,
            PluginRecord, //
        },
        handler::Mounted, //
    },
    context::with_context,
    glue::Thrown,
    graphtp::fetch::{
        Answer,
        Body,
        Broken,
        Chunks,
        Fetch,
        METHOD,
        Reply,
        Request,
        URL, //
    },
};

/// The status a fetch the app rejected with an error of any name but
/// `TimeoutError` answers, as `handleFetchError` of the adapter of Node
/// answers it.
const REJECTED: u16 = 500;

/// The status a fetch the app rejected with a `TimeoutError` answers, by the
/// same lines of the adapter.
const TIMED_OUT: u16 = 504;

/// The status a carrier answers for a request whose field the platform
/// refuses, the status alone of a request the adapter refuses.
const REFUSED: u16 = 400;

/// The path of every call, the one of [`URL`].
const GRAPHQL_PATH: &str = "/graphql";

/// The status of an answer the GraphQL middleware lets fall through to the
/// rest of the app, `if (response.status === 404) return next()`.
const NOT_FOUND: u16 = 404;

/// The text of the not found answer of hono, `c.text("404 Not Found", 404)`,
/// which the router answers where the app sets no not found handler.
///
/// The road answers it for every app it serves, since neither the router nor
/// [`BootedApp`] hands out a handler an app sets, the landing page of
/// `aosabi-landing` among them. The listener answers the text and the landing
/// page alike, the status with substatus 0 and an empty body, since neither
/// names the answer type.
const NOT_FOUND_TEXT: &str = "404 Not Found";

/// The registrations of the core a call passes on every path, which the
/// straight road stands in for: the coding layer, which codes an answer under
/// the feature `base-compress` alone and stands under either setting, the bind
/// of the context, the access log and the dispatch of the plugin middleware of
/// the base pipeline, and the GraphQL middleware the boot mounts last.
const CORE_LAYERS: usize = 5;

/// The members a plugin the core makes may hold while the road serves its app.
///
/// Each runs on the road as it runs on the fetch of the app, a hook of the
/// hook layer, `onParams` or `onExecutionResult`, which
/// `HttpServer::get_result_for_params` runs, or a member that acts at the
/// boot, at the build or at the end of the server alone. Any other member
/// keeps the app on its fetch: `middleware`, which the base pipeline
/// dispatches, the hooks of the server adapter and of the request parse,
/// which read the request of HTTP before the parameters exist and which the
/// road never runs, `onResultProcess`, whose throw on the answer of a handled
/// throw the fetch of the app answers through the error handler of its
/// router where the road could only reject, and every member a later version
/// adds to `MEMBER_ORDER` of the core.
const STRAIGHT_MEMBERS: [&str; 18] = [
    "name",
    "roles",
    "dependsOn",
    "options",
    "setup",
    "build",
    "onPluginInit",
    "onEnveloped",
    "onSchemaChange",
    "onParse",
    "onValidate",
    "onContextBuilding",
    "onExecute",
    "onSubscribe",
    "onDispose",
    "onYogaInit",
    "onParams",
    "onExecutionResult",
];

/// The straight road of an app: the mount of its GraphQL server and the
/// routes the router matches for a call, each with no parameter.
#[derive(Clone)]
struct Straight {
    /// The server the boot mounted with the spawner of its host.
    mounted: Mounted,
    /// The routes of a call in the order the router matches them.
    routes: Rc<[(RouteRecord, Params)]>,
}

/// Returns the fetch function of the booted app `app`, which a served build
/// hands every listener of its card.
///
/// The environment of the request is the process's, since `GraphTP` carries no
/// pair of an incoming and an outgoing message of Node. A call the road
/// rejects answers with no media type and an empty body, 504 for an error
/// named `TimeoutError` and 500 for every other, as `handleFetchError` of the
/// adapter of Node answers, which the listener answers as 504.0 and 500.0 by
/// step 4.
#[must_use]
pub fn fetch(app: Rc<BootedApp>) -> Fetch {
    let straight = straight_server(&app);
    Rc::new(move |call: Request| -> Reply {
        let request = request(&call);
        let pending: LocalBoxFuture<'static, Result<(Response, aosabi_http::Body), Thrown>> =
            if let Some(straight) = &straight {
                let straight = straight.clone();
                Box::pin(async move {
                    run_straight(&straight, request, &call.content)
                        .await
                        .map_err(Thrown)
                })
            } else {
                let fetched = app.fetch(request, Env::Process, None);
                Box::pin(async move { fetched.await.map(with_body) })
            };
        Box::pin(async move {
            match pending.await {
                Ok((response, body)) => answer_with(&response, body),
                Err(thrown) => empty(rejected(&thrown.name())),
            }
        })
    })
}

/// Returns the fetch function of the booted app `app` that a carrier hands
/// its listener, with `spawn` running the driver of a stream body beside the
/// listener.
///
/// Each OPERATION runs straight through the hooks and the engine with the
/// fields [`Request::fields`] names as its request and the content as its
/// parameters, or as `POST http://localhost/graphql` through the whole
/// pipeline of an app the straight road does not serve, and the status, the
/// substatus the HTTP layer decides, the media type and the body go back to
/// the listener, a stream body piece by piece. A field the platform refuses
/// answers 400, and a value the road throws answers 500 whatever its name, as
/// the Node adapter answers it, each with no media type and an empty body.
#[must_use]
pub fn carrier_fetch(app: Rc<BootedApp>, spawn: fn(LocalBoxFuture<'static, ()>)) -> Fetch {
    let straight = straight_server(&app);
    Rc::new(move |call: Request| -> Reply {
        let app = Rc::clone(&app);
        let straight = straight.clone();
        Box::pin(async move { carried(&app, straight.as_ref(), call, spawn).await })
    })
}

/// Sends the request `call` of a carrier along its road and reads its answer.
async fn carried(
    app: &BootedApp,
    straight: Option<&Straight>,
    call: Request,
    spawn: fn(LocalBoxFuture<'static, ()>),
) -> Answer {
    let mut headers = Headers::new();
    for (name, value) in call.fields() {
        let Ok(value) = core::str::from_utf8(value) else {
            return empty(REFUSED);
        };
        if headers.append(name, value).is_err() {
            return empty(REFUSED);
        }
    }
    let content: Rc<[u8]> = Rc::from(call.content);
    let body = aosabi_http::Body::Full(Rc::clone(&content));
    let request = Rc::new(aosabi_http::Request::new(METHOD, URL, headers, body));
    let answered = match straight {
        Some(straight) => run_straight(straight, request, &content).await.ok(),
        None => app
            .fetch(request, Env::Process, None)
            .await
            .ok()
            .map(with_body),
    };
    let Some((response, body)) = answered else {
        return empty(REJECTED);
    };
    let content_type = response
        .headers()
        .get("content-type")
        .map(String::into_bytes);
    let body = match body {
        aosabi_http::Body::Stream(stream) => {
            if let Some(driver) = stream.take_driver() {
                spawn(driver);
            }
            Body::Stream(Box::new(Pieces(stream)))
        }
        other => match other.bytes().await {
            Ok(bytes) => Body::Full(Vec::from(&*bytes)),
            Err(_) => return empty(REJECTED),
        },
    };
    Answer {
        status: response.status(),
        substatus: response.substatus(),
        content_type,
        body,
    }
}

/// Answers the call of `request` with the content `content` straight through
/// the hooks and the engine of the server of `straight`, with no request
/// through the fetch of the app, and returns the response with the body to
/// hand out.
///
/// The call runs inside its own context, which binds `request` for
/// `getContext()` as the base pipeline binds a request and holds the routes
/// of the call at the index of the GraphQL middleware, the last of them, as
/// the router runs it. The steps are those of the request handler of
/// `graphql-over-http` after its request parse, a throw of them handled into
/// a result that is answered in turn. The mount collects the agents of the
/// operations and drains them after the answer as the GraphQL middleware
/// does. An answer of [`NOT_FOUND`] falls through to the rest of the app,
/// which [`straight_server`] leaves no route, so the router answers it with
/// its not found handler, which this function answers with the not found
/// answer of hono, [`NOT_FOUND_TEXT`].
///
/// # Errors
///
/// Returns the value the answer of a handled throw throws in turn, which the
/// fetch answers as a rejection.
async fn run_straight(
    straight: &Straight,
    request: Rc<aosabi_http::Request>,
    content: &[u8],
) -> Result<(Response, aosabi_http::Body), Value> {
    let server = straight.mounted.server();
    let c = aosabi_http::Context::new(
        Rc::clone(&request),
        String::from(GRAPHQL_PATH),
        Rc::clone(&straight.routes),
        Env::Process,
        None,
    );
    c.req()
        .set_route_index(straight.routes.len().saturating_sub(1));
    let server_context = ServerContext {
        env: c.env(),
        execution_ctx: c.execution_ctx(),
    }
    .into_context();
    let answer = async {
        match operate(server, &request, content, &server_context).await {
            Ok(response) => Ok(response),
            Err(error) => {
                let errors = handle_error(error, server.masked_errors_opts(), server.logger());
                server.process_result(
                    &request,
                    HttpResult::Single(errors_result(errors)),
                    &server_context,
                )
            }
        }
    };
    let (response, body) = with_context(c.clone(), straight.mounted.collect(answer)).await?;
    if response.status() != NOT_FOUND {
        return Ok((response, body));
    }
    let response = c
        .text_with::<Thrown>(NOT_FOUND_TEXT, Arg::Status(NOT_FOUND))
        .map_err(|Thrown(value)| value)?;
    let body = response.body();
    Ok((response, body))
}

/// Runs the operation of the content `content` and answers its result, the
/// chain whose throw [`run_straight`] handles.
///
/// A batch takes the refusal of `single_operation`, as the request handler of
/// the HTTP layer refuses every batch.
async fn operate(
    server: &Rc<HttpServer>,
    request: &Rc<aosabi_http::Request>,
    content: &[u8],
    server_context: &HookContext,
) -> Result<Response, Value> {
    let params = single_operation(parse_json_content(content)?)?;
    let result = server
        .get_result_for_params(Rc::clone(request), params, server_context.clone())
        .await?;
    server.process_result(request, HttpResult::Single(result), server_context)
}

/// Returns the straight road of `app` when it serves every call of `app`, and
/// none when a call would meet a part of the app the road does not run.
///
/// A call over HTTP passes the router of the app and the hooks of every
/// plugin, so the road serves an app a call would pass through the core
/// alone, read from what the boot built: every plugin of every pass is one
/// the core makes itself, with members [`STRAIGHT_MEMBERS`] holds, the plugin
/// chain holds no middleware, counted at every push of the loader, the
/// router meets `POST /graphql` with the five registrations of
/// [`CORE_LAYERS`] alone, each from the sources of the core, the endpoint is
/// `/graphql` and no health check endpoint ends the URL of a call, the test
/// of `useHealthCheck`. Any other app answers every call
/// through [`BootedApp::fetch`], so a check it holds, the principal
/// `use_identity` binds above all, holds on every call as it holds over
/// HTTP. What a plugin of the configuration or of the host declares never
/// opens the road, since its members may name fewer hooks than it runs, so
/// every boot that adds the viewer or the landing page of its host keeps its
/// app on the fetch.
fn straight_server(app: &BootedApp) -> Option<Straight> {
    let mounted = app.graphql_server()?;
    let server = mounted.server();
    let health = app
        .config()
        .and_then(|config| config.health_check_endpoint.as_deref())
        .is_some_and(|endpoint| URL.ends_with(endpoint));
    if server.graphql_endpoint() != GRAPHQL_PATH || health {
        return None;
    }
    let boot = app.boot_record();
    let made = boot
        .passes
        .iter()
        .flat_map(|pass| &pass.array)
        .chain(&boot.after_boot)
        .all(|slot| made_by_core(&slot.plugin));
    if !made || app.plugin_middleware() != 0 {
        return None;
    }
    let core = core_sources();
    let mut routes = Vec::new();
    for route in app.routes().into_iter().filter(meets_call) {
        if !route.registered_at.file().starts_with(core) {
            return None;
        }
        routes.push((route, Params::new()));
    }
    (routes.len() == CORE_LAYERS).then(|| Straight {
        mounted: mounted.clone(),
        routes: Rc::from(routes),
    })
}

/// Tells whether `plugin` is a plugin the core makes itself, with members
/// the road runs as the fetch of the app runs them.
///
/// The core makes two plugins of every pass itself, the logger of the errors
/// and the in context plugin, each a plugin object of the core that spreads no
/// plugin, so its members are the fields its literal writes and name every
/// hook it runs. Every other plugin keeps its app on the fetch whatever it
/// names, since its members may name fewer hooks than it runs: a plugin of
/// `config.plugins`, which the boot labels by its index there whatever form it
/// takes, a plugin in the form of the trait or of the HTTP layer joined
/// through `from_http` naming none by default and a plugin object that spreads
/// such a plugin naming none of its hooks, the viewer and the landing page,
/// which the host hands the boot in [󰀅](crate::app::execute_config::Builtins)
/// and which the boot labels by their role whatever the constructor of the
/// host returns, the plugin a host puts before every plugin of a pass, which
/// carries no label of the boot, and the refusal of introspection, a plugin of
/// the HTTP layer joined through `from_http`.
fn made_by_core(plugin: &PluginRecord) -> bool {
    matches!(
        plugin.label,
        PluginLabel::Builtin(Builtin::GraphqlErrorLogger | Builtin::InContext, _)
    ) && plugin
        .members
        .iter()
        .all(|member| STRAIGHT_MEMBERS.contains(member))
}

/// Returns the directory of the sources of `aosabi-core` as this build names
/// them, which every registration of the core records.
///
/// A name this module does not end is no directory of the core, so every
/// registration counts as an app's and the app keeps its fetch.
fn core_sources() -> &'static str {
    let here = file!();
    here.strip_suffix("graphtp/served.rs").unwrap_or(here)
}

/// Tells whether `route` may run for a call, which the router reads as
/// `POST /graphql`.
///
/// A middleware, method `ALL`, and a route of `POST` run where the path
/// matches. A pattern counts as matching where its fixed part before the
/// first wildcard, parameter or group allows `/graphql`, so a pattern the
/// module does not read keeps the app on its fetch.
fn meets_call(route: &RouteRecord) -> bool {
    if route.method != "ALL" && route.method != METHOD {
        return false;
    }
    let path = route.path.as_str();
    let fixed = path
        .split(['*', ':', '{', '(', '?'])
        .next()
        .unwrap_or_default();
    if fixed.len() == path.len() {
        return path.strip_suffix('/').unwrap_or(path) == GRAPHQL_PATH;
    }
    "/graphql/".starts_with(fixed) || fixed.starts_with(GRAPHQL_PATH)
}

/// Returns an answer of `status` with no media type and an empty body.
fn empty(status: u16) -> Answer {
    Answer {
        status,
        substatus: 0,
        content_type: None,
        body: Body::Full(Vec::new()),
    }
}

/// Returns the status of a fetch the app rejected with an error named
/// `name`, the rule of `handleFetchError`, which `handle_fetch_error` of the
/// Node adapter of `aosabi-serve` translates, a crate `aosabi-core` does not
/// take.
fn rejected(name: &str) -> u16 {
    if name == "TimeoutError" {
        TIMED_OUT
    } else {
        REJECTED
    }
}

/// Returns the Fetch request of the listener's request `call`:
/// `POST http://localhost/graphql` with the fields [`Request::fields`] names
/// and the decoded content as the body.
#[must_use]
pub fn request(call: &Request) -> Rc<aosabi_http::Request> {
    let mut headers = Headers::new();
    for (name, value) in call.fields() {
        // The values are the two fixed types and a credential of visible
        // ASCII, which the server of graphtp-homa checks by rule 6 of
        // docs/graphtp.md section 5.3, so each is text the platform takes as
        // a field, and a value past rule 6 travels as no credential does.
        if let Ok(value) = core::str::from_utf8(value) {
            let _ = headers.append(name, value);
        }
    }
    let body = aosabi_http::Body::Full(Rc::from(call.content.as_slice()));
    Rc::new(aosabi_http::Request::new(METHOD, URL, headers, body))
}

/// Returns what the app answered in `response`: its status, the substatus
/// the HTTP layer set on it, its `content-type` and its body.
///
/// The body is taken as it stands, whole bytes for a body the app wrote
/// whole and the stream's pieces for a stream, so the listener reads a
/// stream only while its queue has room.
#[must_use]
pub fn answer(response: &Response) -> Answer {
    answer_with(response, response.body())
}

/// Returns `response` with its body taken apart, as the fetch of the app
/// answers it.
fn with_body(response: Response) -> (Response, aosabi_http::Body) {
    let body = response.body();
    (response, body)
}

/// Returns what the app answered in `response` with the body `body`, which
/// stands in for the body of `response`, as [`answer`] reads them.
fn answer_with(response: &Response, body: aosabi_http::Body) -> Answer {
    let body = match body {
        aosabi_http::Body::Stream(stream) => Body::Stream(Box::new(Pieces(stream))),
        whole => Body::Full(
            whole
                .whole_bytes()
                .map(|bytes| bytes.to_vec())
                .unwrap_or_default(),
        ),
    };
    Answer {
        status: response.status(),
        substatus: response.substatus(),
        content_type: response.header("content-type").map(String::into_bytes),
        body,
    }
}

/// The pieces of a body the app writes as a stream.
struct Pieces(BodyStream);

impl Chunks for Pieces {
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<Vec<u8>, Broken>>> {
        self.0
            .poll_chunk(cx)
            .map(|piece| piece.map(|piece| piece.map_err(|_| Broken)))
    }
}
