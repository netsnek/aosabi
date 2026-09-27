// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise promise: the promise object and its reaction jobs.
 *
 * Translation of ECMA-262 section 27.2.1, Promise Abstract Operations,
 * section 27.2.2, Promise Jobs, the constructor of section 27.2.3.1,
 * Promise.reject and Promise.resolve of sections 27.2.4.6 and 27.2.4.7,
 * Promise.prototype.then of section 27.2.5.4, and AsyncFunctionStart of
 * section 27.7.5.1, read at https://tc39.es/ecma262/#sec-promise-objects and
 * https://tc39.es/ecma262/#sec-async-functions-abstract-operations.
 */

//! The promise object.
//!
//! Every function names the clause it translates and keeps its name in snake
//! case. Three reads of the clauses have no reading here. The species
//! constructor is always `%Promise%`, because no Rust type subclasses a
//! promise. The `[[PromiseIsHandled]]` slot and `HostPromiseRejectionTracker`
//! are left out, because an unhandled rejection changes no byte of a
//! response. A thenable is a [`Promise`] and nothing else, so the `then`
//! lookup of the resolve function cannot throw.
//!
//! A reaction record holds its handlers by their kind. User code and hooks
//! register closures, a [`Reaction`] each, and the promise model and the
//! executor register the few kinds they know with their state in place, so a
//! reaction of theirs and the job that runs it allocate nothing.

use core::{
    cell::{
        Cell,
        RefCell, //
    },
    fmt,
};
use std::rc::Rc;

use crate::{
    execute::Continuation,
    promise::{
        Agent,
        Job,
        combinators::Element,
        future::Awaiting, //
    },
    promise_for_object::Pending,
    value::{
        Completion,
        ErrorObject,
        Value, //
    }, //
};

/// A promise, a placeholder for the eventual result of a deferred computation.
///
/// A clone is the same promise, which [`Promise::ptr_eq`] tells.
#[derive(Clone)]
pub struct Promise(Rc<PromiseInner>);

/// The internal slots of a promise.
// An `unsafe` cell would skip each borrow flag.
struct PromiseInner {
    /// `[[PromiseState]]` and `[[PromiseResult]]`.
    state: RefCell<PromiseState>,
    /// `[[PromiseFulfillReactions]]` and `[[PromiseRejectReactions]]`.
    reactions: RefCell<Reactions>,
    /// `[[AlreadyResolved]]` of the resolving functions the promise is made with.
    already_resolved: Cell<bool>,
}

/// The state of a promise and the value it settled with.
#[derive(Clone, Debug)]
pub enum PromiseState {
    /// Neither fulfilled nor rejected.
    Pending,
    /// Fulfilled with the value.
    Fulfilled(Value),
    /// Rejected with the reason.
    Rejected(Value),
}

/// A handler of `then` of user code or of a hook, which receives the settled value and runs once.
// A closure held in place would skip this box.
pub type Reaction = Box<dyn FnOnce(&Agent, Value) -> Completion>;

/// The reactions of a pending promise, in registration order.
///
/// The two lists of the clause grow in lockstep, so one list of records
/// holds both. The first record stands in the promise itself and the later
/// ones in a list, so a promise with one reaction, which is what the
/// executor registers on each promise it derives, allocates no list.
#[derive(Default)]
struct Reactions {
    /// The record registered first, `None` while there is none.
    first: Option<ReactionRecord>,
    /// The records registered after the first one, in registration order.
    rest: Vec<ReactionRecord>,
}

impl Reactions {
    /// Adds `reaction` after every record registered before it.
    fn push(&mut self, reaction: ReactionRecord) {
        if self.first.is_some() {
            self.rest.push(reaction);
        } else {
            self.first = Some(reaction);
        }
    }

    /// Returns whether no record is registered.
    fn is_empty(&self) -> bool {
        self.first.is_none()
    }
}

