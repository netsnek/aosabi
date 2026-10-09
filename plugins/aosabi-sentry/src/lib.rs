// SPDX-License-Identifier: MIT
/*
 * aosabi-sentry: the Sentry plugin of Aosabi.
 *
 * This file holds the module list.
 */

//! The Sentry plugin of Aosabi.
//!
//! The plugin reports the errors of a GraphQL execution, and its middleware
//! reports the error of a route. An event is sent only where a DSN is set, as
//! one envelope through the network capability of `aosabi-core`, so a build
//! with no network sends nothing and leaves the answers of the service as
//! they are.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `use_sentry` | `plugins/use-sentry.ts` | the plugin, its options and the default skip of an error |
//! | `toucan` | the constructor, `captureException` and the transport of Toucan | the client of a request |
//! | `hub` | `init`, the scopes, `captureException` and `startSpanManual` of the Sentry SDK | the client of `init`, its scope and the span that records nothing |
//! | `middleware` | `index.js` of `@hono/sentry` | the middleware and the client of a request |
//! | `integrations` | the console and Http integrations of `@sentry/node` and the request data of the Sentry SDK | the scope of a request, its request data and the breadcrumbs of the console |
//! | `envelope` | the envelope and event formats of Sentry | the event, the envelope and the event id |
//! | `dsn` | the DSN format of Sentry | the DSN and the envelope endpoint with its key in the query |
//!
//! The root re-exports what an application names: the plugin with its
//! options, `init` with its options, the client of a request and the scope.
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebook is `tests/notebooks/62_sentry.ipynb`.

pub mod dsn;
pub mod envelope;
pub mod hub;
mod integrations;
pub mod middleware;
pub mod toucan;
pub mod use_sentry;

pub use crate::{
    hub::{
        ClientOptions,
        Report,
        Scope,
        Span,
        capture_exception,
        init,
        start_span_manual,
        with_scope, //
    },
    middleware::{
        SENTRY,
        get_sentry,
        sentry, //
    },
    toucan::{
        Toucan,
        ToucanOptions, //
    },
    use_sentry::{
        EventIdKey,
        ExecuteArgs,
        SentryPlugin,
        SentryPluginOptions,
        default_skip_error,
        use_sentry, //
    },
};
