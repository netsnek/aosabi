// SPDX-License-Identifier: MIT
/*
 * aosabi-core pub_sub: experimental_create_pub_sub over the hub of the
 * kernel.
 *
 * Derived from graphql-yoga 5.0.5, cjs/create-pub-sub.js, MIT.
 */

//! The pub sub of `@graphql-yoga/subscription`.
//!
//! The TypeScript entry exports `createPubSub` of `graphql-yoga` as `experimentalCreatePubSub`,
//! and a service keeps the one it makes in a module scope constant. The Rust
//! form is a `static`:
//!
//! ```ignore
//! use aosabi::{PubSub, Topic, experimental_create_pub_sub, json::JsObject};
//!
//! const CHANGED: Topic<(), JsObject> = Topic::new("changed");
//! const CHANGED_IN: Topic<String, JsObject> = Topic::new("changedIn");
//!
//! static PUB_SUB: PubSub = experimental_create_pub_sub();
//! ```
//!
//! The event target of the reference is the hub of the kernel's publish
//! module, so every protocol of a build reads the same events from it.
//!
//! # The rules
//!
//! The rules carry the numbers of the plan's section CORE, PUB SUB.
//!
//! 1. The literal rule. [`PubSub::publish`] builds the argument list of the
//!    reference, `[payload]` for a topic whose id is `()` and `[id,
//!    payload]` otherwise, and applies `args[1] ?? args[0] ?? null` and
//!    `args[1] === undefined ? routingKey : routingKey + ':' + String(args[0])`
//!    over the values the payloads answer, with `String` the `to_js_string`
//!    of the executor. The id stands in for a nullish payload, which is the
//!    proposed quirk `PUBSUB_NULLISH_PAYLOAD_TAKES_ID`.
//! 2. The state. The hub lives in the `static` behind a `std::sync::Mutex`,
//!    whose `new` is a `const fn`, as `Hub::new` is, so the handle is `Sync`
//!    and its address is unique, because the value is not zero sized. It is
//!    process wide, because a `static` has to be `Sync` and a publish may
//!    come from any thread of the process. The served binary runs every agent
//!    on the one thread of its executor, so on a server the lock is never
//!    contended.
//! 3. The subscription repeater. [`PubSub::subscribe`] returns a repeater
//!    whose executor is the port of `subscriptionRepeater` at
//!    `create-pub-sub.js:22-30`: it listens on the hub with the depth of the
//!    carrier policy of the request that subscribed, registers `stop.then` to
//!    unlisten, and spawns the pump. It runs on the first `next`, as the
//!    reference's does, so an event published before the stream is pulled is
//!    not received.
//! 4. The pump. The pump takes the oldest event of its slot, notes its
//!    revision on the request context, turns the payload into a value,
//!    pushes it, and awaits the push before it takes the next one, so a
//!    backlog waits in the hub, bounded and coalesced, where the reference
//!    queues it in the repeater and throws on the 1025th. The pump ends when
//!    its slot is gone.
//! 5. Threads. A publish appends to the hub and wakes the pump of every slot
//!    it reaches through the foreign waker of its task. A publish on an agent
//!    then polls once every future flagged on that agent at that point,
//!    before it returns. The pumps it woke there are among them, and so is
//!    any future a foreign waker flagged before the publish. A subscriber on
//!    the agent of the publish therefore takes the event inside the publish,
//!    as the listener of the reference pushes inside `dispatchEvent`, and a
//!    `next` pending on its stream settles with the event even when a
//!    `return` follows in the same run. A pump on another agent or thread
//!    delivers on the next turn of its own agent, which changes when a job
//!    runs and never which event a stream carries.
//! 6. Payloads. A payload crosses threads, so it is `Send` and `Sync`, which
//!    [`Payload`] states.
//! 7. The form of a card with the standard library. The `static` over
//!    `std::sync::Mutex` with `Arc` payloads is the form of the cards with the
//!    standard library and atomic read modify write.

mod pump;
pub mod topic;

use std::{
    rc::Rc,
    sync::{
        Arc,
        Mutex,
        PoisonError, //
    }, //
};

