// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-sentry: the plugin that reports the errors of a GraphQL execution to
 * Sentry.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/plugins/use-sentry.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `use_sentry`, `SentryPluginOptions` and `default_skip_error`.
//!
//! `useSentry(options)` is the plugin named `sentry`. It declares no strategy
//! and no roles, takes its middleware from `@hono/sentry` with every option
//! but the twelve keys of the GraphQL half, and reports the errors of an
//! execution in the done hook of its `onExecute`. So its members are `name`,
//! `middleware` and `onExecute`.
//!
//! The done hook opens a span that records nothing, and when the result
//! carries errors it forks the scope, writes the transaction name, the tags
//! and the extras, and for every error that `skipError` does not skip adds an
//! `execution-path` breadcrumb, captures the original error with its
//! fingerprint and its `GraphQL` context and writes the event id into the
//! extensions of the error. The id is written whether or not a client exists,
//! since the hub mints it before it looks for one, and it is appended after
//! every key the extensions hold already, as a property write on a plain
//! object appends it. The reference replaces the errors with the mapped list
//! whenever the result has errors, and the map writes into the same error
//! objects, so the port writes into the errors in place.
//!
//! The plugin calls no `init`, so the GraphQL half sends an event only where
//! the application called `init` of [`crate::hub`], and the middleware sends
//! the error of a route only where its options or its environment name a DSN.

use std::rc::Rc;

use aosabi_core::{
    IntoPlugin,
    Plugin,
    PluginObject,
    execute::{ExecutionArgs, VariableValues, errors::ThrownError},
    on_execute_done,
    plugin::object::__wrap, //
};
use aosabi_hooks::{
    context::HookContext,
    document_string_map::get_document_string,
    plugin::{
        OnExecute,
        OnExecuteDone,
        OnExecuteDoneHookResult,
        OnExecuteHooks, //
    },
    plugins::use_masked_errors::is_original,
    utils::handle_stream_or_single_execution_result,
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    DocumentNode,
    GraphQLError,
    OriginalError,
    PathSegment,
    ast::{
        DefinitionNode,
        NameNode, //
    },
    js_string::to_string_lossy,
    printer::print, //
};

use crate::{
    envelope::{
        Breadcrumb,
        Exception, //
    },
    hub::{
        CaptureContext,
        ClientOptions,
        Scope,
        SpanOptions,
        start_span_manual,
        with_current_scope,
        with_scope, //
    },
    sentry, //
};

/// The name of the plugin, `name: 'sentry'`.
const PLUGIN_NAME: &str = "sentry";

/// The key of the event id in the extensions of an error, the only key the
/// reference writes.
const EVENT_ID_KEY: &str = "sentryEventId";

/// The twelve keys of the GraphQL half, which the middleware never sees.
pub const GRAPHQL_KEYS: [&str; 12] = [
    "startTransaction",
    "renameTransaction",
    "includeRawResult",
    "includeExecuteVariables",
    "eventIdKey",
    "appendTags",
    "configureScope",
    "transactionName",
    "traceparentData",
    "operationName",
    "skip",
    "skipError",
];

/// `eventIdKey?: string | null`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventIdKey {
    /// A string, which the reference ignores.
    Key(String),
    /// `null`, which turns the event id off.
    Null,
}

/// The arguments of an execution a callback option reads, `TypedExecutionArgs`.
#[derive(Clone, Copy)]
pub struct ExecuteArgs<'a> {
    /// `document`.
    pub document: &'a DocumentNode,
    /// `operationName`.
    pub operation_name: Option<&'a str>,
    /// `variableValues`, when they are an object.
    pub variable_values: Option<&'a JsObject>,
    /// The arguments as Envelop holds them, of which a host of the
    /// TypeScript option makes the view a callback of its app reads.
    pub execution: &'a ExecutionArgs,
    /// The context of the execution, which that view reads the parameters of
    /// the request from.
    pub context: &'a HookContext,
}

impl core::fmt::Debug for ExecuteArgs<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExecuteArgs")
            .field("operation_name", &self.operation_name)
            .field("variable_values", &self.variable_values)
            .finish_non_exhaustive()
    }
}

