// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: GraphQL over HTTP as one function, the jobs of
 * graphql-yoga 5.18.0 as Aosabi runs it.
 *
 * This file holds the module list. The template graphiql/template.html is
 * written by tools/provenance and by nothing else.
 *
 * Derived from graphql-yoga 5.18.0, cjs/index.js, MIT.
 */

//! GraphQL over HTTP as one function.
//!
//! [`answer()`] is Yoga's five jobs as straight code: it reads GET and POST
//! JSON, and forms, `application/graphql` and uploads, it refuses a wrong
//! method, a mutation over GET, broken parameters, an unknown body and a
//! batch, it answers with the status and the type the client
//! accepts, it masks unexpected errors, and it caches parsed and validated
//! documents. Each step of Yoga's list of plugins stands at its place in the
//! function, and the hooks of an app's plugins, [`HttpPlugin`] with the hooks
//! of the server adapter [`ServerAdapterPlugin`], run at Yoga's points. A
//! mount of an app builds the state of the function, [`HttpServer::new`],
//! once, and answers each request with [`answer()`] over the
//! [`ServerContext`] of that request.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `answer` | `server.js:239-398`, `createServerAdapter.js:95-149` | the function |
//! | `server` | `server.js:57-238` | the options and the state the function reads |
//! | `checks` | `plugins/request-validation/` | the checks of the method, the parameters, a batch and a mutation over GET, and the stamps of the parse and of the validation |
//! | `routes` | `plugins/use-unhandled-route.js`, `plugins/use-health-check.js` | the endpoint and the health check |
//! | `plugin` | `plugins/types.js` | the plugin of an app and the payloads of its hooks |
//! | `process_request` | `process-request.js` | the run of one operation through the hook layer |
//! | `error` | `error.js` | the handling of an escaping error |
//! | `utils` | `utils/mask-error.js` | the default mask of an error |
//! | `result_processor` | `plugins/result-processor/`, `plugins/use-result-processor.js` | the result processors and the choice of one |
//! | `readable_stream` | WHATWG Streams, sections 4.2, 4.5 and 4.9 | the readable stream the server sent events processor writes into, and its driver |
//! | `logger` | `@graphql-yoga/logger` | the logger of Yoga |
//! | `server_adapter` | `@whatwg-node/server`, `@whatwg-node/node-fetch` | the server context of a mount, the hooks of the adapter and the Response Yoga makes |
//! | `request_parser` | `plugins/request-parser/`, `plugins/use-request-parser.js`, the JSON parser of V8 | the request parsers, the choice of one and `JSON.parse` with its refusals |
//! | `form_data`, `file` | the multipart parser of the undici of Node 24.13.0 | the form data body and its files |
//! | `plugins` | `plugins/use-parser-and-validation-cache.js`, `@graphql-yoga/plugin-disable-introspection` | the cache of documents of the hook layer and the introspection refusal |
//! | `graphiql` | `graphiql-html.js`, `plugins/use-graphiql.js` | the `GraphiQL` page |
//! | `host_env` | no file, the feature `host-env` | the variables of the process and the writer of the console a host of the TypeScript option hands in |
//!
//! Every message of the parsers, the checks, the processors and the errors is
//! copied from the pinned file its module names, and the row
//! `graphql_yoga_message_corpus` of `tools/provenance/provenance.toml` records
//! the port.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebook is `tests/notebooks/30_http_tier1.ipynb`,
//! and `tests/notebooks/40_sse.ipynb` for the streams of a subscription.

pub mod answer;
pub mod checks;
pub mod error;
pub mod file;
pub mod form_data;
pub mod graphiql;
#[cfg(feature = "host-env")]
pub mod host_env;
pub mod logger;
pub mod plugin;
pub mod plugins;
pub mod process_request;
pub mod readable_stream;
pub mod request_parser;
pub mod result_processor;
pub mod routes;
pub mod server;
pub mod server_adapter;
pub mod utils;

pub use crate::{
    answer::answer,
    file::FileBody,
    graphiql::{
        Graphiql,
        GraphiqlOptions,
        render_graphiql, //
    },
    logger::{
        HttpLogger,
        LogLevel,
        create_logger, //
    },
    plugin::{
        GraphQLParams,
        HttpPlugin,
        HttpResult,
        RequestParserResult,
        UploadedFile, //
    },
    plugins::disable_introspection::use_disable_introspection,
    server::{
        AgentSetup,
        HttpOptions,
        HttpServer,
        Logging,
        MaskedErrors,
        MaskedErrorsConfig, //
    },
    server_adapter::{
        ServerAdapterPlugin,
        ServerContext, //
    }, //
};
/// The bounds of the table of the request cache, which
/// [`HttpOptions::parser_and_validation_cache_budget`] takes, so a caller of
/// this crate names one dependency.
pub use aosabi_cache::CacheBudget;
