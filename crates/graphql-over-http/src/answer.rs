// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the one function that answers a GraphQL request over HTTP.
 *
 * Derived from graphql-yoga 5.18.0, cjs/server.js, MIT.
 * Derived from whatwg-node 0.10.18, cjs/createServerAdapter.js, MIT.
 */

//! [`answer`], Yoga's five jobs as straight code.
//!
//! The function reads GET and POST JSON, and forms, `application/graphql` and
//! uploads, it refuses a wrong method, a mutation over GET, broken parameters,
//! an unknown body and a batch, it answers with the status and the type the
//! client accepts, JSON or an event stream and never `multipart/mixed`, the
//! named divergence D59, it masks unexpected errors, and it caches parsed and
//! validated documents. Each step stands where its plugin stood in Yoga's
//! list, so every hook of an app's plugin runs at Yoga's point and in Yoga's
//! order:
//!
//! | Phase | Steps in order |
//! | --- | --- |
//! | `onRequest` | the health check, the fall through of an `OPTIONS` request, the page of `GraphiQL`, the hooks of the app, the unhandled route |
//! | the request parse | the request parser the method and the type choose, the hooks of the app, the check of the method, the parse, the done hooks of the app, the refusal of a batch |
//! | `onParams` | the hooks of the app, the check of the parameters |
//! | the run | the hook layer of `aosabi-hooks`, then `onExecutionResult` of the app |
//! | `onResultProcess` | the choice of the result processor, the hooks of the app, the processor |
//! | `onResponse` | the hooks of the app |
//!
//! A throw of the request parse, of the parameters or of the run is handled
//! into a result that is processed in turn, so a 415 and a 406 are the only
//! answers that pass the processors by (rule 3 of section YOGA of the old page
//! of the plan). A hook `onRequest` of the app may answer the request, replace
//! it or replace the request handler, [`HttpServer::handle`] by default, as
//! the server adapter allows.

use std::rc::Rc;

use aosabi_execute::{
    Completion,
    ErrorObject,
    PlainObject,
    Value,
    map_async_iterator,
    map_async_iterator::OnValue, //
};
use aosabi_hooks::{
    ExecutionOutcome,
    HookContext,
    LocalBoxFuture,
    plugin::ResultStream, //
};
use aosabi_http::{
    Request,
    Response, //
};
use aosabi_json::{
    JsString,
    case::upper_ascii_exact, //
};
use aosabi_lang::GraphQLError;

use crate::{
    checks::{
        check_graphql_query_params,
        is_valid_method_for_graphql,
        method_refusal,
        single_operation, //
    },
    error::{
        errors_result,
        handle_error, //
    },
    graphiql::{
        render_graphiql,
        should_render_graphiql, //
    },
    plugin::{
        GraphQLParams,
        HttpResult,
        OnExecutionResult,
        OnParams,
        OnRequestParse,
        OnRequestParseDone,
        OnResultProcess,
        ParamsHandler,
        entry, //
    },
    process_request::process_request,
    routes::matches_endpoint,
    server::HttpServer,
    server_adapter::{
        LazyUrl,
        NO_HEADERS,
        OnRequest,
        OnResponse,
        ResponseBody,
        empty_response,
        new_response, //
    }, //
};

/// Answers `request` with the server context `server_context`, `yoga.fetch(request, serverContext)`.
///
/// Every `onRequest` runs in order until one answers, the request handler runs
/// when none did, and every `onResponse` runs on whatever came out.
///
/// # Errors
///
/// The future answers the value a hook or the request handler throws, which
/// is the rejection of the fetch.
pub fn answer(
    server: &Rc<HttpServer>,
    request: Rc<Request>,
    server_context: HookContext,
) -> LocalBoxFuture<'static, Result<Response, Value>> {
    let server = Rc::clone(server);
    Box::pin(async move {
        let mut request = request;
        let mut request_handler = Rc::clone(&server.handler);
        let url = LazyUrl::new();
        let mut response = on_request_steps(&server, &request, &server_context, &url)?;
        if response.is_none() {
            for plugin in &server.plugins {
                plugin.on_request(&mut OnRequest {
                    request: &mut request,
                    server_context: &server_context,
                    url: &url,
                    request_handler: &mut request_handler,
                    response: &mut response,
                })?;
                if response.is_some() {
                    break;
                }
            }
        }
        // The unhandled route, `useUnhandledRoute`, after the hooks of the
        // app, which may answer a path of their own.
        if response.is_none() {
            let pathname = || url.get(&request.url).map(|url| url.pathname.clone());
            if !matches_endpoint(&request, pathname, &server.graphql_endpoint)? {
                response = Some(new_response(
                    ResponseBody::Text(Vec::new()),
                    404,
                    Some("Not Found"),
                    NO_HEADERS,
                )?);
            }
        }
        let mut response = match response {
            Some(response) => response,
            None => request_handler(Rc::clone(&request), server_context.clone()).await?,
        };
        for plugin in &server.plugins {
            plugin.on_response(&mut OnResponse {
                request: &request,
                server_context: &server_context,
                response: &mut response,
            })?;
        }
        Ok(response)
    })
}