/// A callback option over the arguments of an execution.
pub type ArgsFn<T> = Box<dyn Fn(&ExecuteArgs<'_>) -> T>;

/// The option `configureScope`.
pub type ConfigureScopeFn = Box<dyn Fn(&ExecuteArgs<'_>, &mut Scope)>;

/// The option `skipError`.
pub type SkipErrorFn = Box<dyn Fn(&GraphQLError) -> bool>;

/// The options of [`use_sentry`], `SentryPluginOptions`.
///
/// The twelve keys of the GraphQL half come first, each `None` for the
/// reference default, and then the keys of the middleware this port carries.
/// A key of Toucan the port does not carry has no field, so naming one fails
/// to compile.
#[derive(Default)]
pub struct SentryPluginOptions {
    /// `startTransaction`, `true` by default, which no span of the port reads.
    pub start_transaction: Option<bool>,
    /// `renameTransaction`, `false` by default.
    pub rename_transaction: Option<bool>,
    /// `includeRawResult`, `false` by default, the result as an extra.
    pub include_raw_result: Option<bool>,
    /// `includeExecuteVariables`, `false` by default, the variables as an extra.
    pub include_execute_variables: Option<bool>,
    /// `eventIdKey`.
    pub event_id_key: Option<EventIdKey>,
    /// `appendTags`, tags beside `operationName` and `operation`.
    pub append_tags: Option<ArgsFn<JsObject>>,
    /// `configureScope`, called with the current scope before the execution.
    pub configure_scope: Option<ConfigureScopeFn>,
    /// `transactionName`, the operation name by default.
    pub transaction_name: Option<ArgsFn<String>>,
    /// `traceparentData`, which no span of the port reads.
    pub traceparent_data: Option<ArgsFn<Option<JsObject>>>,
    /// `operationName`, the `op` of the span, `execute` by default.
    pub operation_name: Option<ArgsFn<String>>,
    /// `skip`, which leaves an execution out of the whole flow.
    pub skip: Option<ArgsFn<bool>>,
    /// `skipError`, [`default_skip_error`] by default.
    pub skip_error: Option<SkipErrorFn>,
    /// `dsn` of the middleware.
    pub dsn: Option<String>,
    /// `environment` of the middleware.
    pub environment: Option<String>,
    /// `release` of the middleware.
    pub release: Option<String>,
    /// `sampleRate` of the middleware.
    pub sample_rate: Option<f64>,
}

impl core::fmt::Debug for SentryPluginOptions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SentryPluginOptions")
            .field("event_id_key", &self.event_id_key)
            .field("dsn", &self.dsn)
            .field("environment", &self.environment)
            .field("release", &self.release)
            .field("sample_rate", &self.sample_rate)
            .finish_non_exhaustive()
    }
}

/// The plugin `useSentry` returns, named `sentry`.
///
/// It turns into a plugin object through [`IntoPlugin`], as the identity
/// plugin does, with its middleware and its `onExecute`.
#[derive(Clone, Debug)]
pub struct SentryPlugin {
    /// The options.
    options: Rc<SentryPluginOptions>,
}

impl IntoPlugin for SentryPlugin {
    fn into_plugin(self) -> Rc<dyn Plugin> {
        let options = self.options;
        let middleware = ClientOptions {
            dsn: options.dsn.clone(),
            environment: options.environment.clone(),
            release: options.release.clone(),
            sample_rate: options.sample_rate,
        };
        Rc::new(PluginObject {
            name: __wrap::name(PLUGIN_NAME),
            middleware: Some(sentry(middleware).into()),
            on_execute: __wrap::on_execute(move |execute: &mut OnExecute<'_>| {
                on_execute(&options, execute)
            }),
            ..PluginObject::default()
        })
    }
}

/// Returns the plugin named `sentry`, `useSentry(options)`.
#[must_use]
pub fn use_sentry(options: SentryPluginOptions) -> SentryPlugin {
    SentryPlugin {
        options: Rc::new(options),
    }
}

