// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the plugin type of Yoga with the payload of every hook.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/types.js, MIT.
 */

//! `HttpPlugin` and the payloads of its hooks.
//!
//! A plugin of an app is a plugin of the hook layer and of the server adapter
//! with five hooks of Yoga, `onYogaInit`, `onRequestParse`, `onParams`,
//! `onExecutionResult` and `onResultProcess`, which the function reads off
//! the array the hook layer answers after its splices, as Yoga collects them,
//! and calls at Yoga's points. A hook it lacks has a body that does nothing,
//! as for envelop. Every hook of Yoga and of the app answers at once, so the
//! hooks here answer at once too, and the request parser and the params
//! handler, which read a body and run an operation, answer futures.
//!
//! This file also holds the values the hooks pass: the parameters of an
//! operation, the result of a request parser and the result of a request.

use core::any::Any;
use std::{
    borrow::Cow,
    rc::Rc,
    sync::Arc, //
};

use aosabi_execute::{
    SharedVariables,
    Value,
    VariableValues, //
};
use aosabi_hooks::{
    ExecutionOutcome,
    HookContext,
    HookPlugin,
    LocalBoxFuture, //
};
use aosabi_http::{
    Request,
    Response,
    fetch::url::Url, //
};
use aosabi_json::{
    JsString,
    JsValue, //
};

use crate::{
    file::FileBody,
    server::HttpServer,
    server_adapter::{
        LazyUrl,
        ServerAdapterPlugin, //
    }, //
};

/// The name of the operation a request names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationName<'a> {
    /// No name, `operationName == null`.
    Absent,
    /// A name.
    Name(&'a str),
    /// A value that is no string, which matches no operation of a document.
    Unmatchable,
}

/// A file of a multipart request at the key path its `map` names.
#[derive(Clone, Debug)]
pub struct UploadedFile {
    /// The key path into the parameters, `variables.file` for one.
    pub path: String,
    /// The file.
    pub file: Arc<FileBody>,
}

/// The parameters of one operation, `GraphQLParams`, as a request parser answers them.
///
/// The value is what the parser made, an object for a well formed request and
/// any JSON value otherwise, because the parameter check refuses the rest
/// after the parse. A file of a multipart request sits at its key path as a
/// host object the value carries through unread, which `JSON.stringify`
/// writes as `{}`, as it writes a web `File`, and the resolver reads the file
/// through it. The list `files` holds each file with its key path as well.
#[derive(Clone, Debug, Default)]
pub struct GraphQLParams {
    /// The parameters.
    pub value: JsValue,
    /// The files of a multipart request, by key path.
    pub files: Vec<UploadedFile>,
}

impl GraphQLParams {
    /// Creates the parameters `value` with no file.
    #[must_use]
    pub fn new(value: JsValue) -> Self {
        Self {
            value,
            files: Vec::new(),
        }
    }

    /// Returns `params[key]`, `None` for a missing key and for a value that is no object.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&JsValue> {
        match &self.value {
            JsValue::Object(object) => object.get(key),
            _ => None,
        }
    }

    /// Returns `params.query` when it is a string.
    #[must_use]
    pub fn query(&self) -> Option<&JsString> {
        match self.get("query") {
            Some(JsValue::String(query)) => Some(query),
            _ => None,
        }
    }

    /// Returns `params.operationName`.
    #[must_use]
    pub fn operation_name(&self) -> OperationName<'_> {
        match self.get("operationName") {
            None | Some(JsValue::Undefined | JsValue::Null) => OperationName::Absent,
            Some(JsValue::String(name)) => name
                .as_str()
                .map_or(OperationName::Unmatchable, OperationName::Name),
            Some(_) => OperationName::Unmatchable,
        }
    }

    /// Returns `params.variables` unless it is `undefined` or `null`.
    #[must_use]
    pub fn variables(&self) -> Option<&JsValue> {
        self.get("variables")
            .filter(|value| !matches!(value, JsValue::Undefined | JsValue::Null))
    }
}

/// The parameters hold the raw variables of their operation, which the
/// arguments of its execution share by `Rc` with the context of the request,
/// as the reference hands the executor `params.variables` itself.
impl VariableValues for GraphQLParams {
    fn values(&self) -> &JsValue {
        self.variables().unwrap_or(&JsValue::Undefined)
    }

    /// Keeps the variables, which the context reads as part of the parameters.
    fn take(self: Rc<Self>) -> Result<JsValue, SharedVariables> {
        Err(self)
    }
}

/// What a request parser answers, one set of parameters or the array of a batch.
#[derive(Clone, Debug)]
pub enum RequestParserResult {
    /// The parameters of one operation.
    Single(GraphQLParams),
    /// A batch, `Array.isArray(requestParserResult)`.
    Batch(Vec<GraphQLParams>),
}

