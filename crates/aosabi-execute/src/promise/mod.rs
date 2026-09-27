// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise: the promise jobs of ECMA-262 for one request.
 *
 * Translation of ECMA-262 section 27.2, Promise Objects,
 * https://tc39.es/ecma262/#sec-promise-objects, of the job rules of section
 * 9.5, https://tc39.es/ecma262/#sec-jobs, and of the await of section 27.7.5,
 * https://tc39.es/ecma262/#sec-async-functions-abstract-operations. Each file
 * names the clauses it translates.
 */

//! The promise job model.
//!
//! The order of the `errors` array of a response is the order in which
//! promise reactions run, so the executor runs on a model of the promise jobs
//! of ECMA-262 and not on the schedule of a Rust async runtime. What is
//! modelled is the specification and no engine, because the same corpus runs
//! on more than one engine.
//!
//! | Module | Clauses | Holds |
//! | --- | --- | --- |
//! | [`agent`] | 9.5 Jobs and Host Operations to Enqueue Jobs | [`Agent`], [`AgentOptions`], [`TimerMode`], [`Job`] and the job queue |
//! | [`promise`](mod@promise) | 27.2.1 to 27.2.5, 27.7.5.1 | [`Promise`], [`PromiseState`], [`ResolvingFunctions`], [`Reaction`] and [`async_function`] |
//! | [`combinators`] | 27.2.4.1 | [`Promise::all`] |
//! | [`future`] | 27.7.5.1, 27.7.5.3 | [`Agent::spawn`], [`PromiseFuture`], [`LocalFuture`] and the wake set |
//! | [`timers`] | no clause, the timers of node | the wheel on the virtual or the host clock, [`TimerId`], [`Agent::set_timeout`], [`Agent::set_interval`], [`Agent::clear_timer`] and [`Agent::sleep`] |
//! | [`host`] | 9.5, the host side | [`Drive`], [`Idle`], [`Agent::run`], [`Agent::drain_until_idle`], [`Stalled`], [`sleep`], [`tick`] and [`set_timeout`] |
//! | [`ambient`] | no clause, `AsyncLocalStorage` of node | the running agents of the thread and the ambient snapshot of a future |
//! | [`trace`] | no clause | [`TraceEvent`] |
//!
//! # The rules
//!
//! Nothing ever runs a reaction synchronously. A settlement enqueues one job
//! per recorded reaction in registration order, `then` on a settled promise
//! enqueues its job at once, and the queue of an agent is first in first
//! out. Resolving a promise with a promise costs one extra job before the
//! `then` that adopts its state, so an outer promise settles two jobs after
//! an inner one that is settled already. An `await` costs one job, also on a
//! settled promise, and the continuation of the awaiting future runs inside
//! that job. The host loop drains the queue to empty before it polls a
//! future a foreign waker flagged and before it fires the next timer, and it
//! fires exactly one timer between two drains.
//!
//! # One request, one thread
//!
//! One agent serves one request on one thread. [`Agent`], [`Promise`] and
//! every value are `Rc` based and neither `Send` nor `Sync`. The module
//! starts no operating system thread, reads no clock but the one a host
//! hands an agent, and links no async runtime, so it builds for
//! `wasm32-unknown-unknown`.
//!
//! # The limit
//!
//! Tick depth inside user code is not modelled beyond [`tick`] and
//! [`sleep`]. A Rust `async fn` that awaits a ready foreign future completes
//! in one poll. [`tick`] is the twin of `await Promise.resolve()` and
//! [`sleep`] the twin of awaiting a `setTimeout` promise, each one job after
//! its cause.

pub mod agent;
pub mod ambient;
pub mod combinators;
pub mod future;
pub mod host;
// The plan of the crate names the file after the type it holds, as the
// directory is named after the model.
#[expect(clippy::module_inception)]
pub mod promise;
pub mod timers;
pub mod trace;

pub use self::{
    agent::{
        Agent,
        AgentOptions,
        Job,
        TimerMode, //
    },
    ambient::{
        AmbientCapture,
        AmbientRestore, //
    },
    future::{
        LocalFuture,
        PromiseFuture, //
    },
    host::{
        Drive,
        Idle,
        Stalled,
        set_timeout,
        sleep,
        tick, //
    },
    promise::{
        Promise,
        PromiseState,
        Reaction,
        ResolvingFunctions,
        async_function, //
    },
    timers::TimerId,
    trace::TraceEvent, //
};