/// Returns whether `error` is an original GraphQL error, which the plugin
/// skips by default, `defaultSkipError`, the `isOriginalGraphQLError` of
/// envelop.
#[must_use]
pub fn default_skip_error(error: &GraphQLError) -> bool {
    is_original(error)
}

/// What the hook `onExecute` computed for its done hook.
struct Execution {
    /// The options of the plugin.
    options: Rc<SentryPluginOptions>,
    /// The `op` of the span.
    op: String,
    /// The operation name, `Anonymous Operation` for none.
    op_name: String,
    /// `query`, `mutation` or `subscription`.
    operation_type: &'static str,
    /// The source of the document.
    document: String,
    /// The tags of `appendTags`.
    added_tags: JsObject,
    /// The attributes of the span.
    tags: JsObject,
    /// The name of the transaction.
    transaction_name: String,
}

/// The hook `onExecute` of the plugin, `use-sentry.ts:146-178`.
fn on_execute(
    options: &Rc<SentryPluginOptions>,
    execute: &mut OnExecute<'_>,
) -> Option<OnExecuteHooks> {
    let args: &ExecutionArgs = &*execute.args;
    let view = ExecuteArgs {
        document: &args.document,
        operation_name: args.operation_name.as_deref(),
        variable_values: match args.variables() {
            Some(JsValue::Object(values)) => Some(values),
            _ => None,
        },
        execution: args,
        context: execute.context,
    };
    if options.skip.as_ref().is_some_and(|skip| skip(&view)) {
        return None;
    }
    // The first operation of the document, whichever one the request names.
    let root = view
        .document
        .definitions
        .iter()
        .find_map(|definition| match definition {
            DefinitionNode::OperationDefinition(operation) => Some(operation),
            _ => None,
        })?;
    let operation_type = root.operation.as_str();
    let document = get_document_string(&args.document, Some(print_document)).unwrap_or_default();
    let op_name = view
        .operation_name
        .filter(|name| !name.is_empty())
        .or_else(|| root.name.as_ref().map(NameNode::value))
        .filter(|name| !name.is_empty())
        .unwrap_or("Anonymous Operation");
    let op_name = String::from(op_name);
    let added_tags = options
        .append_tags
        .as_ref()
        .map(|append| append(&view))
        .unwrap_or_default();
    if let Some(traceparent) = &options.traceparent_data {
        // The data feeds a transaction, which the port never starts.
        let _ = traceparent(&view);
    }
    let transaction_name = options
        .transaction_name
        .as_ref()
        .map_or_else(|| op_name.clone(), |name| name(&view));
    let op = options
        .operation_name
        .as_ref()
        .map_or_else(|| String::from("execute"), |name| name(&view));
    let mut tags = JsObject::new();
    tags.insert("operationName", op_name.as_str());
    tags.insert("operation", operation_type);
    for (key, value) in added_tags.iter() {
        tags.insert(key.clone(), value.clone());
    }
    if let Some(configure) = &options.configure_scope {
        with_current_scope(|scope| configure(&view, scope));
    }
    let execution = Execution {
        options: Rc::clone(options),
        op,
        op_name,
        operation_type,
        document,
        added_tags,
        tags,
        transaction_name,
    };
    on_execute_done(move |done| execution.done(done))
}

