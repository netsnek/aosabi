// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core context: the request context and the ambient cell it lives in.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/context.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The request context.
//!
//! The context of a request is the context of hono, `HonoContext<Env>`,
//! so [`Context`] is `aosabi_http::Context`: the request, the response
//! headers, the variables of `c.set` and the typed slots the core keeps its
//! request state in. A TypeScript service keeps the context of a request in an
//! `AsyncLocalStorage`, whose store is in place for every piece of code the
//! request runs, also after an await. A request of Aosabi runs on one thread
//! from its first job to its last, so the store is a cell of the thread.
//! [`run_with_context`] puts a context in place for the length of a call,
//! [`with_context`] for every poll of a future, which is the async context
//! bind of the base pipeline, and the two hooks of [`ambient_hooks`] let the
//! agent carry the cell across the await of a spawned future.
//!
//! [`get_context`] replaces the environment of the context with the one of
//! the runtime before it answers, the process on Node and the bindings of the
//! platform on workerd, as `getContext` does with `env(ctx)`.

use core::{
    any::Any,
    cell::RefCell,
    future::Future,
    pin::Pin,
    task::{
        self,
        Poll, //
    }, //
};
use std::rc::Rc;

use aosabi_execute::{
    execute::FieldNodes,
    promise::{
        AmbientCapture,
        AmbientRestore, //
    }, //
};
pub use aosabi_http::Context;
use aosabi_http::{
    Body,
    Env,
    Headers,
    Request,
    Vars,
    helper::adapter::env, //
};
use aosabi_json::{
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_type::{
    NamedType,
    SharedType, //
};

use crate::{
    glue::Thrown,
    resolve_info::ExecutionContext,
    resolvers::SelectedField, //
};

/// The message of `getContext` with no store in place, `context.ts:34`.
const CONTEXT_NOT_DEFINED: &str = "Context not defined";

/// The URL of the request a context below HTTP is made of.
const SERVICE_URL: &str = "http://localhost/graphql";

/// The key of one cached selection walk.
///
/// The first part is the address of the field node list and the second the
/// address of the definition of the parent type, 0 for a walk without one.
type SelectionKey = (usize, usize);

/// One cached selection walk.
///
/// The entry keeps the node list and the parent type alive, so the two
/// addresses in its key name this list and this type for as long as the
/// cache holds the entry.
type SelectionEntry = (FieldNodes, Option<NamedType>, Rc<Vec<SelectedField>>);

/// What lives as long as one request, the selection set cache.
///
/// The reference keys a `WeakMap` by the identity of the field node array and
/// a `Map` under it by the identity of the parent type. The arrays of one
/// execution die with it, so the port keeps one map per request, keyed by the
/// address of the node list and the address of the parent type, in a typed
/// slot of the context, and a lookup allocates nothing.
#[derive(Default)]
pub struct RequestState {
    /// The cached walks.
    selection_sets: RefCell<IndexMap<SelectionKey, SelectionEntry, KeyHasherBuilder>>,
}

impl RequestState {
    /// Returns the cached walk over `field_nodes` under the parent type `parent`.
    #[must_use]
    pub(crate) fn selected_fields(
        &self,
        field_nodes: &FieldNodes,
        parent: Option<&NamedType>,
    ) -> Option<Rc<Vec<SelectedField>>> {
        let key = selection_key(field_nodes, parent);
        let cache = self.selection_sets.borrow();
        cache.get(&key).map(|(_, _, fields)| Rc::clone(fields))
    }

    /// Caches `fields` as the walk over `field_nodes` under the parent type `parent`.
    pub(crate) fn set_selected_fields(
        &self,
        field_nodes: &FieldNodes,
        parent: Option<&NamedType>,
        fields: &Rc<Vec<SelectedField>>,
    ) {
        let key = selection_key(field_nodes, parent);
        let entry = (Rc::clone(field_nodes), parent.cloned(), Rc::clone(fields));
        self.selection_sets.borrow_mut().insert(key, entry);
    }
}

/// Returns the cache key of a walk.
fn selection_key(field_nodes: &FieldNodes, parent: Option<&NamedType>) -> SelectionKey {
    let address = Rc::as_ptr(field_nodes).cast::<()>().addr();
    (address, parent.map_or(0, type_address))
}

/// Returns the address of the definition `named` holds, which names the type
/// while a holder keeps it alive.
fn type_address(named: &NamedType) -> usize {
    match named {
        NamedType::Scalar(definition) => SharedType::as_ptr(definition).cast::<()>().addr(),
        NamedType::Object(definition) => SharedType::as_ptr(definition).cast::<()>().addr(),
        NamedType::Interface(definition) => SharedType::as_ptr(definition).cast::<()>().addr(),
        NamedType::Union(definition) => SharedType::as_ptr(definition).cast::<()>().addr(),
        NamedType::Enum(definition) => SharedType::as_ptr(definition).cast::<()>().addr(),
        NamedType::InputObject(definition) => SharedType::as_ptr(definition).cast::<()>().addr(),
    }
}

/// Returns the state that lives as long as the request of `context`.
///
/// The state is a typed slot of the context, made at its first read.
#[must_use]
pub fn request_state(context: &Context) -> Rc<RequestState> {
    if let Some(state) = context.extension::<RequestState>() {
        return state;
    }
    let state = Rc::new(RequestState::default());
    context.set_extension(Rc::clone(&state));
    state
}

/// Returns the context of a request below HTTP, the synthetic `POST /graphql`
/// a service runs a request line in.
///
/// The environment is the bindings the Node adapter hands `fetch`, which hold
/// no variable, as the pair `{incoming, outgoing}` holds none, so `get_env()`
/// reads the same before and after `get_context()` as it does over HTTP.
#[must_use]
pub fn service_context() -> Context {
    let request = Rc::new(Request::new(
        "POST",
        SERVICE_URL,
        Headers::new(),
        Body::Empty,
    ));
    Context::new(
        request,
        String::from("/graphql"),
        Rc::from(Vec::new()),
        Env::Bindings(Rc::new(Vars::default())),
        None,
    )
}

/// The stores that are in place on this thread.
#[derive(Clone, Default)]
struct Ambient {
    /// The store of `asyncContext`.
    context: Option<Context>,
    /// The store of `executionAsyncContext`.
    execution: Option<Rc<ExecutionContext>>,
}

thread_local! {
    /// The stores in place, which one request at a time writes.
    static AMBIENT: RefCell<Ambient> = const {
        RefCell::new(Ambient {
            context: None,
            execution: None,
        })
    };
}

/// Puts a saved state back in place when it goes out of scope.
///
/// The swap back sits in a destructor so that a panic that unwinds through a
/// body leaves the cell of the thread as it found it.
struct AmbientGuard(Option<Ambient>);

impl Drop for AmbientGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            swap_ambient(previous);
        }
    }
}