/// The pair of `PromiseReaction` records one call of `PerformPromiseThen` makes.
///
/// Translation of the record of ECMA-262 section 27.2.1.2. Only one of the
/// two handlers ever runs, the one of the settlement.
struct ReactionRecord {
    /// `[[Capability]]`, which is undefined for an await and for the
    /// reactions whose derived promise no code reads.
    capability: Option<ResolvingFunctions>,
    /// `[[Handler]]` of the fulfill reaction and of the reject reaction.
    handlers: Handlers,
}

/// `[[Type]]` of a `PromiseReaction` record.
#[derive(Clone, Copy)]
enum ReactionType {
    /// The promise was fulfilled.
    Fulfill,
    /// The promise was rejected.
    Reject,
}

impl ReactionType {
    /// Returns the completion of an empty `[[Handler]]`, which passes `argument` through.
    ///
    /// Steps 4.a and 4.b of `NewPromiseReactionJob`: a fulfilment is a normal
    /// completion and a rejection a throw completion.
    fn pass(self, argument: Value) -> Completion {
        match self {
            Self::Fulfill => Completion::Normal(argument),
            Self::Reject => Completion::Throw(argument),
        }
    }
}

/// `[[Handler]]` of the fulfill reaction and of the reject reaction of one record, by their kind.
///
/// User code and hooks register closures. The promise model and the
/// executor register the kinds below, each with its state in place, so the
/// type names every kind of reaction they enqueue. A kind that handles one
/// settlement alone passes the other one through, as an empty handler of
/// the clause does.
pub(crate) enum Handlers {
    /// The handlers of a `then` of user code or of a hook, each a closure, and empty when `None`.
    Closures {
        /// `[[Handler]]` of the fulfill reaction.
        on_fulfilled: Option<Reaction>,
        /// `[[Handler]]` of the reject reaction.
        on_rejected: Option<Reaction>,
    },
    /// The resolve and the reject function `NewPromiseResolveThenableJob`
    /// hands the thenable, which settle the promise they hold.
    Adopt(Promise),
    /// The resolve element function and the reject function of one element of `Promise.all`.
    Element(Element),
    /// The continuation of an await, on either settlement.
    Await(Awaiting),
    /// A reaction of `promiseForObject` on a promised member or on the members, on the fulfilment.
    Object(Pending),
    /// A reaction of the executor on the promise of a field or of a list item.
    Execute(Continuation),
}

impl Handlers {
    /// Runs the handler of `reaction_type` with `argument` and returns its completion.
    ///
    /// The second value is the handler that does not run, which the job
    /// drops once the derived promise has the completion, as the closure of
    /// the job drops it in the clause.
    fn call(
        self,
        agent: &Agent,
        reaction_type: ReactionType,
        argument: Value,
    ) -> (Completion, Option<Self>) {
        let done = Completion::Normal(Value::Undefined);
        match self {
            Self::Closures {
                on_fulfilled,
                on_rejected,
            } => {
                let (handler, unused) = match reaction_type {
                    ReactionType::Fulfill => (on_fulfilled, Self::closures(None, on_rejected)),
                    ReactionType::Reject => (on_rejected, Self::closures(on_fulfilled, None)),
                };
                let completion = match handler {
                    Some(handler) => handler(agent, argument),
                    None => reaction_type.pass(argument),
                };
                (completion, unused)
            }
            Self::Adopt(promise) => {
                match reaction_type {
                    ReactionType::Fulfill => resolve_promise(agent, &promise, argument),
                    ReactionType::Reject => reject_promise(agent, &promise, argument),
                }
                (done, None)
            }
            Self::Element(element) => {
                match reaction_type {
                    ReactionType::Fulfill => element.resolve(agent, argument),
                    ReactionType::Reject => element.reject(agent, argument),
                }
                (done, None)
            }
            Self::Await(awaiting) => {
                let settled = match reaction_type {
                    ReactionType::Fulfill => Ok(argument),
                    ReactionType::Reject => Err(argument),
                };
                awaiting.resume(agent, settled);
                (done, None)
            }
            Self::Object(pending) => match reaction_type {
                ReactionType::Fulfill => (pending.run(argument), None),
                ReactionType::Reject => (reaction_type.pass(argument), Some(Self::Object(pending))),
            },
            Self::Execute(continuation) => {
                let rejected = matches!(reaction_type, ReactionType::Reject);
                if continuation.handles_rejection() != rejected {
                    return (
                        reaction_type.pass(argument),
                        Some(Self::Execute(continuation)),
                    );
                }
                (continuation.run(agent, argument), None)
            }
        }
    }