use aosabi_execute::{
    promise::Agent,
    repeater::{
        PushHandle,
        Repeater as ExecutorRepeater,
        StopHandle, //
    },
    value::{
        Completion,
        Value,
        to_js_string, //
    }, //
};
use aosabi_json::JsValue;
use aosabi_kernel::publish::{
    Hub,
    Lock,
    SlotId,
    publish, //
};
use aosabi_lang::js_string::to_string_lossy;

pub use self::topic::{
    Payload,
    Topic,
    TopicId, //
};
use crate::{
    carrier::{
        CarrierPolicy,
        carrier_policy, //
    },
    context::{
        Context,
        ambient_context, //
    },
    descriptor::Repeater,
    glue::into_value::IntoValue, //
};

/// A payload as the hub carries it, one `Arc` per publish shared by every slot it reaches.
// An `unsafe` `Rc` would skip two atomics.
pub type SharedPayload = Arc<dyn Payload>;

/// The pub sub a service keeps in a `static`: the hub behind a `std::sync::Mutex`.
pub struct PubSub {
    /// The event target, the hub of the kernel.
    hub: StdLock<Hub<SharedPayload>>,
}

/// The pub sub of [`PubSub::new`], for a value that is no `static`.
impl Default for PubSub {
    fn default() -> Self {
        Self::new()
    }
}

/// Creates `experimentalCreatePubSub()`, with no event target option.
///
/// The function is `const`, so the pub sub stands in a `static`.
#[must_use]
pub const fn experimental_create_pub_sub() -> PubSub {
    PubSub {
        hub: StdLock::new(Hub::new()),
    }
}

impl PubSub {
    /// Creates the pub sub of the native layer, the same as
    /// [`experimental_create_pub_sub`].
    ///
    /// A Rust author makes a value with `new`, so the native layer writes
    /// `static PUB_SUB: PubSub = PubSub::new();`, and the function is `const`
    /// for that `static`.
    #[must_use]
    pub const fn new() -> Self {
        experimental_create_pub_sub()
    }

    /// Performs `pubSub.publish(topic, ...args)`, callable from any thread, inside or outside a request.
    ///
    /// The arguments are `[payload]` for a topic whose id is `()` and `[id,
    /// payload]` otherwise, which [`PubSub::publish_args`] dispatches by the
    /// literal rule.
    // The author passes the id as the reference does, `publish(U, id, payload)`,
    // so the form takes it by value though the routing key only reads it.
    #[expect(clippy::needless_pass_by_value)]
    pub fn publish<K: TopicId, P: Payload + Clone>(
        &'static self,
        topic: Topic<K, P>,
        id: K,
        payload: P,
    ) {
        let payload: SharedPayload = Arc::new(payload);
        match id.topic_value() {
            None => self.publish_args(topic.name(), &[payload]),
            Some(id) => self.publish_args(topic.name(), &[Arc::new(id), payload]),
        }
    }

