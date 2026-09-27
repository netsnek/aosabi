// SPDX-License-Identifier: MIT
/*
 * aosabi-execute: the execution layer of Aosabi.
 *
 * This file holds the module list and the names the crate exports at its
 * root.
 *
 * Derived from @graphql-tools/executor 1.5.1, ., MIT.
 * Derived from @graphql-tools/utils 11.0.0, ., MIT.
 * Derived from whatwg-node 1.3.2, ., MIT.
 * Derived from @repeaterjs/repeater 3.0.6, ., MIT.
 */

//! The execution layer of Aosabi.
//!
//! The executor on the wire is `normalizedExecutor` of
//! `@graphql-tools/executor` and not the `execute` of graphql-js, because
//! Yoga installs the first as `execute` and as `subscribe`. The order of the
//! `errors` array and the key order of `data` follow from the order in which
//! promise reactions run, so the crate models the promise jobs of ECMA-262
//! and runs the translated executor on them.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | [`value`] | no analogue, `jsutils/inspect.ts`, `jsutils/toError.ts`, `cross-inspect` | the JavaScript value a resolver returns or throws |
//! | [`promise`] | ECMA-262 section 27.2, the promise jobs | the agent of one request, its promises, its futures and its timers |
//! | [`execute`](mod@execute) | `execution/execute.js` | the executor of one operation |
//! | [`normalized_executor`](mod@normalized_executor) | `execution/normalizedExecutor.js` | the entry Yoga calls |
//! | [`executor_from_schema`](mod@executor_from_schema) | `execution/normalizedExecutor.js` | the local executor of a delegation over a schema |
//! | [`promise_for_object`](mod@promise_for_object) | `execution/promiseForObject.js` | the result object of a selection set |
//! | [`coerce_error`](mod@coerce_error) | `execution/coerceError.js` | the error a thrown value becomes |
//! | [`handle_maybe_promise`](mod@handle_maybe_promise) | `@whatwg-node/promise-helpers` | the continuation of a value or a promise |
//! | [`promise_reduce`] | `utils/jsutils.js` | the serial reduction of a mutation |
//! | [`errors`] | `utils/errors.js` | the error constructors |
//! | [`collect_fields`] | `utils/collectFields.js` | the field collection |
//! | [`path`] | `utils/Path.js` | the path of a response position |
//! | [`plan`] | no analogue, the memos of `execution/execute.js` | the field plan of a document under a schema |
//! | [`values`] | `execution/values.js`, `utils/getArgumentValues.js` | the variable and argument coercion |
//! | [`introspection`] | no analogue | the dispatch of an introspection resolver |
//! | [`result`] | `buildResponse`, `GraphQLError.toJSON` | the execution result |
//! | [`arena`] | no analogue, `JSON.stringify` of the result | the arena of the answer of an execution and its writer |
//! | [`kernel_entry`] | no analogue | the request pipeline, the subscription stream and the general executor |
//! | [`subscribe`](mod@subscribe) | `execution/execute.js` | the subscription entry |
//! | [`flatten_async_iterable`](mod@flatten_async_iterable) | `execution/flattenAsyncIterable.js` | the flattening of an async iterable of async iterables |
//! | [`map_async_iterator`](mod@map_async_iterator) | `@whatwg-node/promise-helpers` | `mapAsyncIterator` |
//! | [`repeater`] | `@repeaterjs/repeater` | the `Repeater` a subscription resolver returns |
//!
//! The crate uses the standard library and no operating system thread, no
//! async runtime and no `unsafe`, so it builds for `wasm32-unknown-unknown`.
//! One agent serves one request on one thread, and every value of a request
//! is `Rc` based. There is no test in this crate, which is the rule of
//! CONTRIBUTING.md section 3, and the judging notebook is
//! `tests/notebooks/20_execute.ipynb`.
//!
//! The modules of the async iterator protocol, of the timers, of the repeater
//! and of the subscription entry name `core` and `alloc` and no item of
//! `std`, and share a value they add through `Rc` and never through `Arc`, so
//! a build without the standard library takes them as they are.

// The modules that name no item of `std` reach `Rc`, `Box` and `Vec` through
// `alloc`, which a crate that links the standard library declares as well.
extern crate alloc;

pub mod arena;
pub mod coerce_error;
pub mod collect_fields;
pub mod errors;
pub mod execute;
pub mod executor_from_schema;
pub mod flatten_async_iterable;
pub mod handle_maybe_promise;
pub mod introspection;
pub mod kernel_entry;
pub mod map_async_iterator;
pub mod normalized_executor;
pub mod path;
pub mod plan;
pub mod promise;
pub mod promise_for_object;
pub mod promise_reduce;
pub mod repeater;
pub mod result;
pub mod subscribe;
pub mod value;
pub mod values;

pub use crate::{
    coerce_error::coerce_error,
    execute::{
        CRITICAL_ERROR,
        ExecutionArgs,
        ExecutionContext,
        FieldNodes,
        FieldResolve,
        FieldResolverFn,
        IsTypeOf,
        IsTypeOfFn,
        LeafSerializer,
        ResolveInfo,
        ResolveType,
        SharedVariables,
        TypeResolverFn,
        VariableValues,
        default_field_resolver,
        default_type_resolver,
        execute, //
    },
    flatten_async_iterable::flatten_async_iterable,
    handle_maybe_promise::{
        MaybePromise,
        handle_maybe_promise,
        is_promise, //
    },
    kernel_entry::{
        Executed,
        GeneralExecutor,
        GeneralStream,
        JsonEncoder,
        OneResult,
        Request,
        RootFactory,
        RunFailure,
        Subscribed,
        SubscriptionStream,
        VecSink,
        run_request,
        run_subscription, //
    },
    map_async_iterator::map_async_iterator,
    normalized_executor::{
        normalized_executor,
        operation_type, //
    },
    path::{
        Path,
        PathNode,
        add_path,
        path_to_array, //
    },
    promise::{
        Agent,
        AgentOptions,
        Promise,
        PromiseState,
        TimerMode,
        TraceEvent, //
    },
    repeater::{
        PushHandle,
        PushResult,
        Repeater,
        StopHandle, //
    },
    result::ExecutionResult,
    subscribe::subscribe,
    value::{
        Completion,
        ContextValue,
        ErrorObject,
        Function,
        HostValue,
        ObjectLike,
        PlainObject,
        Value, //
    }, //
};