    /// Returns the closures `on_fulfilled` and `on_rejected` as handlers, or `None` for none.
    fn closures(on_fulfilled: Option<Reaction>, on_rejected: Option<Reaction>) -> Option<Self> {
        if on_fulfilled.is_none() && on_rejected.is_none() {
            return None;
        }
        Some(Self::Closures {
            on_fulfilled,
            on_rejected,
        })
    }
}

/// The job `NewPromiseReactionJob` makes, one reaction with the value its promise settled with.
pub(super) struct ReactionJob {
    /// The record whose handler runs.
    reaction: ReactionRecord,
    /// The settlement of the promise.
    reaction_type: ReactionType,
    /// The value or the reason the promise settled with.
    argument: Value,
}

impl ReactionJob {
    /// Runs the reaction.
    ///
    /// Translation of the closure `NewPromiseReactionJob` of ECMA-262 section
    /// 27.2.2.1 returns. With an empty handler a fulfilment passes through as
    /// a normal completion and a rejection as a throw completion. With a
    /// handler its completion decides. A normal completion resolves the
    /// derived promise and a throw completion rejects it, and a reaction
    /// without a capability ends with its handler.
    pub(super) fn run(self, agent: &Agent) {
        let Self {
            reaction,
            reaction_type,
            argument,
        } = self;
        let (handler_result, unused) = reaction.handlers.call(agent, reaction_type, argument);
        match (reaction.capability, handler_result) {
            (Some(capability), Completion::Throw(reason)) => capability.reject(agent, reason),
            (Some(capability), Completion::Normal(value)) => capability.resolve(agent, value),
            (None, _) => {}
        }
        drop(unused);
    }
}

/// The job `NewPromiseResolveThenableJob` makes, the adoption of the state of a promise.
pub(super) struct ThenableJob {
    /// The promise a resolve function received the thenable for.
    promise_to_resolve: Promise,
    /// The promise whose state the job adopts.
    thenable: Promise,
}

impl ThenableJob {
    /// Runs the adoption.
    ///
    /// Translation of the closure `NewPromiseResolveThenableJob` of ECMA-262
    /// section 27.2.2.2 returns. The job makes fresh resolving functions for
    /// the outer promise and calls `then` on the thenable with them. A
    /// thenable that is settled already therefore settles the outer promise
    /// two jobs after the resolve call, this job and the reaction job its
    /// `then` enqueues, and a reaction of the outer promise runs in the
    /// third.
    ///
    /// The promise that call of `then` derives is never read, since its
    /// reactions are none and the job drops it, and its settlement enqueues
    /// no job. So the record carries no capability and the job derives no
    /// promise, and the jobs and their order are those of the clause. The
    /// fresh resolving functions are the two handlers of that one record, of
    /// which one runs once, so their `alreadyResolved` flag never reads true
    /// and the handlers hold the outer promise alone.
    pub(super) fn run(self, agent: &Agent) {
        perform_promise_then(
            agent,
            &self.thenable,
            Handlers::Adopt(self.promise_to_resolve),
            None,
        );
    }
}