/// Puts `state` in place and returns what it replaced.
///
/// The access fails only while the thread tears its locals down, and then
/// there is no state left to replace.
fn swap_ambient(state: Ambient) -> Ambient {
    AMBIENT
        .try_with(|ambient| ambient.replace(state))
        .unwrap_or_default()
}

/// Returns a copy of the state in place.
fn read_ambient() -> Ambient {
    AMBIENT
        .try_with(|ambient| ambient.borrow().clone())
        .unwrap_or_default()
}

/// Runs `body` with `change` applied to the state in place.
fn run_with_ambient<R>(change: impl FnOnce(&mut Ambient), body: impl FnOnce() -> R) -> R {
    let mut state = read_ambient();
    change(&mut state);
    let _guard = AmbientGuard(Some(swap_ambient(state)));
    body()
}

/// Returns the context of the running request.
///
/// Outside a request it throws, and inside one it sets the environment of the
/// context to the one of the runtime, `ctx.env = env(ctx)`, before it
/// answers.
///
/// # Errors
///
/// Returns the plain error `Context not defined` when no context is in place.
pub fn get_context() -> Result<Context, Thrown> {
    let context = read_ambient()
        .context
        .ok_or_else(|| Thrown::error(CONTEXT_NOT_DEFINED))?;
    context.set_env(env(&context));
    Ok(context)
}

/// Returns the context in place without touching its environment, for the
/// readers that are no call of `getContext`.
///
/// The readers are those of the core and the integrations of an SDK, which
/// the reference keys by the request in an `AsyncLocalStorage` of its own and
/// which read no environment, as the scope of a request of `aosabi-sentry`
/// does.
#[must_use]
pub fn ambient_context() -> Option<Context> {
    read_ambient().context
}

/// Puts `context` in place for the rest of the running synchronous code.
///
/// It is `asyncContext.enterWith(context)`. The context stays in place until
/// a surrounding [`run_with_context`] returns or the next call replaces it.
pub fn set_context(context: Context) {
    let mut state = read_ambient();
    state.context = Some(context);
    swap_ambient(state);
}

/// Runs `body` with `context` in place, which is `asyncContext.run(context, body)`.
///
/// The context that was in place before comes back when `body` returns.
pub fn run_with_context<R>(context: Context, body: impl FnOnce() -> R) -> R {
    run_with_ambient(|state| state.context = Some(context), body)
}

/// A future that runs with a context in place around every poll.
///
/// It is the twin of an async function run inside `asyncContext.run`, whose
/// store follows it across every await, and the base pipeline wraps the rest
/// of a request in it.
pub struct WithContext<F> {
    /// The context put in place around every poll.
    context: Context,
    /// The future.
    future: Pin<Box<F>>,
}

impl<F: Future> Future for WithContext<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<F::Output> {
        let this = self.get_mut();
        let context = this.context.clone();
        run_with_context(context, || this.future.as_mut().poll(cx))
    }
}

/// Returns `future` with `context` in place around every poll.
pub fn with_context<F: Future>(context: Context, future: F) -> WithContext<F> {
    WithContext {
        context,
        future: Box::pin(future),
    }
}

/// Returns the store of `executionAsyncContext` that is in place.
pub(crate) fn ambient_execution_context() -> Option<Rc<ExecutionContext>> {
    read_ambient().execution
}

/// Runs `body` with `execution` in place as the store of `executionAsyncContext`.
pub(crate) fn run_with_ambient_execution_context<R>(
    execution: Rc<ExecutionContext>,
    body: impl FnOnce() -> R,
) -> R {
    run_with_ambient(|state| state.execution = Some(execution), body)
}

/// Returns the two hooks an agent carries the ambient cell across an await with.
///
/// The first hook answers the state in place, which the agent keeps with a
/// future it spawns. The second puts a kept state in place and answers the
/// one it replaced, which the agent calls before and after every poll of that
/// future. This is how a store of `AsyncLocalStorage` follows an async
/// function across an await.
#[must_use]
pub fn ambient_hooks() -> (AmbientCapture, AmbientRestore) {
    let capture: AmbientCapture = Rc::new(|| {
        let state: Rc<dyn Any> = Rc::new(read_ambient());
        Some(state)
    });
    let restore: AmbientRestore = Rc::new(|state| {
        let state = state
            .and_then(|state| state.downcast::<Ambient>().ok())
            .map(|state| (*state).clone())
            .unwrap_or_default();
        let previous: Rc<dyn Any> = Rc::new(swap_ambient(state));
        Some(previous)
    });
    (capture, restore)
}
