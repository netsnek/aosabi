// SPDX-License-Identifier: MIT
/*
 * aosabi-core pub_sub topic: the typed topic, its id and its payload.
 *
 * One member of it per topic.
 *
 * Derived from graphql-yoga 5.0.5, typings/create-pub-sub.d.ts, MIT.
 */

//! The typed topic of the pub sub.
//!
//! The reference types a pub sub by one map of its topics,
//! `{changed: [payload: Change], changedIn: [section: string, payload:
//! Change]}`, and a Rust service writes one constant per member of that map,
//! `const CHANGED: Topic<(), Change> = Topic::new("changed");`. A member with
//! no id takes `()` as its id, the one place the Rust call carries an argument
//! the TypeScript call leaves out, because Rust has no overload by arity.
//!
//! A payload crosses threads, because a publish may come from any thread of
//! the process, so [`Payload`] and [`TopicId`] are `Send` and `Sync`. A
//! derived struct with an `Rc` field is neither and cannot be published,
//! which the compiler refuses at the call. A `JsObject` is both, and so is
//! every plain derived struct.

use core::marker::PhantomData;

use aosabi_execute::value::Value;
use aosabi_json::JsValue;

use crate::{
    descriptor::ID,
    glue::into_value::IntoValue, //
};

/// One member of the topic map, `messageAdded: [channel: string, payload: Message]`.
///
/// `K` is the type of the id, `()` for a member without one, and `P` the
/// type of the payload.
pub struct Topic<K, P> {
    /// The routing key of the member.
    name: &'static str,
    /// The types of the id and of the payload.
    marker: PhantomData<fn() -> (K, P)>,
}

impl<K, P> Topic<K, P> {
    /// Creates the member `name` of the topic map.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            marker: PhantomData,
        }
    }

    /// Returns the routing key of the member.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

impl<K, P> Clone for Topic<K, P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K, P> Copy for Topic<K, P> {}

/// The id of a topic, `()` for a topic without one.
pub trait TopicId: Send + Sync + 'static {
    /// Returns the id as the value the routing key is built from, `None` for `()`.
    ///
    /// `None` is the `undefined` of an absent argument, which the rule of the
    /// routing key reads as no id.
    fn topic_value(&self) -> Option<JsValue>;
}

impl TopicId for () {
    fn topic_value(&self) -> Option<JsValue> {
        None
    }
}

impl TopicId for String {
    fn topic_value(&self) -> Option<JsValue> {
        Some(JsValue::from(self.as_str()))
    }
}

impl TopicId for &'static str {
    fn topic_value(&self) -> Option<JsValue> {
        Some(JsValue::from(*self))
    }
}

impl TopicId for ID {
    fn topic_value(&self) -> Option<JsValue> {
        Some(JsValue::from(self.0.as_str()))
    }
}

/// Implements [`TopicId`] for a type whose id is the value `JsValue::from` makes of it.
///
/// A number becomes the double of the TypeScript `number`, rounded to the
/// nearest one above 2^53 as the reference holds it.
macro_rules! plain_topic_id {
    ($($type:ty),+ $(,)?) => {
        $(
            impl TopicId for $type {
                fn topic_value(&self) -> Option<JsValue> {
                    Some(JsValue::from(*self))
                }
            }
        )+
    };
}

plain_topic_id!(bool, f64, i32, u32, i64, u64);

/// A value a publish carries across threads and a subscriber turns into a value on its agent.
///
/// The payload is shared, one `Arc` per publish and one clone of it per slot
/// the publish reaches, and each subscriber converts it on the agent of its
/// stream, because a value of the executor belongs to one agent. A type with
/// an `Rc` inside is neither `Send` nor `Sync` and has no implementation.
pub trait Payload: Send + Sync + 'static {
    /// Returns the payload as a value of the executor of the calling agent.
    fn to_value(&self) -> Value;
}

impl<T: IntoValue + Clone + Send + Sync + 'static> Payload for T {
    fn to_value(&self) -> Value {
        self.clone().into_value()
    }
}