/// The resolve function and the reject function of one promise.
///
/// Translation of the functions `CreateResolvingFunctions` makes, ECMA-262
/// section 27.2.1.3. The two share the `alreadyResolved` flag, so the first
/// call of either wins and every later call does nothing. A clone shares the
/// flag as well.
///
/// The pair is the one [`Promise::with_resolvers`] makes with its promise,
/// and the flag of the pair stands in the promise, so the pair shares the
/// one `Rc` of the promise and allocates nothing. No other pair of a promise
/// exists, since the resolving functions `NewPromiseResolveThenableJob`
/// makes are the two handlers of the one reaction it registers on the
/// thenable, of which one runs once, so they need no flag.
#[derive(Clone)]
pub struct ResolvingFunctions {
    /// `[[Promise]]`, which holds `[[AlreadyResolved]]` as well.
    promise: Promise,
}

impl ResolvingFunctions {
    /// Resolves the promise with `resolution`.
    ///
    /// Translation of the Promise Resolve Functions of ECMA-262 section
    /// 27.2.1.3.2. A resolution that is a promise enqueues one
    /// `PromiseResolveThenableJob` and settles nothing yet, which is the
    /// extra tick of a thenable. Any other value fulfils the promise at once.
    /// A promise resolved with itself is rejected with a `TypeError`, whose
    /// message the clause does not fix and which carries none here.
    pub fn resolve(&self, agent: &Agent, resolution: Value) {
        if self.promise.0.already_resolved.replace(true) {
            return;
        }
        resolve_promise(agent, &self.promise, resolution);
    }

    /// Rejects the promise with `reason`.
    ///
    /// Translation of the Promise Reject Functions of ECMA-262 section
    /// 27.2.1.3.1.
    pub fn reject(&self, agent: &Agent, reason: Value) {
        if self.promise.0.already_resolved.replace(true) {
            return;
        }
        reject_promise(agent, &self.promise, reason);
    }
}

/// Resolves `promise` with `resolution`, the resolve function past its flag.
///
/// Steps 7 to 16 of the Promise Resolve Functions of ECMA-262 section
/// 27.2.1.3.2, as [`ResolvingFunctions::resolve`] describes them.
fn resolve_promise(agent: &Agent, promise: &Promise, resolution: Value) {
    match resolution {
        Value::Promise(thenable) if thenable.ptr_eq(promise) => {
            let self_resolution_error = Value::from(ErrorObject::type_error(""));
            reject_promise(agent, promise, self_resolution_error);
        }
        Value::Promise(thenable) => {
            agent.enqueue_job(Job::thenable(ThenableJob {
                promise_to_resolve: promise.clone(),
                thenable,
            }));
        }
        value => fulfill_promise(agent, promise, value),
    }
}

/// Fulfils `promise` with `value` and enqueues its fulfill reactions.
///
/// Translation of `FulfillPromise` of ECMA-262 section 27.2.1.4. The reaction
/// lists are emptied before the state changes, as the clause orders it. The
/// state keeps what [`Promise::kept`] answers and each reaction takes a copy
/// of the value, the last one the value itself, so a promise with no reaction
/// copies nothing.
fn fulfill_promise(agent: &Agent, promise: &Promise, value: Value) {
    let reactions = promise.0.reactions.take();
    if reactions.is_empty() {
        *promise.0.state.borrow_mut() = PromiseState::Fulfilled(value);
        return;
    }
    *promise.0.state.borrow_mut() = PromiseState::Fulfilled(promise.kept(&value));
    trigger_promise_reactions(agent, reactions, ReactionType::Fulfill, value);
}

/// Rejects `promise` with `reason` and enqueues its reject reactions.
///
/// Translation of `RejectPromise` of ECMA-262 section 27.2.1.7, without the
/// rejection tracker of its step 7. The state keeps the reason as
/// [`fulfill_promise`] keeps the value.
fn reject_promise(agent: &Agent, promise: &Promise, reason: Value) {
    let reactions = promise.0.reactions.take();
    if reactions.is_empty() {
        *promise.0.state.borrow_mut() = PromiseState::Rejected(reason);
        return;
    }
    *promise.0.state.borrow_mut() = PromiseState::Rejected(promise.kept(&reason));
    trigger_promise_reactions(agent, reactions, ReactionType::Reject, reason);
}