impl RequestParserResult {
    /// Returns the result of the parsed `value`, a batch when it is an array.
    ///
    /// A file whose key path starts with the index of an entry of a batch
    /// goes to that entry with the rest of its path.
    #[must_use]
    pub fn from_value(value: JsValue, files: Vec<UploadedFile>) -> Self {
        match value {
            JsValue::Array(items) => {
                let mut batch: Vec<GraphQLParams> =
                    items.into_iter().map(GraphQLParams::new).collect();
                for file in files {
                    let (head, rest) = file
                        .path
                        .split_once('.')
                        .unwrap_or((file.path.as_str(), ""));
                    if let Ok(index) = head.parse::<usize>()
                        && let Some(entry) = batch.get_mut(index)
                    {
                        entry.files.push(UploadedFile {
                            path: String::from(rest),
                            file: file.file,
                        });
                    }
                }
                Self::Batch(batch)
            }
            value => Self::Single(GraphQLParams { value, files }),
        }
    }
}

/// The result of a request, one result or the array of a batch.
#[derive(Debug)]
pub enum HttpResult {
    /// The result of one operation.
    Single(ExecutionOutcome),
    /// The results of a batch in the order of its entries.
    Batch(Vec<ExecutionOutcome>),
}

/// A request parser, `(request) => params`.
pub type RequestParser =
    Rc<dyn Fn(Rc<Request>) -> LocalBoxFuture<'static, Result<RequestParserResult, Value>>>;

/// A result processor, `(result, fetchAPI, acceptedMediaType) => response`.
pub type ResultProcessor = Rc<dyn Fn(&HttpResult, &str) -> Result<Response, Value>>;

/// A params handler, `({ request, params, context }) => result`, which answers the result of the operation.
///
/// Yoga's own handler answers a throw of the run as a result and rejects only
/// with a throw of `onEnveloped`.
pub type ParamsHandler = Rc<
    dyn Fn(
        Rc<Request>,
        GraphQLParams,
        HookContext,
    ) -> LocalBoxFuture<'static, Result<ExecutionOutcome, Value>>,
>;

/// The done hook of `onRequestParse`.
pub type OnRequestParseDoneHook = Box<dyn FnOnce(&mut OnRequestParseDone<'_>) -> Result<(), Value>>;

/// A plugin of Yoga.
pub trait HttpPlugin: HookPlugin + ServerAdapterPlugin {
    /// The hook `onYogaInit`, called once as the server is made.
    fn on_yoga_init(&self, _p: &mut OnYogaInit<'_>) {}

    /// The hook `onRequestParse`, which may set the parser of the request and answer a done hook.
    ///
    /// # Errors
    ///
    /// Returns the value the hook throws.
    fn on_request_parse(
        &self,
        _p: &mut OnRequestParse<'_>,
    ) -> Result<Option<OnRequestParseDoneHook>, Value> {
        Ok(None)
    }

    /// The hook `onParams`, which may replace the parameters, the params handler or the result.
    ///
    /// # Errors
    ///
    /// Returns the value the hook throws.
    fn on_params(&self, _p: &mut OnParams<'_>) -> Result<(), Value> {
        Ok(())
    }

    /// The hook `onExecutionResult`, which may replace the result of an operation.
    ///
    /// # Errors
    ///
    /// Returns the value the hook throws.
    fn on_execution_result(&self, _p: &mut OnExecutionResult<'_>) -> Result<(), Value> {
        Ok(())
    }

    /// The hook `onResultProcess`, which may choose the result processor.
    ///
    /// # Errors
    ///
    /// Returns the value the hook throws.
    fn on_result_process(&self, _p: &mut OnResultProcess<'_>) -> Result<(), Value> {
        Ok(())
    }

    /// Returns the names of the hooks the plugin object of the reference carries.
    ///
    /// A hook is a key the object holds, `key in plugin`, and the names come
    /// in the order of envelop, the server adapter and Yoga of
    /// `docs/plugins.md` sections 3.1 to 3.3. The default is none.
    fn hooks(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

/// The payload of `onYogaInit`.
pub struct OnYogaInit<'a> {
    /// The server, `yoga`.
    pub yoga: &'a HttpServer,
}

/// The payload of `onRequestParse`.
pub struct OnRequestParse<'a> {
    /// The request.
    pub request: &'a Rc<Request>,
    /// The server context of the request.
    pub server_context: &'a HookContext,
    /// The URL of the request.
    pub(crate) url: &'a LazyUrl,
    /// The parser a hook set.
    pub(crate) request_parser: &'a mut Option<RequestParser>,
}

impl OnRequestParse<'_> {
    /// Returns the URL of the request, `url`.
    ///
    /// # Errors
    ///
    /// Returns the `TypeError` of a URL the parser refuses.
    pub fn url(&self) -> Result<&Url, Value> {
        self.url.get(&self.request.url)
    }

    /// Returns the parser an earlier hook set, `requestParser`.
    #[must_use]
    pub fn request_parser(&self) -> Option<&RequestParser> {
        self.request_parser.as_ref()
    }

    /// Sets the parser of the request, `setRequestParser(parser)`.
    pub fn set_request_parser(&mut self, parser: RequestParser) {
        *self.request_parser = Some(parser);
    }
}

/// The payload of the done hook of `onRequestParse`.
pub struct OnRequestParseDone<'a> {
    /// The parsed parameters, which `setRequestParserResult` replaces.
    pub request_parser_result: &'a mut RequestParserResult,
}