    /// Performs `pubSub.subscribe(topic, id)` and returns the stream of the topic's payloads.
    // The author passes the id as the reference does, `subscribe(U, id)`, so
    // the form takes it by value though the routing key only reads it.
    #[expect(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn subscribe<K: TopicId, P: Payload + Clone + IntoValue>(
        &'static self,
        topic: Topic<K, P>,
        id: K,
    ) -> Repeater<P> {
        Repeater::from_inner(self.subscribe_value(topic.name(), id.topic_value()))
    }

    /// Performs the untyped `publish(routingKey, ...args)`, which the typed form and the devtool both reach.
    ///
    /// The payload is `args[1] ?? args[0] ?? null` and the topic is the routing
    /// key, followed by `:` and `String(args[0])` when `args[1]` is not
    /// `undefined`. Each argument the rule reads is turned into a value of the
    /// executor once, on the calling thread, and the slots of the topic take the
    /// chosen payload as it is. On an agent every future flagged there once the
    /// hub has woken its pumps runs once before the publish returns, the pumps
    /// the publish woke among them, the dispatch of rule 5.
    pub fn publish_args(&'static self, routing_key: &str, args: &[SharedPayload]) {
        let first = args.first().map(|arg| arg.to_value());
        let second = args.get(1).map(|arg| arg.to_value());
        let payload: SharedPayload = match (args, &first, &second) {
            ([_, payload, ..], _, Some(value)) if !value.is_nullish() => Arc::clone(payload),
            ([payload, ..], Some(value), _) if !value.is_nullish() => Arc::clone(payload),
            _ => Arc::new(JsValue::Null),
        };
        let topic = match (&first, &second) {
            (Some(id), Some(value)) if !matches!(value, Value::Undefined) => {
                format!("{routing_key}:{}", to_string_lossy(&to_js_string(id)))
            }
            _ => String::from(routing_key),
        };
        publish(&self.hub, &topic, &payload);
        if let Some(agent) = Agent::current() {
            agent.poll_woken_tasks();
        }
    }

    /// Performs the untyped `subscribe(routingKey, id)` and returns the repeater of the executor.
    ///
    /// The topic is the routing key, followed by `:` and the id when the id is
    /// not `undefined`. The depth of the slot is the one of the carrier policy
    /// of the request that subscribes, and the request context takes the
    /// revision of every event the stream delivers.
    #[must_use]
    pub fn subscribe_value(
        &'static self,
        routing_key: &str,
        id: Option<JsValue>,
    ) -> Rc<ExecutorRepeater> {
        let topic = match id {
            None | Some(JsValue::Undefined) => String::from(routing_key),
            Some(id) => {
                let id = to_js_string(&Value::from_js(&id));
                format!("{routing_key}:{}", to_string_lossy(&id))
            }
        };
        let context = ambient_context();
        let depth = context
            .as_ref()
            .map_or(CarrierPolicy::NETWORK.depth, |context| {
                carrier_policy(context).depth
            });
        ExecutorRepeater::new(Box::new(move |agent, push, stop| {
            self.subscription_repeater(agent, &topic, depth, context, push, stop)
        }))
    }

    /// Runs the executor of a subscription, `subscriptionRepeater` at `create-pub-sub.js:22-30`.
    ///
    /// The slot is listened, the stop of the repeater unlistens it, and the
    /// pump moves the slot's events into the repeater. The executor answers
    /// `undefined`, as the function of the reference does, so the repeater
    /// runs until it is stopped.
    fn subscription_repeater(
        &'static self,
        agent: &Agent,
        topic: &str,
        depth: u16,
        context: Option<Context>,
        push: PushHandle,
        stop: StopHandle,
    ) -> Completion {
        let slot = self.hub.with(|hub| hub.listen(topic, depth));
        let _ = stop.stopped().then(
            agent,
            Some(Box::new(move |_, _| {
                self.unlisten(slot);
                Completion::Normal(Value::Undefined)
            })),
            None,
        );
        pump::spawn(agent, self, slot, push, stop, context);
        Completion::Normal(Value::Undefined)
    }

    /// Performs `removeEventListener` of the slot `slot` and wakes the pump that waits on it.
    fn unlisten(&self, slot: SlotId) {
        if let Some(waker) = self.hub.with(|hub| hub.unlisten(slot)) {
            waker.wake();
        }
    }
}

/// The server's lock of the kernel, `std::sync::Mutex` with a poisoned guard taken as it stands.
///
/// A holder of the lock runs one operation of the hub, none of which leaves
/// the hub torn when it panics, so the value under a poisoned lock is used
/// as it stands.
// An `unsafe` `Sync` cell would skip the lock.
pub struct StdLock<T>(Mutex<T>);

impl<T> StdLock<T> {
    /// Creates the lock around `value`.
    ///
    /// The function is `const`, so a lock and its hub stand in a `static`.
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self(Mutex::new(value))
    }
}

impl<T> Lock<T> for StdLock<T> {
    /// Runs `body` with the value locked.
    fn with<R>(&self, body: impl FnOnce(&mut T) -> R) -> R {
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        body(&mut guard)
    }
}

// A `static` pub sub has to be `Sync`, and a payload of the twins is a
// `JsObject`, so both claims are checked where the types are defined.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PubSub>();
    assert_send_sync::<aosabi_json::JsObject>();
};