/// Returns the answer of the steps of Yoga's list before the hooks `onRequest` of the app, or none.
///
/// The health check answers every method whose whole URL ends in its endpoint
/// with 200 and `x-yoga-id`, an `OPTIONS` request gets the empty 404 of the
/// unhandled route, which the mount hands on to the web server or the host,
/// where Yoga's CORS answers every one with 204 and `Content-Length: 0`, the
/// named divergence D46, and `GraphiQL` answers a GET at the endpoint that
/// accepts HTML with its page unless the option is `false`.
///
/// # Errors
///
/// Returns the `TypeError` of a URL the parser refuses or of a header the
/// response refuses.
fn on_request_steps(
    server: &HttpServer,
    request: &Rc<Request>,
    server_context: &HookContext,
    url: &LazyUrl,
) -> Result<Option<Response>, Value> {
    if request.url.ends_with(&server.health_check_endpoint) {
        server.logger.debug("Responding Health Check");
        let headers = [(String::from("x-yoga-id"), server.id.clone())];
        return empty_response(200, None, &headers).map(Some);
    }
    // The engine answers no `OPTIONS` request, which falls through to the web
    // server or the host as an unhandled route does, the named divergence D46.
    if upper_ascii_exact(&request.method) == "OPTIONS" {
        return new_response(
            ResponseBody::Text(Vec::new()),
            404,
            Some("Not Found"),
            NO_HEADERS,
        )
        .map(Some);
    }
    match &server.graphiql {
        Some(options) if should_render_graphiql(request) => {
            render_page(server, request, server_context, url, options)
        }
        _ => Ok(None),
    }
}

/// Returns the page of `GraphiQL` for `request`, which asks for it, where its
/// path is the endpoint and the options of the request are not `false`.
///
/// # Errors
///
/// Returns the `TypeError` of a URL the parser refuses or of a header the
/// response refuses.
// Out of line, so a request that asks for no page compiles none of it.
#[inline(never)]
fn render_page(
    server: &HttpServer,
    request: &Rc<Request>,
    server_context: &HookContext,
    url: &LazyUrl,
    options: &crate::graphiql::GraphiqlOptionsFactory,
) -> Result<Option<Response>, Value> {
    let pathname = || url.get(&request.url).map(|url| url.pathname.clone());
    if !matches_endpoint(request, pathname, &server.graphql_endpoint)? {
        return Ok(None);
    }
    server.logger.debug("Rendering GraphiQL");
    let Some(options) = options(request, server_context) else {
        return Ok(None);
    };
    let body = render_graphiql(&options);
    let headers = [(String::from("Content-Type"), String::from("text/html"))];
    new_response(ResponseBody::Text(body.into_bytes()), 200, None, &headers).map(Some)
}

