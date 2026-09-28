// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the options of the function and the state it reads on every request.
 *
 * Derived from graphql-yoga 5.18.0, cjs/server.js, MIT.
 */

//! [`HttpOptions`] and [`HttpServer`].
//!
//! [`HttpServer::new`] reads the options once, the way Yoga's constructor
//! reads them: the logger, the masking, the endpoint, the options of
//! `GraphiQL`, the request parsers and the result processors. The option
//! `batching` has no reading, the named divergence D58 of
//! `docs/byte-equality.md` section 3, so the function refuses every batch as
//! Yoga does with batching off. It hands the hook layer of `aosabi-hooks` the
//! list of hooks Yoga hands envelop, with no plugin of its own list that has
//! no hook there:
//!
//! | Place in Yoga's list | Hook of the hook layer |
//! | --- | --- |
//! | 1 | the engine, graphql-js with `normalizedExecutor` or the single pass |
//! | 2 | the schema of the options, set in `onPluginInit` |
//! | 3 | the factory of the context of the options |
//! | 13 | the plugins of the app, in their order |
//! | 14 | the cache of documents and of their validation, unless the options turn it off |
//! | 19 | the stamp of a parse error and the refusal of a mutation over GET |
//! | 20 | the masking of an unexpected error, unless the options turn it off |
//! | 21 | the stamp of a validation error |
//!
//! The steps of Yoga's list that hold no hook of the hook layer, the health
//! check, `GraphiQL`, the five request parsers, the result processors, the
//! refusal of a batch, the checks of the parameters and of the method and the
//! unhandled route, are straight code of the function in [`crate::answer()`]
//! at the same places. The hooks of Yoga and of the server adapter an app's
//! plugin implements are read off the array the hook layer answers after its
//! splices, as Yoga reads them.
//!
//! Four options of the reference have no reading here, because the app passes
//! none of them: `fetchAPI`, `allowedHeaders`, `renderGraphiQL` and a schema
//! given as a promise or a factory. Yoga's own landing page is one more,
//! because the app passes `landingPage: false`, so the unhandled route always
//! answers its empty 404.

use std::{
    rc::{
        Rc,
        Weak, //
    },
    sync::Arc, //
};

use aosabi_cache::CacheBudget;
use aosabi_execute::{
    AgentOptions,
    ErrorObject,
    FieldResolverFn,
    LeafSerializer,
    TypeResolverFn,
    Value, //
};
use aosabi_hooks::{
    HookContext,
    HookPlugin,
    Hooked,
    Hooks,
    LocalBoxFuture,
    MaskErrorFn,
    MaskedErrorsOptions,
    hooks,
    plugins::{
        use_engine::{
            Engine,
            PassCounts,
            SinglePass,
            use_engine, //
        },
        use_extend_context::{
            ContextFactory,
            use_extend_context, //
        },
        use_masked_errors::use_masked_errors, //
    },
    use_schema, //
};
use aosabi_http::{
    Request,
    Response, //
};
use aosabi_type::{
    ObjMap,
    Schema, //
};
use aosabi_validate::pass::MAX_DEPTH;

use crate::{
    checks::{
        MutationCheck,
        ValidationStamp, //
    },
    error::{
        HttpMaskErrorFn,
        MaskedErrorsOpts, //
    },
    graphiql::{
        Graphiql,
        GraphiqlOptionsFactory, //
    },
    logger::{
        HttpLogger,
        LogLevel,
        create_logger, //
    },
    plugin::{
        HttpPlugin,
        OnYogaInit,
        address, //
    },
    plugins::{
        disable_introspection::NO_SCHEMA_INTROSPECTION_CUSTOM_RULE,
        use_parser_and_validation_cache::use_parser_and_validation_cache_with, //
    },
    request_parser::Parsers,
    result_processor::Processors,
    routes::DEFAULT_HEALTH_CHECK_ENDPOINT,
    server_adapter::RequestHandler,
    utils::mask_error::mask_error, //
};

/// The version of Yoga, `version`.
pub const VERSION: &str = "5.18.0";

/// The default endpoint, `/graphql`.
pub const DEFAULT_GRAPHQL_ENDPOINT: &str = "/graphql";

/// Aosabi only: whether a server validates through the single pass when its
/// options choose neither, on, verbatim "i want to decide between single pass
/// or conventional in build time but default is single pass".
pub const SINGLE_PASS_DEFAULT: bool = true;

/// The option `logging`.
#[derive(Clone, Copy, Debug)]
pub enum Logging {
    /// `true` for the default level, `false` for `silent`.
    Enabled(bool),
    /// A level.
    Level(LogLevel),
}

impl Default for Logging {
    fn default() -> Self {
        Self::Enabled(true)
    }
}