impl OnRequestParseDone<'_> {
    /// Replaces the parsed parameters, `setRequestParserResult(result)`.
    pub fn set_request_parser_result(&mut self, result: RequestParserResult) {
        *self.request_parser_result = result;
    }
}

/// The payload of `onParams`.
pub struct OnParams<'a> {
    /// The parameters, which `setParams` replaces.
    pub params: &'a mut GraphQLParams,
    /// The request.
    pub request: &'a Rc<Request>,
    /// The context of the operation.
    pub context: &'a HookContext,
    /// The params handler a hook set, `None` for Yoga's own.
    pub(crate) params_handler: &'a mut Option<ParamsHandler>,
    /// The result a hook set, which skips the handler.
    pub(crate) result: &'a mut Option<ExecutionOutcome>,
}

impl OnParams<'_> {
    /// Replaces the parameters, `setParams(params)`.
    pub fn set_params(&mut self, params: GraphQLParams) {
        *self.params = params;
    }

    /// Returns the params handler a hook set, `None` for Yoga's own, `paramsHandler`.
    #[must_use]
    pub fn params_handler(&self) -> Option<&ParamsHandler> {
        self.params_handler.as_ref()
    }

    /// Replaces the params handler, `setParamsHandler(handler)`.
    pub fn set_params_handler(&mut self, handler: ParamsHandler) {
        *self.params_handler = Some(handler);
    }

    /// Sets the result, which skips the params handler, `setResult(result)`.
    pub fn set_result(&mut self, result: ExecutionOutcome) {
        *self.result = Some(result);
    }
}

/// The payload of `onExecutionResult`.
pub struct OnExecutionResult<'a> {
    /// The result of the operation, which `setResult` replaces.
    pub result: &'a mut ExecutionOutcome,
    /// The request.
    pub request: &'a Rc<Request>,
    /// The context of the operation.
    pub context: &'a HookContext,
}

impl OnExecutionResult<'_> {
    /// Replaces the result, `setResult(result)`.
    pub fn set_result(&mut self, result: ExecutionOutcome) {
        *self.result = result;
    }
}

/// The payload of `onResultProcess`.
pub struct OnResultProcess<'a> {
    /// The request.
    pub request: &'a Rc<Request>,
    /// The media types the processors offer, which a hook appends to.
    pub acceptable_media_types: &'a mut Vec<Cow<'static, str>>,
    /// The result, which `setResult` replaces.
    pub result: &'a mut HttpResult,
    /// The processor a hook chose with its media type.
    pub(crate) result_processor: &'a mut Option<(ResultProcessor, Cow<'static, str>)>,
    /// The server context of the request.
    pub server_context: &'a HookContext,
}

impl OnResultProcess<'_> {
    /// Replaces the result, `setResult(result)`.
    pub fn set_result(&mut self, result: HttpResult) {
        *self.result = result;
    }

    /// Returns the processor an earlier hook chose, `resultProcessor`.
    #[must_use]
    pub fn result_processor(&self) -> Option<&ResultProcessor> {
        self.result_processor
            .as_ref()
            .map(|(processor, _)| processor)
    }

    /// Chooses `processor` for the media type `accepted`, `setResultProcessor(processor, acceptedMediaType)`.
    pub fn set_result_processor(
        &mut self,
        processor: ResultProcessor,
        accepted: impl Into<Cow<'static, str>>,
    ) {
        *self.result_processor = Some((processor, accepted.into()));
    }
}

/// Returns the thin address of `plugin`, which tells two handles of one plugin apart from any other.
#[must_use]
pub(crate) fn address<T: ?Sized>(plugin: &Rc<T>) -> *const () {
    Rc::as_ptr(plugin).cast::<()>()
}

/// Returns `value` as the context entry `Rc<dyn Any>`.
#[must_use]
pub(crate) fn entry<T: 'static>(value: Rc<T>) -> Rc<dyn Any> {
    value
}