impl Execution {
    /// The done hook, `use-sentry.ts:180-272`, which answers the hooks of a
    /// stream as `return handleStreamOrSingleExecutionResult(...)` does.
    fn done(self, payload: &mut OnExecuteDone<'_>) -> Option<OnExecuteDoneHookResult> {
        let variables = payload.args.variable_values.clone();
        handle_stream_or_single_execution_result(payload, move |result| {
            let variables = variables
                .as_deref()
                .map_or(&JsValue::Undefined, VariableValues::values);
            let span = SpanOptions {
                op: self.op.clone(),
                name: self.op_name.clone(),
                attributes: self.tags.clone(),
            };
            start_span_manual(&span, |span| {
                if self.options.rename_transaction == Some(true) {
                    span.update_name(&self.transaction_name);
                }
                span.set_attribute("document", &JsValue::from(self.document.as_str()));
                let include_raw_result = self.options.include_raw_result == Some(true);
                let raw = include_raw_result.then(|| result.to_js_value());
                if let Some(raw) = &raw {
                    span.set_attribute("result", raw);
                }
                if !result.errors.is_empty() {
                    with_scope(|scope| {
                        scope.set_transaction_name(self.op_name.as_str());
                        scope.set_tag("operation", self.operation_type);
                        scope.set_tag("operationName", self.op_name.as_str());
                        scope.set_extra("document", self.document.as_str());
                        scope.set_tags(&self.added_tags);
                        if let Some(raw) = raw {
                            scope.set_extra("result", raw);
                        }
                        if self.options.include_execute_variables == Some(true) {
                            scope.set_extra("variables", variables.clone());
                        }
                        for error in &mut result.errors {
                            self.report(scope, error, variables);
                        }
                    });
                }
                span.end();
            });
        })
    }

    /// Reports `error` under `scope` and writes its event id, the map at
    /// `use-sentry.ts:219-258`.
    fn report(&self, scope: &mut Scope, error: &mut GraphQLError, variables: &JsValue) {
        let skipped = match &self.options.skip_error {
            Some(skip) => skip(error),
            None => default_skip_error(error),
        };
        if skipped {
            return;
        }
        let path = error_path(error.path.as_deref().unwrap_or_default());
        if !path.is_empty() {
            scope.add_breadcrumb(Breadcrumb {
                category: Some(String::from("execution-path")),
                message: Some(path.clone()),
                level: Some(String::from("debug")),
                timestamp: None,
            });
        }
        let mut graphql = JsObject::new();
        graphql.insert("operationName", self.op_name.as_str());
        graphql.insert("operationType", self.operation_type);
        graphql.insert("variables", variables.clone());
        let mut contexts = JsObject::new();
        contexts.insert("GraphQL", JsValue::Object(graphql));
        let hint = CaptureContext {
            fingerprint: Some(vec![
                String::from("graphql"),
                path,
                self.op_name.clone(),
                String::from(self.operation_type),
            ]),
            contexts,
        };
        let exception = exception_of(error.original_error.as_ref());
        let event_id = scope.capture_exception(exception, &hint);
        if self.options.event_id_key != Some(EventIdKey::Null)
            && let Some(event_id) = event_id
        {
            error.extensions.insert(EVENT_ID_KEY, event_id);
        }
    }
}

/// Returns the path of an error joined with ` > `, every index written `$index`.
fn error_path(path: &[PathSegment]) -> String {
    let mut joined = String::new();
    for (at, segment) in path.iter().enumerate() {
        if at > 0 {
            joined.push_str(" > ");
        }
        match segment {
            PathSegment::Key(key) => joined.push_str(key),
            PathSegment::Index(_) => joined.push_str("$index"),
        }
    }
    joined
}

/// Returns the exception of the original error of a GraphQL error, as the
/// event builder of the SDK reads it.
///
/// An `Error` gives its name and its message, and a missing original error is
/// the value `undefined`, which the SDK wraps as an `Error` whose message is
/// `undefined`.
fn exception_of(original: Option<&OriginalError>) -> Exception {
    let (ty, value) = match original {
        None => (String::from("Error"), String::from("undefined")),
        Some(OriginalError::GraphQL(error)) => (
            String::from("GraphQLError"),
            to_string_lossy(&error.message).into_owned(),
        ),
        Some(OriginalError::Other(error)) => match error.downcast_ref::<ThrownError>() {
            Some(thrown) => (
                to_string_lossy(&thrown.name).into_owned(),
                to_string_lossy(&thrown.message).into_owned(),
            ),
            None => (String::from("Error"), error.to_string()),
        },
    };
    Exception { ty, value }
}

/// Prints `document`, the `print` of `getDocumentString(document, print)`.
fn print_document(document: &DocumentNode) -> String {
    print(document)
}