/// The options of masking, the object form of `maskedErrors`.
#[derive(Clone, Default)]
pub struct MaskedErrorsConfig {
    /// `maskError`, the default mask of Yoga when absent.
    pub mask_error: Option<HttpMaskErrorFn>,
    /// `errorMessage`, `Unexpected error.` when absent.
    pub error_message: Option<String>,
    /// `isDev`, read from `NODE_ENV` at each mask when absent.
    pub is_dev: Option<bool>,
}

/// The option `maskedErrors`.
#[derive(Clone, Default)]
pub enum MaskedErrors {
    /// Absent or `true`, the default masking.
    #[default]
    On,
    /// `false`, no masking.
    Off,
    /// An options object.
    Options(MaskedErrorsConfig),
}

/// The hook that prepares the agent of an operation.
pub type AgentHook = Rc<dyn Fn(&aosabi_execute::Agent)>;

/// The agent each operation runs on and the resolvers of the executor.
///
/// Aosabi only. The reference runs every operation on the one event loop of
/// the process with the resolvers bound in its schema, and here an operation
/// runs on an agent of its own made with `options` and handed to `setup`,
/// and the executor takes the resolvers the schema binds (rule 4 of
/// section YOGA of the old page of the plan).
#[derive(Clone, Default)]
pub struct AgentSetup {
    /// The options of the agent.
    pub options: AgentOptions,
    /// The hook that runs on a new agent before the operation.
    pub setup: Option<AgentHook>,
    /// The resolver of a field without one.
    pub field_resolver: Option<FieldResolverFn>,
    /// The resolver of an abstract type without one.
    pub type_resolver: Option<TypeResolverFn>,
    /// The subscribe resolver of a field without one.
    pub subscribe_field_resolver: Option<FieldResolverFn>,
    /// The serializers that see the executor value, by scalar name.
    pub leaf_serializers: Option<Arc<ObjMap<LeafSerializer>>>,
}

/// The options of Yoga, `YogaServerOptions`, as far as the app passes them.
#[derive(Clone, Default)]
pub struct HttpOptions {
    /// `id`, `yoga` when absent.
    pub id: Option<String>,
    /// `schema`.
    pub schema: Option<Arc<Schema>>,
    /// `context`, the factory of the extension of the context.
    pub context: Option<ContextFactory>,
    /// `logging`.
    pub logging: Logging,
    /// `maskedErrors`.
    pub masked_errors: MaskedErrors,
    /// `graphqlEndpoint`, `/graphql` when absent or empty.
    pub graphql_endpoint: Option<String>,
    /// `healthCheckEndpoint`, `/health` when absent.
    pub health_check_endpoint: Option<String>,
    /// `graphiql`.
    pub graphiql: Graphiql,
    /// `multipart`, which puts the multipart parser in only when `true`, the
    /// named divergence D49 of `docs/byte-equality.md` section 3, where Yoga
    /// puts it in unless the option is `false`.
    pub multipart: Option<bool>,
    /// `parserAndValidationCache`, which leaves the cache out when `false`.
    pub parser_and_validation_cache: Option<bool>,
    /// Aosabi only: the budget of the table of that cache, Yoga's 1,024
    /// entries with no bound in bytes when absent. The card a build serves
    /// sets it, and no answer depends on it.
    pub parser_and_validation_cache_budget: Option<CacheBudget>,
    /// `extraParamNames`.
    pub extra_param_names: Option<Vec<String>>,
    /// `plugins`, placed after the result processors.
    pub plugins: Vec<Rc<dyn HttpPlugin>>,
    /// Aosabi only: the agent of an operation and the resolvers of the executor.
    pub agent: AgentSetup,
    /// Aosabi only: whether the engine validates through the single pass,
    /// with the rules for every document the pass does not accept, or
    /// through the rules alone, [`SINGLE_PASS_DEFAULT`] when absent. Both
    /// answer every request alike.
    pub single_pass: Option<bool>,
    /// Aosabi only: the counters the engine of the single pass counts its
    /// verdicts and the gates that closed into, which only an instance that
    /// judges the pass sets. An instance that counts with the single pass off
    /// takes the engine of the single pass with its first gate closed, so
    /// every validation runs the rules and counts that gate.
    pub single_pass_counts: Option<Rc<PassCounts>>,
    /// Aosabi only: the most levels a document may nest before the single
    /// pass leaves it to the rules, the bound of the card a build serves, and
    /// `aosabi_validate::pass::MAX_DEPTH` when absent. No answer depends on it.
    pub single_pass_max_depth: Option<u32>,
}