/// Enqueues one reaction job per record, in registration order.
///
/// Translation of `TriggerPromiseReactions` of ECMA-262 section 27.2.1.8. No
/// reaction runs here. Each becomes a job at the end of the queue, every job
/// with a copy of `argument` and the last one with `argument` itself.
fn trigger_promise_reactions(
    agent: &Agent,
    reactions: Reactions,
    reaction_type: ReactionType,
    argument: Value,
) {
    let Reactions { first, rest } = reactions;
    let Some(mut reaction) = first else {
        return;
    };
    for next in rest {
        agent.enqueue_job(Job::reaction(ReactionJob {
            reaction,
            reaction_type,
            argument: argument.clone(),
        }));
        reaction = next;
    }
    agent.enqueue_job(Job::reaction(ReactionJob {
        reaction,
        reaction_type,
        argument,
    }));
}

/// Registers one pair of reactions on `promise`.
///
/// Translation of `PerformPromiseThen` of ECMA-262 section 27.2.5.4.1. On a
/// pending promise the pair is recorded. On a settled promise the matching
/// reaction job is enqueued at once, and it still runs as a job. `capability`
/// is `None` for an await, which derives no promise, and for a reaction whose
/// derived promise no code reads.
pub(super) fn perform_promise_then(
    agent: &Agent,
    promise: &Promise,
    handlers: Handlers,
    capability: Option<ResolvingFunctions>,
) {
    let reaction = ReactionRecord {
        capability,
        handlers,
    };
    let settled = match &*promise.0.state.borrow() {
        PromiseState::Pending => None,
        PromiseState::Fulfilled(value) => Some((ReactionType::Fulfill, value.clone())),
        PromiseState::Rejected(reason) => Some((ReactionType::Reject, reason.clone())),
    };
    let Some((reaction_type, argument)) = settled else {
        promise.0.reactions.borrow_mut().push(reaction);
        return;
    };
    agent.enqueue_job(Job::reaction(ReactionJob {
        reaction,
        reaction_type,
        argument,
    }));
}

impl Promise {
    /// Creates a pending promise and returns it with its resolving functions.
    ///
    /// Translation of `NewPromiseCapability` of ECMA-262 section 27.2.1.5 for
    /// the constructor `%Promise%`, which is what `Promise.withResolvers`
    /// answers. The agent takes no part yet and stands in the signature
    /// because every other constructor of a promise needs it.
    ///
    /// This is `CreateResolvingFunctions` of ECMA-262 section 27.2.1.3 for
    /// the new promise as well, whose flag stands in the promise.
    #[must_use]
    pub fn with_resolvers(_agent: &Agent) -> (Self, ResolvingFunctions) {
        let promise = Self(Rc::new(PromiseInner {
            state: RefCell::new(PromiseState::Pending),
            reactions: RefCell::new(Reactions::default()),
            already_resolved: Cell::new(false),
        }));
        let resolving_functions = ResolvingFunctions {
            promise: promise.clone(),
        };
        (promise, resolving_functions)
    }

    /// Creates a promise and runs `executor` with its resolving functions.
    ///
    /// Translation of the `Promise` constructor of ECMA-262 section 27.2.3.1.
    /// The executor runs synchronously, a throw of it rejects the promise,
    /// and the value of its normal completion is dropped.
    pub fn new(
        agent: &Agent,
        executor: impl FnOnce(&Agent, &ResolvingFunctions) -> Completion,
    ) -> Self {
        let (promise, resolving_functions) = Self::with_resolvers(agent);
        if let Completion::Throw(reason) = executor(agent, &resolving_functions) {
            resolving_functions.reject(agent, reason);
        }
        promise
    }

