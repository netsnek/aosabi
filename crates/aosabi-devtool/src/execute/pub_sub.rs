// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool execute pub_sub: the pub sub of one request of a behaviour
 * table and the payload its publishes carry.
 *
 * Follows `Run.pubSub`, `materializeTopic` and `materializePublish` of
 * tests/sidecar/execute.mjs, whose grammar is tests/sidecar/README.md, the
 * behaviour table. No analogue in the reference.
 */

//! The pub sub of a request.
//!
//! The pub sub belongs to the request and not to the table. It is made on
//! first use, with `experimental_create_pub_sub` of aosabi-core, every spec of
//! the table reaches it, and the `interleave` requests of a `subscribe` share
//! it, so no listener of one request reaches a publish of another.
//! [`begin_request`] starts a request with none, and [`request_pub_sub`] makes
//! it.
//!
//! A payload crosses threads in the pub sub of aosabi-core and a value of the
//! executor belongs to one agent, so a publish of a spec carries a
//! [`Frozen`] copy of the value it made, which every subscriber turns back
//! into a value of its own agent.

use std::{
    cell::Cell,
    rc::Rc, //
};

use aosabi_core::{
    execute::value::{
        PlainObject,
        Value, //
    },
    glue::into_value::IntoValue,
    pub_sub::{
        PubSub,
        experimental_create_pub_sub, //
    },
};
use aosabi_json::{
    JsString,
    JsValue, //
};
use aosabi_scalars::JsDate;

thread_local! {
    /// The pub sub of the request that runs, once one of its specs needed it.
    static REQUEST_PUB_SUB: Cell<Option<&'static PubSub>> = const { Cell::new(None) };
}

/// Starts a request with no pub sub.
pub fn begin_request() {
    REQUEST_PUB_SUB.set(None);
}

/// Returns the pub sub of the request that runs, made on first use.
///
/// The pub sub of aosabi-core is a `static`, so the seam leaks one per
/// request that uses one, which lives as long as the run.
pub fn request_pub_sub() -> &'static PubSub {
    REQUEST_PUB_SUB.get().unwrap_or_else(|| {
        let pub_sub: &'static PubSub = Box::leak(Box::new(experimental_create_pub_sub()));
        REQUEST_PUB_SUB.set(Some(pub_sub));
        pub_sub
    })
}

/// A copy of a value of the executor that crosses threads.
///
/// The data a spec makes, the primitives, a `Date`, a list and a plain
/// object, is copied as it is. Any other value, a promise, a function or an
/// error, is copied as the JSON value `JSON.stringify` sees, which no spec of
/// the corpus publishes.
#[derive(Clone, Debug)]
pub enum Frozen {
    /// `undefined`.
    Undefined,
    /// `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A number.
    Number(f64),
    /// A string.
    String(JsString),
    /// A `Date` by its time value.
    Date(f64),
    /// A list.
    List(Vec<Frozen>),
    /// An object, its own keys in order.
    Object(Vec<(String, Frozen)>),
    /// Any other value, as `JSON.stringify` sees it.
    Json(JsValue),
}

impl Frozen {
    /// Returns the copy of `value`.
    pub fn of(value: &Value) -> Self {
        match value {
            Value::Undefined => Self::Undefined,
            Value::Null => Self::Null,
            Value::Bool(boolean) => Self::Bool(*boolean),
            Value::Number(number) => Self::Number(*number),
            Value::String(string) => Self::String(string.clone()),
            Value::Date(date) => Self::Date(date.get_time()),
            Value::List(items) => Self::List(items.iter().map(Self::of).collect()),
            Value::Object(object) => Self::Object(
                object
                    .own_keys()
                    .into_iter()
                    .map(|key| {
                        let member = Self::of(&Rc::clone(object).get(&key));
                        (key, member)
                    })
                    .collect(),
            ),
            other => Self::Json(other.to_js()),
        }
    }
}

impl IntoValue for Frozen {
    fn into_value(self) -> Value {
        match self {
            Self::Undefined => Value::Undefined,
            Self::Null => Value::Null,
            Self::Bool(boolean) => Value::Bool(boolean),
            Self::Number(number) => Value::Number(number),
            Self::String(string) => Value::String(string),
            Self::Date(time) => Value::Date(JsDate::from_time_value(time)),
            Self::List(items) => Value::from(
                items
                    .into_iter()
                    .map(IntoValue::into_value)
                    .collect::<Vec<Value>>(),
            ),
            Self::Object(members) => Value::from(PlainObject::from_entries(
                members
                    .into_iter()
                    .map(|(key, member)| (key, member.into_value()))
                    .collect(),
            )),
            Self::Json(json) => Value::from_js(&json),
        }
    }
}