/// The state the function reads on every request, the server of Yoga, `YogaServer`.
///
/// The fields are the options read once and the hook layer built once, and
/// the hook `onYogaInit` of an app's plugin receives the server.
pub struct HttpServer {
    /// `id`.
    pub(crate) id: String,
    /// The logger, `logger`.
    pub(crate) logger: HttpLogger,
    /// `graphqlEndpoint`.
    pub(crate) graphql_endpoint: String,
    /// `healthCheckEndpoint`, which the health check answers.
    pub(crate) health_check_endpoint: String,
    /// The options of the page of `GraphiQL` of a request, none when the option `graphiql` is `false`.
    pub(crate) graphiql: Option<GraphiqlOptionsFactory>,
    /// The request parsers.
    pub(crate) parsers: Parsers,
    /// The result processors.
    pub(crate) processors: Processors,
    /// `extraParamNames`.
    pub(crate) extra_param_names: Option<Vec<String>>,
    /// The app's plugins after the splices of the hook layer, `plugins`.
    pub(crate) plugins: Vec<Rc<dyn HttpPlugin>>,
    /// The hook layer, `getEnveloped`.
    pub(crate) hooks: Hooks,
    /// The masking options, `maskedErrorsOpts`.
    pub(crate) masked_errors: Option<Rc<MaskedErrorsOpts>>,
    /// The agent of an operation.
    pub(crate) agent: AgentSetup,
    /// The request handler a request runs unless a hook `onRequest` replaces it, [`HttpServer::handle`].
    pub(crate) handler: RequestHandler,
}

impl HttpServer {
    /// Creates the server of `options`, `new YogaServer(options)`.
    ///
    /// The hook `onYogaInit` of every plugin of the app runs once the server
    /// stands, in the order of the plugins.
    #[must_use]
    pub fn new(options: HttpOptions) -> Rc<Self> {
        let logger = match options.logging {
            Logging::Enabled(true) => create_logger(None),
            Logging::Enabled(false) => create_logger(Some(LogLevel::Silent)),
            Logging::Level(level) => create_logger(Some(level)),
        };
        let masked_errors = masked_errors_opts(&options.masked_errors, logger);
        let hooks = hooks(hook_list(&options, masked_errors.as_ref()));
        let plugins = app_plugins(&hooks, &options.plugins);
        let graphiql = crate::graphiql::options_factory(&options.graphiql);
        let HttpOptions {
            id,
            graphql_endpoint,
            health_check_endpoint,
            multipart,
            extra_param_names,
            agent,
            ..
        } = options;
        let server = Rc::new_cyclic(|weak: &Weak<Self>| Self {
            id: id.unwrap_or_else(|| String::from("yoga")),
            logger,
            graphql_endpoint: graphql_endpoint
                .filter(|endpoint| !endpoint.is_empty())
                .unwrap_or_else(|| String::from(DEFAULT_GRAPHQL_ENDPOINT)),
            health_check_endpoint: health_check_endpoint
                .unwrap_or_else(|| String::from(DEFAULT_HEALTH_CHECK_ENDPOINT)),
            graphiql,
            parsers: Parsers::new(multipart == Some(true)),
            processors: Processors::new(),
            extra_param_names,
            plugins,
            hooks,
            masked_errors,
            agent,
            handler: default_handler(weak.clone()),
        });
        for plugin in &server.plugins {
            plugin.on_yoga_init(&mut OnYogaInit { yoga: &server });
        }
        server
    }

    /// Returns `id`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the logger, `logger`.
    #[must_use]
    pub fn logger(&self) -> &HttpLogger {
        &self.logger
    }

    /// Returns `graphqlEndpoint`.
    #[must_use]
    pub fn graphql_endpoint(&self) -> &str {
        &self.graphql_endpoint
    }

    /// Returns the app's plugins after the splices of the hook layer, in the order their hooks run.
    #[must_use]
    pub fn plugins(&self) -> &[Rc<dyn HttpPlugin>] {
        &self.plugins
    }

    /// Returns the masking options, `maskedErrorsOpts`.
    #[must_use]
    pub fn masked_errors_opts(&self) -> Option<&MaskedErrorsOpts> {
        self.masked_errors.as_deref()
    }

    /// Returns the request handler a request runs unless a hook `onRequest` replaces it.
    #[must_use]
    pub fn request_handler(&self) -> &RequestHandler {
        &self.handler
    }

    /// Returns the enveloped functions of a request over `context`, `getEnveloped(context)`.
    ///
    /// # Errors
    ///
    /// Returns the value an `onEnveloped` hook throws.
    pub fn get_hooked(&self, context: HookContext) -> Result<Hooked, Value> {
        self.hooks.hooked(context)
    }
}

