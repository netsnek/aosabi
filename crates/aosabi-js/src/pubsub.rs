// SPDX-License-Identifier: MIT
/*
 * aosabi-js pubsub: the pub subs of an app of the TypeScript option, whose
 * topics live in the pub sub of the engine.
 *
 * No analogue in the reference, whose createPubSub of @graphql-yoga/subscription 5.0.5
 * dispatches the events of its topics on an EventTarget of the process.
 */

//! The pub subs of the app in the pub sub of the engine.
//!
//! The function `createPubSub` of the entry `aosabi`, which the entry also exports as
//! `experimentalCreatePubSub`, answers a pub sub of the host that
//! holds a number and no topic of its own. Its `subscribe(routingKey, id)` answers a
//! marker, which crosses as the tag [󰀄](crate::host::Tag::Topic) where a resolver
//! answers it, and [`topic`] reads it into the repeater of the topic that
//! [`aosabi_core::PubSub::subscribe_value`] answers, the repeater of the executor a
//! Rust service subscribes through. Its `publish(routingKey, ...args)` crosses through
//! the export `publish` in a microtask of the host, never inside an import, with each
//! argument a copy of JSON data, and [`publish`] hands it to
//! [`aosabi_core::PubSub::publish_args`], which keeps the rules of the payload and the
//! topic of the reference.
//!
//! Each pub sub of the host is one [`PubSub`] here, made at the first use of its
//! number and kept as long as the instance, since a pub sub of an app lives in a
//! module scope constant as long as its process. A pump of a subscriber waits on the
//! agent of the request of its stream, which the module drives only from an export, so
//! a publish drives every request whose body the module pushes, and the pumps the hub
//! woke move the event into their repeaters there.

#![forbid(unsafe_code)]

use core::cell::RefCell;
use std::sync::Arc;

use aosabi_core::{
    PubSub,
    experimental_create_pub_sub,
    pub_sub::SharedPayload, //
};
use aosabi_execute::value::{
    ObjectLike,
    Value, //
};
use aosabi_json::JsValue;
use aosabi_lang::js_string::to_string_lossy;

use crate::{
    host::{
        self,
        Reader, //
    },
    request,
};

/// The pub subs a process may make, past which a number names no pub sub, its markers
/// fail and its publishes are lost.
const MOST: usize = 1024;

/// The most arguments a publish takes, past which the publish is lost.
const ARGUMENTS: usize = 64;

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The pub subs of the app, by their number.
    static PUB_SUBS: RefCell<Vec<&'static PubSub>> = const { RefCell::new(Vec::new()) };
}

/// Returns the pub sub of the number `number`, made at its first use, or
/// `None` past [`MOST`].
fn pub_sub(number: u32) -> Option<&'static PubSub> {
    let index = usize::try_from(number).ok().filter(|index| *index < MOST)?;
    PUB_SUBS.with_borrow_mut(|made| {
        while made.len() <= index {
            made.push(Box::leak(Box::new(experimental_create_pub_sub())));
        }
        made.get(index).copied()
    })
}

/// Reads the topic of a marker of `subscribe` at the cursor of `reader`, the
/// number of its pub sub, its routing key and its id, into the repeater of
/// that topic, or answers `None` for a broken record.
///
/// The repeater runs its executor at the first `next`, which listens on the
/// hub with the depth of the carrier of the request that subscribes, as a
/// subscription of a Rust service does.
pub fn topic(reader: &mut Reader<'_>) -> Option<Value> {
    let number = reader.word()?;
    let routing_key = reader.string()?;
    let id = host::read_json(reader)?;
    let repeater = pub_sub(number)?.subscribe_value(&to_string_lossy(&routing_key), Some(id));
    let object: std::rc::Rc<dyn ObjectLike> = repeater;
    Some(Value::Object(object))
}

/// Publishes the record in the first `len` bytes of the input buffer, the
/// export `publish`: the number of the pub sub, the routing key and the count
/// and the copies of the arguments, and drives every request whose body the
/// module pushes. A broken record publishes nothing.
pub fn publish(len: u32) {
    let record = host::input(len);
    let mut reader = Reader::new(&record);
    let Some((pub_sub, routing_key, args)) = read_publish(&mut reader) else {
        return;
    };
    pub_sub.publish_args(&routing_key, &args);
    request::drive_pushed();
}

/// Reads the record of a publish at the cursor of `reader`, or answers `None`
/// for a broken record.
fn read_publish(reader: &mut Reader<'_>) -> Option<(&'static PubSub, String, Vec<SharedPayload>)> {
    let pub_sub = pub_sub(reader.word()?)?;
    let routing_key = to_string_lossy(&reader.string()?).into_owned();
    let count = reader.count()?;
    if count > ARGUMENTS {
        return None;
    }
    let mut args: Vec<SharedPayload> = Vec::with_capacity(count);
    for _ in 0..count {
        let arg: JsValue = host::read_json(reader)?;
        // An `unsafe` `Rc` would skip the atomic count each subscriber takes.
        args.push(Arc::new(arg));
    }
    Some((pub_sub, routing_key, args))
}
