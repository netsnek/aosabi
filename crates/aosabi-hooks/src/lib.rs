// SPDX-License-Identifier: MIT
/*
 * aosabi-hooks: Aosabi's hook layer, the plugins of a GraphQL request and the
 * order their hooks run in.
 *
 * The answers follow @envelop/core 5.5.1.
 * Each module names the file of it that it follows in its own header. This
 * file holds the module list.
 */

//! Aosabi's hook layer.
//!
//! A plugin hangs hooks on the steps of a GraphQL request: its start, the
//! parse, the validation, the building of the context, the execution and the
//! subscription. The layer runs the hooks of every plugin in the order of the
//! list, the order the reference runs them in. Both roads of a request reach
//! it: an HTTP request through the server of `graphql-over-http`, and a
//! `GraphTP` call through the same server, since the fetch of a served build
//! hands the call to the fetch of the app.
//!
//! The crate holds no runtime and starts no thread, so it builds for
//! `wasm32-unknown-unknown` as it builds for a host. Under the feature
//! `host-hooks` a done hook of a plugin of the TypeScript option may throw or
//! answer a promise, which the layer awaits.
//!
//! | Module | Follows | Holds |
//! | --- | --- | --- |
//! | `plugin` | the type `Plugin` of `@envelop/types` 5.2.1 | the trait of a plugin and the payloads of its hooks |
//! | `orchestrator` | `orchestrator.js` | the order the hooks run in |
//! | `create` | `create.js` | the entry [`hooks`] and the functions of one request |
//! | `context` | no analogue | the context of a request |
//! | `plugins` | `plugins/` | the engine, schema, context and masked errors plugins |
//! | `utils` | `utils.js` | the helpers over a single result and a stream |
//! | `document_string_map` | `document-string-map.js` | the source text of a parsed document |
//!
//! There is no test in this crate, which is the rule of CONTRIBUTING.md
//! section 3. The judging notebooks are `tests/notebooks/30_http_tier1.ipynb`
//! for the hooks of a request and `tests/notebooks/40_sse.ipynb` for the
//! stream of a subscription.

pub mod context;
pub mod create;
pub mod document_string_map;
pub mod orchestrator;
pub mod plugin;
pub mod plugins;
pub mod utils;

pub use crate::{
    context::HookContext,
    create::{
        Hooked,
        Hooks,
        hooks, //
    },
    plugin::{
        ExecuteFn,
        ExecutionOutcome,
        HookPlugin,
        LocalBoxFuture,
        ParseFn,
        ResultStream,
        ValidateFn, //
    },
    plugins::{
        use_engine::{
            Engine,
            use_engine, //
        },
        use_extend_context::use_extend_context,
        use_masked_errors::{
            DEFAULT_ERROR_MESSAGE,
            MaskErrorFn,
            MaskedErrorsOptions,
            use_masked_errors, //
        },
        use_schema::use_schema, //
    }, //
};