/// Returns the request handler over the server `weak` holds, `server.handle`.
///
/// The server holds its handler, so the handler holds the server weakly. A
/// request runs the handler while the function holds the server, and a
/// handler a plugin keeps after its server is gone answers the `TypeError`
/// of a server that no longer stands.
fn default_handler(weak: Weak<HttpServer>) -> RequestHandler {
    Rc::new(
        move |request: Rc<Request>,
              server_context: HookContext|
              -> LocalBoxFuture<'static, Result<Response, Value>> {
            match weak.upgrade() {
                Some(server) => server.handle(request, server_context),
                None => Box::pin(async {
                    Err(Value::from(ErrorObject::type_error(
                        "The server of the request handler no longer stands.",
                    )))
                }),
            }
        },
    )
}

/// Returns the list of hooks the server hands the hook layer, in the places of Yoga's list.
fn hook_list(
    options: &HttpOptions,
    masked_errors: Option<&Rc<MaskedErrorsOpts>>,
) -> Vec<Rc<dyn HookPlugin>> {
    let mut list: Vec<Rc<dyn HookPlugin>> = vec![use_engine(engine(options))];
    // Use the schema provided by the user.
    if let Some(schema) = &options.schema {
        list.push(use_schema(Arc::clone(schema)));
    }
    if let Some(context) = &options.context {
        list.push(use_extend_context(Rc::clone(context)));
    }
    list.extend(
        options
            .plugins
            .iter()
            .map(|plugin| Rc::clone(plugin) as Rc<dyn HookPlugin>),
    );
    if options.parser_and_validation_cache != Some(false) {
        list.push(use_parser_and_validation_cache_with(
            options.parser_and_validation_cache_budget,
        ));
    }
    // We make sure that the user doesn't send a mutation with GET.
    list.push(Rc::new(MutationCheck));
    if let Some(masked_errors) = masked_errors {
        let wrapper = Rc::clone(masked_errors);
        let mask_error: MaskErrorFn =
            Rc::new(move |error, message| wrapper.mask_error(error, message));
        list.push(use_masked_errors(MaskedErrorsOptions {
            mask_error: Some(mask_error),
            error_message: Some(masked_errors.error_message.clone()),
        }));
    }
    // We handle validation errors at the end.
    list.push(Rc::new(ValidationStamp));
    list
}

/// Returns the app's plugins in the order of the array the hook layer answers after its splices.
///
/// A plugin the hook layer spliced in that is no plugin of the app holds no
/// hook of Yoga or of the adapter, as Yoga wraps it in none.
fn app_plugins(hooks: &Hooks, plugins: &[Rc<dyn HttpPlugin>]) -> Vec<Rc<dyn HttpPlugin>> {
    hooks
        .plugins()
        .iter()
        .filter_map(|spliced| {
            plugins
                .iter()
                .find(|plugin| address(*plugin) == address(spliced))
                .map(Rc::clone)
        })
        .collect()
}

/// Returns the masking options of `masked_errors`, `maskedErrorsOpts`.
fn masked_errors_opts(
    masked_errors: &MaskedErrors,
    logger: HttpLogger,
) -> Option<Rc<MaskedErrorsOpts>> {
    let default_mask: HttpMaskErrorFn = Rc::new(mask_error);
    match masked_errors {
        MaskedErrors::Off => None,
        MaskedErrors::On => Some(Rc::new(MaskedErrorsOpts::new(
            String::from("Unexpected error."),
            default_mask,
            None,
            logger,
        ))),
        MaskedErrors::Options(config) => Some(Rc::new(MaskedErrorsOpts::new(
            config
                .error_message
                .clone()
                .unwrap_or_else(|| String::from("Unexpected error.")),
            config.mask_error.clone().unwrap_or(default_mask),
            config.is_dev,
            logger,
        ))),
    }
}

/// Returns the engine the hook layer runs first.
///
/// The engine of Yoga is graphql-js with `normalizedExecutor`, and Aosabi
/// installs the same engine with the single pass in the seat of the validator
/// where `single_pass` is on or absent with [`SINGLE_PASS_DEFAULT`] on. That
/// engine meets the refusal of introspection after the specified rules, which
/// the server hands it, and the bound on nesting of the options.
fn engine(options: &HttpOptions) -> Engine {
    let on = options.single_pass.unwrap_or(SINGLE_PASS_DEFAULT);
    if !on && options.single_pass_counts.is_none() {
        return Engine::graphql();
    }
    Engine::graphql_single_pass(SinglePass {
        on,
        max_depth: options.single_pass_max_depth.unwrap_or(MAX_DEPTH),
        extra_rule: Some(NO_SCHEMA_INTROSPECTION_CUSTOM_RULE),
        counts: options.single_pass_counts.clone(),
    })
}