impl HttpServer {
    /// Answers `request`, Yoga's request handler, `handle(request, serverContext)`.
    ///
    /// The request parse, the run of the one operation, and the result
    /// processing, where every throw of the chain is handled into a result
    /// that is processed in turn. A batch is refused, the named divergence D58
    /// of `docs/byte-equality.md` section 3.
    pub fn handle(
        self: &Rc<Self>,
        request: Rc<Request>,
        server_context: HookContext,
    ) -> LocalBoxFuture<'static, Result<Response, Value>> {
        let server = Rc::clone(self);
        Box::pin(async move {
            match server.run(&request, &server_context).await {
                Ok(response) => Ok(response),
                Err(error) => {
                    let errors =
                        handle_error(error, server.masked_errors.as_deref(), &server.logger);
                    server.process_result(
                        &request,
                        HttpResult::Single(errors_result(errors)),
                        &server_context,
                    )
                }
            }
        })
    }

    /// Runs the chain of [`HttpServer::handle`] whose throw the handler handles.
    ///
    /// The request parse is `server.js:307-345`. The parsers of Yoga's list
    /// choose the parser before the hooks `onRequestParse` of the app, which
    /// may replace it, and the check of the method runs after them, since a
    /// plugin may serve more methods. A request no parser matched answers 415
    /// with its status text, no header and no body. The done hooks of the app
    /// run over the parsed parameters before the refusal of a batch.
    async fn run(
        self: &Rc<Self>,
        request: &Rc<Request>,
        server_context: &HookContext,
    ) -> Result<Response, Value> {
        let url = LazyUrl::new();
        let mut request_parser = self.parsers.choose(request);
        let mut done_list = Vec::new();
        for plugin in &self.plugins {
            let done = plugin.on_request_parse(&mut OnRequestParse {
                request,
                server_context,
                url: &url,
                request_parser: &mut request_parser,
            })?;
            if let Some(done) = done {
                done_list.push(done);
            }
        }
        if !is_valid_method_for_graphql(&request.method) {
            return Err(method_refusal());
        }
        self.logger
            .debug("Parsing request to extract GraphQL parameters");
        let Some(request_parser) = request_parser else {
            return empty_response(415, Some("Unsupported Media Type"), NO_HEADERS);
        };
        let mut request_parser_result = request_parser(Rc::clone(request)).await?;
        for done in done_list {
            done(&mut OnRequestParseDone {
                request_parser_result: &mut request_parser_result,
            })?;
        }
        let params = single_operation(request_parser_result)?;
        let result = self
            .get_result_for_params(Rc::clone(request), params, server_context.clone())
            .await?;
        self.process_result(request, HttpResult::Single(result), server_context)
    }

    /// Runs the parameter hooks, the params handler and the result hooks, `getResultForParams({ params, request }, context)`.
    ///
    /// The check of the parameters runs after the hooks `onParams` of the app,
    /// over the parameters they leave.
    pub fn get_result_for_params(
        self: &Rc<Self>,
        request: Rc<Request>,
        params: GraphQLParams,
        context: HookContext,
    ) -> LocalBoxFuture<'static, Result<ExecutionOutcome, Value>> {
        let server = Rc::clone(self);
        Box::pin(async move {
            let mut params = params;
            let mut params_handler: Option<ParamsHandler> = None;
            let mut result = None;
            for plugin in &server.plugins {
                plugin.on_params(&mut OnParams {
                    params: &mut params,
                    request: &request,
                    context: &context,
                    params_handler: &mut params_handler,
                    result: &mut result,
                })?;
            }
            check_graphql_query_params(&params, server.extra_param_names.as_deref())?;
            let mut result = match (result, params_handler) {
                (Some(result), _) => result,
                (None, Some(handler)) => {
                    handler(Rc::clone(&request), params, context.clone()).await?
                }
                (None, None) => {
                    server
                        .handle_params(Rc::clone(&request), params, context.clone())
                        .await?
                }
            };
            for plugin in &server.plugins {
                plugin.on_execution_result(&mut OnExecutionResult {
                    result: &mut result,
                    request: &request,
                    context: &context,
                })?;
            }
            Ok(result)
        })
    }

    /// Runs the operation of `params`, `handleParams({ request, context, params })`.
    ///
    /// The context takes the request and the parameters, the enveloped
    /// functions are made over it, and a throw of the run becomes the result
    /// of the handled errors. A throw of `onEnveloped` escapes. A stream is
    /// mapped so that an error it fails with becomes one more result of the
    /// handled errors, `server.js:260-271`.
    pub fn handle_params(
        self: &Rc<Self>,
        request: Rc<Request>,
        params: GraphQLParams,
        context: HookContext,
    ) -> LocalBoxFuture<'static, Result<ExecutionOutcome, Value>> {
        let server = Rc::clone(self);
        Box::pin(async move {
            let same_request = context
                .get_as::<Request>("request")
                .is_some_and(|known| Rc::ptr_eq(&known, &request));
            if !same_request {
                context.extend("request", entry(Rc::clone(&request)));
            }
            // The context holds the parameters the run reads, as the reference
            // stores a reference to them.
            let params = Rc::new(params);
            context.extend("params", entry(Rc::clone(&params)));
            let hooked = server.get_hooked(context)?;
            server.logger.debug("Processing GraphQL Parameters");
            let mut result = match process_request(&params, &hooked, &server.agent).await {
                Ok(result) => {
                    server.logger.debug("Processing GraphQL Parameters done.");
                    result
                }
                Err(error) => errors_result(handle_error(
                    error,
                    server.masked_errors.as_deref(),
                    &server.logger,
                )),
            };
            if let ExecutionOutcome::Stream(stream) = &mut result {
                server.map_stream_errors(stream);
            }
            Ok(result)
        })
    }

    /// Maps the errors of `stream` to results, the stream half of `handleParams`.
    ///
    /// An error whose `name` is `AbortError` is logged at the debug level and
    /// thrown on, and any other error goes through `handleError`, whose
    /// masking writes the original error through `logger.error`, and becomes
    /// the result `{ errors }`. The helper keeps the first answer of `onError`
    /// for every later rejection.
    // Out of line, so a server compiles the mapping of a stream where an operation answers one.
    #[inline(never)]
    fn map_stream_errors(&self, stream: &mut ResultStream) {
        let masked_errors = self.masked_errors.clone();
        let logger = self.logger;
        let on_next: OnValue = Rc::new(|_, value| Completion::Normal(value));
        let on_error: OnValue = Rc::new(move |_, error| {
            match error_name(&error) {
                Err(type_error) => return Completion::Throw(type_error),
                Ok(Some(name)) if name == "AbortError" => {
                    logger.debug("Request aborted");
                    return Completion::Throw(error);
                }
                Ok(_) => {}
            }
            let errors = handle_error(error, masked_errors.as_deref(), &logger);
            Completion::Normal(errors_value(errors))
        });
        stream.wrap(|agent, iterable| {
            map_async_iterator(agent, iterable, on_next, Some(on_error), None)
        });
    }

    /// Answers `result` through the result processor the steps and the hooks choose, `processResult({ request, result, ... })`.
    ///
    /// The result processors of Yoga's list choose before the hooks
    /// `onResultProcess` of the app, which may replace the choice or the
    /// result, and no choice answers 406 with the acceptable media types.
    ///
    /// # Errors
    ///
    /// Returns the value a hook or the processor throws.
    pub fn process_result(
        &self,
        request: &Rc<Request>,
        result: HttpResult,
        server_context: &HookContext,
    ) -> Result<Response, Value> {
        let mut result = result;
        let mut result_processor = None;
        let mut acceptable_media_types = Vec::new();
        self.processors.choose(
            request,
            &result,
            &mut acceptable_media_types,
            &mut result_processor,
        );
        for plugin in &self.plugins {
            plugin.on_result_process(&mut OnResultProcess {
                request,
                acceptable_media_types: &mut acceptable_media_types,
                result: &mut result,
                result_processor: &mut result_processor,
                server_context,
            })?;
        }
        // If no result processor found for this result, return an error.
        let Some((processor, accepted)) = result_processor else {
            let accept = acceptable_media_types.join("; charset=utf-8, ");
            return empty_response(
                406,
                Some("Not Acceptable"),
                &[(String::from("accept"), accept)],
            );
        };
        processor(&result, &accepted)
    }
}

/// Returns `error.name`, or the `TypeError` of the read off `undefined` or `null`.
///
/// An error and an object answer their `name` when it is a string, and any
/// other value answers none, which never equals `AbortError`.
fn error_name(error: &Value) -> Result<Option<JsString>, Value> {
    match error {
        Value::Undefined | Value::Null => {
            let target = if matches!(error, Value::Null) {
                "null"
            } else {
                "undefined"
            };
            Err(Value::from(ErrorObject::type_error(format!(
                "Cannot read properties of {target} (reading 'name')"
            ))))
        }
        Value::Error(object) => Ok(Some(object.name())),
        Value::Object(object) => match Rc::clone(object).get("name") {
            Value::String(name) => Ok(Some(name)),
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

/// Returns the result `{ errors }` of a stream as a value of the agent.
fn errors_value(errors: Vec<GraphQLError>) -> Value {
    let errors: Vec<Value> = errors
        .into_iter()
        .map(|error| Value::from(ErrorObject::GraphQL(error)))
        .collect();
    Value::from(PlainObject::from_entries(vec![(
        String::from("errors"),
        Value::from(errors),
    )]))
}