    /// Returns `value` itself when it is a promise, and a promise resolved with it otherwise.
    ///
    /// Translation of `Promise.resolve` and `PromiseResolve` of ECMA-262
    /// section 27.2.4.7. The constructor check of the clause always holds,
    /// because every promise is a `%Promise%`.
    #[must_use]
    pub fn resolve(agent: &Agent, value: Value) -> Self {
        if let Value::Promise(promise) = value {
            return promise;
        }
        let (promise, resolving_functions) = Self::with_resolvers(agent);
        resolving_functions.resolve(agent, value);
        promise
    }

    /// Returns a promise rejected with `reason`.
    ///
    /// Translation of `Promise.reject` of ECMA-262 section 27.2.4.6.
    #[must_use]
    pub fn reject(agent: &Agent, reason: Value) -> Self {
        let (promise, resolving_functions) = Self::with_resolvers(agent);
        resolving_functions.reject(agent, reason);
        promise
    }

    /// Registers the two handlers and returns the promise their completion settles.
    ///
    /// Translation of `Promise.prototype.then` of ECMA-262 section 27.2.5.4.
    /// `None` is a handler that is not callable, so `then(None, Some(h))` is
    /// the `then(undefined, h)` of the reference and also its `catch(h)`,
    /// which the clause defines as that call. No handler runs before this
    /// function returns.
    #[must_use = "the derived promise is the only way to observe a throw of a handler"]
    pub fn then(
        &self,
        agent: &Agent,
        on_fulfilled: Option<Reaction>,
        on_rejected: Option<Reaction>,
    ) -> Self {
        self.then_with(
            agent,
            Handlers::Closures {
                on_fulfilled,
                on_rejected,
            },
        )
    }

    /// Registers `handlers` and returns the promise their completion settles.
    ///
    /// This is [`Promise::then`] for the handlers of every kind, the one the
    /// executor registers with its state in place among them.
    #[must_use = "the derived promise is the only way to observe a throw of a handler"]
    pub(crate) fn then_with(&self, agent: &Agent, handlers: Handlers) -> Self {
        let (derived, result_capability) = Self::with_resolvers(agent);
        perform_promise_then(agent, self, handlers, Some(result_capability));
        derived
    }

    /// Returns the state of the promise and the value it settled with.
    #[must_use]
    pub fn state(&self) -> PromiseState {
        self.0.state.borrow().clone()
    }

    /// Returns what the state of the promise keeps of `value` as it settles with reactions.
    ///
    /// That is a copy of `value`, or `undefined` where no code can read the
    /// state again: a promise whose one handle is the one the caller settles
    /// it through, the resolving functions or the handlers of an adoption,
    /// neither of which reads the state or hands out a handle, so the jobs
    /// of its reactions take the value with one copy fewer and nothing tells
    /// the two apart.
    fn kept(&self, value: &Value) -> Value {
        if Rc::strong_count(&self.0) == 1 {
            return Value::Undefined;
        }
        value.clone()
    }

    /// Returns whether `other` is this promise, which is `SameValue` of the two.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Promise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Promise").field(&self.state()).finish()
    }
}

/// Runs the body of an async function that has no `await` and returns its promise.
///
/// Translation of `AsyncFunctionStart` and `AsyncBlockStart` of ECMA-262
/// sections 27.7.5.1 and 27.7.5.2. `body` runs now. A normal completion goes
/// through the resolve function of the promise, so an async function that
/// returns a promise costs the extra jobs of [`ResolvingFunctions::resolve`],
/// and a throw rejects the promise. This is what reproduces the nested async
/// functions of a root resolver.
pub fn async_function(agent: &Agent, body: impl FnOnce(&Agent) -> Completion) -> Promise {
    let (promise, promise_capability) = Promise::with_resolvers(agent);
    match body(agent) {
        Completion::Normal(value) => promise_capability.resolve(agent, value),
        Completion::Throw(reason) => promise_capability.reject(agent, reason),
    }
    promise
}
